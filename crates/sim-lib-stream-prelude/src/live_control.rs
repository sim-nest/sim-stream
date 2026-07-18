use std::{sync::Arc, time::Duration};

use sim_kernel::{Cx, Error, Expr, NumberLiteral, Result, Symbol, Value};
use sim_lib_stream_core::{StreamDiagnostic, StreamPacket, StreamStats};
use sim_value::kind::expr_kind;

use crate::{
    cap::{
        stream_control_capability, stream_open_capability, stream_read_capability,
        stream_stats_capability,
    },
    card::{
        cell_card, clock_card, diagnostic_card, diagnostic_explanation, graph_card,
        graph_expr_card, packet_card, stream_card, stream_card_with_age,
    },
    handle::StreamHandle,
    live::{GraphHandle, LiveCell, StreamRuntime},
};

pub(crate) fn list_fn(runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_read_capability())?;
    cx.require(&stream_stats_capability())?;
    let dropped_only = list_dropped_only(cx, args)?;
    let mut cards = Vec::new();
    for entry in runtime.stream_entries()? {
        if dropped_only && !has_dropped_packets(&entry.handle().stats()?) {
            continue;
        }
        let card = stream_card_with_age(cx, entry.handle(), entry.age().as_secs())?;
        cards.push(card.object().as_table(cx)?);
    }
    cx.factory().list(cards)
}

pub(crate) fn describe_fn(runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_read_capability())?;
    let [subject] = args else {
        return Err(Error::Eval(
            "stream/describe expects one subject".to_owned(),
        ));
    };
    let raw = unquote(subject);
    if let Expr::Call { .. } = raw {
        return graph_expr_card(cx, raw);
    }
    if let Some(clock) = clock_symbol(&raw) {
        return clock_card(cx, clock);
    }
    if let Ok(packet) = StreamPacket::try_from(raw.clone()) {
        return describe_packet(cx, &packet, None);
    }
    if let Ok(id) = symbol_or_string_expr(&raw)
        && let Some(cell) = runtime.cell_by_id(&id)?
    {
        return cell_card(cx, &cell);
    }

    let value = eval_value(cx, subject)?;
    if let Some(handle) = value.object().downcast_ref::<StreamHandle>() {
        cx.require(&stream_stats_capability())?;
        return stream_card(cx, handle);
    }
    if let Some(cell) = value.object().downcast_ref::<LiveCell>() {
        return cell_card(cx, cell);
    }
    if let Some(graph) = value.object().downcast_ref::<GraphHandle>() {
        return graph_card(cx, graph);
    }
    let expr = value.object().as_expr(cx)?;
    if let Ok(packet) = StreamPacket::try_from(expr.clone()) {
        return describe_packet(cx, &packet, None);
    }
    if let Some(clock) = clock_symbol(&expr) {
        return clock_card(cx, clock);
    }
    Err(Error::TypeMismatch {
        expected: "stream, packet, clock, diagnostic, cell, or graph",
        found: "unsupported-stream-describe-subject",
    })
}

pub(crate) fn graph_lisp_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    let [graph] = args else {
        return Err(Error::Eval(
            "stream/graph-lisp expects one stream graph".to_owned(),
        ));
    };
    let value = eval_value(cx, graph)?;
    if let Some(handle) = value.object().downcast_ref::<StreamHandle>() {
        return cx.factory().expr(handle.graph_lisp_expr());
    }
    if let Some(graph) = value.object().downcast_ref::<GraphHandle>() {
        return cx.factory().expr(graph.lisp_expr());
    }
    let expr = value.object().as_expr(cx)?;
    cx.factory().expr(expr)
}

pub(crate) fn explain_diagnostic_fn(
    _runtime: &StreamRuntime,
    cx: &mut Cx,
    args: &[Expr],
) -> Result<Value> {
    cx.require(&stream_read_capability())?;
    let [diagnostic, rest @ ..] = args else {
        return Err(Error::Eval(
            "stream/explain-diagnostic expects a diagnostic packet".to_owned(),
        ));
    };
    let diagnostic = diagnostic_arg(cx, diagnostic)?;
    let stream_id = match rest {
        [] => None,
        [stream] => Some(stream_id_arg(cx, stream)?),
        _ => {
            return Err(Error::Eval(
                "stream/explain-diagnostic accepts at most one stream id".to_owned(),
            ));
        }
    };
    cx.factory().table(vec![
        (
            Symbol::new("stream-id"),
            match &stream_id {
                Some(stream_id) => cx.factory().string(stream_id.to_string())?,
                None => cx.factory().nil()?,
            },
        ),
        (
            Symbol::new("kind"),
            cx.factory().symbol(diagnostic.kind().clone())?,
        ),
        (
            Symbol::new("message"),
            cx.factory().string(diagnostic.message().to_owned())?,
        ),
        (
            Symbol::new("explanation"),
            cx.factory()
                .string(diagnostic_explanation(&diagnostic, stream_id.as_ref()))?,
        ),
    ])
}

pub(crate) fn cell_fn(runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_open_capability())?;
    let [id, value] = args else {
        return Err(Error::Eval(
            "stream/cell expects an id and initial value".to_owned(),
        ));
    };
    let cell = Arc::new(LiveCell::new(symbol_arg(cx, id)?, f64_arg(cx, value)?));
    runtime.register_cell(Arc::clone(&cell))?;
    cx.factory().opaque(cell)
}

pub(crate) fn cell_value_fn(runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_read_capability())?;
    let [cell] = args else {
        return Err(Error::Eval("stream/cell-value expects one cell".to_owned()));
    };
    let cell = cell_arg(runtime, cx, cell)?;
    cell_snapshot_value(cx, &cell)
}

pub(crate) fn cell_set_fn(runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_control_capability())?;
    let [cell, value, rest @ ..] = args else {
        return Err(Error::Eval(
            "stream/cell-set! expects a cell and value".to_owned(),
        ));
    };
    let expected = match rest {
        [] => None,
        [version] => Some(u64_arg(cx, version)?),
        _ => {
            return Err(Error::Eval(
                "stream/cell-set! accepts at most one expected version".to_owned(),
            ));
        }
    };
    let cell = cell_arg(runtime, cx, cell)?;
    cell.set(f64_arg(cx, value)?, expected)?;
    cell_snapshot_value(cx, &cell)
}

pub(crate) fn reroute_fn(runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_control_capability())?;
    let [source, target] = args else {
        return Err(Error::Eval(
            "stream/reroute! expects source and target".to_owned(),
        ));
    };
    let graph = runtime.reroute(stream_id_arg(cx, source)?, stream_id_arg(cx, target)?)?;
    cx.factory().opaque(Arc::new(graph))
}

pub(crate) fn cancel_older_than_fn(
    runtime: &StreamRuntime,
    cx: &mut Cx,
    args: &[Expr],
) -> Result<Value> {
    cx.require(&stream_control_capability())?;
    let [seconds] = args else {
        return Err(Error::Eval(
            "stream/cancel-older-than! expects age seconds".to_owned(),
        ));
    };
    let cancelled = runtime.cancel_older_than(Duration::from_secs(u64_arg(cx, seconds)?))?;
    cx.factory().list(
        cancelled
            .into_iter()
            .map(|id| cx.factory().symbol(id))
            .collect::<Result<Vec<_>>>()?,
    )
}

fn describe_packet(
    cx: &mut Cx,
    packet: &StreamPacket,
    stream_id: Option<&Symbol>,
) -> Result<Value> {
    match packet {
        StreamPacket::Diagnostic(diagnostic) => diagnostic_card(cx, diagnostic, stream_id),
        _ => packet_card(cx, packet),
    }
}

fn handle_arg(cx: &mut Cx, expr: &Expr) -> Result<StreamHandle> {
    let value = eval_value(cx, expr)?;
    value
        .object()
        .downcast_ref::<StreamHandle>()
        .cloned()
        .ok_or(Error::TypeMismatch {
            expected: "stream handle",
            found: "non-stream-handle",
        })
}

fn cell_arg(runtime: &StreamRuntime, cx: &mut Cx, expr: &Expr) -> Result<Arc<LiveCell>> {
    if let Ok(id) = symbol_arg(cx, expr)
        && let Some(cell) = runtime.cell_by_id(&id)?
    {
        return Ok(cell);
    }
    let value = eval_value(cx, expr)?;
    if let Some(cell) = value.object().downcast_ref::<LiveCell>() {
        return runtime
            .cell_by_id(cell.id())?
            .ok_or_else(|| Error::Eval(format!("unknown stream cell {}", cell.id())));
    }
    Err(Error::TypeMismatch {
        expected: "stream cell",
        found: "non-stream-cell",
    })
}

fn diagnostic_arg(cx: &mut Cx, expr: &Expr) -> Result<StreamDiagnostic> {
    match StreamPacket::try_from(data_expr(cx, expr)?)? {
        StreamPacket::Diagnostic(diagnostic) => Ok(diagnostic),
        _ => Err(Error::TypeMismatch {
            expected: "stream diagnostic packet",
            found: "non-diagnostic-stream-packet",
        }),
    }
}

fn stream_id_arg(cx: &mut Cx, expr: &Expr) -> Result<Symbol> {
    if let Ok(handle) = handle_arg(cx, expr) {
        return Ok(handle.metadata().id().clone());
    }
    symbol_arg(cx, expr)
}

fn symbol_arg(cx: &mut Cx, expr: &Expr) -> Result<Symbol> {
    let raw = literal_expr(cx, expr)?;
    symbol_or_string_expr(&raw)
}

fn symbol_or_string_expr(expr: &Expr) -> Result<Symbol> {
    match expr {
        Expr::Symbol(symbol) => Ok(symbol.clone()),
        Expr::String(value) => Ok(Symbol::new(value.clone())),
        other => Err(Error::TypeMismatch {
            expected: "symbol or string",
            found: expr_kind(other),
        }),
    }
}

fn f64_arg(cx: &mut Cx, expr: &Expr) -> Result<f64> {
    let expr = literal_expr(cx, expr)?;
    match expr {
        Expr::Number(NumberLiteral { canonical, .. }) | Expr::String(canonical) => canonical
            .parse::<f64>()
            .map_err(|err| Error::Eval(format!("invalid f64 value {canonical}: {err}"))),
        Expr::Symbol(symbol) => {
            let canonical = symbol.to_string();
            canonical
                .parse::<f64>()
                .map_err(|err| Error::Eval(format!("invalid f64 value {canonical}: {err}")))
        }
        other => Err(Error::TypeMismatch {
            expected: "number or numeric string",
            found: expr_kind(&other),
        }),
    }
}

fn u64_arg(cx: &mut Cx, expr: &Expr) -> Result<u64> {
    let expr = literal_expr(cx, expr)?;
    match expr {
        Expr::Number(NumberLiteral { canonical, .. }) | Expr::String(canonical) => canonical
            .parse::<u64>()
            .map_err(|err| Error::Eval(format!("invalid u64 value {canonical}: {err}"))),
        Expr::Symbol(symbol) => {
            let canonical = symbol.to_string();
            canonical
                .parse::<u64>()
                .map_err(|err| Error::Eval(format!("invalid u64 value {canonical}: {err}")))
        }
        other => Err(Error::TypeMismatch {
            expected: "integer or integer string",
            found: expr_kind(&other),
        }),
    }
}

fn cell_snapshot_value(cx: &mut Cx, cell: &LiveCell) -> Result<Value> {
    cx.factory().table(vec![
        (
            Symbol::new("id"),
            cx.factory().string(cell.id().to_string())?,
        ),
        (
            Symbol::new("value"),
            cx.factory().string(cell.value()?.to_string())?,
        ),
        (
            Symbol::new("version"),
            cx.factory().string(cell.version()?.to_string())?,
        ),
    ])
}

fn list_dropped_only(cx: &mut Cx, args: &[Expr]) -> Result<bool> {
    if args.is_empty() {
        return Ok(false);
    }
    let mut dropped_only = false;
    let mut index = 0;
    while index < args.len() {
        let key = literal_expr(cx, &args[index])?;
        if keyword_name(&key).as_deref() != Some("dropped") {
            return Err(Error::Eval(
                "stream/list only accepts :dropped true".to_owned(),
            ));
        }
        let value = args
            .get(index + 1)
            .map(|expr| bool_arg(cx, expr))
            .transpose()?
            .unwrap_or(true);
        dropped_only = value;
        index += 2;
    }
    Ok(dropped_only)
}

fn bool_arg(cx: &mut Cx, expr: &Expr) -> Result<bool> {
    match literal_expr(cx, expr)? {
        Expr::Bool(value) => Ok(value),
        other => Err(Error::TypeMismatch {
            expected: "bool",
            found: expr_kind(&other),
        }),
    }
}

fn keyword_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Symbol(symbol) => Some(symbol.name.trim_start_matches(':').to_owned()),
        Expr::String(value) => Some(value.trim_start_matches(':').to_owned()),
        _ => None,
    }
}

fn has_dropped_packets(stats: &StreamStats) -> bool {
    stats.dropped_newest > 0
        || stats.dropped_oldest > 0
        || stats.overflow_errors > 0
        || stats.timeouts > 0
        || stats.rejected > 0
        || stats.timed_out > 0
}

fn clock_symbol(expr: &Expr) -> Option<Symbol> {
    let Expr::Symbol(symbol) = expr else {
        return None;
    };
    let text = symbol.to_string();
    if text.contains("clock") {
        Some(symbol.clone())
    } else {
        None
    }
}

fn literal_expr(cx: &mut Cx, expr: &Expr) -> Result<Expr> {
    let expr = unquote(expr);
    match expr {
        Expr::Nil
        | Expr::Bool(_)
        | Expr::Number(_)
        | Expr::Symbol(_)
        | Expr::String(_)
        | Expr::Bytes(_)
        | Expr::Map(_)
        | Expr::List(_)
        | Expr::Vector(_) => Ok(expr),
        other => cx.eval_expr(other)?.object().as_expr(cx),
    }
}

fn eval_value(cx: &mut Cx, expr: &Expr) -> Result<Value> {
    cx.eval_expr(unquote(expr))
}

fn data_expr(cx: &mut Cx, expr: &Expr) -> Result<Expr> {
    let expr = unquote(expr);
    match expr {
        Expr::Map(_) | Expr::List(_) | Expr::Vector(_) | Expr::Bytes(_) => Ok(expr),
        other => cx.eval_expr(other)?.object().as_expr(cx),
    }
}

fn unquote(expr: &Expr) -> Expr {
    match expr {
        Expr::Quote {
            mode: sim_kernel::QuoteMode::Quote,
            expr,
        } => (**expr).clone(),
        other => other.clone(),
    }
}

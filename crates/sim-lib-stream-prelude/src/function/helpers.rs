use std::sync::Arc;

use sim_kernel::{Args, Cx, Error, Expr, NumberLiteral, Result, Symbol, Value};
use sim_lib_stream_combinators::{Stream, StreamNode};
use sim_lib_stream_core::{StreamItem, StreamMetadata, StreamPacket, StreamValue};
use sim_value::kind::expr_kind;

use crate::handle::{RunReport, StreamHandle};

pub(super) fn handle_arg(cx: &mut Cx, expr: &Expr) -> Result<StreamHandle> {
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

pub(super) fn symbol_arg(cx: &mut Cx, expr: &Expr) -> Result<Symbol> {
    match literal_expr(cx, expr)? {
        Expr::Symbol(symbol) => Ok(symbol),
        Expr::String(value) => Ok(Symbol::new(value)),
        other => Err(Error::TypeMismatch {
            expected: "symbol or string",
            found: expr_kind(&other),
        }),
    }
}

pub(super) fn usize_arg(cx: &mut Cx, expr: &Expr) -> Result<usize> {
    let expr = literal_expr(cx, expr)?;
    let canonical = match expr {
        Expr::Number(NumberLiteral { canonical, .. }) | Expr::String(canonical) => canonical,
        Expr::Symbol(symbol) => symbol.to_string(),
        other => {
            return Err(Error::TypeMismatch {
                expected: "integer or integer string",
                found: expr_kind(&other),
            });
        }
    };
    canonical
        .parse::<usize>()
        .map_err(|err| Error::Eval(format!("invalid stream/window count {canonical}: {err}")))
}

pub(super) fn collect_stream_to_handle(cx: &mut Cx, stream: Stream) -> Result<Value> {
    let metadata = stream.metadata().clone();
    let mut items = Vec::new();
    while let Some(item) = stream.next_packet()? {
        items.push(item);
    }
    if !stream.is_done()? {
        return Err(Error::Eval(
            "stream transform source has not reached done".to_owned(),
        ));
    }
    handle_value_from_items(cx, metadata, items)
}

pub(super) fn handle_stream(handle: StreamHandle) -> Stream {
    Stream::new(HandleStream { handle })
}

pub(super) fn map_data_payload(cx: &mut Cx, item: StreamItem, mapper: Value) -> Result<StreamItem> {
    let ticks = item.ticks().to_vec();
    let packet = match item.packet().clone() {
        StreamPacket::Data(mut packet) => {
            let payload = cx.factory().expr(packet.payload)?;
            let mapped = cx.call_value(mapper, Args::new(vec![payload]))?;
            packet.payload = mapped.object().as_expr(cx)?;
            StreamPacket::Data(packet)
        }
        other => other,
    };
    StreamItem::with_ticks(packet, ticks)
}

pub(super) fn ensure_done(handle: &StreamHandle, op: &str) -> Result<()> {
    if handle.done()? {
        Ok(())
    } else {
        Err(Error::Eval(format!("{op} source has not reached done")))
    }
}

pub(super) fn eval_value(cx: &mut Cx, expr: &Expr) -> Result<Value> {
    cx.eval_expr(unquote(expr))
}

pub(super) fn data_expr(cx: &mut Cx, expr: &Expr) -> Result<Expr> {
    let expr = unquote(expr);
    match expr {
        Expr::Map(_) | Expr::List(_) | Expr::Vector(_) | Expr::Bytes(_) => Ok(expr),
        other => cx.eval_expr(other)?.object().as_expr(cx),
    }
}

pub(super) fn run_report_value(cx: &mut Cx, report: RunReport) -> Result<Value> {
    cx.factory().table(vec![
        (
            Symbol::new("packets"),
            cx.factory().string(report.packets.to_string())?,
        ),
        (
            Symbol::new("written"),
            cx.factory().string(report.written.to_string())?,
        ),
    ])
}

pub(super) fn handle_value_from_items(
    cx: &mut Cx,
    metadata: StreamMetadata,
    items: Vec<StreamItem>,
) -> Result<Value> {
    let stream = Arc::new(StreamValue::pull(metadata.clone(), items));
    stream.publish_claims(cx, metadata.subject_ref())?;
    cx.factory()
        .opaque(Arc::new(StreamHandle::source(metadata, stream)))
}

struct HandleStream {
    handle: StreamHandle,
}

impl StreamNode for HandleStream {
    fn metadata(&self) -> &StreamMetadata {
        self.handle.metadata()
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        self.handle.next_packet()
    }

    fn is_done(&self) -> Result<bool> {
        self.handle.done()
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

fn unquote(expr: &Expr) -> Expr {
    match expr {
        Expr::Quote {
            mode: sim_kernel::QuoteMode::Quote,
            expr,
        } => (**expr).clone(),
        other => other.clone(),
    }
}

//! Finite event algebra helpers for data streams.
//!
//! These helpers intentionally live in the stream-combinators crate. They build
//! programmable joins, lenses, and ordering on top of ordinary stream packets
//! without adding kernel protocol surface.

use sim_kernel::{Error, Expr, Result, Symbol};
use sim_lib_stream_core::{StreamItem, StreamPacket};

use crate::{Stream, filter_data_shape, map_data_expr, record_bang};

/// Returns the canonical data-packet kind for model events.
pub fn model_event_data_kind() -> Symbol {
    Symbol::qualified("stream/data", "model-event")
}

/// Returns the canonical data-packet kind for rank-frontier updates.
pub fn rank_frontier_data_kind() -> Symbol {
    Symbol::qualified("stream/data", "rank-frontier")
}

/// Returns the canonical data-packet kind produced by event joins.
pub fn event_join_data_kind() -> Symbol {
    Symbol::qualified("stream/data", "event-join")
}

/// Resolves a dotted `path` of symbol keys through nested map expressions.
///
/// Walks `expr` one segment at a time, descending into
/// [`Expr::Map`](sim_kernel::Expr) entries keyed by each symbol. Returns the
/// addressed sub-expression, or `None` if any segment is missing or the cursor
/// is not a map.
pub fn expr_path<'a>(expr: &'a Expr, path: &[Symbol]) -> Option<&'a Expr> {
    let mut cursor = expr;
    for segment in path {
        let Expr::Map(entries) = cursor else {
            return None;
        };
        cursor = entries.iter().find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol == segment => Some(value),
            _ => None,
        })?;
    }
    Some(cursor)
}

/// Keeps data packets whose payload field at `path` equals `expected`.
pub fn filter_data_field_eq(source: Stream, path: Vec<Symbol>, expected: Expr) -> Stream {
    filter_data_shape(source, move |payload| {
        Ok(expr_path(payload, &path) == Some(&expected))
    })
}

/// Projects each data payload down to the sub-expression at `path`.
///
/// A packet whose payload lacks `path` fails the stream with an eval error.
pub fn project_data_field(source: Stream, path: Vec<Symbol>) -> Stream {
    map_data_expr(source, move |payload| {
        expr_path(&payload, &path)
            .cloned()
            .ok_or_else(|| Error::Eval("stream/project field path not found".to_owned()))
    })
}

/// Replaces each data payload's value at `path` with `redaction`.
///
/// A packet whose payload does not traverse to `path` fails with an eval error.
pub fn redact_data_field(source: Stream, path: Vec<Symbol>, redaction: Expr) -> Stream {
    map_data_expr(source, move |payload| {
        redact_expr_path(payload, &path, redaction.clone())
    })
}

/// Inner-joins two data streams on equal field values.
///
/// Both streams are recorded to completion, then every `left` payload whose
/// value at `left_path` equals a `right` payload's value at `right_path` emits
/// a packet of `output_kind` carrying `{key, left, right}` and the union of
/// both items' ticks. Both sources must reach `done`.
pub fn join_data_on_field(
    left: Stream,
    right: Stream,
    left_path: Vec<Symbol>,
    right_path: Vec<Symbol>,
    output_kind: Symbol,
) -> Result<Stream> {
    let metadata = left.metadata().clone();
    let left_items = record_bang(&left)?.items().to_vec();
    let right_items = record_bang(&right)?.items().to_vec();
    let mut joined = Vec::new();

    for left_item in &left_items {
        let Some(left_payload) = data_payload(left_item) else {
            continue;
        };
        let Some(key) = expr_path(left_payload, &left_path).cloned() else {
            continue;
        };
        for right_item in &right_items {
            let Some(right_payload) = data_payload(right_item) else {
                continue;
            };
            if expr_path(right_payload, &right_path) == Some(&key) {
                joined.push(join_item(
                    &output_kind,
                    key.clone(),
                    left_item,
                    left_payload,
                    right_item,
                    right_payload,
                )?);
            }
        }
    }

    Ok(Stream::pull(metadata, joined))
}

/// Reorders recorded data packets by an `i64` score read from `path`.
///
/// The source is recorded to completion, each payload's value at `path` is
/// parsed as an `i64` (missing or unparseable scores default to `0`), and the
/// packets are stably sorted ascending, or descending when `descending` is set.
pub fn rank_data_by_i64_field(
    source: Stream,
    path: Vec<Symbol>,
    descending: bool,
) -> Result<Stream> {
    let recording = record_bang(&source)?;
    let metadata = recording.metadata().clone();
    let mut ranked = recording
        .items()
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, item)| {
            let score = data_payload(&item)
                .and_then(|payload| expr_path(payload, &path))
                .and_then(expr_i64)
                .unwrap_or(0);
            (index, score, item)
        })
        .collect::<Vec<_>>();

    ranked.sort_by(
        |(left_index, left_score, _), (right_index, right_score, _)| {
            let order = if descending {
                right_score.cmp(left_score)
            } else {
                left_score.cmp(right_score)
            };
            order.then_with(|| left_index.cmp(right_index))
        },
    );

    Ok(Stream::pull(
        metadata,
        ranked.into_iter().map(|(_, _, item)| item).collect(),
    ))
}

fn join_item(
    kind: &Symbol,
    key: Expr,
    left_item: &StreamItem,
    left_payload: &Expr,
    right_item: &StreamItem,
    right_payload: &Expr,
) -> Result<StreamItem> {
    let mut ticks = left_item.ticks().to_vec();
    for tick in right_item.ticks() {
        if !ticks.iter().any(|existing| existing.clock == tick.clock) {
            ticks.push(tick.clone());
        }
    }
    StreamItem::with_ticks(
        StreamPacket::data(
            kind.clone(),
            Expr::Map(vec![
                (field("key"), key),
                (field("left"), left_payload.clone()),
                (field("right"), right_payload.clone()),
            ]),
        ),
        ticks,
    )
}

fn data_payload(item: &StreamItem) -> Option<&Expr> {
    match item.packet() {
        StreamPacket::Data(packet) => Some(&packet.payload),
        _ => None,
    }
}

fn expr_i64(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Number(number) => number.canonical.parse().ok(),
        Expr::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn redact_expr_path(mut expr: Expr, path: &[Symbol], redaction: Expr) -> Result<Expr> {
    if path.is_empty() {
        return Ok(redaction);
    }
    redact_expr_path_inner(&mut expr, path, redaction)?;
    Ok(expr)
}

fn redact_expr_path_inner(expr: &mut Expr, path: &[Symbol], redaction: Expr) -> Result<()> {
    let Expr::Map(entries) = expr else {
        return Err(Error::Eval(
            "stream/redact field path does not traverse a map".to_owned(),
        ));
    };
    let Some((_, value)) = entries
        .iter_mut()
        .find(|(key, _)| matches!(key, Expr::Symbol(symbol) if symbol == &path[0]))
    else {
        return Err(Error::Eval("stream/redact field path not found".to_owned()));
    };
    if path.len() == 1 {
        *value = redaction;
        Ok(())
    } else {
        redact_expr_path_inner(value, &path[1..], redaction)
    }
}

fn field(name: &str) -> Expr {
    Expr::Symbol(Symbol::new(name))
}

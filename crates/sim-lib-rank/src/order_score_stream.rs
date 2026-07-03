//! Data-stream projection of a ranked frontier.
//!
//! Serializes a [`RankFrontier`] (its summary plus each scored ordinal) into
//! `sim-lib-stream-core` packets, so a ranked candidate set can be emitted over
//! a data stream as ordinal or coordinate payloads.

use sim_kernel::{Expr, Symbol};
use sim_lib_stream_core::{
    BufferPolicy, StreamDirection, StreamItem, StreamMedia, StreamMetadata, StreamPacket,
    StreamValue,
};

use crate::{Nat, order_score::RankFrontier};

use super::{RankFrontierPayload, ScoredOrdinal};

/// Returns the data-kind symbol for a frontier summary packet.
pub fn rank_frontier_data_kind() -> Symbol {
    Symbol::qualified("stream/data", "rank-frontier")
}

/// Returns the data-kind symbol for a coordinate payload packet.
pub fn rank_coordinate_data_kind() -> Symbol {
    Symbol::qualified("stream/data", "rank-coordinate")
}

/// Returns the data-kind symbol for an ordinal payload packet.
pub fn rank_ordinal_data_kind() -> Symbol {
    Symbol::qualified("stream/data", "rank-ordinal")
}

/// Projects a ranked frontier into a pull-based data stream.
///
/// Emits a leading summary packet followed by one packet per scored ordinal,
/// rendering each as an ordinal or coordinate payload per `payload`.
pub fn rank_frontier_stream(frontier: &RankFrontier, payload: RankFrontierPayload) -> StreamValue {
    let mut items = Vec::with_capacity(frontier.items().len().saturating_add(1));
    let payload_kind = match &payload {
        RankFrontierPayload::OrdinalContent => rank_ordinal_data_kind(),
        RankFrontierPayload::Coordinate { .. } => rank_coordinate_data_kind(),
    };
    items.push(StreamItem::new(StreamPacket::rank_frontier(
        rank_frontier_summary_expr(frontier, payload_kind),
    )));
    items.extend(frontier.items().iter().enumerate().map(|(position, item)| {
        StreamItem::new(rank_frontier_item_packet(
            frontier, position, item, &payload,
        ))
    }));
    StreamValue::pull(
        rank_data_metadata(frontier.id().clone(), items.len()),
        items,
    )
}

/// Builds the stream metadata for a frontier data stream of `capacity` items.
pub(crate) fn rank_data_metadata(id: Symbol, capacity: usize) -> StreamMetadata {
    StreamMetadata::new(
        Symbol::new(id.to_string()),
        StreamMedia::Data,
        StreamDirection::Source,
        Symbol::qualified("clock", "rank"),
        BufferPolicy::bounded(capacity.max(1)).expect("rank stream buffer capacity is nonzero"),
    )
}

/// Builds a coordinate-payload data packet for a ranked position.
///
/// Encodes the owner, position, space, and ordinal as map entries, appending
/// any `extra` key/value pairs (such as the score).
pub(crate) fn rank_coordinate_packet(
    owner_field: &'static str,
    owner: &Symbol,
    position_field: &'static str,
    position: usize,
    space: &Symbol,
    ordinal: &Nat,
    extra: Vec<(Expr, Expr)>,
) -> StreamPacket {
    let mut entries = rank_position_entries(owner_field, owner, position_field, position);
    entries.push(key_symbol("space", space.clone()));
    entries.push(key_nat("ordinal", ordinal));
    entries.extend(extra);
    StreamPacket::data(rank_coordinate_data_kind(), Expr::Map(entries))
}

fn rank_frontier_item_packet(
    frontier: &RankFrontier,
    position: usize,
    item: &ScoredOrdinal,
    payload: &RankFrontierPayload,
) -> StreamPacket {
    let score = key_string("score", item.score.to_string());
    match payload {
        RankFrontierPayload::OrdinalContent => {
            let mut entries =
                rank_position_entries("frontier", frontier.id(), "position", position);
            entries.push(key_nat("ordinal", &item.ordinal));
            entries.push(score);
            StreamPacket::data(rank_ordinal_data_kind(), Expr::Map(entries))
        }
        RankFrontierPayload::Coordinate { space } => rank_coordinate_packet(
            "frontier",
            frontier.id(),
            "position",
            position,
            space,
            &item.ordinal,
            vec![score],
        ),
    }
}

fn rank_frontier_summary_expr(frontier: &RankFrontier, payload_kind: Symbol) -> Expr {
    Expr::Map(vec![
        key_symbol("frontier", frontier.id().clone()),
        key_symbol("payload-kind", payload_kind),
        key_nat("count", &Nat::from(frontier.items().len())),
    ])
}

fn rank_position_entries(
    owner_field: &'static str,
    owner: &Symbol,
    position_field: &'static str,
    position: usize,
) -> Vec<(Expr, Expr)> {
    vec![
        key_symbol(owner_field, owner.clone()),
        key_nat(position_field, &Nat::from(position)),
    ]
}

fn key_symbol(name: &'static str, value: Symbol) -> (Expr, Expr) {
    (Expr::Symbol(Symbol::new(name)), Expr::Symbol(value))
}

/// Builds a map entry pairing a symbol key with a [`Nat`] number value.
pub(crate) fn key_nat(name: &'static str, value: &Nat) -> (Expr, Expr) {
    (
        Expr::Symbol(Symbol::new(name)),
        Expr::Number(value.to_number_literal()),
    )
}

fn key_string(name: &'static str, value: impl Into<String>) -> (Expr, Expr) {
    (Expr::Symbol(Symbol::new(name)), Expr::String(value.into()))
}

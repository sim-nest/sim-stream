use sim_kernel::{ContentId, Expr, Ref, Symbol, Tick};
use sim_lib_stream_core::{
    BufferOverflowPolicy, BufferPolicy, StreamDirection, StreamItem, StreamMedia, StreamMetadata,
    StreamPacket,
};

use crate::{
    Stream, event_join_data_kind, expr_path, fan, filter_data_field_eq, join_data_on_field,
    model_event_data_kind, project_data_field, rank_data_by_i64_field, rank_frontier_data_kind,
    record_bang, redact_data_field, take,
};

#[test]
fn event_algebra_filters_ranks_and_replays_model_branch() {
    let source = Stream::pull(
        data_metadata(),
        vec![
            model_event("a", "delta", "draft-a", 20, 1),
            model_event("b", "tool-call", "tool-b", 0, 2),
            model_event("b", "delta", "draft-b", 90, 3),
            rank_frontier("a", "coord-a", 20, 4),
            rank_frontier("b", "coord-b", 90, 5),
        ],
    );
    let fanout = fan(source);
    let deltas = filter_data_field_eq(
        fanout.left.filter_data_kind(model_event_data_kind()),
        path(&["event"]),
        Expr::String("delta".to_owned()),
    );
    let frontiers = fanout.right.filter_data_kind(rank_frontier_data_kind());

    let joined = join_data_on_field(
        deltas,
        frontiers,
        path(&["span-id"]),
        path(&["span-id"]),
        event_join_data_kind(),
    )
    .unwrap();
    let ranked = rank_data_by_i64_field(joined, path(&["right", "score"]), true).unwrap();
    let chosen = take(ranked, 1);
    let recording = record_bang(&chosen).unwrap();

    assert_eq!(recording.len(), 1);
    let payload = data_payload(&recording.items()[0]).unwrap();
    assert_eq!(
        expr_path(payload, &path(&["left", "text"])),
        Some(&Expr::String("draft-b".to_owned()))
    );
    assert_eq!(
        expr_path(payload, &path(&["right", "coordinate"])),
        Some(&Expr::String("coord-b".to_owned()))
    );

    let projected = project_data_field(recording.replay(), path(&["left", "text"]));
    let projected_items = projected.take_packets(8).unwrap();
    assert_eq!(projected_items.len(), 1);
    assert_eq!(
        data_payload(&projected_items[0]),
        Some(&Expr::String("draft-b".to_owned()))
    );

    let redacted = redact_data_field(
        recording.replay(),
        path(&["left", "text"]),
        Expr::String("REDACTED".to_owned()),
    );
    let redacted_items = redacted.take_packets(8).unwrap();
    assert_eq!(
        expr_path(
            data_payload(&redacted_items[0]).unwrap(),
            &path(&["left", "text"])
        ),
        Some(&Expr::String("REDACTED".to_owned()))
    );
    assert_eq!(
        recording.replay().take_packets(8).unwrap(),
        recording.replay().take_packets(8).unwrap()
    );
}

fn data_metadata() -> StreamMetadata {
    StreamMetadata::new(
        Symbol::qualified("stream", "event-algebra-test"),
        StreamMedia::Data,
        StreamDirection::Source,
        clock_symbol(),
        BufferPolicy::bounded_with_overflow(8, BufferOverflowPolicy::DropNewest).unwrap(),
    )
}

fn model_event(span: &str, event: &str, text: &str, score: i64, tick_index: u8) -> StreamItem {
    StreamItem::with_ticks(
        StreamPacket::model_event(Expr::Map(vec![
            field("span-id", span),
            field("event", event),
            field("text", text),
            score_field(score),
        ])),
        vec![tick(tick_index)],
    )
    .unwrap()
}

fn rank_frontier(span: &str, coordinate: &str, score: i64, tick_index: u8) -> StreamItem {
    StreamItem::with_ticks(
        StreamPacket::rank_frontier(Expr::Map(vec![
            field("span-id", span),
            field("coordinate", coordinate),
            score_field(score),
        ])),
        vec![tick(tick_index)],
    )
    .unwrap()
}

fn data_payload(item: &StreamItem) -> Option<&Expr> {
    match item.packet() {
        StreamPacket::Data(packet) => Some(&packet.payload),
        _ => None,
    }
}

fn field(name: &str, value: &str) -> (Expr, Expr) {
    (
        Expr::Symbol(Symbol::new(name)),
        Expr::String(value.to_owned()),
    )
}

fn score_field(score: i64) -> (Expr, Expr) {
    (
        Expr::Symbol(Symbol::new("score")),
        Expr::String(score.to_string()),
    )
}

fn path(segments: &[&str]) -> Vec<Symbol> {
    segments
        .iter()
        .map(|segment| Symbol::new(*segment))
        .collect()
}

fn clock_symbol() -> Symbol {
    Symbol::qualified("clock", "event-algebra-test")
}

fn tick(index: u8) -> Tick {
    Tick::new(
        clock_symbol(),
        Ref::Content(ContentId::from_bytes(
            Symbol::qualified("core", "sha256"),
            [index; 32],
        )),
    )
}

use sim_kernel::{ContentId, Cx, EventKind, Expr, Ref, Symbol};
use sim_lib_stream_core::{ClockDomain, StreamItem, StreamPacket};

#[cfg(feature = "rank-learn")]
use crate::order_score::{
    RankFrozenLearnedModel, learned_frequency_order, rank_learned_model_card,
};
use crate::order_score::{
    rank_coordinate_data_kind, rank_frontier_data_kind, rank_ordinal_data_kind,
};
use crate::{
    BandFrontierSpec, BeamFrontierSpec, GenericNodeNeighborhood, Nat, RankBuilder, RankCodec,
    RankError, RankFrontier, RankFrontierPayload, RankLimits, RankNode, RankOrdinalRange,
    RankPrimitiveCodec, RankScoreFn, ScoreValue, band_frontier, beam_frontier, novelty_frontier,
    ordinal_content_id,
};

use sim_kernel::testing::bare_cx as cx;

fn bool_triple_codec() -> RankPrimitiveCodec {
    RankPrimitiveCodec::new(
        RankBuilder::product(Symbol::qualified("rank-test", "score-bools"))
            .field(Symbol::new("a"), RankBuilder::bool())
            .field(Symbol::new("b"), RankBuilder::bool())
            .field(Symbol::new("c"), RankBuilder::bool())
            .build()
            .unwrap(),
    )
}

type TestScoreFn = fn(&dyn RankCodec, &Nat, &RankNode) -> crate::RankResult<ScoreValue>;

fn true_count_score() -> RankScoreFn<TestScoreFn> {
    RankScoreFn::new(
        Symbol::qualified("rank-score", "true-count"),
        |_codec, _ordinal, node| {
            let RankNode::Product(values) = node else {
                return Ok(0);
            };
            Ok(values
                .iter()
                .filter(|value| matches!(value, RankNode::Bool(true)))
                .count() as ScoreValue)
        },
    )
}

#[test]
fn band_frontier_yields_only_in_range_scores() {
    let codec = bool_triple_codec();
    let score = true_count_score();
    let frontier = band_frontier(
        BandFrontierSpec::new(
            Symbol::qualified("rank-frontier", "band"),
            RankOrdinalRange::closed_open(Nat::zero(), Nat::from(8_u64)),
            2,
            3,
            16,
        ),
        &codec,
        &score,
        &mut RankLimits::default(),
    )
    .unwrap();

    assert!(!frontier.items().is_empty());
    assert!(
        frontier
            .items()
            .iter()
            .all(|item| (2..=3).contains(&item.score))
    );
    assert!(
        frontier
            .items()
            .windows(2)
            .all(|pair| pair[0].ordinal < pair[1].ordinal)
    );
}

#[test]
fn beam_frontier_is_deterministic_for_fixed_width_and_limits() {
    let codec = bool_triple_codec();
    let metric = GenericNodeNeighborhood::default();
    let score = true_count_score();

    let left = beam_frontier(
        BeamFrontierSpec::new(
            Symbol::qualified("rank-frontier", "beam"),
            Nat::zero(),
            2,
            3,
            8,
        ),
        &codec,
        &metric,
        &score,
        &mut RankLimits::new(100, 64),
    )
    .unwrap();
    let right = beam_frontier(
        BeamFrontierSpec::new(
            Symbol::qualified("rank-frontier", "beam"),
            Nat::zero(),
            2,
            3,
            8,
        ),
        &codec,
        &metric,
        &score,
        &mut RankLimits::new(100, 64),
    )
    .unwrap();

    assert_eq!(left, right);
    assert_eq!(left.items()[0].score, 3);
}

#[test]
fn frontier_events_carry_content_or_coordinate_refs() {
    let codec = bool_triple_codec();
    let score = true_count_score();
    let frontier = band_frontier(
        BandFrontierSpec::new(
            Symbol::qualified("rank-frontier", "band"),
            RankOrdinalRange::closed_open(Nat::zero(), Nat::from(8_u64)),
            3,
            3,
            1,
        ),
        &codec,
        &score,
        &mut RankLimits::default(),
    )
    .unwrap();
    let run = Ref::Symbol(Symbol::qualified("rank-test", "run"));
    let mut cx = cx();

    let content_source = frontier.event_source(run.clone(), RankFrontierPayload::OrdinalContent);
    let content_event = content_source.next(&mut cx).unwrap().unwrap();
    assert!(matches!(
        content_event.kind,
        EventKind::Chunk {
            payload: Ref::Content(_)
        }
    ));
    assert!(matches!(content_event.ticks[0].index, Ref::Content(_)));

    let coord_source = frontier.event_source(
        run,
        RankFrontierPayload::Coordinate {
            space: Symbol::qualified("rank-test", "score-bools"),
        },
    );
    let coord_event = coord_source.next(&mut cx).unwrap().unwrap();
    assert!(matches!(
        coord_event.kind,
        EventKind::Chunk {
            payload: Ref::Coord(_)
        }
    ));
}

#[test]
fn frontier_stream_emits_same_ordinals_as_event_source() {
    let codec = bool_triple_codec();
    let score = true_count_score();
    let frontier = band_frontier(
        BandFrontierSpec::new(
            Symbol::qualified("rank-frontier", "stream-band"),
            RankOrdinalRange::closed_open(Nat::zero(), Nat::from(8_u64)),
            2,
            3,
            16,
        ),
        &codec,
        &score,
        &mut RankLimits::default(),
    )
    .unwrap();
    let mut cx = cx();
    let source = frontier.event_source(
        Ref::Symbol(Symbol::qualified("rank-test", "stream-run")),
        RankFrontierPayload::OrdinalContent,
    );
    let source_ordinals = collect_event_ordinals(&mut cx, &frontier, source);

    let stream = frontier.data_stream(RankFrontierPayload::OrdinalContent);
    assert_eq!(
        stream.metadata().clock(),
        &ClockDomain::ServerFrame.symbol()
    );
    let packets = stream.take_packets(32).unwrap();
    assert_data_packet_kind(&packets[0], rank_frontier_data_kind());
    assert_eq!(
        symbol_field(data_payload(&packets[0]), "payload-kind"),
        rank_ordinal_data_kind()
    );
    let stream_ordinals = packets
        .iter()
        .filter(|item| data_kind(item) == Some(rank_ordinal_data_kind()))
        .map(|item| nat_field(data_payload(item), "ordinal"))
        .collect::<Vec<_>>();

    assert_eq!(stream_ordinals, source_ordinals);
}

#[test]
fn coordinate_frontier_stream_emits_coordinate_payloads() {
    let codec = bool_triple_codec();
    let score = true_count_score();
    let space = Symbol::qualified("rank-test", "score-bools");
    let frontier = band_frontier(
        BandFrontierSpec::new(
            Symbol::qualified("rank-frontier", "stream-coordinates"),
            RankOrdinalRange::closed_open(Nat::zero(), Nat::from(8_u64)),
            3,
            3,
            4,
        ),
        &codec,
        &score,
        &mut RankLimits::default(),
    )
    .unwrap();
    let packets = frontier
        .data_stream(RankFrontierPayload::Coordinate {
            space: space.clone(),
        })
        .take_packets(16)
        .unwrap();

    assert_eq!(
        symbol_field(data_payload(&packets[0]), "payload-kind"),
        rank_coordinate_data_kind()
    );
    let coordinate_payloads = packets
        .iter()
        .filter(|item| data_kind(item) == Some(rank_coordinate_data_kind()))
        .map(data_payload)
        .collect::<Vec<_>>();

    assert_eq!(coordinate_payloads.len(), frontier.items().len());
    for (payload, item) in coordinate_payloads.iter().zip(frontier.items()) {
        assert_eq!(symbol_field(payload, "space"), space);
        assert_eq!(nat_field(payload, "ordinal"), item.ordinal);
    }
}

#[test]
fn fuel_event_buffer_and_position_unavailable_are_honored() {
    let codec = bool_triple_codec();
    let score = true_count_score();

    let limited = band_frontier(
        BandFrontierSpec::new(
            Symbol::qualified("rank-frontier", "band"),
            RankOrdinalRange::closed_open(Nat::zero(), Nat::from(8_u64)),
            0,
            3,
            2,
        ),
        &codec,
        &score,
        &mut RankLimits::default(),
    )
    .unwrap();
    assert_eq!(limited.items().len(), 2);
    assert_eq!(
        limited.position_of(&Nat::zero()),
        Err(RankError::PositionUnavailable {
            id: Symbol::qualified("rank-frontier", "band")
        })
    );

    let fuel_error = band_frontier(
        BandFrontierSpec::new(
            Symbol::qualified("rank-frontier", "band"),
            RankOrdinalRange::closed_open(Nat::zero(), Nat::from(8_u64)),
            0,
            3,
            8,
        ),
        &codec,
        &score,
        &mut RankLimits::new(1, 64),
    )
    .unwrap_err();
    assert!(matches!(fuel_error, RankError::LimitExceeded { .. }));
}

#[test]
fn novelty_frontier_rewards_distance_from_archive() {
    let codec = bool_triple_codec();
    let metric = GenericNodeNeighborhood::default();
    let frontier = novelty_frontier(
        Symbol::qualified("rank-frontier", "novelty"),
        &codec,
        &metric,
        [Nat::zero(), Nat::from(7_u64)],
        &[Nat::zero()],
        &mut RankLimits::default(),
        2,
    )
    .unwrap();

    assert_eq!(frontier.items()[0].ordinal, Nat::from(7_u64));
    assert!(frontier.items()[0].score > frontier.items()[1].score);
}

fn collect_event_ordinals(
    cx: &mut Cx,
    frontier: &RankFrontier,
    source: std::sync::Arc<dyn sim_kernel::EventSource>,
) -> Vec<Nat> {
    let mut ordinals = Vec::new();
    while let Some(event) = source.next(cx).unwrap() {
        match event.kind {
            EventKind::Chunk {
                payload: Ref::Content(content),
            } => ordinals.push(ordinal_for_content(frontier, &content)),
            EventKind::Done => break,
            _ => {}
        }
    }
    ordinals
}

fn ordinal_for_content(frontier: &RankFrontier, content: &ContentId) -> Nat {
    frontier
        .items()
        .iter()
        .find(|item| ordinal_content_id(&item.ordinal).unwrap() == *content)
        .map(|item| item.ordinal.clone())
        .expect("frontier event ordinal content must match an item")
}

fn assert_data_packet_kind(item: &StreamItem, expected: Symbol) {
    assert_eq!(data_kind(item), Some(expected));
}

fn data_kind(item: &StreamItem) -> Option<Symbol> {
    match item.packet() {
        StreamPacket::Data(packet) => Some(packet.kind.clone()),
        _ => None,
    }
}

fn data_payload(item: &StreamItem) -> &Expr {
    let StreamPacket::Data(packet) = item.packet() else {
        panic!("expected data packet");
    };
    &packet.payload
}

fn nat_field(expr: &Expr, name: &str) -> Nat {
    let Expr::Number(number) = field(expr, name) else {
        panic!("expected number field {name}");
    };
    Nat::from_number_literal(number).unwrap()
}

fn symbol_field(expr: &Expr, name: &str) -> Symbol {
    let Expr::Symbol(symbol) = field(expr, name) else {
        panic!("expected symbol field {name}");
    };
    symbol.clone()
}

fn field<'a>(expr: &'a Expr, name: &str) -> &'a Expr {
    let Expr::Map(entries) = expr else {
        panic!("expected map payload");
    };
    entries
        .iter()
        .find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == name => {
                Some(value)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing field {name}"))
}

#[cfg(feature = "rank-learn")]
#[test]
fn learned_order_changes_traversal_not_coordinate_identity() {
    let codec = bool_triple_codec();
    let model = RankFrozenLearnedModel::new(
        Symbol::qualified("rank-test", "learned-model"),
        "digest-a",
        [(Nat::from(7_u64), 100), (Nat::zero(), 1)],
        [Nat::from(7_u64), Nat::zero(), Nat::zero()],
    );
    let order = learned_frequency_order(
        Symbol::qualified("rank-order", "learned-frequency"),
        &codec,
        &model,
        &mut RankLimits::default(),
    )
    .unwrap();

    assert_eq!(
        order.canonical_ordinal(&Nat::zero()).unwrap(),
        Nat::from(7_u64)
    );
    let first = order.unrank_node(&codec, &Nat::zero()).unwrap();
    assert_eq!(codec.rank_node(&first).unwrap(), Nat::from(7_u64));
    assert_eq!(order.rank_node(&codec, &first).unwrap(), Nat::zero());
    assert_eq!(model.novelty_archive(), &[Nat::zero(), Nat::from(7_u64)]);
}

#[cfg(feature = "rank-learn")]
#[test]
fn repeated_learned_order_runs_emit_same_prefix() {
    let codec = bool_triple_codec();
    let model = RankFrozenLearnedModel::new(
        Symbol::qualified("rank-test", "learned-model"),
        "digest-a",
        [
            (Nat::from(5_u64), 9),
            (Nat::from(2_u64), 7),
            (Nat::from(7_u64), 9),
        ],
        [],
    );
    let left = learned_frequency_order(
        Symbol::qualified("rank-order", "learned-frequency"),
        &codec,
        &model,
        &mut RankLimits::default(),
    )
    .unwrap();
    let right = learned_frequency_order(
        Symbol::qualified("rank-order", "learned-frequency"),
        &codec,
        &model,
        &mut RankLimits::default(),
    )
    .unwrap();

    assert_eq!(
        &left.canonical_ordinals()[..4],
        &right.canonical_ordinals()[..4]
    );
    assert_eq!(left.canonical_ordinals()[0], Nat::from(5_u64));
    assert_eq!(left.canonical_ordinals()[1], Nat::from(7_u64));
}

#[cfg(feature = "rank-learn")]
#[test]
fn frozen_model_metadata_appears_in_card() {
    let mut cx = cx();
    let model = RankFrozenLearnedModel::new(
        Symbol::qualified("rank-test", "learned-model"),
        "sha256:abc",
        [(Nat::from(1_u64), 3)],
        [Nat::from(1_u64)],
    );
    let card = rank_learned_model_card(&mut cx, &model).unwrap();
    let table = card.object().as_table(&mut cx).unwrap();

    assert_eq!(
        field_display(&mut cx, &table, "model-id"),
        "rank-test/learned-model"
    );
    assert_eq!(field_display(&mut cx, &table, "version"), "1.0.0");
    assert_eq!(field_display(&mut cx, &table, "digest"), "sha256:abc");
    assert_eq!(field_display(&mut cx, &table, "frequency-count"), "1");
}

#[cfg(feature = "rank-learn")]
fn field_display(cx: &mut Cx, table: &sim_kernel::Value, field: &str) -> String {
    let Some(entries) = table.object().as_table_impl() else {
        panic!("expected card table");
    };
    entries
        .get(cx, Symbol::new(field))
        .unwrap()
        .object()
        .display(cx)
        .unwrap()
}

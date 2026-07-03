use std::collections::BTreeSet;

use sim_kernel::Symbol;

use crate::{
    Nat, RankBuilder, RankCodec, RankExactOrder, RankGroupCodec, RankNode, canonical_order,
    coordinate_for_nat, grade_first_order, reverse_window_order, round_robin_order,
    seeded_shuffle_order, then_order,
};

use sim_kernel::testing::bare_cx as cx;

fn color_space() -> Symbol {
    Symbol::qualified("rank-test", "color-order")
}

fn color_codec() -> RankGroupCodec {
    RankGroupCodec::new(
        RankBuilder::enumeration(
            color_space(),
            [
                Symbol::new("red"),
                Symbol::new("orange"),
                Symbol::new("yellow"),
                Symbol::new("green"),
                Symbol::new("blue"),
                Symbol::new("violet"),
            ],
        )
        .unwrap(),
    )
}

#[test]
fn finite_space_enumerates_differently_under_three_orders() {
    let codec = color_codec();
    let canonical = canonical_order(Symbol::qualified("rank/order", "canonical"), 6).unwrap();
    let reverse =
        reverse_window_order(Symbol::qualified("rank/order", "reverse-window"), 6, 3).unwrap();
    let round_robin =
        round_robin_order(Symbol::qualified("rank/order", "round-robin"), 6, 2).unwrap();

    let canonical_items = enum_indices(&canonical, &codec);
    let reverse_items = enum_indices(&reverse, &codec);
    let round_robin_items = enum_indices(&round_robin, &codec);

    assert_eq!(canonical_items, vec!["0", "1", "2", "3", "4", "5"]);
    assert_eq!(reverse_items, vec!["2", "1", "0", "5", "4", "3"]);
    assert_eq!(round_robin_items, vec!["0", "2", "4", "1", "3", "5"]);
}

#[test]
fn canonical_coordinate_is_unchanged_across_orders() {
    let mut cx = cx();
    let codec = color_codec();
    let node = RankNode::Enum {
        id: color_space(),
        index: Nat::from(4_u64),
    };
    let canonical_rank = codec.rank_node(&node).unwrap();
    let canonical_coordinate = coordinate_for_nat(&mut cx, color_space(), &canonical_rank).unwrap();
    let orders = [
        canonical_order(Symbol::qualified("rank/order", "canonical"), 6).unwrap(),
        reverse_window_order(Symbol::qualified("rank/order", "reverse-window"), 6, 3).unwrap(),
        seeded_shuffle_order(Symbol::qualified("rank/order", "seeded-shuffle"), 6, 42).unwrap(),
    ];

    for order in orders {
        let ordered_position = order.rank_node(&codec, &node).unwrap();
        let ordered_node = order.unrank_node(&codec, &ordered_position).unwrap();
        let rank_after_order_round_trip = codec.rank_node(&ordered_node).unwrap();
        let coordinate_after_order =
            coordinate_for_nat(&mut cx, color_space(), &rank_after_order_round_trip).unwrap();

        assert_eq!(rank_after_order_round_trip, canonical_rank);
        assert_eq!(coordinate_after_order, canonical_coordinate);
    }
}

#[test]
fn inverse_is_exact_for_canonical_grade_first_and_seeded_shuffle() {
    let codec = color_codec();
    let orders = [
        canonical_order(Symbol::qualified("rank/order", "canonical"), 6).unwrap(),
        grade_first_order(Symbol::qualified("rank/order", "grade-first"), &codec, 0).unwrap(),
        seeded_shuffle_order(Symbol::qualified("rank/order", "seeded-shuffle"), 6, 17).unwrap(),
    ];

    for order in orders {
        for position in 0_usize..6 {
            let position = Nat::from(position);
            let canonical = order.canonical_ordinal(&position).unwrap();
            let node = order.unrank_node(&codec, &position).unwrap();

            assert_eq!(order.position_of(&canonical).unwrap(), position);
            assert_eq!(order.rank_node(&codec, &node).unwrap(), position);
        }
    }
}

#[test]
fn composed_exact_orders_emit_no_duplicate_ordinal() {
    let first =
        reverse_window_order(Symbol::qualified("rank/order", "reverse-window"), 6, 3).unwrap();
    let second =
        seeded_shuffle_order(Symbol::qualified("rank/order", "seeded-shuffle"), 6, 99).unwrap();

    let composed = then_order(Symbol::qualified("rank/order", "then"), &first, &second).unwrap();

    let ordinals = composed.canonical_ordinals();
    let unique = ordinals.iter().cloned().collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), ordinals.len());
    assert_eq!(unique.len(), 6);
    for position in 0_usize..6 {
        let position = Nat::from(position);
        let canonical = composed.canonical_ordinal(&position).unwrap();
        assert_eq!(composed.position_of(&canonical).unwrap(), position);
    }
}

fn enum_indices(order: &RankExactOrder, codec: &RankGroupCodec) -> Vec<String> {
    (0_usize..6)
        .map(
            |position| match order.unrank_node(codec, &Nat::from(position)).unwrap() {
                RankNode::Enum { index, .. } => index.to_string(),
                other => panic!("expected enum node, found {other:?}"),
            },
        )
        .collect()
}

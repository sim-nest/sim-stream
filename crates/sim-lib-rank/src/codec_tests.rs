use num_bigint::BigInt;
use sim_kernel::Symbol;

use crate::{Nat, RankBuilder, RankCodec, RankNode, RankPrimitiveCodec};

fn color_grammar() -> crate::RankGrammar {
    RankBuilder::enumeration(
        Symbol::qualified("rank-test", "color"),
        [
            Symbol::new("red"),
            Symbol::new("green"),
            Symbol::new("blue"),
        ],
    )
    .unwrap()
}

fn roundtrip(grammar: crate::RankGrammar, node: RankNode) {
    let codec = RankPrimitiveCodec::new(grammar);
    let ordinal = codec.rank_node(&node).unwrap();
    assert!(codec.r_ok(&ordinal));
    assert_eq!(codec.unrank_node(&ordinal).unwrap(), node);
}

#[test]
fn primitive_codecs_round_trip_nodes() {
    roundtrip(RankBuilder::unit(), RankNode::Unit);
    roundtrip(RankBuilder::bool(), RankNode::Bool(false));
    roundtrip(RankBuilder::bool(), RankNode::Bool(true));
    roundtrip(RankBuilder::nat(), RankNode::Nat(Nat::from(42_u64)));
    roundtrip(RankBuilder::int(), RankNode::Int(BigInt::from(0)));
    roundtrip(RankBuilder::int(), RankNode::Int(BigInt::from(-7)));
    roundtrip(RankBuilder::int(), RankNode::Int(BigInt::from(7)));

    let color = color_grammar();
    roundtrip(
        color.clone(),
        RankNode::Enum {
            id: Symbol::qualified("rank-test", "color"),
            index: Nat::from(2_u64),
        },
    );

    let maybe_color = RankBuilder::sum(Symbol::qualified("rank-test", "maybe-color"))
        .alt(Symbol::new("none"), RankBuilder::unit())
        .alt(Symbol::new("some"), color)
        .build()
        .unwrap();
    roundtrip(
        maybe_color,
        RankNode::sum(
            1,
            RankNode::Enum {
                id: Symbol::qualified("rank-test", "color"),
                index: Nat::from(1_u64),
            },
        ),
    );
}

#[test]
fn mixed_radix_products_and_lists_round_trip() {
    let color = color_grammar();
    let pair = RankBuilder::product(Symbol::qualified("rank-test", "bool-color"))
        .field(Symbol::new("flag"), RankBuilder::bool())
        .field(Symbol::new("color"), color.clone())
        .build()
        .unwrap();
    let pair_codec = RankPrimitiveCodec::new(pair);
    assert_eq!(pair_codec.count(), Some(Nat::from(6_u64)));

    let node = RankNode::Product(vec![
        RankNode::Bool(true),
        RankNode::Enum {
            id: Symbol::qualified("rank-test", "color"),
            index: Nat::from(2_u64),
        },
    ]);
    assert_eq!(pair_codec.rank_node(&node).unwrap(), Nat::from(5_u64));
    for ordinal in 0_u64..6 {
        let ordinal = Nat::from(ordinal);
        let value = pair_codec.unrank_node(&ordinal).unwrap();
        assert_eq!(pair_codec.rank_node(&value).unwrap(), ordinal);
    }

    let list = RankBuilder::list(
        Symbol::qualified("rank-test", "bool-list"),
        RankBuilder::bool(),
        0,
        Some(3),
    )
    .unwrap();
    let list_codec = RankPrimitiveCodec::new(list);
    assert_eq!(list_codec.count(), Some(Nat::from(15_u64)));
    for ordinal in 0_u64..15 {
        let ordinal = Nat::from(ordinal);
        let value = list_codec.unrank_node(&ordinal).unwrap();
        assert_eq!(list_codec.rank_node(&value).unwrap(), ordinal);
    }
}

#[test]
fn set_and_map_rank_is_independent_of_insertion_order() {
    let set = RankBuilder::set(
        Symbol::qualified("rank-test", "color-set"),
        color_grammar(),
        Some(3),
    )
    .unwrap();
    let set_codec = RankPrimitiveCodec::new(set);
    let red = RankNode::Enum {
        id: Symbol::qualified("rank-test", "color"),
        index: Nat::from(0_u64),
    };
    let green = RankNode::Enum {
        id: Symbol::qualified("rank-test", "color"),
        index: Nat::from(1_u64),
    };
    let left = RankNode::Set(vec![green.clone(), red.clone()]);
    let right = RankNode::Set(vec![red.clone(), green.clone()]);
    let ordinal = set_codec.rank_node(&left).unwrap();
    assert_eq!(ordinal, set_codec.rank_node(&right).unwrap());
    assert_eq!(
        set_codec.unrank_node(&ordinal).unwrap(),
        RankNode::Set(vec![red, green])
    );

    let map = RankBuilder::map(
        Symbol::qualified("rank-test", "bool-color-map"),
        RankBuilder::bool(),
        color_grammar(),
        Some(2),
    )
    .unwrap();
    let map_codec = RankPrimitiveCodec::new(map);
    let false_key = RankNode::Bool(false);
    let true_key = RankNode::Bool(true);
    let blue = RankNode::Enum {
        id: Symbol::qualified("rank-test", "color"),
        index: Nat::from(2_u64),
    };
    let red = RankNode::Enum {
        id: Symbol::qualified("rank-test", "color"),
        index: Nat::from(0_u64),
    };
    let forward = RankNode::Map(vec![
        (true_key.clone(), blue.clone()),
        (false_key.clone(), red.clone()),
    ]);
    let reverse = RankNode::Map(vec![
        (false_key.clone(), red.clone()),
        (true_key.clone(), blue.clone()),
    ]);
    let ordinal = map_codec.rank_node(&forward).unwrap();
    assert_eq!(ordinal, map_codec.rank_node(&reverse).unwrap());
    assert_eq!(
        map_codec.unrank_node(&ordinal).unwrap(),
        RankNode::Map(vec![(false_key, red), (true_key, blue)])
    );
}

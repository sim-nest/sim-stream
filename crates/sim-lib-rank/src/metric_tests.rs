use sim_kernel::Symbol;

use crate::{
    GenericNodeNeighborhood, Nat, RankBuilder, RankCodec, RankLimits, RankNeighborhood, RankNode,
    RankPrimitiveCodec, beam_search, hill_climb,
};

fn bool_pair_codec() -> RankPrimitiveCodec {
    RankPrimitiveCodec::new(
        RankBuilder::product(Symbol::qualified("rank-test", "bool-pair"))
            .field(Symbol::new("left"), RankBuilder::bool())
            .field(Symbol::new("right"), RankBuilder::bool())
            .build()
            .unwrap(),
    )
}

fn bool_triple_codec() -> RankPrimitiveCodec {
    RankPrimitiveCodec::new(
        RankBuilder::product(Symbol::qualified("rank-test", "bool-triple"))
            .field(Symbol::new("a"), RankBuilder::bool())
            .field(Symbol::new("b"), RankBuilder::bool())
            .field(Symbol::new("c"), RankBuilder::bool())
            .build()
            .unwrap(),
    )
}

fn true_count(_ordinal: &Nat, node: &RankNode) -> crate::RankResult<i128> {
    let RankNode::Product(values) = node else {
        return Ok(0);
    };
    Ok(values
        .iter()
        .filter(|value| matches!(value, RankNode::Bool(true)))
        .count() as i128)
}

#[test]
fn every_neighbor_mutation_and_crossover_result_is_valid() {
    let codec = bool_pair_codec();
    let metric = GenericNodeNeighborhood::default();
    let start = codec
        .rank_node(&RankNode::Product(vec![
            RankNode::Bool(false),
            RankNode::Bool(false),
        ]))
        .unwrap();
    let target = codec
        .rank_node(&RankNode::Product(vec![
            RankNode::Bool(true),
            RankNode::Bool(true),
        ]))
        .unwrap();
    let mut limits = RankLimits::default();

    let neighbors = metric.neighbors(&codec, &start, &mut limits).unwrap();
    assert!(!neighbors.is_empty());
    for neighbor in &neighbors {
        assert_valid_round_trip(&codec, neighbor);
    }

    let mutation = metric
        .mutate(&codec, &start, 0xabcddcba, &mut RankLimits::default())
        .unwrap();
    assert_valid_round_trip(&codec, &mutation);

    let crossover = metric
        .crossover(&codec, &start, &target, 17, &mut RankLimits::default())
        .unwrap();
    assert_valid_round_trip(&codec, &crossover);
}

#[test]
fn mutate_is_deterministic_for_a_fixed_seed() {
    let codec = bool_triple_codec();
    let metric = GenericNodeNeighborhood::default();
    let start = Nat::zero();

    let left = metric
        .mutate(&codec, &start, 42, &mut RankLimits::default())
        .unwrap();
    let right = metric
        .mutate(&codec, &start, 42, &mut RankLimits::default())
        .unwrap();

    assert_eq!(left, right);
}

#[test]
fn distance_is_symmetric_and_zero_only_for_equal_addresses() {
    let codec = bool_pair_codec();
    let metric = GenericNodeNeighborhood::default();
    let left = Nat::zero();
    let right = Nat::from(3_u64);

    let zero = metric
        .distance(&codec, &left, &left, &mut RankLimits::default())
        .unwrap()
        .unwrap();
    let forward = metric
        .distance(&codec, &left, &right, &mut RankLimits::default())
        .unwrap()
        .unwrap();
    let reverse = metric
        .distance(&codec, &right, &left, &mut RankLimits::default())
        .unwrap()
        .unwrap();

    assert_eq!(zero, Nat::zero());
    assert_eq!(forward, reverse);
    assert!(forward > Nat::zero());
}

#[test]
fn hill_climb_improves_toy_score_until_local_optimum() {
    let codec = bool_triple_codec();
    let metric = GenericNodeNeighborhood::default();

    let result = hill_climb(
        &metric,
        &codec,
        &Nat::zero(),
        &mut RankLimits::default(),
        true_count,
    )
    .unwrap();

    assert_eq!(result.best.score, 3);
    assert_eq!(result.path.first().unwrap().score, 0);
    assert_eq!(result.path.last().unwrap().score, 3);
}

#[test]
fn beam_search_keeps_deterministic_best_frontier() {
    let codec = bool_triple_codec();
    let metric = GenericNodeNeighborhood::default();

    let left = beam_search(
        &metric,
        &codec,
        &Nat::zero(),
        2,
        3,
        &mut RankLimits::default(),
        true_count,
    )
    .unwrap();
    let right = beam_search(
        &metric,
        &codec,
        &Nat::zero(),
        2,
        3,
        &mut RankLimits::default(),
        true_count,
    )
    .unwrap();

    assert_eq!(left, right);
    assert_eq!(left.best.score, 3);
}

fn assert_valid_round_trip(codec: &dyn RankCodec, ordinal: &Nat) {
    assert!(codec.r_ok(ordinal));
    let node = codec.unrank_node(ordinal).unwrap();
    assert_eq!(codec.rank_node(&node).unwrap(), *ordinal);
}

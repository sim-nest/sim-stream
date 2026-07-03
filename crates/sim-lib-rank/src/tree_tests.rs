use sim_kernel::Symbol;

use crate::{
    Nat, RankBuilder, RankCodec, RankExactOrder, RankNode,
    tree::{
        RankFiniteMapCodec, RankListCodec, RankSetPermutationCodec, RankTreeCodec,
        rank_tree_balanced_order, rank_tree_grade_first_order, rank_tree_payload_order,
    },
};

fn payload_space(name: &str) -> Symbol {
    Symbol::qualified("rank-test", name)
}

fn enum_node(id: &Symbol, index: u64) -> RankNode {
    RankNode::Enum {
        id: id.clone(),
        index: Nat::from(index),
    }
}

#[test]
fn tree_composes_with_two_payload_spaces() {
    let bool_tree = RankTreeCodec::new(payload_space("bool-payload")).unwrap();
    let nat_tree = RankTreeCodec::new(payload_space("nat-payload")).unwrap();

    for codec in [&bool_tree, &nat_tree] {
        let leaf = RankTreeCodec::empty_node();
        let tree = codec.node(leaf.clone(), Nat::from(1_u64), leaf);
        let ordinal = codec.rank_node(&tree).unwrap();

        assert_eq!(codec.unrank_node(&ordinal).unwrap(), tree);
    }

    assert_ne!(bool_tree.grammar(), nat_tree.grammar());
}

#[test]
fn list_set_permutation_and_map_examples_round_trip() {
    let list = RankListCodec::new(
        Symbol::qualified("rank-test", "bool-list"),
        RankBuilder::bool(),
        0,
        Some(3),
    )
    .unwrap();
    let list_node = RankNode::List(vec![RankNode::Bool(true), RankNode::Bool(false)]);
    let list_ordinal = list.rank_node(&list_node).unwrap();
    assert_eq!(list.unrank_node(&list_ordinal).unwrap(), list_node);

    let item_space = Symbol::qualified("rank-test", "perm-item");
    let permutation = RankSetPermutationCodec::new(
        item_space.clone(),
        [Symbol::new("a"), Symbol::new("b"), Symbol::new("c")],
    )
    .unwrap();
    let permutation_node = RankNode::List(vec![
        enum_node(&item_space, 2),
        enum_node(&item_space, 0),
        enum_node(&item_space, 1),
    ]);
    let permutation_ordinal = permutation.rank_node(&permutation_node).unwrap();
    assert_eq!(
        permutation.unrank_node(&permutation_ordinal).unwrap(),
        permutation_node
    );
    assert_eq!(permutation.count().unwrap(), Nat::from(6_u64));

    let key_space = Symbol::qualified("rank-test", "map-key");
    let key = RankBuilder::enumeration(
        key_space.clone(),
        [Symbol::new("x"), Symbol::new("y"), Symbol::new("z")],
    )
    .unwrap();
    let map = RankFiniteMapCodec::new(
        Symbol::qualified("rank-test", "bool-map"),
        key,
        RankBuilder::bool(),
        Some(2),
    )
    .unwrap();
    let map_node = RankNode::Map(vec![
        (enum_node(&key_space, 2), RankNode::Bool(true)),
        (enum_node(&key_space, 0), RankNode::Bool(false)),
    ]);
    let reordered = RankNode::Map(vec![
        (enum_node(&key_space, 0), RankNode::Bool(false)),
        (enum_node(&key_space, 2), RankNode::Bool(true)),
    ]);
    let map_ordinal = map.rank_node(&map_node).unwrap();
    assert_eq!(map.rank_node(&reordered).unwrap(), map_ordinal);
    assert_eq!(map.unrank_node(&map_ordinal).unwrap(), reordered);
}

#[test]
fn balanced_order_differs_from_grade_first() {
    let codec = RankTreeCodec::new(payload_space("balanced-payload")).unwrap();
    let grade_first = rank_tree_grade_first_order(&codec, 2).unwrap();
    let balanced = rank_tree_balanced_order(&codec, 2).unwrap();

    assert_ne!(
        grade_first.canonical_ordinals(),
        balanced.canonical_ordinals()
    );

    let first_changed = grade_first
        .canonical_ordinals()
        .iter()
        .zip(balanced.canonical_ordinals())
        .position(|(left, right)| left != right)
        .unwrap();
    let balanced_node = balanced
        .unrank_node(&codec, &Nat::from(first_changed))
        .unwrap();
    let grade_node = grade_first
        .unrank_node(&codec, &Nat::from(first_changed))
        .unwrap();
    assert_ne!(balanced_node, grade_node);
}

#[test]
fn payload_order_changes_traversal_but_not_address() {
    let codec = RankTreeCodec::new(payload_space("ordered-payload")).unwrap();
    let grade_first = rank_tree_grade_first_order(&codec, 2).unwrap();
    let reversed_payload = RankExactOrder::new(
        Symbol::qualified("rank-test", "reverse-payload"),
        vec![Nat::from(2_u64), Nat::from(1_u64), Nat::zero()],
    )
    .unwrap();
    let payload_order = rank_tree_payload_order(&codec, 2, &reversed_payload).unwrap();

    assert_ne!(
        grade_first.canonical_ordinals(),
        payload_order.canonical_ordinals()
    );

    let empty = RankTreeCodec::empty_node();
    let payload_zero = codec.node(empty.clone(), Nat::zero(), empty.clone());
    let payload_one = codec.node(empty.clone(), Nat::one(), empty);
    let zero_address = codec.rank_node(&payload_zero).unwrap();
    let one_address = codec.rank_node(&payload_one).unwrap();

    assert!(
        payload_order.position_of(&one_address).unwrap()
            < payload_order.position_of(&zero_address).unwrap()
    );

    let ordered_position = payload_order.position_of(&one_address).unwrap();
    let node_from_order = payload_order
        .unrank_node(&codec, &ordered_position)
        .unwrap();
    assert_eq!(node_from_order, payload_one);
    assert_eq!(codec.rank_node(&node_from_order).unwrap(), one_address);
}

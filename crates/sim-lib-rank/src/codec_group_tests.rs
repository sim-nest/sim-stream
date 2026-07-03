use sim_kernel::Symbol;

use crate::{
    GradeCompiler, GroupCodec, Nat, RankBuilder, RankCodec, RankGrammar, RankGroupCodec, RankNode,
    grade_of_node,
};

fn nat_pair_grammar() -> RankGrammar {
    RankBuilder::product(Symbol::qualified("rank-test", "nat-pair"))
        .field(Symbol::new("left"), RankBuilder::nat())
        .field(Symbol::new("right"), RankBuilder::nat())
        .build()
        .unwrap()
}

fn nat_list_grammar() -> RankGrammar {
    RankBuilder::list(
        Symbol::qualified("rank-test", "nat-list"),
        RankBuilder::nat(),
        0,
        None,
    )
    .unwrap()
}

fn bool_tree_grammar() -> RankGrammar {
    let tree = Symbol::qualified("rank-test", "bool-tree");
    let branch = RankBuilder::product(Symbol::new("branch"))
        .field(
            Symbol::new("left"),
            RankBuilder::recursive_ref(tree.clone()),
        )
        .field(Symbol::new("value"), RankBuilder::bool())
        .field(
            Symbol::new("right"),
            RankBuilder::recursive_ref(tree.clone()),
        )
        .build()
        .unwrap();

    RankBuilder::sum(tree)
        .alt(Symbol::new("leaf"), RankBuilder::unit())
        .alt_with_cost(Symbol::new("branch"), 1, branch)
        .build_recursive()
        .unwrap()
}

#[test]
fn product_of_two_naturals_round_trips_first_10000_ordinals() {
    let codec = RankGroupCodec::new(nat_pair_grammar());

    for ordinal in 0_u64..10_000 {
        let ordinal = Nat::from(ordinal);
        let node = codec.unrank_node(&ordinal).unwrap();
        assert_eq!(codec.rank_node(&node).unwrap(), ordinal);
    }
}

#[test]
fn list_of_naturals_round_trips_first_10000_ordinals() {
    let codec = RankGroupCodec::new(nat_list_grammar());

    for ordinal in 0_u64..10_000 {
        let ordinal = Nat::from(ordinal);
        let node = codec.unrank_node(&ordinal).unwrap();
        assert_eq!(codec.rank_node(&node).unwrap(), ordinal);
    }
}

#[test]
fn binary_tree_of_bool_round_trips_for_bounded_grades() {
    let grammar = bool_tree_grammar();
    let codec = RankGroupCodec::new(grammar.clone());
    let mut compiler = GradeCompiler::default();
    let mut offset = Nat::zero();

    for grade in 0_u64..=5 {
        let count = compiler.count_at_grade(&grammar, grade).unwrap();
        for index in 0..nat_to_u64(&count) {
            let ordinal = offset.checked_add(&Nat::from(index));
            let node = codec.unrank_node(&ordinal).unwrap();
            assert_eq!(grade_of_node(&grammar, &node).unwrap(), grade);
            assert_eq!(codec.rank_node(&node).unwrap(), ordinal);
        }
        offset = offset.checked_add(&count);
    }
}

#[test]
fn group_trait_reports_grade_counts_and_members() {
    let codec = RankGroupCodec::new(nat_pair_grammar());
    let node = RankNode::Product(vec![
        RankNode::Nat(Nat::from(2_u64)),
        RankNode::Nat(Nat::from(3_u64)),
    ]);

    assert_eq!(codec.group_of_node(&node).unwrap(), 5);
    assert_eq!(codec.group_count_at(5).unwrap(), Nat::from(6_u64));
    let rank = codec.rank_in_group(5, &node).unwrap();
    assert_eq!(codec.unrank_in_group(5, &rank).unwrap(), node);
}

fn nat_to_u64(value: &Nat) -> u64 {
    value.to_decimal_string().parse().unwrap()
}

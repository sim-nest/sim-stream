use sim_kernel::Symbol;

use crate::{
    GradeCompiler, Nat, RankBuilder, RankError, RankGrammar, RankLimits, RankNode, count_at_grade,
    grade_count_is_finite, grade_of_node,
};

fn tree_grammar() -> RankGrammar {
    let tree = Symbol::qualified("rank-test", "tree");
    let branch = RankBuilder::product(Symbol::new("branch"))
        .field(
            Symbol::new("left"),
            RankBuilder::recursive_ref(tree.clone()),
        )
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
fn grade_of_node_follows_generic_rules() {
    assert_eq!(
        grade_of_node(&RankBuilder::nat(), &RankNode::Nat(Nat::from(7_u64))).unwrap(),
        7
    );

    let list = RankBuilder::list(
        Symbol::qualified("rank-test", "nat-list"),
        RankBuilder::nat(),
        0,
        None,
    )
    .unwrap();
    assert_eq!(
        grade_of_node(
            &list,
            &RankNode::List(vec![
                RankNode::Nat(Nat::from(0_u64)),
                RankNode::Nat(Nat::from(2_u64))
            ])
        )
        .unwrap(),
        4
    );

    let pair = RankBuilder::product(Symbol::qualified("rank-test", "pair"))
        .field(Symbol::new("flag"), RankBuilder::bool())
        .field(Symbol::new("n"), RankBuilder::nat())
        .build()
        .unwrap();
    assert_eq!(
        grade_of_node(
            &pair,
            &RankNode::Product(vec![RankNode::Bool(true), RankNode::Nat(Nat::from(3_u64))])
        )
        .unwrap(),
        3
    );

    let tree = tree_grammar();
    let one_branch = RankNode::sum(
        1,
        RankNode::Product(vec![
            RankNode::sum(0, RankNode::Unit),
            RankNode::sum(0, RankNode::Unit),
        ]),
    );
    assert_eq!(grade_of_node(&tree, &one_branch).unwrap(), 1);
}

#[test]
fn grade_counts_are_finite_for_nat_list_product_and_tree() {
    assert_eq!(count_at_grade(&RankBuilder::nat(), 5).unwrap(), Nat::one());

    let list = RankBuilder::list(
        Symbol::qualified("rank-test", "nat-list"),
        RankBuilder::nat(),
        0,
        None,
    )
    .unwrap();
    assert_eq!(count_at_grade(&list, 2).unwrap(), Nat::from(2_u64));

    let pair = RankBuilder::product(Symbol::qualified("rank-test", "pair"))
        .field(Symbol::new("flag"), RankBuilder::bool())
        .field(Symbol::new("n"), RankBuilder::nat())
        .build()
        .unwrap();
    assert_eq!(count_at_grade(&pair, 2).unwrap(), Nat::from(2_u64));

    let tree = tree_grammar();
    assert!(grade_count_is_finite(&tree, 3).unwrap());
    assert_eq!(count_at_grade(&tree, 0).unwrap(), Nat::one());
    assert_eq!(count_at_grade(&tree, 1).unwrap(), Nat::one());
    assert_eq!(count_at_grade(&tree, 2).unwrap(), Nat::from(2_u64));
    assert_eq!(count_at_grade(&tree, 3).unwrap(), Nat::from(5_u64));
}

#[test]
fn recursive_grade_counts_terminate_under_limits() {
    let tree = tree_grammar();
    let mut compiler = GradeCompiler::new(RankLimits::new(10_000, 128));

    assert_eq!(
        compiler.count_at_grade(&tree, 8).unwrap(),
        Nat::from(1430_u64)
    );
    assert!(compiler.limits().remaining_fuel() < 10_000);

    let mut tiny = GradeCompiler::new(RankLimits::new(2, 128));
    assert!(matches!(
        tiny.count_at_grade(&tree, 8),
        Err(RankError::LimitExceeded {
            limit: "rank.grade.count",
            ..
        })
    ));
}

#[test]
fn grade_memoization_does_not_change_answers() {
    let tree = tree_grammar();
    let mut compiler = GradeCompiler::new(RankLimits::new(10_000, 128));

    let first = compiler.count_at_grade(&tree, 7).unwrap();
    let before = compiler.memo_stats();
    let second = compiler.count_at_grade(&tree, 7).unwrap();
    let after = compiler.memo_stats();

    assert_eq!(first, second);
    assert_eq!(first, Nat::from(429_u64));
    assert!(after.count_entries >= before.count_entries);
    assert!(after.count_hits > before.count_hits);

    let leaf = RankNode::sum(0, RankNode::Unit);
    let first_grade = compiler.grade_of_node(&tree, &leaf).unwrap();
    let grade_before = compiler.memo_stats();
    let second_grade = compiler.grade_of_node(&tree, &leaf).unwrap();
    let grade_after = compiler.memo_stats();

    assert_eq!(first_grade, second_grade);
    assert!(grade_after.grade_hits > grade_before.grade_hits);
}

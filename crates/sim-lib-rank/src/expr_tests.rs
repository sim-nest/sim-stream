use sim_kernel::{Expr, Symbol};

use crate::{
    Nat, RankCodec, RankError, RankExprCodec, RankExprNeighborhood, RankExprSpec, RankSearchScore,
    beam_search, rank_expr_lex_order, rank_expr_size_first_order,
};

fn spec() -> RankExprSpec {
    RankExprSpec::new(
        [
            Symbol::qualified("fixture", "f"),
            Symbol::qualified("fixture", "x"),
        ],
        ["literal".to_owned()],
        2,
        2,
        1,
    )
    .unwrap()
}

fn call_f(args: Vec<Expr>) -> Expr {
    Expr::Call {
        operator: Box::new(Expr::Symbol(Symbol::qualified("fixture", "f"))),
        args,
    }
}

fn sym_x() -> Expr {
    Expr::Symbol(Symbol::qualified("fixture", "x"))
}

#[test]
fn restricted_expressions_round_trip() {
    let codec = RankExprCodec::new(spec()).unwrap();
    let exprs = [
        Expr::Nil,
        Expr::Bool(true),
        sym_x(),
        Expr::String("literal".to_owned()),
        Expr::List(vec![sym_x(), Expr::Bool(false)]),
        call_f(vec![Expr::List(vec![sym_x()])]),
    ];

    for expr in exprs {
        let ordinal = codec.rank_expr(&expr).unwrap();
        assert_eq!(codec.unrank_expr(&ordinal).unwrap(), expr);
        let node = codec.expr_to_node(&expr).unwrap();
        assert_eq!(codec.expr_from_node(&node).unwrap(), expr);
        assert_eq!(codec.rank_node(&node).unwrap(), ordinal);
        assert_eq!(codec.unrank_node(&ordinal).unwrap(), node);
    }
}

#[test]
fn invalid_arity_is_rejected() {
    let codec = RankExprCodec::new(spec()).unwrap();
    let too_many_list_items = Expr::List(vec![Expr::Nil, Expr::Nil, Expr::Nil]);
    let too_many_call_args = call_f(vec![Expr::Nil, Expr::Bool(false)]);

    assert_invalid_arity(codec.rank_expr(&too_many_list_items).unwrap_err());
    assert_invalid_arity(codec.rank_expr(&too_many_call_args).unwrap_err());
}

#[test]
fn size_first_emits_smaller_expressions_earlier_than_lex() {
    let codec = RankExprCodec::new(spec()).unwrap();
    let lex = rank_expr_lex_order(&codec).unwrap();
    let size_first = rank_expr_size_first_order(&codec).unwrap();
    let atom = sym_x();
    let call = call_f(Vec::new());
    let atom_ordinal = codec.rank_expr(&atom).unwrap();
    let call_ordinal = codec.rank_expr(&call).unwrap();

    assert!(lex.position_of(&call_ordinal).unwrap() < lex.position_of(&atom_ordinal).unwrap());
    assert!(
        size_first.position_of(&atom_ordinal).unwrap()
            < size_first.position_of(&call_ordinal).unwrap()
    );

    let mut last_nodes = 0;
    for position in 0_u64..12 {
        let node = size_first
            .unrank_node(&codec, &Nat::from(position))
            .unwrap();
        let expr = codec.expr_from_node(&node).unwrap();
        let grade = crate::expr::rank_expr_grade(codec.spec(), &expr).unwrap();
        assert!(grade.nodes >= last_nodes);
        last_nodes = grade.nodes;
    }
}

#[test]
fn bounded_genetic_search_reaches_target_fixture() {
    let spec = spec();
    let codec = RankExprCodec::new(spec.clone()).unwrap();
    let neighborhood = RankExprNeighborhood::new(spec);
    let target = call_f(vec![sym_x()]);
    let target_ordinal = codec.rank_expr(&target).unwrap();
    let start = codec.rank_expr(&call_f(Vec::new())).unwrap();

    let result = beam_search(
        &neighborhood,
        &codec,
        &start,
        4,
        2,
        &mut crate::RankLimits::new(200, 64),
        |ordinal, node| target_score(&codec, &target_ordinal, ordinal, node),
    )
    .unwrap();

    assert_eq!(result.best.ordinal, target_ordinal);
    assert_eq!(result.best.score, 100);
}

fn target_score(
    codec: &RankExprCodec,
    target: &Nat,
    ordinal: &Nat,
    node: &crate::RankNode,
) -> crate::RankResult<RankSearchScore> {
    if ordinal == target {
        return Ok(100);
    }
    let expr = codec.expr_from_node(node)?;
    let grade = crate::expr::rank_expr_grade(codec.spec(), &expr)?;
    Ok(-(grade.nodes as RankSearchScore) - (grade.cost as RankSearchScore))
}

fn assert_invalid_arity(err: RankError) {
    assert!(matches!(
        err,
        RankError::InvalidNode { message } if message.contains("arity")
    ));
}

use sim_kernel::{
    ClaimPattern, DefaultFactory, Expr, NoopEvalPolicy, Ref, Step, Symbol,
    card::{card_for_ref, card_kind_predicate},
    force_list_to_vec, invoke_op,
    rank::{rank_coordinate_kind, rank_ordinal_predicate, rank_space_kind},
};
use std::sync::Arc;

use crate::{
    Nat, RankBuilder, RankNode, RankSpace, RankSpaceRegistry, coordinate_from_value,
    publish_space_claims, rank_node_from_value, rank_node_value, rank_rank_op_key,
    rank_read_capability, rank_unrank_op_key,
};

fn nat_pair_space() -> RankSpace {
    let grammar = RankBuilder::product(Symbol::qualified("rank-test", "nat-pair"))
        .field(Symbol::new("left"), RankBuilder::nat())
        .field(Symbol::new("right"), RankBuilder::nat())
        .build()
        .unwrap();
    RankSpace::group(Symbol::qualified("rank-test", "nat-pair-space"), grammar)
}

fn cx() -> sim_kernel::Cx {
    sim_kernel::Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

#[test]
fn space_registers_and_is_found_by_symbol() {
    let space = nat_pair_space();
    let id = space.symbol().clone();
    let mut registry = RankSpaceRegistry::new();

    registry.register(space).unwrap();

    assert_eq!(registry.require(&id).unwrap().symbol(), &id);
    assert_eq!(registry.len(), 1);
}

#[test]
fn rank_and_unrank_ops_round_trip_through_coordinate_ref() {
    let mut cx = cx();
    cx.grant(rank_read_capability());
    let space = nat_pair_space();
    let target = space.value(&mut cx).unwrap();
    let node = RankNode::Product(vec![
        RankNode::Nat(Nat::from(2_u64)),
        RankNode::Nat(Nat::from(3_u64)),
    ]);
    let input = rank_node_value(&mut cx, node.clone()).unwrap();

    let Step::Value(coord_value) =
        invoke_op(&mut cx, target.clone(), &rank_rank_op_key(), input).unwrap()
    else {
        panic!("rank op should return a value");
    };
    let coordinate = coordinate_from_value(&coord_value).unwrap();

    assert_eq!(&coordinate.space, space.symbol());
    assert!(matches!(coord_value.header().id, Ref::Coord(_)));

    let Step::Value(output) =
        invoke_op(&mut cx, target, &rank_unrank_op_key(), coord_value).unwrap()
    else {
        panic!("unrank op should return a value");
    };

    assert_eq!(rank_node_from_value(&output).unwrap(), node);
}

#[test]
fn rank_claims_project_to_space_and_coordinate_cards() {
    let mut cx = cx();
    cx.grant(rank_read_capability());
    let space = nat_pair_space();
    publish_space_claims(&mut cx, &space, Some("natural pair test space")).unwrap();

    let space_card = card_for_ref(&mut cx, Ref::Symbol(space.symbol().clone())).unwrap();
    let space_entries = space_card.object().as_table(&mut cx).unwrap();
    assert_symbol_field(&mut cx, &space_entries, "kind", rank_space_kind());
    assert_list_contains_symbol(
        &mut cx,
        &space_entries,
        "ops",
        Symbol::qualified("rank", "rank.v1"),
    );
    assert_list_contains_symbol(
        &mut cx,
        &space_entries,
        "ops",
        Symbol::qualified("rank", "unrank.v1"),
    );

    let target = space.value(&mut cx).unwrap();
    let input = rank_node_value(
        &mut cx,
        RankNode::Product(vec![
            RankNode::Nat(Nat::from(1_u64)),
            RankNode::Nat(Nat::from(1_u64)),
        ]),
    )
    .unwrap();
    let Step::Value(coord_value) = invoke_op(&mut cx, target, &rank_rank_op_key(), input).unwrap()
    else {
        panic!("rank op should return a value");
    };
    let coordinate = coordinate_from_value(&coord_value).unwrap();

    let coordinate_card = card_for_ref(&mut cx, Ref::Coord(coordinate.clone())).unwrap();
    let coordinate_entries = coordinate_card.object().as_table(&mut cx).unwrap();
    assert_symbol_field(&mut cx, &coordinate_entries, "kind", rank_coordinate_kind());

    let ordinal_claims = cx
        .query_facts(ClaimPattern::exact(
            Ref::Coord(coordinate.clone()),
            rank_ordinal_predicate(),
            Ref::Content(coordinate.ordinal),
        ))
        .unwrap();
    assert_eq!(ordinal_claims.len(), 1);

    let kind_claims = cx
        .query_facts(ClaimPattern {
            subject: Some(Ref::Symbol(space.symbol().clone())),
            predicate: Some(card_kind_predicate()),
            object: Some(Ref::Symbol(rank_space_kind())),
            include_revoked: false,
        })
        .unwrap();
    assert_eq!(kind_claims.len(), 1);
}

fn assert_symbol_field(
    cx: &mut sim_kernel::Cx,
    table: &sim_kernel::Value,
    field: &str,
    expected: Symbol,
) {
    let Some(entries) = table.object().as_table_impl() else {
        panic!("expected card table");
    };
    let value = entries.get(cx, Symbol::new(field)).unwrap();
    assert_eq!(value.object().as_expr(cx).unwrap(), Expr::Symbol(expected));
}

fn assert_list_contains_symbol(
    cx: &mut sim_kernel::Cx,
    table: &sim_kernel::Value,
    field: &str,
    expected: Symbol,
) {
    let Some(entries) = table.object().as_table_impl() else {
        panic!("expected card table");
    };
    let value = entries.get(cx, Symbol::new(field)).unwrap();
    let Some(list) = value.object().as_list() else {
        panic!("expected list field");
    };
    let values = force_list_to_vec(cx, list, "rank test card ops").unwrap();
    assert!(
        values
            .into_iter()
            .any(|value| value.object().as_expr(cx).unwrap() == Expr::Symbol(expected.clone())),
        "expected list field {field} to contain {expected}"
    );
}

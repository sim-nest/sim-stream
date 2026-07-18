use sim_kernel::{
    CapabilitySet, Cx, EncodeOptions, EncodePosition, Expr, ReadPolicy, Symbol, TrustLevel,
    WriteCx, force_list_to_vec, read_construct_capability,
};

use crate::{
    RankBuilder, RankLimits, RankNode, RankSpace, RankSpaceCardMetadata, coordinate_from_value,
    default_order_for_context, install_rank_lib, install_rank_space, rank_coordinate_class_symbol,
    rank_enumerate_capability, rank_enumerate_symbol, rank_fn_symbol, rank_heavy_capability,
    rank_mutate_symbol, rank_neighbor_capability, rank_node_class_symbol, rank_node_from_value,
    rank_node_value, rank_read_capability, rank_space_card, unrank_fn_symbol,
};

use sim_kernel::testing::eager_cx as cx;

fn bool_pair_space() -> RankSpace {
    RankSpace::group(
        Symbol::qualified("rank-test", "bool-pair-space"),
        RankBuilder::product(Symbol::qualified("rank-test", "bool-pair"))
            .field(Symbol::new("left"), RankBuilder::bool())
            .field(Symbol::new("right"), RankBuilder::bool())
            .build()
            .unwrap(),
    )
}

fn nat_space() -> RankSpace {
    RankSpace::group(
        Symbol::qualified("rank-test", "nat-space"),
        RankBuilder::nat(),
    )
}

#[test]
fn lisp_rank_unrank_enumerate_and_mutate_work() {
    let mut cx = cx();
    cx.grant(rank_read_capability());
    cx.grant(rank_enumerate_capability());
    cx.grant(rank_neighbor_capability());
    let space = bool_pair_space();
    let space_symbol = space.symbol().clone();
    install_rank_space(
        &mut cx,
        space,
        RankSpaceCardMetadata::default(),
        Some("bool pair space"),
    )
    .unwrap();
    let node = RankNode::Product(vec![RankNode::Bool(false), RankNode::Bool(true)]);
    let node_expr = rank_node_value(&mut cx, node.clone())
        .unwrap()
        .object()
        .as_expr(&mut cx)
        .unwrap();

    let coord = cx
        .eval_expr(call(
            rank_fn_symbol(),
            vec![Expr::Symbol(space_symbol.clone()), node_expr],
        ))
        .unwrap();
    let coord_expr = coord.object().as_expr(&mut cx).unwrap();
    assert_eq!(coordinate_from_value(&coord).unwrap().space, space_symbol);

    let unranked = cx
        .eval_expr(call(
            unrank_fn_symbol(),
            vec![Expr::Symbol(space_symbol.clone()), coord_expr],
        ))
        .unwrap();
    assert_eq!(rank_node_from_value(&unranked).unwrap(), node);

    let enumerated = cx
        .eval_expr(call(
            rank_enumerate_symbol(),
            vec![
                Expr::Symbol(space_symbol.clone()),
                Expr::Symbol(Symbol::new(":limit")),
                number(3),
            ],
        ))
        .unwrap();
    let items = force_list_to_vec(
        &mut cx,
        enumerated.object().as_list().unwrap(),
        "rank/enumerate test",
    )
    .unwrap();
    assert_eq!(items.len(), 3);

    let mutate_node_expr = rank_node_value(&mut cx, node)
        .unwrap()
        .object()
        .as_expr(&mut cx)
        .unwrap();
    let mutated = cx
        .eval_expr(call(
            rank_mutate_symbol(),
            vec![
                Expr::Symbol(space_symbol),
                mutate_node_expr,
                Expr::Symbol(Symbol::new(":seed")),
                number(7),
            ],
        ))
        .unwrap();
    assert!(matches!(
        rank_node_from_value(&mutated).unwrap(),
        RankNode::Product(_)
    ));
}

#[test]
fn denied_capabilities_fail_closed() {
    let mut cx = cx();
    let space = bool_pair_space();
    let space_symbol = space.symbol().clone();
    install_rank_space(&mut cx, space, RankSpaceCardMetadata::default(), None).unwrap();
    let node_expr = rank_node_value(
        &mut cx,
        RankNode::Product(vec![RankNode::Bool(false), RankNode::Bool(false)]),
    )
    .unwrap()
    .object()
    .as_expr(&mut cx)
    .unwrap();

    let denied = cx
        .eval_expr(call(
            rank_fn_symbol(),
            vec![Expr::Symbol(space_symbol.clone()), node_expr],
        ))
        .unwrap_err();
    assert!(matches!(
        denied,
        sim_kernel::Error::CapabilityDenied { capability }
            if capability == rank_read_capability()
    ));

    cx.grant(rank_read_capability());
    let denied = cx
        .eval_expr(call(
            rank_enumerate_symbol(),
            vec![Expr::Symbol(space_symbol.clone()), number(1)],
        ))
        .unwrap_err();
    assert!(matches!(
        denied,
        sim_kernel::Error::CapabilityDenied { capability }
            if capability == rank_enumerate_capability()
    ));

    let node_expr = rank_node_value(
        &mut cx,
        RankNode::Product(vec![RankNode::Bool(true), RankNode::Bool(false)]),
    )
    .unwrap()
    .object()
    .as_expr(&mut cx)
    .unwrap();
    let denied = cx
        .eval_expr(call(
            rank_mutate_symbol(),
            vec![Expr::Symbol(space_symbol), node_expr],
        ))
        .unwrap_err();
    assert!(matches!(
        denied,
        sim_kernel::Error::CapabilityDenied { capability }
            if capability == rank_neighbor_capability()
    ));
}

#[test]
fn rank_enumerate_large_limits_require_bounded_heavy_authority() {
    let mut cx = cx();
    cx.grant(rank_enumerate_capability());
    let space = nat_space();
    let space_symbol = space.symbol().clone();
    install_rank_space(&mut cx, space, RankSpaceCardMetadata::default(), None).unwrap();

    let ordinary_limit = u64::try_from(RankLimits::ORDINARY_ENUMERATION_LIMIT + 1).unwrap();
    let denied = cx
        .eval_expr(call(
            rank_enumerate_symbol(),
            vec![Expr::Symbol(space_symbol.clone()), number(ordinary_limit)],
        ))
        .unwrap_err();
    assert!(matches!(
        denied,
        sim_kernel::Error::CapabilityDenied { capability }
            if capability == rank_heavy_capability()
    ));

    cx.grant(rank_heavy_capability());
    let enumerated = cx
        .eval_expr(call(
            rank_enumerate_symbol(),
            vec![Expr::Symbol(space_symbol.clone()), number(ordinary_limit)],
        ))
        .unwrap();
    let items = force_list_to_vec(
        &mut cx,
        enumerated.object().as_list().unwrap(),
        "heavy rank/enumerate test",
    )
    .unwrap();
    assert_eq!(items.len(), usize::try_from(ordinary_limit).unwrap());

    let heavy_limit = u64::try_from(RankLimits::HEAVY_ENUMERATION_LIMIT + 1).unwrap();
    let denied = cx
        .eval_expr(call(
            rank_enumerate_symbol(),
            vec![Expr::Symbol(space_symbol), number(heavy_limit)],
        ))
        .unwrap_err();
    assert!(
        denied
            .to_string()
            .contains("rank limit rank.enumerate.limit exceeded")
    );
}

#[test]
fn rank_classes_and_read_constructors_expose_real_shapes() {
    let mut cx = cx();
    install_rank_lib(&mut cx).unwrap();
    let class_value = cx
        .registry()
        .class_by_symbol(&rank_node_class_symbol())
        .unwrap()
        .clone();
    let class = class_value.object().as_class().unwrap();

    let instance_shape = class.instance_shape(&mut cx).unwrap();
    let instance_shape = instance_shape.object().as_shape().unwrap();
    let node = RankNode::Product(vec![RankNode::Bool(true), RankNode::Bool(false)]);
    let node_value = rank_node_value(&mut cx, node.clone()).unwrap();
    assert!(
        instance_shape
            .check_value(&mut cx, node_value)
            .unwrap()
            .accepted
    );

    let args_expr = Expr::List(crate::lisp::rank_node_constructor_args(&node));
    let constructor_shape = class.constructor_shape(&mut cx).unwrap();
    let constructor_shape = constructor_shape.object().as_shape().unwrap();
    assert!(
        constructor_shape
            .check_expr(&mut cx, &args_expr)
            .unwrap()
            .accepted
    );

    let read_constructor = class.read_constructor(&mut cx).unwrap().unwrap();
    let read_constructor_shape = read_constructor
        .object()
        .as_read_constructor()
        .unwrap()
        .args_shape(&mut cx)
        .unwrap();
    assert!(
        read_constructor_shape
            .object()
            .as_shape()
            .unwrap()
            .check_expr(&mut cx, &args_expr)
            .unwrap()
            .accepted
    );
}

#[test]
fn rank_space_cards_include_r6_10_metadata() {
    let mut cx = cx();
    let space = bool_pair_space();
    let metadata = RankSpaceCardMetadata::default();
    let card = rank_space_card(&mut cx, &space, &metadata, Some("rank card test"))
        .unwrap()
        .object()
        .as_expr(&mut cx)
        .unwrap();

    for field in [
        "grammar",
        "codec",
        "orders",
        "metrics",
        "default-contexts",
        "grades",
        "limits",
        "tests",
        "requires",
    ] {
        assert!(table_value(&card, field).is_some(), "missing {field}");
    }
    assert_list_contains_symbol(
        table_value(&card, "orders").unwrap(),
        Symbol::qualified("rank-order", "grade-first"),
    );
    assert_list_contains_symbol(
        table_value(&card, "metrics").unwrap(),
        Symbol::qualified("rank/metric", "generic-node"),
    );
    assert_list_contains_symbol(
        table_value(&card, "requires").unwrap(),
        Symbol::qualified("capability", "rank.read"),
    );
    assert_eq!(
        default_order_for_context(&crate::RankContext::Search),
        Symbol::qualified("rank-order", "cost-first-then-grade-first")
    );
}

#[test]
fn rank_read_constructs_round_trip_through_lisp_codec() {
    let mut cx = cx();
    install_lisp_codec(&mut cx);
    install_rank_lib(&mut cx).unwrap();
    cx.grant(read_construct_capability());
    let space = bool_pair_space();
    let space_symbol = space.symbol().clone();
    install_rank_space(&mut cx, space, RankSpaceCardMetadata::default(), None).unwrap();
    let node = RankNode::Product(vec![RankNode::Bool(true), RankNode::Bool(false)]);
    let value = rank_node_value(&mut cx, node.clone()).unwrap();

    let mut write = WriteCx {
        cx: &mut cx,
        codec: sim_kernel::CodecId(1),
        options: EncodeOptions {
            position: EncodePosition::Quote,
            ..Default::default()
        },
    };
    let encoded = sim_codec_lisp::encode_object_lisp(&mut write, value).unwrap();
    assert!(encoded.starts_with("#(rank/Node "));
    let decoded = sim_codec::decode_with_codec(
        &mut cx,
        &Symbol::qualified("codec", "lisp"),
        sim_codec::Input::Text(encoded),
        read_policy_with_construct(),
    )
    .unwrap();
    let Expr::Call { operator, args } = decoded else {
        panic!("expected rank node constructor expression");
    };
    assert_eq!(*operator, Expr::Symbol(rank_node_class_symbol()));
    let reconstructed = cx
        .read_construct(
            &rank_node_class_symbol(),
            args.into_iter()
                .map(|expr| cx.factory().expr(expr).unwrap())
                .collect(),
        )
        .unwrap();
    assert_eq!(rank_node_from_value(&reconstructed).unwrap(), node);

    let coord = cx
        .read_construct(
            &rank_coordinate_class_symbol(),
            vec![
                cx.factory().symbol(space_symbol.clone()).unwrap(),
                cx.factory()
                    .number_literal(crate::bigint_number_domain(), "2".to_owned())
                    .unwrap(),
            ],
        )
        .unwrap();
    assert_eq!(coordinate_from_value(&coord).unwrap().space, space_symbol);
}

fn call(symbol: Symbol, args: Vec<Expr>) -> Expr {
    Expr::Call {
        operator: Box::new(Expr::Symbol(symbol)),
        args,
    }
}

fn number(value: u64) -> Expr {
    Expr::Number(sim_kernel::NumberLiteral {
        domain: crate::bigint_number_domain(),
        canonical: value.to_string(),
    })
}

fn install_lisp_codec(cx: &mut Cx) {
    let lib = sim_codec_lisp::LispCodecLib::new(cx.registry_mut().fresh_codec_id()).unwrap();
    cx.load_lib(&lib).unwrap();
}

fn read_policy_with_construct() -> ReadPolicy {
    ReadPolicy {
        trust: TrustLevel::TrustedSource,
        capabilities: CapabilitySet::new().grant(read_construct_capability()),
    }
}

fn table_value<'a>(expr: &'a Expr, key: &str) -> Option<&'a Expr> {
    let Expr::Map(entries) = expr else {
        return None;
    };
    entries.iter().find_map(|(entry_key, entry_value)| {
        let Expr::Symbol(entry_key) = entry_key else {
            return None;
        };
        (entry_key == &Symbol::new(key)).then_some(entry_value)
    })
}

fn assert_list_contains_symbol(expr: &Expr, expected: Symbol) {
    let Expr::List(items) = expr else {
        panic!("expected list");
    };
    assert!(
        items
            .iter()
            .any(|item| item == &Expr::Symbol(expected.clone())),
        "expected list to contain {expected}"
    );
}

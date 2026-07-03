use sim_kernel::{Datum, DatumStore, NumberLiteral, Ref, Symbol};

use crate::{
    Nat, RankBuilder, RankError, RankGrammar, RankLimits, RankNode, RankVersion,
    bigint_number_domain, binomial, coordinate_for_nat, ordinal_content_id, ordinal_datum,
};

use sim_kernel::testing::bare_cx as cx;

#[test]
fn negative_ordinals_are_impossible() {
    assert_eq!(
        Nat::try_from(-1_i64),
        Err(RankError::NegativeOrdinal {
            value: "-1".to_owned()
        })
    );
    assert_eq!(
        "-1".parse::<Nat>(),
        Err(RankError::NegativeOrdinal {
            value: "-1".to_owned()
        })
    );

    let literal = NumberLiteral {
        domain: bigint_number_domain(),
        canonical: "-1".to_owned(),
    };
    assert_eq!(
        Nat::from_number_literal(&literal),
        Err(RankError::NegativeOrdinal {
            value: "-1".to_owned()
        })
    );
}

#[test]
fn parse_print_and_number_literal_bridge_are_canonical() {
    let value: Nat = "00042".parse().unwrap();

    assert_eq!(value.to_string(), "42");
    assert_eq!(
        value.to_number_literal(),
        NumberLiteral {
            domain: bigint_number_domain(),
            canonical: "42".to_owned()
        }
    );
    assert_eq!(
        Nat::from_number_literal(&value.to_number_literal()).unwrap(),
        value
    );

    let wrong_domain = NumberLiteral {
        domain: Symbol::qualified("numbers", "i64"),
        canonical: "42".to_owned(),
    };
    assert_eq!(
        Nat::from_number_literal(&wrong_domain),
        Err(RankError::InvalidNumberDomain {
            expected: bigint_number_domain(),
            found: Symbol::qualified("numbers", "i64")
        })
    );
}

#[test]
fn ordinal_datum_content_ids_are_stable() {
    let left: Nat = "00042".parse().unwrap();
    let right = Nat::from(42_u64);
    let other = Nat::from(43_u64);

    assert_eq!(ordinal_datum(&left), ordinal_datum(&right));
    assert_eq!(
        ordinal_datum(&left),
        Datum::Number(NumberLiteral {
            domain: bigint_number_domain(),
            canonical: "42".to_owned()
        })
    );
    assert_eq!(
        ordinal_content_id(&left).unwrap(),
        ordinal_content_id(&right).unwrap()
    );
    assert_ne!(
        ordinal_content_id(&left).unwrap(),
        ordinal_content_id(&other).unwrap()
    );

    let space = Symbol::qualified("rank", "nat-test");
    let mut cx = cx();
    let left_coord = coordinate_for_nat(&mut cx, space.clone(), &left).unwrap();
    let right_coord = coordinate_for_nat(&mut cx, space, &right).unwrap();
    assert_eq!(left_coord, right_coord);

    let Ref::Coord(coord) = left_coord else {
        panic!("coordinate_for_nat must return Ref::Coord");
    };
    assert!(cx.datum_store().contains(&coord.ordinal));
    assert_eq!(
        cx.datum_store().get(&coord.ordinal).unwrap(),
        Some(&ordinal_datum(&left))
    );
}

#[test]
fn limits_return_deterministic_structured_errors() {
    let mut fuel_limits = RankLimits::new(2, 64);
    assert_eq!(
        fuel_limits.consume(3, "rank.test"),
        Err(RankError::LimitExceeded {
            limit: "rank.test",
            needed: 3,
            remaining: 2
        })
    );
    assert_eq!(fuel_limits.remaining_fuel(), 2);

    let bit_limits = RankLimits::new(10, 4);
    let wide = Nat::from(32_u64);
    assert_eq!(
        bit_limits.check_nat(&wide, "rank.bits"),
        Err(RankError::BitLimitExceeded {
            limit: "rank.bits",
            bits: 6,
            max_bits: 4
        })
    );
}

#[test]
fn pow_and_binomial_consume_fuel_and_match_values() {
    let two = Nat::from(2_u64);
    let mut pow_limits = RankLimits::new(10, 64);
    assert_eq!(
        two.pow_u32(10, &mut pow_limits).unwrap(),
        Nat::from(1024_u64)
    );
    assert_eq!(pow_limits.remaining_fuel(), 0);

    let mut binomial_limits = RankLimits::new(3, 64);
    assert_eq!(
        binomial(&Nat::from(10_u64), &Nat::from(3_u64), &mut binomial_limits).unwrap(),
        Nat::from(120_u64)
    );
    assert_eq!(binomial_limits.remaining_fuel(), 0);

    let mut tiny_limits = RankLimits::new(2, 64);
    assert_eq!(
        binomial(&Nat::from(10_u64), &Nat::from(3_u64), &mut tiny_limits),
        Err(RankError::LimitExceeded {
            limit: "rank.binomial",
            needed: 1,
            remaining: 0
        })
    );
}

#[test]
fn checked_arithmetic_and_division_are_total_over_naturals() {
    let seven = Nat::from(7_u64);
    let three = Nat::from(3_u64);

    assert_eq!(seven.checked_add(&three), Nat::from(10_u64));
    assert_eq!(seven.checked_sub(&three).unwrap(), Nat::from(4_u64));
    assert_eq!(seven.checked_mul(&three), Nat::from(21_u64));
    assert_eq!(
        seven.div_mod(&three).unwrap(),
        (Nat::from(2_u64), Nat::from(1_u64))
    );
    assert_eq!(
        three.checked_sub(&seven).unwrap_err().to_string(),
        "rank ordinal must be non-negative, found 3 - 7"
    );
    assert_eq!(seven.div_mod(&Nat::zero()), Err(RankError::DivideByZero));
}

#[test]
fn version_parse_prints_stably() {
    let version: RankVersion = "1.2.3".parse().unwrap();

    assert_eq!(version, RankVersion::new(1, 2, 3));
    assert_eq!(version.to_string(), "1.2.3");
    assert_eq!(RankVersion::v1().to_string(), "1.0.0");
    assert_eq!(
        "1.2".parse::<RankVersion>(),
        Err(RankError::InvalidVersion {
            input: "1.2".to_owned()
        })
    );
}

#[test]
fn toy_enum_grammar_builds() {
    let color = Symbol::qualified("rank-test", "color");
    let red = Symbol::new("red");
    let green = Symbol::new("green");

    let grammar = RankBuilder::enumeration(color.clone(), [red.clone(), green.clone()]).unwrap();

    assert_eq!(
        grammar,
        RankGrammar::Enum {
            id: color.clone(),
            items: vec![red, green]
        }
    );
    assert_eq!(
        RankNode::Enum {
            id: color,
            index: Nat::from(1_u64)
        },
        RankNode::Enum {
            id: Symbol::qualified("rank-test", "color"),
            index: Nat::from(1_u64)
        }
    );
}

#[test]
fn structural_builders_cover_composite_forms() {
    let nat_space = Symbol::qualified("rank-test", "nat");
    let pair_id = Symbol::qualified("rank-test", "pair");
    let list_id = Symbol::qualified("rank-test", "list");
    let set_id = Symbol::qualified("rank-test", "set");
    let map_id = Symbol::qualified("rank-test", "map");

    let pair = RankBuilder::product(pair_id.clone())
        .field(
            Symbol::new("left"),
            RankBuilder::reference(nat_space.clone()),
        )
        .field(Symbol::new("right"), RankBuilder::nat())
        .build()
        .unwrap();
    assert!(matches!(pair, RankGrammar::Product { .. }));

    let list = RankBuilder::list(list_id.clone(), pair.clone(), 1, Some(3)).unwrap();
    assert_eq!(
        list,
        RankGrammar::List {
            id: list_id,
            element: Box::new(pair.clone()),
            min_len: 1,
            max_len: Some(3)
        }
    );

    let set = RankBuilder::set(set_id.clone(), pair.clone(), Some(4)).unwrap();
    assert_eq!(
        set,
        RankGrammar::Set {
            id: set_id,
            element: Box::new(pair.clone()),
            max_len: Some(4)
        }
    );

    let map = RankBuilder::map(map_id.clone(), RankBuilder::bool(), pair, Some(2)).unwrap();
    assert!(matches!(map, RankGrammar::Map { .. }));

    assert_eq!(
        RankBuilder::list(
            Symbol::qualified("rank-test", "bad-list"),
            RankBuilder::unit(),
            4,
            Some(3)
        ),
        Err(RankError::InvalidLengthBounds {
            id: Symbol::qualified("rank-test", "bad-list"),
            min_len: 4,
            max_len: 3
        })
    );
}

#[test]
fn unproductive_recursion_is_rejected() {
    let tree = Symbol::qualified("rank-test", "tree");
    let branch_id = Symbol::new("branch");
    let bad_branch = RankBuilder::product(branch_id.clone())
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

    assert_eq!(
        RankBuilder::sum(tree.clone())
            .alt(branch_id.clone(), bad_branch)
            .build_recursive(),
        Err(RankError::UnproductiveRecursion { id: tree.clone() })
    );

    let good_branch = RankBuilder::product(branch_id.clone())
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
    let grammar = RankBuilder::sum(tree.clone())
        .alt(Symbol::new("leaf"), RankBuilder::unit())
        .alt_with_cost(branch_id, 1, good_branch)
        .build_recursive()
        .unwrap();

    assert!(matches!(grammar, RankGrammar::Sum { .. }));
    assert_eq!(
        RankBuilder::recursive_space(
            tree,
            RankBuilder::recursive_ref(Symbol::qualified("rank-test", "missing"))
        ),
        Err(RankError::UnresolvedRecursiveRef {
            id: Symbol::qualified("rank-test", "missing")
        })
    );
}

#[test]
fn grammar_summary_datums_are_stable() {
    let color = Symbol::qualified("rank-test", "color");
    let left = RankBuilder::enumeration(color.clone(), [Symbol::new("red"), Symbol::new("green")])
        .unwrap();
    let right = RankBuilder::enumeration(color.clone(), [Symbol::new("red"), Symbol::new("green")])
        .unwrap();

    assert_eq!(left.summary_datum(), right.summary_datum());
    assert_eq!(
        left.summary_content_id().unwrap(),
        right.summary_content_id().unwrap()
    );
    assert_eq!(
        left.summary_datum(),
        Datum::Node {
            tag: Symbol::qualified("rank", "grammar-enum"),
            fields: vec![
                (Symbol::new("id"), Datum::Symbol(color)),
                (
                    Symbol::new("items"),
                    Datum::List(vec![
                        Datum::Symbol(Symbol::new("red")),
                        Datum::Symbol(Symbol::new("green"))
                    ])
                )
            ]
        }
    );

    let duplicate = RankBuilder::enumeration(
        Symbol::qualified("rank-test", "dup"),
        [Symbol::new("red"), Symbol::new("red")],
    );
    assert_eq!(
        duplicate,
        Err(RankError::DuplicateGrammarSymbol {
            kind: "enum",
            id: Symbol::qualified("rank-test", "dup"),
            symbol: Symbol::new("red")
        })
    );
}

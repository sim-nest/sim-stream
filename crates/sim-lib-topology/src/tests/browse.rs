use std::collections::BTreeSet;
use std::sync::Arc;

use sim_kernel::{Cx, DefaultFactory, EagerPolicy, Expr, NumberLiteral, Symbol};

use crate::{
    browse::{
        topology_browse_symbols, topology_card_expr, topology_example_specs,
        topology_function_specs, topology_verb_specs,
    },
    install_topology_lib, parse_package, topology_run_capability,
};

const CARD_V2_FIELDS: [&str; 14] = [
    "subject",
    "kind",
    "help",
    "args",
    "result",
    "tests",
    "ops",
    "requires",
    "see-also",
    "shape-known",
    "facets",
    "coverage",
    "provenance",
    "freshness",
];

const HELP_FIELDS: [&str; 9] = [
    "subject",
    "kind",
    "summary",
    "detail",
    "exported-by",
    "stability",
    "capabilities",
    "demand",
    "see-also",
];

const TEST_FIELDS: [&str; 12] = [
    "name",
    "subjects",
    "lib",
    "mode",
    "expr",
    "expr-codec",
    "expected",
    "expected-codec",
    "expected-error",
    "codecs",
    "example",
    "capabilities",
];

const COVERAGE_FIELDS: [&str; 8] = [
    "tests", "examples", "runnable", "passed", "failed", "skipped", "last-run", "stale",
];

#[test]
fn topology_browse_cards_validate_against_card_schema() {
    let symbols = topology_browse_symbols();
    let unique = symbols.iter().cloned().collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), symbols.len());
    assert_eq!(
        symbols.len(),
        topology_function_specs().len()
            + topology_verb_specs().len()
            + topology_example_specs().len()
            + 1
    );

    for spec in topology_function_specs() {
        let card = topology_card_expr(&Symbol::qualified(
            "topology/card",
            format!("function-{}", spec.symbol),
        ));
        assert_card_schema(
            &card,
            Symbol::qualified("topology", spec.symbol),
            Symbol::qualified("core", "function"),
        );
        assert_coverage(&card, 0, 0, false);
    }

    for spec in topology_verb_specs() {
        let card = topology_card_expr(&Symbol::qualified("topology/verb", spec.name));
        assert_card_schema(
            &card,
            Symbol::qualified("topology/verb", spec.name),
            Symbol::qualified("topology", "node-verb"),
        );
        assert_coverage(&card, 0, 0, false);
    }

    let package = topology_card_expr(&Symbol::qualified("topology", "package-format"));
    assert_card_schema(
        &package,
        Symbol::qualified("topology", "package-format"),
        Symbol::qualified("topology", "package-format"),
    );
    assert_coverage(
        &package,
        topology_example_specs().len() as u64,
        topology_example_specs().len() as u64,
        true,
    );

    for spec in topology_example_specs() {
        let card = topology_card_expr(&Symbol::qualified("topology/example", spec.name));
        assert_card_schema(
            &card,
            Symbol::qualified("topology/example", spec.name),
            Symbol::qualified("core", "test"),
        );
        assert_coverage(&card, 1, 1, true);
        assert_example_test_calls_topology_test(first_test(&card), spec.package);
    }
}

#[test]
fn topology_lib_installs_public_function_and_browse_surfaces() {
    let mut cx = test_cx();
    install_topology_lib(&mut cx).expect("installed topology lib");

    for spec in topology_function_specs() {
        cx.resolve_function(&Symbol::qualified("topology", spec.symbol))
            .expect("topology function export");
    }

    for symbol in topology_browse_symbols() {
        let value = cx.resolve_value(&symbol).expect("topology browse value");
        let expr = value.object().as_expr(&mut cx).expect("browse expr");
        assert_field_order(&expr, &CARD_V2_FIELDS);
    }
}

#[test]
fn generated_examples_parse_and_embedded_tests_run() {
    let mut cx = test_cx();
    cx.grant(topology_run_capability());
    install_topology_lib(&mut cx).expect("installed topology lib");

    for spec in topology_example_specs() {
        let package = parse_package(spec.package).expect("example package parses");
        let test_fn = cx
            .resolve_function(&Symbol::qualified("topology", "test"))
            .expect("topology/test");
        let value = cx
            .call_exprs(test_fn, vec![Expr::String(spec.package.to_owned())])
            .expect("topology/test runs");
        let report = value.object().as_expr(&mut cx).expect("test report expr");

        assert_eq!(
            field(&report, "graph"),
            &Expr::Symbol(package.graph.name.clone())
        );
        assert_eq!(field(&report, "passed"), &Expr::Bool(true));
        assert_eq!(
            field(&report, "total"),
            &number_expr(package.graph.tests.len() as u64)
        );
        assert_eq!(
            field(&report, "ok"),
            &number_expr(package.graph.tests.len() as u64)
        );
        assert_eq!(field(&report, "failed"), &number_expr(0));
    }
}

fn assert_card_schema(expr: &Expr, subject: Symbol, kind: Symbol) {
    assert_field_order(expr, &CARD_V2_FIELDS);
    assert_eq!(field(expr, "subject"), &Expr::Symbol(subject));
    assert_eq!(field(expr, "kind"), &Expr::Symbol(kind));
    assert_field_order(field(expr, "help"), &HELP_FIELDS);
    assert_field_order(field(expr, "coverage"), &COVERAGE_FIELDS);
    assert_allowed_freshness(field(expr, "freshness"));

    let Expr::List(tests) = field(expr, "tests") else {
        panic!("card tests must be a list");
    };
    for test in tests {
        assert_field_order(test, &TEST_FIELDS);
    }
}

fn assert_coverage(card: &Expr, tests: u64, examples: u64, runnable: bool) {
    let coverage = field(card, "coverage");
    assert_eq!(field(coverage, "tests"), &number_expr(tests));
    assert_eq!(field(coverage, "examples"), &number_expr(examples));
    assert_eq!(field(coverage, "runnable"), &Expr::Bool(runnable));
}

fn assert_example_test_calls_topology_test(test: &Expr, package: &str) {
    assert_eq!(
        field(test, "name"),
        &Expr::Symbol(Symbol::qualified(
            "topology/example",
            example_name_from_package(package)
        ))
    );
    assert_eq!(
        field(test, "expr-codec"),
        &Expr::Symbol(Symbol::qualified("codec", "lisp"))
    );
    let Expr::Call { operator, args } = field(test, "expr") else {
        panic!("example test expression must be a call");
    };
    assert_eq!(
        operator.as_ref(),
        &Expr::Symbol(Symbol::qualified("topology", "test"))
    );
    assert_eq!(args, &[Expr::String(package.to_owned())]);
    assert_eq!(field(test, "example"), &Expr::Bool(true));
}

fn first_test(card: &Expr) -> &Expr {
    let Expr::List(tests) = field(card, "tests") else {
        panic!("card tests must be a list");
    };
    tests.first().expect("example card has test")
}

fn assert_field_order(expr: &Expr, expected: &[&str]) {
    let Expr::Map(entries) = expr else {
        panic!("expected map, got {expr:?}");
    };
    let actual = entries
        .iter()
        .map(|(key, _)| match key {
            Expr::Symbol(symbol) if symbol.namespace.is_none() => symbol.name.to_string(),
            other => panic!("expected unqualified symbol key, got {other:?}"),
        })
        .collect::<Vec<_>>();
    let expected = expected
        .iter()
        .map(|field| (*field).to_owned())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

fn field<'a>(expr: &'a Expr, name: &str) -> &'a Expr {
    let Expr::Map(entries) = expr else {
        panic!("expected map, got {expr:?}");
    };
    entries
        .iter()
        .find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == name => {
                Some(value)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing field {name} in {expr:?}"))
}

fn assert_allowed_freshness(expr: &Expr) {
    let Expr::Symbol(symbol) = expr else {
        panic!("freshness must be a symbol");
    };
    assert!(
        ["unknown", "fresh", "stale", "live"].contains(&symbol.name.as_ref()),
        "unexpected freshness symbol: {symbol}"
    );
}

fn example_name_from_package(package: &str) -> String {
    let package = parse_package(package).expect("example package parses");
    package
        .name()
        .name
        .as_ref()
        .trim_start_matches("example-")
        .to_owned()
}

fn number_expr(value: u64) -> Expr {
    Expr::Number(NumberLiteral {
        domain: Symbol::new("i64"),
        canonical: value.to_string(),
    })
}

fn test_cx() -> Cx {
    Cx::new(Arc::new(EagerPolicy), Arc::new(DefaultFactory))
}

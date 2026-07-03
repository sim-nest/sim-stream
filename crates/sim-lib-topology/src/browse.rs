//! Agent-facing topology browse, help, and example data.

use sim_kernel::{Expr, LoadCx, Result, Symbol, Value};

mod examples;
mod specs;

pub use examples::topology_example_specs;
pub use specs::{topology_function_specs, topology_verb_specs};

/// Public topology function metadata used to generate Cards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TopologyFunctionSpec {
    /// The function symbol name.
    pub symbol: &'static str,
    /// One-line summary.
    pub summary: &'static str,
    /// Longer description.
    pub detail: &'static str,
    /// Argument description.
    pub args: &'static str,
    /// Result description.
    pub result: &'static str,
}

/// Public topology node verb metadata used to generate Cards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TopologyVerbSpec {
    /// The verb name.
    pub name: &'static str,
    /// One-line summary.
    pub summary: &'static str,
    /// Longer description.
    pub detail: &'static str,
}

/// Generated topology package example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TopologyExampleSpec {
    /// The example name.
    pub name: &'static str,
    /// One-line summary.
    pub summary: &'static str,
    /// The `.simtopo` package source for the example.
    pub package: &'static str,
}

/// Returns the symbols of every browsable topology Card (functions, verbs,
/// the package format, and examples).
pub fn topology_browse_symbols() -> Vec<Symbol> {
    let function_cards = topology_function_specs()
        .into_iter()
        .map(|spec| card_symbol("function", spec.symbol));
    let verb_cards = topology_verb_specs()
        .into_iter()
        .map(|spec| Symbol::qualified("topology/verb", spec.name));
    let examples = topology_example_specs()
        .into_iter()
        .map(|spec| Symbol::qualified("topology/example", spec.name));
    function_cards
        .chain(verb_cards)
        .chain([Symbol::qualified("topology", "package-format")])
        .chain(examples)
        .collect()
}

/// Resolves a browse symbol to its Card as a runtime [`Value`].
pub fn topology_browse_value(cx: &mut LoadCx, symbol: Symbol) -> Result<Value> {
    cx.factory().expr(topology_card_expr(&symbol))
}

/// Builds the Card expression for a browse symbol (function, verb, package
/// format, or example).
pub fn topology_card_expr(symbol: &Symbol) -> Expr {
    if let Some(name) = symbol.name.as_ref().strip_prefix("function-")
        && let Some(spec) = topology_function_specs()
            .into_iter()
            .find(|spec| spec.symbol == name)
    {
        return function_card(spec);
    }
    if symbol.namespace.as_deref() == Some("topology/verb")
        && let Some(spec) = topology_verb_specs()
            .into_iter()
            .find(|spec| spec.name == symbol.name.as_ref())
    {
        return verb_card(spec);
    }
    if symbol == &Symbol::qualified("topology", "package-format") {
        return package_format_card();
    }
    if symbol.namespace.as_deref() == Some("topology/example")
        && let Some(spec) = topology_example_specs()
            .into_iter()
            .find(|spec| spec.name == symbol.name.as_ref())
    {
        return example_card(spec);
    }
    card_v2(CardV2Spec {
        subject: symbol.clone(),
        kind: Symbol::qualified("topology", "unknown"),
        summary: "unknown topology browse subject".to_owned(),
        detail: String::new(),
        args: Expr::Symbol(Symbol::qualified("core", "Any")),
        result: Expr::Symbol(Symbol::qualified("core", "Any")),
        tests: Vec::new(),
        see_also: Vec::new(),
    })
}

fn function_card(spec: TopologyFunctionSpec) -> Expr {
    card_v2(CardV2Spec {
        subject: Symbol::qualified("topology", spec.symbol),
        kind: Symbol::qualified("core", "function"),
        summary: spec.summary.to_owned(),
        detail: spec.detail.to_owned(),
        args: Expr::String(spec.args.to_owned()),
        result: Expr::String(spec.result.to_owned()),
        tests: Vec::new(),
        see_also: vec![
            Expr::Symbol(Symbol::qualified("topology", "package-format")),
            Expr::Symbol(Symbol::qualified("topology/card", "function-test")),
        ],
    })
}

fn verb_card(spec: TopologyVerbSpec) -> Expr {
    card_v2(CardV2Spec {
        subject: Symbol::qualified("topology/verb", spec.name),
        kind: Symbol::qualified("topology", "node-verb"),
        summary: spec.summary.to_owned(),
        detail: spec.detail.to_owned(),
        args: Expr::String("node input ports".to_owned()),
        result: Expr::String("node output ports".to_owned()),
        tests: Vec::new(),
        see_also: vec![Expr::Symbol(Symbol::qualified(
            "topology",
            "package-format",
        ))],
    })
}

fn package_format_card() -> Expr {
    let tests = topology_example_specs()
        .into_iter()
        .map(example_test_card)
        .collect::<Vec<_>>();
    card_v2(CardV2Spec {
        subject: Symbol::qualified("topology", "package-format"),
        kind: Symbol::qualified("topology", "package-format"),
        summary: "section-based .simtopo package format".to_owned(),
        detail: "Packages contain graph, tests, metadata, and capabilities sections. Graph and tests reuse the topology text DSL.".to_owned(),
        args: Expr::String("package source text".to_owned()),
        result: Expr::String("TopologyPackage graph, tests, metadata, and capabilities".to_owned()),
        tests,
        see_also: topology_example_specs()
            .into_iter()
            .map(|spec| Expr::Symbol(Symbol::qualified("topology/example", spec.name)))
            .collect(),
    })
}

fn example_card(spec: TopologyExampleSpec) -> Expr {
    card_v2(CardV2Spec {
        subject: Symbol::qualified("topology/example", spec.name),
        kind: Symbol::qualified("core", "test"),
        summary: spec.summary.to_owned(),
        detail: "Generated .simtopo package example that can be run with topology/test.".to_owned(),
        args: Expr::String(spec.package.to_owned()),
        result: Expr::String("topology test report".to_owned()),
        tests: vec![example_test_card(spec)],
        see_also: vec![Expr::Symbol(Symbol::qualified(
            "topology",
            "package-format",
        ))],
    })
}

struct CardV2Spec {
    subject: Symbol,
    kind: Symbol,
    summary: String,
    detail: String,
    args: Expr,
    result: Expr,
    tests: Vec<Expr>,
    see_also: Vec<Expr>,
}

fn card_v2(spec: CardV2Spec) -> Expr {
    let CardV2Spec {
        subject,
        kind,
        summary,
        detail,
        args,
        result,
        tests,
        see_also,
    } = spec;
    let coverage = coverage_expr(&tests);
    Expr::Map(vec![
        entry("subject", Expr::Symbol(subject.clone())),
        entry("kind", Expr::Symbol(kind)),
        entry(
            "help",
            help_expr(subject, summary, detail, see_also.clone()),
        ),
        entry("args", args),
        entry("result", result),
        entry("tests", Expr::List(tests)),
        entry("ops", Expr::List(Vec::new())),
        entry("requires", Expr::List(Vec::new())),
        entry("see-also", Expr::List(see_also)),
        entry("shape-known", Expr::Bool(true)),
        entry("facets", Expr::List(Vec::new())),
        entry("coverage", coverage),
        entry("provenance", Expr::List(Vec::new())),
        entry("freshness", Expr::Symbol(Symbol::new("fresh"))),
    ])
}

fn help_expr(subject: Symbol, summary: String, detail: String, see_also: Vec<Expr>) -> Expr {
    Expr::Map(vec![
        entry("subject", Expr::Symbol(subject)),
        entry("kind", Expr::Symbol(Symbol::qualified("core", "function"))),
        entry("summary", Expr::String(summary)),
        entry("detail", Expr::String(detail)),
        entry(
            "exported-by",
            Expr::Symbol(Symbol::qualified("sim", "topology")),
        ),
        entry("stability", Expr::Symbol(Symbol::new("experimental"))),
        entry("capabilities", Expr::List(Vec::new())),
        entry("demand", Expr::List(Vec::new())),
        entry("see-also", Expr::List(see_also)),
    ])
}

fn example_test_card(spec: TopologyExampleSpec) -> Expr {
    Expr::Map(vec![
        entry(
            "name",
            Expr::Symbol(Symbol::qualified("topology/example", spec.name)),
        ),
        entry(
            "subjects",
            Expr::List(vec![Expr::Symbol(Symbol::qualified(
                "topology",
                "package-format",
            ))]),
        ),
        entry("lib", Expr::Symbol(Symbol::qualified("sim", "topology"))),
        entry("mode", Expr::Symbol(Symbol::new("example"))),
        entry(
            "expr",
            Expr::Call {
                operator: Box::new(Expr::Symbol(Symbol::qualified("topology", "test"))),
                args: vec![Expr::String(spec.package.to_owned())],
            },
        ),
        entry(
            "expr-codec",
            Expr::Symbol(Symbol::qualified("codec", "lisp")),
        ),
        entry("expected", Expr::Nil),
        entry("expected-codec", Expr::Nil),
        entry("expected-error", Expr::Nil),
        entry("codecs", Expr::List(Vec::new())),
        entry("example", Expr::Bool(true)),
        entry("capabilities", Expr::List(Vec::new())),
    ])
}

fn coverage_expr(tests: &[Expr]) -> Expr {
    Expr::Map(vec![
        entry("tests", number_expr(tests.len() as u64)),
        entry("examples", number_expr(example_count(tests) as u64)),
        entry("runnable", Expr::Bool(!tests.is_empty())),
        entry("passed", Expr::Nil),
        entry("failed", Expr::Nil),
        entry("skipped", Expr::Nil),
        entry("last-run", Expr::Nil),
        entry("stale", Expr::Bool(false)),
    ])
}

fn example_count(tests: &[Expr]) -> usize {
    tests
        .iter()
        .filter(|test| map_bool_field(test, "example") == Some(true))
        .count()
}

fn map_bool_field(expr: &Expr, name: &str) -> Option<bool> {
    let Expr::Map(entries) = expr else {
        return None;
    };
    entries.iter().find_map(|(key, value)| match (key, value) {
        (Expr::Symbol(symbol), Expr::Bool(value))
            if symbol.namespace.is_none() && symbol.name.as_ref() == name =>
        {
            Some(*value)
        }
        _ => None,
    })
}

fn card_symbol(kind: &str, name: &str) -> Symbol {
    Symbol::qualified("topology/card", format!("{kind}-{name}"))
}

use sim_value::build::{entry, uint as number_expr};

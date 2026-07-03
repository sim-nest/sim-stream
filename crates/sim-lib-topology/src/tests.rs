use std::sync::Arc;

use sim_kernel::{Cx, DefaultFactory, Error, Expr, NoopEvalPolicy, NumberLiteral, Symbol, Value};

use crate::{
    Budget, BudgetExhausted, Edge, Graph, Node, PortMode, PortRef, Scheduler, SchedulerMode,
    graph_from_value, parse_graph,
};

mod browse;
mod compile;
mod control;
mod diagram;
mod instrument;
mod nonlinear;
mod package;
mod patch;
mod place;
mod reflect;
mod registry;
mod replay;
mod run_core;
mod site;
mod text;
mod validate;

#[test]
fn model_minimal_graph_uses_deterministic_defaults() {
    let graph = Graph::minimal("review-flow");

    assert_eq!(graph.name, Symbol::new("review-flow"));
    assert_eq!(graph.version, "0.1.0");
    assert_eq!(graph.api, "sim.topology.v3");
    assert!(graph.input.is_none());
    assert!(graph.output.is_none());
    assert!(graph.nodes.is_empty());
    assert!(graph.edges.is_empty());
    assert!(graph.cells.is_empty());
    assert!(graph.capabilities.is_empty());
    assert!(graph.metadata.is_empty());
    assert!(graph.tests.is_empty());
    assert_eq!(graph.scheduler, Scheduler::default());
    assert_eq!(graph.budget, Budget::default());
}

#[test]
fn model_scheduler_and_budget_defaults_are_bounded() {
    let scheduler = Scheduler::default();
    assert_eq!(scheduler.mode, SchedulerMode::Sequential);
    assert_eq!(scheduler.seed, None);
    assert_eq!(scheduler.max_concurrency, 1);
    assert!(scheduler.deterministic);

    let budget = Budget::default();
    assert_eq!(budget.max_steps, 256);
    assert_eq!(budget.max_node_visits, 64);
    assert_eq!(budget.max_edge_visits, 64);
    assert_eq!(budget.max_outputs, 64);
    assert_eq!(budget.max_child_runs, 16);
    assert_eq!(budget.deadline_ms, None);
    assert_eq!(budget.on_exhausted, BudgetExhausted::Fail);
}

#[test]
fn model_node_default_ports_follow_verbs() {
    let source = Node::named("source", "in");
    assert!(source.inputs.is_empty());
    assert_eq!(port_names(&source.outputs), ["out"]);

    let sink = Node::named("sink", "out");
    assert_eq!(port_names(&sink.inputs), ["in"]);
    assert!(sink.outputs.is_empty());

    let call = Node::named("step", "call");
    assert_eq!(port_names(&call.inputs), ["in"]);
    assert_eq!(port_names(&call.outputs), ["out", "error"]);
    assert!(call.outputs.iter().any(|port| !port.required));

    let wire = Node::named("wire", "wire");
    assert_eq!(port_names(&wire.inputs), ["in"]);
    assert_eq!(port_names(&wire.outputs), ["out"]);
    assert!(wire.inputs.iter().all(|port| port.mode == PortMode::Value));
}

#[test]
fn model_port_refs_and_edges_normalize_default_ports() {
    let from = PortRef::output("left");
    let to = PortRef::input("right");
    let edge = Edge::new(0, from.clone(), to.clone());

    assert_eq!(from.node.as_symbol(), &Symbol::new("left"));
    assert_eq!(from.port, Symbol::new("out"));
    assert_eq!(to.node.as_symbol(), &Symbol::new("right"));
    assert_eq!(to.port, Symbol::new("in"));
    assert_eq!(edge.id.0, 0);
    assert_eq!(edge.priority, 0);
    assert!(edge.when.is_none());
    assert!(edge.transform.is_none());
    assert!(edge.as_name.is_none());
    assert!(edge.max_visits.is_none());
    assert!(edge.buffer.is_none());
    assert!(edge.metadata.is_empty());
}

#[test]
fn model_cell_and_test_constructors_are_deterministic() {
    let cell = crate::Cell::new(Symbol::new("state"), Expr::Nil);
    assert_eq!(cell.name, Symbol::new("state"));
    assert!(cell.shape.is_none());
    assert!(cell.merge.is_none());
    assert!(!cell.private);

    let case = crate::GraphTest::new(Symbol::new("empty"), Expr::Nil, Expr::Bool(true));
    assert_eq!(case.name, Symbol::new("empty"));
    assert!(case.fixtures.is_empty());
}

#[test]
fn parse_minimal_map_graph() {
    let mut cx = test_cx();
    let graph = parse_expr(
        &mut cx,
        map(vec![("kind", sym("topology")), ("name", sym("map-flow"))]),
    );

    assert_eq!(graph.name, Symbol::new("map-flow"));
    assert_eq!(graph.api, "sim.topology.v3");
    assert!(graph.nodes.is_empty());
    assert!(graph.edges.is_empty());
}

#[test]
fn parse_minimal_list_graph() {
    let mut cx = test_cx();
    let graph = parse_expr(
        &mut cx,
        Expr::List(vec![
            Expr::Symbol(Symbol::qualified("topology", "graph")),
            kw("name"),
            sym("list-flow"),
            kw("nodes"),
            Expr::List(vec![
                Expr::List(vec![sym("in"), kw("verb"), sym("in")]),
                Expr::List(vec![sym("out"), kw("verb"), sym("out")]),
            ]),
            kw("edges"),
            Expr::List(vec![Expr::List(vec![sym("in"), sym("->"), sym("out")])]),
            kw("budget"),
            Expr::List(vec![kw("max-steps"), int(20)]),
        ]),
    );

    assert_eq!(graph.name, Symbol::new("list-flow"));
    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);
    assert_eq!(graph.edges[0].from.node.as_symbol(), &Symbol::new("in"));
    assert_eq!(graph.edges[0].from.port, Symbol::new("out"));
    assert_eq!(graph.edges[0].to.node.as_symbol(), &Symbol::new("out"));
    assert_eq!(graph.edges[0].to.port, Symbol::new("in"));
    assert_eq!(graph.budget.max_steps, 20);
}

#[test]
fn parse_explicit_ports_and_preserves_unknown_node_options() {
    let mut cx = test_cx();
    let target = Expr::Call {
        operator: Box::new(sym("explode-if-evaluated")),
        args: Vec::new(),
    };
    let graph = parse_expr(
        &mut cx,
        map(vec![
            ("name", sym("ports-flow")),
            (
                "nodes",
                Expr::List(vec![map(vec![
                    ("id", sym("gate")),
                    ("verb", sym("branch")),
                    (
                        "in",
                        Expr::List(vec![map(vec![
                            ("name", sym("payload")),
                            ("mode", sym("stream")),
                            ("required", Expr::Bool(false)),
                        ])]),
                    ),
                    ("out", Expr::List(vec![sym("accepted"), sym("rejected")])),
                    ("target", target.clone()),
                    ("unknown-option", Expr::String("kept".to_owned())),
                ])]),
            ),
        ]),
    );

    let node = &graph.nodes[0];
    assert_eq!(node.id.as_symbol(), &Symbol::new("gate"));
    assert_eq!(node.inputs.len(), 1);
    assert_eq!(node.inputs[0].name, Symbol::new("payload"));
    assert_eq!(node.inputs[0].mode, PortMode::Stream);
    assert!(!node.inputs[0].required);
    assert_eq!(port_names(&node.outputs), ["accepted", "rejected"]);
    assert!(node.target.as_ref().unwrap().canonical_eq(&target));
    assert_eq!(node.options[0].0, Symbol::new("unknown_option"));
    assert!(
        node.options[0]
            .1
            .canonical_eq(&Expr::String("kept".to_owned()))
    );
}

#[test]
fn parse_cells_scheduler_and_budget() {
    let mut cx = test_cx();
    let graph = parse_expr(
        &mut cx,
        map(vec![
            ("name", sym("state-flow")),
            (
                "cells",
                Expr::List(vec![map(vec![
                    ("name", sym("state")),
                    ("initial", Expr::String("empty".to_owned())),
                    ("merge", sym("last")),
                    ("private", Expr::Bool(true)),
                ])]),
            ),
            (
                "scheduler",
                map(vec![
                    ("mode", sym("sequential")),
                    ("seed", int(7)),
                    ("max_concurrency", int(1)),
                    ("deterministic", Expr::Bool(true)),
                ]),
            ),
            (
                "budget",
                Expr::List(vec![
                    kw("max-node-visits"),
                    int(9),
                    kw("max-edge-visits"),
                    int(10),
                    kw("deadline-ms"),
                    Expr::Nil,
                    kw("on-exhausted"),
                    sym("partial"),
                ]),
            ),
        ]),
    );

    assert_eq!(graph.cells.len(), 1);
    assert_eq!(graph.cells[0].name, Symbol::new("state"));
    assert_eq!(graph.cells[0].merge, Some(Symbol::new("last")));
    assert!(graph.cells[0].private);
    assert_eq!(graph.scheduler.mode, SchedulerMode::Sequential);
    assert_eq!(graph.scheduler.seed, Some(7));
    assert_eq!(graph.budget.max_node_visits, 9);
    assert_eq!(graph.budget.max_edge_visits, 10);
    assert_eq!(graph.budget.deadline_ms, None);
    assert_eq!(graph.budget.on_exhausted, BudgetExhausted::Partial);
}

#[test]
fn parse_metadata_tests_and_capabilities() {
    let mut cx = test_cx();
    let graph = parse_expr(
        &mut cx,
        map(vec![
            ("name", sym("tested-flow")),
            ("capabilities", Expr::List(vec![sym("net"), sym("file")])),
            (
                "metadata",
                map(vec![("owner", Expr::String("ops".to_owned()))]),
            ),
            (
                "tests",
                Expr::List(vec![map(vec![
                    ("name", sym("smoke")),
                    ("input", Expr::String("in".to_owned())),
                    ("expect", Expr::String("out".to_owned())),
                    ("fixtures", map(vec![("agent", sym("fake"))])),
                ])]),
            ),
        ]),
    );

    assert_eq!(
        graph.capabilities,
        [Symbol::new("net"), Symbol::new("file")]
    );
    assert_eq!(graph.metadata[0].0, Symbol::new("owner"));
    assert_eq!(graph.tests.len(), 1);
    assert_eq!(graph.tests[0].name, Symbol::new("smoke"));
    assert_eq!(graph.tests[0].fixtures[0].0, Symbol::new("agent"));
}

#[test]
fn parse_graph_from_value_alias_uses_parser() {
    let mut cx = test_cx();
    let value = value_expr(&mut cx, map(vec![("name", sym("alias-flow"))]));
    let graph = graph_from_value(&mut cx, value).expect("parsed graph");

    assert_eq!(graph.name, Symbol::new("alias-flow"));
}

#[test]
fn parse_bad_port_ref_reports_graph_field_path() {
    let mut cx = test_cx();
    let value = value_expr(
        &mut cx,
        map(vec![
            ("name", sym("bad-flow")),
            (
                "edges",
                Expr::List(vec![map(vec![
                    ("from", Expr::String("a:b:c".to_owned())),
                    ("to", sym("out")),
                ])]),
            ),
        ]),
    );

    let error = parse_graph(&mut cx, value).expect_err("bad port ref should fail");
    let Error::Eval(message) = error else {
        panic!("unexpected parse error type: {error}");
    };
    assert!(message.contains("graph.edges[0].from"));
}

fn port_names(ports: &[crate::Port]) -> Vec<&str> {
    ports
        .iter()
        .map(|port| port.name.name.as_ref())
        .collect::<Vec<_>>()
}

fn test_cx() -> Cx {
    Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
}

fn parse_expr(cx: &mut Cx, expr: Expr) -> Graph {
    let value = value_expr(cx, expr);
    parse_graph(cx, value).expect("parsed graph")
}

fn value_expr(cx: &mut Cx, expr: Expr) -> Value {
    cx.factory().expr(expr).expect("expr value")
}

fn map(entries: Vec<(&str, Expr)>) -> Expr {
    Expr::Map(
        entries
            .into_iter()
            .map(|(key, value)| (sym(key), value))
            .collect(),
    )
}

fn sym(name: &str) -> Expr {
    Expr::Symbol(Symbol::new(name))
}

fn kw(name: &str) -> Expr {
    Expr::Symbol(Symbol::new(format!(":{name}")))
}

fn int(value: i64) -> Expr {
    Expr::Number(NumberLiteral {
        domain: Symbol::new("i64"),
        canonical: value.to_string(),
    })
}

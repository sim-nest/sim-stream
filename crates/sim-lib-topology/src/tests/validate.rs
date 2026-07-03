use sim_kernel::{Error, Expr, Symbol};

use super::{sym, test_cx};
use crate::{Edge, Graph, Node, PortRef, validate::validate_graph};

#[test]
fn validate_accepts_connected_graph_without_calling_target() {
    let mut cx = test_cx();
    let mut graph = valid_call_graph();
    graph.nodes[1].target = Some(Expr::Call {
        operator: Box::new(sym("explode-if-evaluated")),
        args: Vec::new(),
    });

    validate_graph(&mut cx, &graph).expect("valid graph");
}

#[test]
fn validate_rejects_duplicate_node() {
    let mut graph = valid_call_graph();
    graph.nodes.push(Node::named("step", "wire"));

    assert_validate_error(&graph, &["validate-flow", "node step", "duplicate node"]);
}

#[test]
fn validate_rejects_unknown_edge_endpoint() {
    let mut graph = valid_call_graph();
    graph.edges[0].from = PortRef::output("missing");

    assert_validate_error(&graph, &["edge 0", "unknown output endpoint node missing"]);
}

#[test]
fn validate_rejects_missing_input() {
    let mut graph = Graph::minimal("no-input");
    graph.nodes.push(Node::named("out", "out"));

    assert_validate_error(&graph, &["no-input", "missing input"]);
}

#[test]
fn validate_rejects_missing_output() {
    let mut graph = Graph::minimal("no-output");
    graph.nodes.push(Node::named("in", "in"));

    assert_validate_error(&graph, &["no-output", "missing output"]);
}

#[test]
fn validate_rejects_unconnected_required_input_port() {
    let mut graph = valid_call_graph();
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("out")),
        Edge::new(1, PortRef::output("step"), PortRef::input("out")),
    ];

    assert_validate_error(&graph, &["node step input port in", "required input"]);
}

#[test]
fn validate_rejects_unconnected_required_output_port() {
    let mut graph = valid_call_graph();
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("step")),
        Edge::new(1, PortRef::output("in"), PortRef::input("out")),
    ];

    assert_validate_error(&graph, &["node step output port out", "required output"]);
}

#[test]
fn validate_rejects_unreachable_output() {
    let mut graph = Graph::minimal("unreachable-flow");
    let mut loop_node = Node::named("loop", "call");
    loop_node.target = Some(sym("worker"));
    graph.nodes = vec![
        Node::named("in", "in"),
        Node::named("out", "out"),
        loop_node,
        Node::named("ghost_out", "out"),
    ];

    let mut bounded_self_edge = Edge::new(1, PortRef::output("loop"), PortRef::input("loop"));
    bounded_self_edge.max_visits = Some(1);
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("out")),
        bounded_self_edge,
        Edge::new(2, PortRef::output("loop"), PortRef::input("ghost_out")),
    ];

    assert_validate_error(&graph, &["node ghost_out", "unreachable"]);
}

#[test]
fn validate_rejects_unbounded_cycle() {
    let mut graph = Graph::minimal("cycle-flow");
    let mut a = Node::named("a", "call");
    a.target = Some(sym("a-target"));
    let mut b = Node::named("b", "call");
    b.target = Some(sym("b-target"));
    graph.nodes = vec![Node::named("in", "in"), a, b, Node::named("out", "out")];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("a")),
        Edge::new(1, PortRef::output("a"), PortRef::input("b")),
        Edge::new(2, PortRef::output("b"), PortRef::input("a")),
        Edge::new(3, PortRef::output("b"), PortRef::input("out")),
    ];

    assert_validate_error(&graph, &["cycle-flow", "unbounded cycle", "a -> b -> a"]);
}

#[test]
fn validate_rejects_call_without_target() {
    let mut graph = valid_call_graph();
    graph.nodes[1].target = None;

    assert_validate_error(&graph, &["node step", "call node requires target"]);
}

#[test]
fn validate_rejects_bad_budget() {
    let mut graph = valid_call_graph();
    graph.budget.max_steps = 0;

    assert_validate_error(&graph, &["budget.max_steps", "positive"]);
}

#[test]
fn validate_rejects_bad_shape_value() {
    let mut graph = valid_call_graph();
    graph.input = Some(Expr::String("not-a-shape".to_owned()));

    assert_validate_error(&graph, &["graph.input", "invalid shape value"]);
}

#[test]
fn validate_rejects_bad_capability_symbol() {
    let mut graph = valid_call_graph();
    graph.capabilities.push(Symbol::new(":net"));

    assert_validate_error(&graph, &["capabilities", "invalid capability symbol"]);
}

fn assert_validate_error(graph: &Graph, fragments: &[&str]) {
    let mut cx = test_cx();
    let error = validate_graph(&mut cx, graph).expect_err("validation should fail");
    let Error::Eval(message) = error else {
        panic!("unexpected validation error type: {error}");
    };
    for fragment in fragments {
        assert!(
            message.contains(fragment),
            "validation error {message:?} did not contain {fragment:?}"
        );
    }
}

fn valid_call_graph() -> Graph {
    let mut graph = Graph::minimal("validate-flow");
    let mut step = Node::named("step", "call");
    step.target = Some(sym("target"));
    graph.nodes = vec![Node::named("in", "in"), step, Node::named("out", "out")];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("step")),
        Edge::new(1, PortRef::output("step"), PortRef::input("out")),
    ];
    graph
}

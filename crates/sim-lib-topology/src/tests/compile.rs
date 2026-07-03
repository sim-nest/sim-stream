use sim_kernel::{Error, Symbol};

use super::{Graph, Node, PortRef, sym, test_cx};
use crate::{Edge, EdgeId, NodeId, compile_graph};

#[test]
fn compile_builds_stable_indexes_and_edge_lists() {
    let mut cx = test_cx();
    let graph = priority_graph();

    let compiled = compile_graph(&mut cx, &graph).expect("compiled graph");

    assert_eq!(compiled.name, Symbol::new("compile-flow"));
    assert_eq!(compiled.nodes.len(), 4);
    assert_eq!(compiled.edges.len(), 4);
    assert_eq!(compiled.node_index_by_id[&NodeId::from("in")], 0);
    assert_eq!(compiled.node_index_by_id[&NodeId::from("step")], 1);
    assert_eq!(compiled.edge_index_by_id[&EdgeId::from(1)], 1);
    assert_eq!(compiled.input_nodes, [0]);
    assert_eq!(compiled.output_nodes, [3]);
    assert_eq!(compiled.incoming_edges[3], [1, 3]);
    assert_eq!(compiled.outgoing_edges[1], [2, 1]);
    assert_eq!(compiled.edges[2].priority, -2);
    assert_eq!(compiled.edges[1].priority, 5);
}

#[test]
fn compile_is_deterministic_for_same_graph_twice() {
    let mut cx = test_cx();
    let graph = priority_graph();

    let first = compile_graph(&mut cx, &graph).expect("first compile");
    let second = compile_graph(&mut cx, &graph).expect("second compile");

    assert_eq!(first, second);
}

#[test]
fn compile_rejects_invalid_graph_before_plan() {
    let mut cx = test_cx();
    let graph = Graph::minimal("invalid-compile");

    let error = compile_graph(&mut cx, &graph).expect_err("compile should validate");
    let Error::Eval(message) = error else {
        panic!("unexpected compile error type: {error}");
    };
    assert!(message.contains("topology validation error"));
    assert!(message.contains("missing input"));
}

#[test]
fn compile_records_reachability_and_cycle_metadata() {
    let mut cx = test_cx();
    let graph = bounded_cycle_graph();

    let compiled = compile_graph(&mut cx, &graph).expect("compiled graph");

    assert_eq!(compiled.reachable_from_inputs, [true, true, true, true]);
    assert_eq!(compiled.cyclic_nodes, [false, true, true, false]);
    assert_eq!(compiled.cycle_edges, [false, true, true, false]);
}

fn priority_graph() -> Graph {
    let mut graph = Graph::minimal("compile-flow");
    let mut step = Node::named("step", "call");
    step.target = Some(sym("worker"));
    let mut side = Node::named("side", "call");
    side.target = Some(sym("side-worker"));
    graph.nodes = vec![
        Node::named("in", "in"),
        step,
        side,
        Node::named("out", "out"),
    ];

    let mut high_priority = Edge::new(1, PortRef::output("step"), PortRef::input("out"));
    high_priority.priority = 5;
    let mut low_priority = Edge::new(2, PortRef::output("step"), PortRef::input("side"));
    low_priority.priority = -2;
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("step")),
        high_priority,
        low_priority,
        Edge::new(3, PortRef::output("side"), PortRef::input("out")),
    ];
    graph
}

fn bounded_cycle_graph() -> Graph {
    let mut graph = Graph::minimal("cycle-compile");
    let mut a = Node::named("a", "call");
    a.target = Some(sym("a-target"));
    let mut b = Node::named("b", "call");
    b.target = Some(sym("b-target"));
    graph.nodes = vec![Node::named("in", "in"), a, b, Node::named("out", "out")];

    let mut bounded_back_edge = Edge::new(2, PortRef::output("b"), PortRef::input("a"));
    bounded_back_edge.max_visits = Some(2);
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("a")),
        Edge::new(1, PortRef::output("a"), PortRef::input("b")),
        bounded_back_edge,
        Edge::new(3, PortRef::output("b"), PortRef::input("out")),
    ];
    graph
}

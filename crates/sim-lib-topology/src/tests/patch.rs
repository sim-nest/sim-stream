use std::sync::Arc;

use sim_codec_binary::BinaryCodecLib;
use sim_kernel::{
    Args, Cx, DefaultFactory, EagerPolicy, Error, Expr, Symbol, eval_fabric_capability,
};

use crate::{
    Edge, Graph, Node, PortRef, TopologyConnection, TopologyPatch, apply_topology_patch,
    compile_graph, install_topology_lib, run::run_graph, text::graph_to_expr,
    topology_run_capability, topology_write_capability,
};

#[test]
fn patch_adds_node_and_edges_to_clone() {
    let mut cx = runtime_cx();
    cx.grant(topology_write_capability());
    let graph = identity_graph("patch-add");
    let patch = TopologyPatch::from_expr(&Expr::List(vec![
        op("remove-edge", vec![sym("in"), sym("out")]),
        op("add-node", vec![node_expr("tap", "wire")]),
        op("add-edge", vec![edge_expr("in", "tap")]),
        op("add-edge", vec![edge_expr("tap", "out")]),
    ]))
    .expect("patch parsed");

    let patched = apply_topology_patch(&mut cx, &graph, &patch).expect("patch applied");
    let plan = compile_graph(&mut cx, &patched).expect("compiled patched graph");
    let output = run_graph(&mut cx, &patched, &plan, Expr::String("payload".to_owned()))
        .expect("ran patched graph");

    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(patched.nodes.len(), 3);
    assert_eq!(patched.edges.len(), 2);
    assert_eq!(output, Expr::String("payload".to_owned()));
}

#[test]
fn patch_replaces_node() {
    let mut cx = runtime_cx();
    cx.grant(topology_write_capability());
    let graph = wire_graph("patch-replace");
    let patch = TopologyPatch::from_expr(&op(
        "replace-node",
        vec![sym("wire"), node_expr("wire", "tee")],
    ))
    .expect("patch parsed");

    let patched = apply_topology_patch(&mut cx, &graph, &patch).expect("patch applied");

    assert_eq!(patched.nodes[1].id.as_symbol(), &Symbol::new("wire"));
    assert_eq!(patched.nodes[1].verb, Symbol::new("tee"));
}

#[test]
fn patch_rejects_invalid_result() {
    let mut cx = runtime_cx();
    cx.grant(topology_write_capability());
    let graph = identity_graph("patch-invalid");
    let patch =
        TopologyPatch::from_expr(&op("remove-node", vec![sym("out")])).expect("patch parsed");

    let error = apply_topology_patch(&mut cx, &graph, &patch).expect_err("invalid patch rejected");

    assert!(error.to_string().contains("missing output node"));
    assert_eq!(graph.nodes.len(), 2);
}

#[test]
fn rejected_patch_leaves_old_topology_connection_runnable() {
    let mut cx = runtime_cx();
    install_binary_codec(&mut cx);
    cx.grant(topology_write_capability());
    let graph = identity_graph("patch-survival");
    let connection = crate::connection_from_graph(&mut cx, &graph).expect("connection");
    let patch =
        TopologyPatch::from_expr(&op("remove-node", vec![sym("out")])).expect("patch parsed");

    let error = match crate::patched_connection(&mut cx, &graph, &patch) {
        Ok(_) => panic!("patch should have been rejected"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("missing output node"));

    let output = connection
        .request(
            &mut cx,
            Expr::String("still-live".to_owned()),
            None,
            Vec::new(),
        )
        .expect("old connection still runs")
        .object()
        .as_expr(&mut cx)
        .expect("expr output");
    assert_eq!(output, Expr::String("still-live".to_owned()));
}

#[test]
fn patch_node_proposes_data_without_applying_it() {
    let mut cx = runtime_cx();
    let proposal = op("add-node", vec![node_expr("later", "wire")]);
    let mut patch_node = Node::named("patcher", "patch");
    patch_node.options = vec![
        (Symbol::new("mode"), sym("produce")),
        (Symbol::new("patch"), proposal.clone()),
    ];
    let mut graph = Graph::minimal("patch-proposal");
    graph.nodes = vec![
        Node::named("in", "in"),
        patch_node,
        Node::named("out", "out"),
    ];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("patcher")),
        Edge::new(1, PortRef::output("patcher"), PortRef::input("out")),
    ];
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let output = run_graph(&mut cx, &graph, &plan, Expr::Nil).expect("ran graph");
    let denied = apply_topology_patch(
        &mut cx,
        &identity_graph("patch-proposal-base"),
        &TopologyPatch::from_expr(&output).expect("proposal is patch data"),
    )
    .expect_err("proposal does not apply without explicit capability");

    assert_eq!(output, proposal);
    assert_capability(denied, topology_write_capability());
}

#[test]
fn topology_patch_function_returns_new_connection() {
    let mut cx = runtime_cx();
    install_binary_codec(&mut cx);
    cx.grant(topology_write_capability());
    install_topology_lib(&mut cx).expect("installed topology lib");
    let graph = identity_graph("patch-function");
    let patch = TopologyPatch::from_expr(&Expr::List(vec![
        op("remove-edge", vec![sym("in"), sym("out")]),
        op("add-node", vec![node_expr("tap", "wire")]),
        op("add-edge", vec![edge_expr("in", "tap")]),
        op("add-edge", vec![edge_expr("tap", "out")]),
    ]))
    .expect("patch parsed");
    let patch_fn = cx
        .resolve_function(&Symbol::qualified("topology", "patch"))
        .expect("topology/patch");

    let value = cx
        .call_exprs(patch_fn, vec![graph_to_expr(&graph), patch.to_expr()])
        .expect("called topology/patch");
    let connection = value
        .object()
        .downcast_ref::<TopologyConnection>()
        .expect("patch returned connection");
    let output = connection
        .request(
            &mut cx,
            Expr::String("patched".to_owned()),
            None,
            Vec::new(),
        )
        .expect("patched connection runs")
        .object()
        .as_expr(&mut cx)
        .expect("expr output");

    assert_eq!(output, Expr::String("patched".to_owned()));
}

#[test]
fn topology_patch_function_accepts_live_connection_source() {
    let mut cx = runtime_cx();
    install_binary_codec(&mut cx);
    cx.grant(topology_write_capability());
    install_topology_lib(&mut cx).expect("installed topology lib");
    let source = crate::connection_from_graph(&mut cx, &identity_graph("patch-live"))
        .expect("source connection");
    let patch = TopologyPatch::from_expr(&op(
        "set-metadata",
        vec![sym("revision"), Expr::String("patched".to_owned())],
    ))
    .expect("patch parsed");
    let patch_fn = cx
        .resolve_function(&Symbol::qualified("topology", "patch"))
        .expect("topology/patch");
    let source = cx.factory().opaque(Arc::new(source)).expect("source value");
    let patch = cx.factory().expr(patch.to_expr()).expect("patch value");

    let value = cx
        .call_value(patch_fn, Args::new(vec![source, patch]))
        .expect("called topology/patch");

    assert!(
        value
            .object()
            .downcast_ref::<TopologyConnection>()
            .is_some()
    );
}

fn runtime_cx() -> Cx {
    let mut cx = Cx::new(Arc::new(EagerPolicy), Arc::new(DefaultFactory));
    cx.grant(eval_fabric_capability());
    cx.grant(topology_run_capability());
    cx
}

fn install_binary_codec(cx: &mut Cx) {
    let binary = BinaryCodecLib::new(cx.registry_mut().fresh_codec_id());
    cx.load_lib(&binary).unwrap();
}

fn identity_graph(name: &str) -> Graph {
    let mut graph = Graph::minimal(name);
    graph.nodes = vec![Node::named("in", "in"), Node::named("out", "out")];
    graph.edges = vec![Edge::new(0, PortRef::output("in"), PortRef::input("out"))];
    graph
}

fn wire_graph(name: &str) -> Graph {
    let mut graph = Graph::minimal(name);
    graph.nodes = vec![
        Node::named("in", "in"),
        Node::named("wire", "wire"),
        Node::named("out", "out"),
    ];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("wire")),
        Edge::new(1, PortRef::output("wire"), PortRef::input("out")),
    ];
    graph
}

fn node_expr(id: &str, verb: &str) -> Expr {
    map(vec![("id", sym(id)), ("verb", sym(verb))])
}

fn edge_expr(from: &str, to: &str) -> Expr {
    Expr::List(vec![sym(from), sym("->"), sym(to)])
}

fn op(name: &str, args: Vec<Expr>) -> Expr {
    let mut items = Vec::with_capacity(args.len() + 1);
    items.push(sym(name));
    items.extend(args);
    Expr::List(items)
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

fn assert_capability(error: Error, expected: sim_kernel::CapabilityName) {
    let Error::CapabilityDenied { capability } = error else {
        panic!("unexpected error: {error}");
    };
    assert_eq!(capability, expected);
}

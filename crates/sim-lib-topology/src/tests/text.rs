use std::sync::Arc;

use sim_codec_binary::BinaryCodecLib;
use sim_kernel::{Cx, DefaultFactory, EagerPolicy, Error, Expr, Symbol};

use crate::{
    compile_graph, parse_graph,
    text::{from_text, graph_from_text, parse_text},
    topology_run_capability,
};

#[test]
fn text_parse_returns_canonical_graph_data() {
    let graph = graph_from_text(
        r#"
# Small model friendly pipeline.
topology review-flow
node in verb=in
node draft verb=call target=writer role=worker
node out verb=out
wire in -> draft
wire draft -> out
meta owner="ops team"
test smoke input="hello world" expect=done
budget max-steps=20
"#,
    )
    .expect("parsed graph");

    assert_eq!(graph.name, Symbol::new("review-flow"));
    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.edges.len(), 2);
    assert_eq!(
        graph.nodes[1].target,
        Some(Expr::Symbol(Symbol::new("writer")))
    );
    assert_eq!(graph.nodes[1].role, Some(Symbol::new("worker")));
    assert_eq!(graph.metadata[0].0, Symbol::new("owner"));
    assert_eq!(graph.metadata[0].1, Expr::String("ops team".to_owned()));
    assert_eq!(graph.tests[0].input, Expr::String("hello world".to_owned()));
    assert_eq!(graph.budget.max_steps, 20);

    let mut cx = test_cx();
    let data = parse_text(
        r#"
topology review-flow
node in verb=in
node out verb=out
wire in -> out
"#,
    )
    .expect("canonical data");
    let value = cx.factory().expr(data).expect("expr value");
    let reparsed = parse_graph(&mut cx, value).expect("reparsed canonical data");
    assert_eq!(reparsed.name, Symbol::new("review-flow"));
    assert_eq!(reparsed.edges.len(), 1);
}

#[test]
fn text_branch_loop_wiring_compiles_with_bounded_back_edge() {
    let graph = graph_from_text(
        r#"
topology bounded-loop
node in verb=in
node gate verb=branch when=test/done
node out verb=out
wire in -> gate
wire gate:false -> gate max-visits=2
wire gate:true -> out
budget max-steps=12
"#,
    )
    .expect("parsed graph");

    assert_eq!(
        graph.nodes[1].options[0],
        (
            Symbol::new("when"),
            Expr::Symbol(Symbol::qualified("test", "done"))
        )
    );
    assert_eq!(graph.edges[1].max_visits, Some(2));

    let mut cx = test_cx();
    compile_graph(&mut cx, &graph).expect("bounded loop compiles");
}

#[test]
fn text_cells_and_budget_parse_conservative_values() {
    let graph = graph_from_text(
        r#"
topology state-flow
node in verb=in
node save verb=cell name=transcript op=append emit=cell
node out verb=out
cell transcript initial=[] merge=append private=true
wire in -> save
wire save -> out
budget max-steps=20 max-node-visits=7 deadline-ms=nil on-exhausted=partial
"#,
    )
    .expect("parsed graph");

    assert_eq!(graph.cells.len(), 1);
    assert_eq!(graph.cells[0].name, Symbol::new("transcript"));
    assert_eq!(graph.cells[0].initial, Expr::List(Vec::new()));
    assert!(graph.cells[0].private);
    assert_eq!(graph.budget.max_steps, 20);
    assert_eq!(graph.budget.max_node_visits, 7);
    assert_eq!(graph.budget.deadline_ms, None);
    assert!(matches!(
        graph.budget.on_exhausted,
        crate::BudgetExhausted::Partial
    ));

    let mut cx = test_cx();
    compile_graph(&mut cx, &graph).expect("cell graph compiles");
}

#[test]
fn text_from_text_returns_runnable_topology_connection() {
    let mut cx = runtime_cx();
    let connection = from_text(
        &mut cx,
        r#"
topology identity
node in verb=in
node out verb=out
wire in -> out
"#,
    )
    .expect("topology connection");

    assert_eq!(connection.site_kind(), "topology");
}

#[test]
fn text_invalid_wire_reports_line_and_column() {
    let error = graph_from_text(
        r#"
topology bad
node in verb=in
wire in => out
"#,
    )
    .expect_err("bad arrow should fail");

    let Error::Eval(message) = error else {
        panic!("unexpected error type: {error}");
    };
    assert!(message.contains("line 4, column 9"));
    assert!(message.contains("expected ->"));
}

fn test_cx() -> Cx {
    Cx::new(Arc::new(EagerPolicy), Arc::new(DefaultFactory))
}

fn runtime_cx() -> Cx {
    let mut cx = test_cx();
    cx.grant(topology_run_capability());
    install_binary_codec(&mut cx);
    cx
}

fn install_binary_codec(cx: &mut Cx) {
    let binary = BinaryCodecLib::new(cx.registry_mut().fresh_codec_id());
    cx.load_lib(&binary).unwrap();
}

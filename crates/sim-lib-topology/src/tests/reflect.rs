use std::sync::Arc;

use sim_kernel::{
    Args, CORE_FUNCTION_CLASS_ID, Callable, ClassRef, Cx, DefaultFactory, EagerPolicy, Expr,
    Object, Symbol, Value,
};
use sim_value::access::field;

use crate::{
    Cell, Edge, Graph, Node, PortRef, parse_graph, topology_explain, topology_reflect_capability,
    topology_reflect_graph, topology_reflect_run, topology_run_capability,
};

#[test]
fn reflect_graph_round_trips_canonical_data() {
    let mut cx = runtime_cx();
    let graph = in_out_graph("reflect-round-trip");
    let value = cx
        .factory()
        .expr(topology_reflect_graph(&cx, &graph))
        .expect("reflected graph value");

    let reparsed = parse_graph(&mut cx, value).expect("reparsed graph");

    assert_eq!(reparsed.name, Symbol::new("reflect-round-trip"));
    assert_eq!(reparsed.nodes.len(), 2);
    assert_eq!(reparsed.edges.len(), 1);
}

#[test]
fn reflect_explanation_includes_node_visits_and_edge_choices() {
    let mut cx = runtime_cx();
    let graph = in_out_graph("reflect-explain");

    let report = topology_reflect_run(&mut cx, &graph, Expr::String("seed".to_owned()))
        .expect("reflected run");
    let explanation = topology_explain(&report);

    assert_eq!(report.output, Expr::String("seed".to_owned()));
    assert!(!report.node_visits.is_empty());
    assert!(report.edge_visits.iter().any(|visit| visit.visits > 0));
    assert!(list_field(&explanation, "node-visits").is_some_and(|items| !items.is_empty()));
    assert!(list_field(&explanation, "edge-choices").is_some_and(|items| !items.is_empty()));
}

#[test]
fn reflect_redacts_private_cells_and_targets_without_capability() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "secret", "secret:");
    let graph = private_cell_call_graph();

    let redacted = topology_reflect_run(&mut cx, &graph, Expr::String("seed".to_owned()))
        .expect("redacted report");
    let call_node = redacted
        .nodes
        .iter()
        .find(|node| node.id == Symbol::new("call"))
        .expect("call node");
    assert!(call_node.redacted);
    assert_eq!(
        call_node.target,
        Some(Expr::Symbol(Symbol::qualified("topology", "redacted")))
    );
    assert!(redacted.cells[0].redacted);
    assert_eq!(
        redacted.cells[0].value,
        Expr::Symbol(Symbol::qualified("topology", "redacted"))
    );

    cx.grant(topology_reflect_capability());
    let revealed = topology_reflect_run(&mut cx, &graph, Expr::String("seed".to_owned()))
        .expect("revealed report");
    let call_node = revealed
        .nodes
        .iter()
        .find(|node| node.id == Symbol::new("call"))
        .expect("call node");
    assert!(!call_node.redacted);
    assert_eq!(
        call_node.target,
        Some(Expr::Symbol(Symbol::qualified("test", "secret")))
    );
    assert!(!revealed.cells[0].redacted);
    assert_eq!(
        revealed.cells[0].value,
        Expr::List(vec![Expr::String("secret:seed".to_owned())])
    );
}

#[test]
fn reflect_history_keeps_bounded_length() {
    let mut cx = runtime_cx();
    let graph = in_out_graph("reflect-history");
    let first = topology_reflect_run(&mut cx, &graph, Expr::String("first".to_owned()))
        .expect("first report");
    let second = topology_reflect_run(&mut cx, &graph, Expr::String("second".to_owned()))
        .expect("second report");
    let mut history = crate::TopologyHistory::new(1);

    let first_id = history.record(first);
    let second_id = history.record(second);

    assert_eq!(history.run_ids(), vec![second_id]);
    assert!(history.get(first_id).is_none());
    assert_eq!(
        history.get(second_id).expect("second retained").output,
        Expr::String("second".to_owned())
    );
}

fn runtime_cx() -> Cx {
    let mut cx = Cx::new(Arc::new(EagerPolicy), Arc::new(DefaultFactory));
    cx.grant(topology_run_capability());
    cx
}

fn in_out_graph(name: &str) -> Graph {
    let mut graph = Graph::minimal(name);
    graph.nodes = vec![Node::named("in", "in"), Node::named("out", "out")];
    graph.edges = vec![Edge::new(0, PortRef::output("in"), PortRef::input("out"))];
    graph
}

fn private_cell_call_graph() -> Graph {
    let mut graph = Graph::minimal("private-cell-call");
    let mut call = Node::named("call", "call");
    call.target = Some(Expr::Symbol(Symbol::qualified("test", "secret")));
    let mut save = Node::named("save", "cell");
    save.options = vec![
        option("name", "secret-log"),
        option("op", "append"),
        option("emit", "cell"),
    ];
    let mut cell = Cell::new(Symbol::new("secret-log"), Expr::List(Vec::new()));
    cell.private = true;
    graph.nodes = vec![
        Node::named("in", "in"),
        call,
        save,
        Node::named("out", "out"),
    ];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("call")),
        Edge::new(1, PortRef::output("call"), PortRef::input("save")),
        Edge::new(2, PortRef::output("save"), PortRef::input("out")),
    ];
    graph.cells = vec![cell];
    graph
}

fn option(key: &str, value: &str) -> (Symbol, Expr) {
    (Symbol::new(key), Expr::Symbol(Symbol::new(value)))
}

fn list_field<'a>(expr: &'a Expr, name: &str) -> Option<&'a [Expr]> {
    match field(expr, name) {
        Some(Expr::List(items)) => Some(items.as_slice()),
        _ => None,
    }
}

fn register_prefix(cx: &mut Cx, name: &str, prefix: &'static str) {
    let value = cx.factory().opaque(Arc::new(PrefixFn { prefix })).unwrap();
    cx.registry_mut()
        .register_value(Symbol::qualified("test", name), value)
        .unwrap();
}

#[derive(Clone)]
struct PrefixFn {
    prefix: &'static str,
}

impl Object for PrefixFn {
    fn display(&self, _cx: &mut Cx) -> sim_kernel::Result<String> {
        Ok("#<function test/prefix>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl sim_kernel::ObjectCompat for PrefixFn {
    fn class(&self, cx: &mut Cx) -> sim_kernel::Result<ClassRef> {
        cx.factory().class_stub(
            CORE_FUNCTION_CLASS_ID,
            Symbol::qualified("core", "Function"),
        )
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }
}

impl Callable for PrefixFn {
    fn call(&self, cx: &mut Cx, args: Args) -> sim_kernel::Result<Value> {
        let Some(first) = args.values().first() else {
            return cx.factory().string(self.prefix.to_owned());
        };
        let Expr::String(text) = first.object().as_expr(cx)? else {
            return cx.factory().string(self.prefix.to_owned());
        };
        cx.factory().string(format!("{}{text}", self.prefix))
    }
}

use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use sim_kernel::{
    Args, CORE_FUNCTION_CLASS_ID, Callable, ClassRef, Cx, DefaultFactory, EagerPolicy, Error, Expr,
    Object, Symbol, Value,
};

use crate::{
    Cell, Edge, Graph, Node, PortRef, compile_graph, run::run_graph, topology_run_capability,
};

#[test]
fn control_branch_true_routes_true_port() {
    let mut cx = runtime_cx();
    let graph = branch_graph("branch-true", "true");
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let output = run_graph(&mut cx, &graph, &plan, Expr::Bool(true)).expect("executed graph");

    assert_eq!(output, Expr::Bool(true));
}

#[test]
fn control_branch_false_routes_false_port() {
    let mut cx = runtime_cx();
    let graph = branch_graph("branch-false", "false");
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let output = run_graph(&mut cx, &graph, &plan, Expr::Bool(false)).expect("executed graph");

    assert_eq!(output, Expr::Bool(false));
}

#[test]
fn control_bounded_loop_with_max_visits_stops() {
    let mut cx = runtime_cx();
    let calls = register_counting_predicate(&mut cx, "done", 2);
    let graph = loop_graph("bounded-loop", Some(2));
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");
    let input = Expr::String("payload".to_owned());

    let output = run_graph(&mut cx, &graph, &plan, input.clone()).expect("executed graph");

    assert_eq!(output, input);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[test]
fn control_rejects_unbounded_loop_during_validation() {
    let mut cx = runtime_cx();
    let graph = loop_graph("unbounded-loop", None);

    let error = compile_graph(&mut cx, &graph).expect_err("unbounded loop should fail");

    assert_error_contains(error, &["unbounded-loop", "unbounded cycle"]);
}

#[test]
fn control_cell_append_collects_transcript() {
    let mut cx = runtime_cx();
    let graph = cell_append_graph();
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let output = run_graph(&mut cx, &graph, &plan, Expr::String("line one".to_owned()))
        .expect("executed graph");

    assert_eq!(
        output,
        Expr::List(vec![Expr::String("line one".to_owned())])
    );
}

#[test]
fn control_budget_exhaustion_returns_topology_error() {
    let mut cx = runtime_cx();
    let mut graph = Graph::minimal("budget-flow");
    graph.nodes = vec![Node::named("in", "in"), Node::named("out", "out")];
    graph.edges = vec![Edge::new(0, PortRef::output("in"), PortRef::input("out"))];
    graph.budget.max_steps = 1;
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let error =
        run_graph(&mut cx, &graph, &plan, Expr::Nil).expect_err("budget exhaustion should fail");

    assert_error_contains(
        error,
        &[
            "topology budget exhausted",
            "resource=max-steps",
            "limit=1",
            "actual=2",
            "topology-budget-exhausted",
        ],
    );
}

fn runtime_cx() -> Cx {
    let mut cx = Cx::new(Arc::new(EagerPolicy), Arc::new(DefaultFactory));
    cx.grant(topology_run_capability());
    cx
}

fn branch_graph(name: &str, selected_port: &str) -> Graph {
    let mut graph = Graph::minimal(name);
    graph.nodes = vec![
        Node::named("in", "in"),
        Node::named("gate", "branch"),
        Node::named("out", "out"),
    ];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("gate")),
        Edge::new(
            1,
            PortRef::named("gate", selected_port),
            PortRef::input("out"),
        ),
    ];
    graph
}

fn loop_graph(name: &str, max_visits: Option<u32>) -> Graph {
    let mut graph = Graph::minimal(name);
    let mut gate = Node::named("gate", "branch");
    gate.options.push((
        Symbol::new("when"),
        Expr::Symbol(Symbol::qualified("test", "done")),
    ));
    graph.nodes = vec![Node::named("in", "in"), gate, Node::named("out", "out")];
    let mut back_edge = Edge::new(1, PortRef::named("gate", "false"), PortRef::input("gate"));
    back_edge.max_visits = max_visits;
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("gate")),
        back_edge,
        Edge::new(2, PortRef::named("gate", "true"), PortRef::input("out")),
    ];
    graph
}

fn cell_append_graph() -> Graph {
    let mut graph = Graph::minimal("cell-append");
    let mut save = Node::named("save", "cell");
    save.options = vec![
        option("name", "transcript"),
        option("op", "append"),
        option("emit", "cell"),
    ];
    graph.nodes = vec![Node::named("in", "in"), save, Node::named("out", "out")];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("save")),
        Edge::new(1, PortRef::output("save"), PortRef::input("out")),
    ];
    graph.cells = vec![Cell::new(Symbol::new("transcript"), Expr::List(Vec::new()))];
    graph
}

fn option(key: &str, value: &str) -> (Symbol, Expr) {
    (Symbol::new(key), Expr::Symbol(Symbol::new(value)))
}

fn register_counting_predicate(cx: &mut Cx, name: &str, true_after: u32) -> Arc<AtomicU32> {
    let calls = Arc::new(AtomicU32::new(0));
    let value = cx
        .factory()
        .opaque(Arc::new(CountingPredicate {
            calls: calls.clone(),
            true_after,
        }))
        .expect("predicate value");
    cx.registry_mut()
        .register_value(Symbol::qualified("test", name), value)
        .expect("registered predicate");
    calls
}

#[derive(Clone)]
struct CountingPredicate {
    calls: Arc<AtomicU32>,
    true_after: u32,
}

impl Object for CountingPredicate {
    fn display(&self, _cx: &mut Cx) -> sim_kernel::Result<String> {
        Ok("#<function test/done>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl sim_kernel::ObjectCompat for CountingPredicate {
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

impl Callable for CountingPredicate {
    fn call(&self, cx: &mut Cx, _args: Args) -> sim_kernel::Result<Value> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        cx.factory().bool(call > self.true_after)
    }
}

fn assert_error_contains(error: Error, fragments: &[&str]) {
    let Error::Eval(message) = error else {
        panic!("unexpected error type: {error}");
    };
    for fragment in fragments {
        assert!(
            message.contains(fragment),
            "error {message:?} did not contain {fragment:?}"
        );
    }
}

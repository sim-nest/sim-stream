use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use sim_kernel::{
    Args, CORE_FUNCTION_CLASS_ID, Callable, ClassRef, Cx, DefaultFactory, EagerPolicy, Expr,
    Object, Symbol, Value,
};

use crate::{
    Edge, EdgeId, Graph, Node, PortRef, TopologyCounterfactual, counterfactual_replay,
    replay_report, run::TopologyEventKind, topology_reflect_run, topology_run_capability,
};

#[test]
fn replay_report_repeats_output_without_calling_target() {
    let mut cx = runtime_cx();
    let calls = register_counting_prefix(&mut cx, "count", "count:");
    let graph = call_graph("replay-count", "count");

    let report = topology_reflect_run(&mut cx, &graph, Expr::String("seed".to_owned()))
        .expect("reflected run");
    let replayed = replay_report(&report).expect("replayed report");

    assert_eq!(report.output, Expr::String("count:seed".to_owned()));
    assert_eq!(replayed, report.output);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(report.recorded_replies.len(), 1);
}

#[test]
fn replay_report_rejects_tampered_output_evidence() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "count", "count:");
    let graph = call_graph("replay-output-tamper", "count");
    let mut report = topology_reflect_run(&mut cx, &graph, Expr::String("seed".to_owned()))
        .expect("reflected run");
    report.output = Expr::String("tampered".to_owned());

    let error = replay_report(&report).expect_err("tampered output should fail");

    assert_replay_error_contains(error, "output does not match output events");
}

#[test]
fn replay_report_rejects_tampered_edge_evidence() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "count", "count:");
    let graph = call_graph("replay-edge-tamper", "count");
    let mut report = topology_reflect_run(&mut cx, &graph, Expr::String("seed".to_owned()))
        .expect("reflected run");
    let event = report
        .events
        .iter_mut()
        .find(|event| event.kind == TopologyEventKind::EdgeRouted)
        .expect("edge event");
    event.edge_index = Some(99);

    let error = replay_report(&report).expect_err("tampered edge should fail");

    assert_replay_error_contains(error, "unknown edge 99");
}

#[test]
fn replay_counterfactual_replace_target_changes_output() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "a", "a:");
    register_prefix(&mut cx, "b", "b:");
    let graph = call_graph("replace-target", "a");
    let report = topology_reflect_run(&mut cx, &graph, Expr::String("seed".to_owned()))
        .expect("reflected run");

    let changed = counterfactual_replay(
        &mut cx,
        &report,
        TopologyCounterfactual::ReplaceTarget {
            node: Symbol::new("call"),
            target: Expr::Symbol(Symbol::qualified("test", "b")),
        },
    )
    .expect("counterfactual replay");

    assert_eq!(report.output, Expr::String("a:seed".to_owned()));
    assert_eq!(changed, Expr::String("b:seed".to_owned()));
}

#[test]
fn replay_counterfactual_disable_edge_changes_output() {
    let mut cx = runtime_cx();
    let graph = branch_graph("disable-edge", true, true);
    let report = topology_reflect_run(&mut cx, &graph, Expr::Bool(false)).expect("reflected run");

    let changed = counterfactual_replay(
        &mut cx,
        &report,
        TopologyCounterfactual::DisableEdge { edge: EdgeId(2) },
    )
    .expect("counterfactual replay");

    assert_eq!(report.output, Expr::Bool(false));
    assert_eq!(changed, Expr::Nil);
}

#[test]
fn replay_counterfactual_force_predicate_changes_output() {
    let mut cx = runtime_cx();
    let graph = branch_graph("force-predicate", false, false);
    let report = topology_reflect_run(&mut cx, &graph, Expr::String("seed".to_owned()))
        .expect("reflected run");

    let changed = counterfactual_replay(
        &mut cx,
        &report,
        TopologyCounterfactual::ForcePredicate {
            node: Symbol::new("gate"),
            result: true,
        },
    )
    .expect("counterfactual replay");

    assert_eq!(report.output, Expr::Nil);
    assert_eq!(changed, Expr::String("seed".to_owned()));
}

fn runtime_cx() -> Cx {
    let mut cx = Cx::new(Arc::new(EagerPolicy), Arc::new(DefaultFactory));
    cx.grant(topology_run_capability());
    cx
}

fn call_graph(name: &str, target: &str) -> Graph {
    let mut graph = Graph::minimal(name);
    let mut call = Node::named("call", "call");
    call.target = Some(Expr::Symbol(Symbol::qualified("test", target)));
    graph.nodes = vec![Node::named("in", "in"), call, Node::named("out", "out")];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("call")),
        Edge::new(1, PortRef::output("call"), PortRef::input("out")),
    ];
    graph
}

fn branch_graph(name: &str, false_to_out: bool, predicate: bool) -> Graph {
    let mut graph = Graph::minimal(name);
    let mut gate = Node::named("gate", "branch");
    gate.options
        .push((Symbol::new("when"), Expr::Bool(predicate)));
    graph.nodes = vec![Node::named("in", "in"), gate, Node::named("out", "out")];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("gate")),
        Edge::new(1, PortRef::named("gate", "true"), PortRef::input("out")),
    ];
    if false_to_out {
        graph.edges.push(Edge::new(
            2,
            PortRef::named("gate", "false"),
            PortRef::input("out"),
        ));
    }
    graph
}

fn register_prefix(cx: &mut Cx, name: &str, prefix: &'static str) {
    let value = cx.factory().opaque(Arc::new(PrefixFn { prefix })).unwrap();
    cx.registry_mut()
        .register_value(Symbol::qualified("test", name), value)
        .unwrap();
}

fn register_counting_prefix(cx: &mut Cx, name: &str, prefix: &'static str) -> Arc<AtomicU32> {
    let calls = Arc::new(AtomicU32::new(0));
    let value = cx
        .factory()
        .opaque(Arc::new(CountingPrefixFn {
            prefix,
            calls: calls.clone(),
        }))
        .unwrap();
    cx.registry_mut()
        .register_value(Symbol::qualified("test", name), value)
        .unwrap();
    calls
}

fn assert_replay_error_contains(error: sim_kernel::Error, expected: &str) {
    let sim_kernel::Error::Eval(message) = error else {
        panic!("unexpected replay error: {error}");
    };
    assert!(
        message.contains(expected),
        "expected {message:?} to contain {expected:?}"
    );
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
        prefix_call(cx, self.prefix, args)
    }
}

#[derive(Clone)]
struct CountingPrefixFn {
    prefix: &'static str,
    calls: Arc<AtomicU32>,
}

impl Object for CountingPrefixFn {
    fn display(&self, _cx: &mut Cx) -> sim_kernel::Result<String> {
        Ok("#<function test/counting-prefix>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl sim_kernel::ObjectCompat for CountingPrefixFn {
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

impl Callable for CountingPrefixFn {
    fn call(&self, cx: &mut Cx, args: Args) -> sim_kernel::Result<Value> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        prefix_call(cx, self.prefix, args)
    }
}

fn prefix_call(cx: &mut Cx, prefix: &str, args: Args) -> sim_kernel::Result<Value> {
    let Some(first) = args.values().first() else {
        return cx.factory().string(prefix.to_owned());
    };
    let Expr::String(text) = first.object().as_expr(cx)? else {
        return cx.factory().string(prefix.to_owned());
    };
    cx.factory().string(format!("{prefix}{text}"))
}

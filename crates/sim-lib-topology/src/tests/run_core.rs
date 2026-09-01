use std::sync::Arc;

use sim_codec_binary::BinaryCodecLib;
use sim_kernel::{
    Args, CORE_FUNCTION_CLASS_ID, Callable, ClassRef, Cx, DefaultFactory, EagerPolicy, Error, Expr,
    Object, Symbol, Value,
};
use sim_shape::{ExprKind, ExprKindShape, shape_value};
use sim_value::access::field;

use crate::{
    BudgetExhausted, Edge, Graph, Node, PortRef, TopologyAdapterRegistry, compile_graph,
    connection_from_graph,
    run::{TopologyEventKind, TopologyRun, run_graph},
    topology_run_capability,
};

#[test]
fn continuation_step_resume_is_identical_to_uninterrupted_run() {
    let mut cx = runtime_cx();
    let graph = in_out_graph("resume-linear");
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");
    let input = Expr::String("seed".to_owned());

    let mut uninterrupted = TopologyRun::new(&graph, &plan, input.clone()).expect("run");
    uninterrupted.run(&mut cx).expect("uninterrupted");

    let mut stepped = TopologyRun::new(&graph, &plan, input).expect("run");
    stepped.step(&mut cx).expect("one item");
    let encoded = stepped.continuation().to_expr();
    let decoded = crate::TopologyContinuation::from_expr(&encoded).expect("decoded continuation");
    assert_eq!(decoded.to_expr(), encoded);
    let mut resumed = TopologyRun::resume(&graph, &plan, decoded, crate::TopologyBindings::new())
        .expect("resumed");
    resumed.run(&mut cx).expect("completed resume");

    assert_eq!(
        resumed.continuation().to_expr(),
        uninterrupted.continuation().to_expr()
    );
}

#[test]
fn continuation_rejects_changed_graph_before_running() {
    let mut cx = runtime_cx();
    let graph = in_out_graph("resume-source");
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");
    let continuation = TopologyRun::new(&graph, &plan, Expr::Nil)
        .expect("run")
        .continuation();
    let changed = in_out_graph("changed-source");
    let changed_plan = compile_graph(&mut cx, &changed).expect("changed plan");

    let error = TopologyRun::resume(
        &changed,
        &changed_plan,
        continuation,
        crate::TopologyBindings::new(),
    )
    .err()
    .expect("fingerprint rejected");
    assert!(error.to_string().contains("fingerprint mismatch"));
}

#[test]
fn runtime_binding_precedes_ambient_target_and_is_required_on_resume() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "ambient", "ambient:");
    register_prefix(&mut cx, "bound", "bound:");
    let graph = call_graph(
        "bound-call",
        Expr::Symbol(Symbol::qualified("test", "ambient")),
    );
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");
    let value =
        crate::adapter::resolve_target(&mut cx, &Expr::Symbol(Symbol::qualified("test", "bound")))
            .expect("bound value");
    let mut bindings = crate::TopologyBindings::new();
    bindings.bind(
        "call",
        crate::TopologyBindingDescriptor::for_node("bound-v1", &graph.nodes[1]),
        value,
    );
    let mut run = TopologyRun::new(&graph, &plan, Expr::String("seed".into())).expect("run");
    run.set_bindings(bindings);
    run.step(&mut cx).expect("input step");
    let continuation = run.continuation();
    assert!(
        TopologyRun::resume(
            &graph,
            &plan,
            continuation.clone(),
            crate::TopologyBindings::new()
        )
        .is_err()
    );

    let value =
        crate::adapter::resolve_target(&mut cx, &Expr::Symbol(Symbol::qualified("test", "bound")))
            .expect("bound value");
    let mut resumed_bindings = crate::TopologyBindings::new();
    resumed_bindings.bind(
        "call",
        crate::TopologyBindingDescriptor::for_node("bound-v1", &graph.nodes[1]),
        value,
    );
    let mut resumed =
        TopologyRun::resume(&graph, &plan, continuation, resumed_bindings).expect("resume");
    resumed.run(&mut cx).expect("bound run");
    assert_eq!(resumed.output_expr(), Expr::String("bound:seed".into()));
}

#[test]
fn continuation_rejects_malformed_queue_and_widened_counter() {
    let mut cx = runtime_cx();
    let graph = in_out_graph("invalid-resume");
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");
    let encoded = TopologyRun::new(&graph, &plan, Expr::Nil)
        .expect("run")
        .continuation()
        .to_expr();

    let mut bad_queue = encoded.clone();
    let Expr::Map(root) = &mut bad_queue else {
        unreachable!()
    };
    let Expr::List(queue) = map_value_mut(root, "queue") else {
        unreachable!()
    };
    let Expr::List(first) = &mut queue[0] else {
        unreachable!()
    };
    first[0] = Expr::String("999".into());
    let decoded = crate::TopologyContinuation::from_expr(&bad_queue).expect("structurally decoded");
    assert!(TopologyRun::resume(&graph, &plan, decoded, crate::TopologyBindings::new()).is_err());

    let mut widened = encoded;
    let Expr::Map(root) = &mut widened else {
        unreachable!()
    };
    let Expr::Map(budget) = map_value_mut(root, "budget") else {
        unreachable!()
    };
    *map_value_mut(budget, "steps") = Expr::String("999999".into());
    let decoded = crate::TopologyContinuation::from_expr(&widened).expect("structurally decoded");
    assert!(TopologyRun::resume(&graph, &plan, decoded, crate::TopologyBindings::new()).is_err());
}

fn map_value_mut<'a>(entries: &'a mut [(Expr, Expr)], name: &str) -> &'a mut Expr {
    entries
        .iter_mut()
        .find_map(|(key, value)| {
            matches!(key, Expr::Symbol(symbol) if symbol.name.as_ref() == name).then_some(value)
        })
        .expect("map field")
}

#[test]
fn run_core_in_to_out_returns_input_and_events() {
    let mut cx = runtime_cx();
    let graph = in_out_graph("core-in-out");
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");
    let input = Expr::String("seed".to_owned());
    let mut run = TopologyRun::new(&graph, &plan, input.clone()).expect("run");

    run.run(&mut cx).expect("executed graph");

    assert_eq!(run.output_expr(), input);
    assert_eq!(run.outputs(), &[Expr::String("seed".to_owned())]);
    assert!(
        run.events()
            .iter()
            .any(|event| event.kind == TopologyEventKind::OutputEmitted)
    );
}

#[test]
fn run_core_call_invokes_callable_target() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "prefix", "called:");
    let graph = call_graph(
        "callable-flow",
        Expr::Symbol(Symbol::qualified("test", "prefix")),
    );
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let output =
        run_graph(&mut cx, &graph, &plan, Expr::String("seed".to_owned())).expect("executed graph");

    assert_eq!(output, Expr::String("called:seed".to_owned()));
}

#[test]
fn run_core_call_supports_nested_topology_connection() {
    let mut cx = runtime_cx();
    install_binary_codec(&mut cx);
    let child = in_out_graph("child-flow");
    let child = connection_from_graph(&mut cx, &child).expect("child connection");
    let child = cx.factory().opaque(Arc::new(child)).unwrap();
    cx.registry_mut()
        .register_value(Symbol::qualified("test", "child"), child)
        .unwrap();

    let graph = call_graph(
        "parent-flow",
        Expr::Symbol(Symbol::qualified("test", "child")),
    );
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");
    let output = run_graph(&mut cx, &graph, &plan, Expr::String("nested".to_owned()))
        .expect("executed graph");

    assert_eq!(output, Expr::String("nested".to_owned()));
}

#[test]
fn adapter_registry_uses_deterministic_core_order() {
    let registry = TopologyAdapterRegistry::core();

    assert_eq!(
        registry.names(),
        vec![
            Symbol::qualified("topology/adapter", "topology"),
            Symbol::qualified("topology/adapter", "shape"),
            Symbol::qualified("topology/adapter", "codec"),
            Symbol::qualified("topology/adapter", "table"),
            Symbol::qualified("topology/adapter", "list"),
            Symbol::qualified("topology/adapter", "stream"),
            Symbol::qualified("topology/adapter", "fabric"),
            Symbol::qualified("topology/adapter", "callable"),
        ]
    );
}

#[test]
fn non_agent_graph_can_use_shape_codec_and_table_adapters() {
    let mut cx = runtime_cx();
    install_binary_codec(&mut cx);
    register_string_shape(&mut cx);
    register_table(&mut cx);
    let graph = shape_codec_table_graph();
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let output = run_graph(&mut cx, &graph, &plan, Expr::String("payload".to_owned()))
        .expect("executed graph");

    assert_eq!(field(&output, "seen"), Some(&Expr::String("ok".to_owned())));
}

#[test]
fn run_core_wire_edge_transform_changes_value() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "transform", "tx:");
    let mut graph = Graph::minimal("wire-transform");
    graph.nodes = vec![
        Node::named("in", "in"),
        Node::named("wire", "wire"),
        Node::named("out", "out"),
    ];
    let mut transformed = Edge::new(1, PortRef::output("wire"), PortRef::input("out"));
    transformed.transform = Some(Expr::Symbol(Symbol::qualified("test", "transform")));
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("wire")),
        transformed,
    ];
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let output =
        run_graph(&mut cx, &graph, &plan, Expr::String("seed".to_owned())).expect("executed graph");

    assert_eq!(output, Expr::String("tx:seed".to_owned()));
}

#[test]
fn run_core_edge_as_wraps_value() {
    let mut cx = runtime_cx();
    let mut graph = Graph::minimal("edge-wrap");
    graph.nodes = vec![Node::named("in", "in"), Node::named("out", "out")];
    let mut edge = Edge::new(0, PortRef::output("in"), PortRef::input("out"));
    edge.as_name = Some(Symbol::new("payload"));
    graph.edges = vec![edge];
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let output =
        run_graph(&mut cx, &graph, &plan, Expr::String("seed".to_owned())).expect("executed graph");

    assert_eq!(
        output,
        Expr::Map(vec![(
            Expr::Symbol(Symbol::new("payload")),
            Expr::String("seed".to_owned())
        )])
    );
}

#[test]
fn run_core_rejects_wrong_graph_input_shape() {
    let mut cx = runtime_cx();
    let mut graph = in_out_graph("wrong-input-shape");
    graph.input = Some(bool_shape());
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let error = run_graph(&mut cx, &graph, &plan, Expr::String("seed".to_owned()))
        .expect_err("input shape should reject string");

    assert_eval_error_contains(error, "graph input");
}

#[test]
fn run_core_rejects_wrong_node_output_shape() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "prefix", "called:");
    let mut graph = call_graph(
        "wrong-node-output-shape",
        Expr::Symbol(Symbol::qualified("test", "prefix")),
    );
    graph.nodes[1].output = Some(bool_shape());
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let error = run_graph(&mut cx, &graph, &plan, Expr::String("seed".to_owned()))
        .expect_err("node output shape should reject string");

    assert_eval_error_contains(error, "node call output");
}

#[test]
fn run_core_rejects_wrong_edge_delivered_port_shape() {
    let mut cx = runtime_cx();
    let mut graph = in_out_graph("wrong-edge-port-shape");
    graph.nodes[1].inputs[0].shape = Some(bool_shape());
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let error = run_graph(&mut cx, &graph, &plan, Expr::String("seed".to_owned()))
        .expect_err("edge input port shape should reject string");

    assert_eval_error_contains(error, "edge into node out input port in");
}

#[test]
fn run_core_rejects_wrong_final_result_shape() {
    let mut cx = runtime_cx();
    let mut graph = in_out_graph("wrong-output-shape");
    graph.output = Some(bool_shape());
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let error = run_graph(&mut cx, &graph, &plan, Expr::String("seed".to_owned()))
        .expect_err("graph output shape should reject string");

    assert_eval_error_contains(error, "graph output");
}

#[test]
fn run_core_rejects_partial_exhaustion_policy() {
    let mut cx = runtime_cx();
    let mut graph = in_out_graph("partial-budget");
    graph.budget.on_exhausted = BudgetExhausted::Partial;
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let error = match TopologyRun::new(&graph, &plan, Expr::String("seed".to_owned())) {
        Ok(_) => panic!("partial exhaustion policy should be rejected"),
        Err(error) => error,
    };

    assert_eval_error_contains(error, "partial exhaustion policy");
}

fn runtime_cx() -> Cx {
    let mut cx = Cx::new(
        Arc::new(EagerPolicy),
        Arc::new(DefaultFactory),
        sim_kernel::HandleSeed::new(1),
    );
    cx.grant(topology_run_capability());
    cx
}

fn install_binary_codec(cx: &mut Cx) {
    let binary = BinaryCodecLib::new(cx.registry_mut().fresh_codec_id());
    cx.load_lib(&binary).unwrap();
}

fn register_string_shape(cx: &mut Cx) {
    let symbol = Symbol::qualified("test", "string-shape");
    let value = shape_value(
        symbol.clone(),
        Arc::new(ExprKindShape::new(ExprKind::String)),
    );
    cx.registry_mut()
        .register_shape_value(symbol, value)
        .unwrap();
}

fn register_table(cx: &mut Cx) {
    let value = cx.factory().string("ok".to_owned()).unwrap();
    let table = cx
        .factory()
        .table(vec![(Symbol::new("seen"), value)])
        .unwrap();
    cx.registry_mut()
        .register_value(Symbol::qualified("test", "table"), table)
        .unwrap();
}

fn in_out_graph(name: &str) -> Graph {
    let mut graph = Graph::minimal(name);
    graph.nodes = vec![Node::named("in", "in"), Node::named("out", "out")];
    graph.edges = vec![Edge::new(0, PortRef::output("in"), PortRef::input("out"))];
    graph
}

fn bool_shape() -> Expr {
    Expr::Symbol(Symbol::new("Bool"))
}

fn assert_eval_error_contains(error: Error, expected: &str) {
    let Error::Eval(message) = error else {
        panic!("unexpected error: {error}");
    };
    assert!(
        message.contains(expected),
        "expected {message:?} to contain {expected:?}"
    );
}

fn call_graph(name: &str, target: Expr) -> Graph {
    let mut graph = Graph::minimal(name);
    let mut call = Node::named("call", "call");
    call.target = Some(target);
    graph.nodes = vec![Node::named("in", "in"), call, Node::named("out", "out")];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("call")),
        Edge::new(1, PortRef::output("call"), PortRef::input("out")),
    ];
    graph
}

fn shape_codec_table_graph() -> Graph {
    let mut graph = Graph::minimal("shape-codec-table");
    let mut shape = Node::named("shape", "call");
    shape.target = Some(Expr::Symbol(Symbol::qualified("test", "string-shape")));
    let mut codec = Node::named("codec", "call");
    codec.target = Some(Expr::Symbol(Symbol::qualified("codec", "binary")));
    let mut table = Node::named("table", "call");
    table.target = Some(Expr::Symbol(Symbol::qualified("test", "table")));
    graph.nodes = vec![
        Node::named("in", "in"),
        shape,
        codec,
        table,
        Node::named("out", "out"),
    ];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("shape")),
        Edge::new(1, PortRef::output("shape"), PortRef::input("codec")),
        Edge::new(2, PortRef::output("codec"), PortRef::input("table")),
        Edge::new(3, PortRef::output("table"), PortRef::input("out")),
    ];
    graph
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
            return Err(sim_kernel::Error::TypeMismatch {
                expected: "string",
                found: "non-string",
            });
        };
        cx.factory().string(format!("{}{text}", self.prefix))
    }
}

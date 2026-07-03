use std::sync::Arc;

use sim_kernel::{
    Args, CORE_FUNCTION_CLASS_ID, Callable, ClassRef, Cx, DefaultFactory, EagerPolicy, Error, Expr,
    Object, Symbol, Value,
};

use crate::{
    compile_graph,
    diagram::{from_diagram, graph_from_diagram, parse_diagram},
    parse_graph,
    run::run_graph,
    topology_run_capability,
};

const DOC_EXAMPLE: &str = r#"
[in] --> [draft:call] --> [review:call] --> [gate:branch] --> [out]
                                        ^          |
                                        | false    | true
                                        +----------+

attrs:
  draft.target = writer
  draft.role = worker
  review.target = critic
  review.role = critic
  gate.when = accepted?
  gate:false.max-visits = 3
  budget.max-steps = 20
"#;

#[test]
fn diagram_pipeline_parses_to_canonical_graph_data() {
    let graph = graph_from_diagram("[in] --> [step:wire] --> [out]").expect("parsed graph");

    assert_eq!(graph.name, Symbol::new("diagram"));
    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.nodes[1].id.as_symbol(), &Symbol::new("step"));
    assert_eq!(graph.nodes[1].verb, Symbol::new("wire"));
    assert_eq!(graph.edges.len(), 2);

    let mut cx = runtime_cx();
    let data = parse_diagram("[in] --> [step:wire] --> [out]").expect("canonical data");
    let value = cx.factory().expr(data).expect("expr value");
    let reparsed = parse_graph(&mut cx, value).expect("reparsed graph");
    assert_eq!(reparsed.nodes.len(), 3);
}

#[test]
fn diagram_branch_uses_true_port_for_rightward_route() {
    let graph = graph_from_diagram("[in] --> [gate:branch] --> [out]").expect("parsed graph");

    assert_eq!(graph.edges[1].from.node.as_symbol(), &Symbol::new("gate"));
    assert_eq!(graph.edges[1].from.port, Symbol::new("true"));

    let mut cx = runtime_cx();
    compile_graph(&mut cx, &graph).expect("branch diagram compiles");
}

#[test]
fn diagram_loop_back_edge_uses_false_port_and_attrs() {
    let graph = graph_from_diagram(
        r#"
[in] --> [work:wire] --> [gate:branch] --> [out]
                       ^          |
                       | false    | true
                       +----------+

attrs:
  gate:false.max-visits = 2
"#,
    )
    .expect("parsed graph");

    let back_edge = graph
        .edges
        .iter()
        .find(|edge| {
            edge.from.node.as_symbol() == &Symbol::new("gate")
                && edge.from.port == Symbol::new("false")
        })
        .expect("false back edge");
    assert_eq!(back_edge.to.node.as_symbol(), &Symbol::new("work"));
    assert_eq!(back_edge.max_visits, Some(2));

    let mut cx = runtime_cx();
    compile_graph(&mut cx, &graph).expect("bounded loop diagram compiles");
}

#[test]
fn diagram_attrs_attach_target_and_role() {
    let graph = graph_from_diagram(
        r#"
[in] --> [draft:call] --> [out]

attrs:
  draft.target = writer
  draft.role = worker
"#,
    )
    .expect("parsed graph");

    assert_eq!(
        graph.nodes[1].target,
        Some(Expr::Symbol(Symbol::new("writer")))
    );
    assert_eq!(graph.nodes[1].role, Some(Symbol::new("worker")));
}

#[test]
fn diagram_ambiguous_arrow_reports_line_and_column() {
    let error = graph_from_diagram("[in] -- [out]").expect_err("bad arrow should fail");

    let Error::Eval(message) = error else {
        panic!("unexpected error type: {error}");
    };
    assert!(message.contains("line 1, column 5"));
    assert!(message.contains("expected -->"));
}

#[test]
fn diagram_doc_example_parses_and_runs() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "writer", "writer:");
    register_prefix(&mut cx, "critic", "critic:");
    register_true(&mut cx, "accepted?");
    let graph = graph_from_diagram(DOC_EXAMPLE).expect("parsed doc example");
    let plan = compile_graph(&mut cx, &graph).expect("compiled doc example");

    let output = run_graph(&mut cx, &graph, &plan, Expr::String("note".to_owned()))
        .expect("executed doc example");

    assert_eq!(output, Expr::String("critic:writer:note".to_owned()));
}

#[test]
fn diagram_from_diagram_returns_connection() {
    let mut cx = runtime_cx();
    let connection = from_diagram(&mut cx, "[in] --> [step:wire] --> [out]").expect("connection");

    assert_eq!(connection.site_kind(), "topology");
}

fn runtime_cx() -> Cx {
    let mut cx = Cx::new(Arc::new(EagerPolicy), Arc::new(DefaultFactory));
    cx.grant(topology_run_capability());
    let binary = sim_codec_binary::BinaryCodecLib::new(cx.registry_mut().fresh_codec_id());
    cx.load_lib(&binary).unwrap();
    cx
}

fn register_prefix(cx: &mut Cx, name: &str, prefix: &'static str) {
    let value = cx.factory().opaque(Arc::new(PrefixFn { prefix })).unwrap();
    cx.registry_mut()
        .register_value(Symbol::new(name), value)
        .unwrap();
}

fn register_true(cx: &mut Cx, name: &str) {
    let value = cx.factory().opaque(Arc::new(TrueFn)).unwrap();
    cx.registry_mut()
        .register_value(Symbol::new(name), value)
        .unwrap();
}

#[derive(Clone)]
struct PrefixFn {
    prefix: &'static str,
}

impl Object for PrefixFn {
    fn display(&self, _cx: &mut Cx) -> sim_kernel::Result<String> {
        Ok("#<function prefix>".to_owned())
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
            return Err(Error::TypeMismatch {
                expected: "string",
                found: "non-string",
            });
        };
        cx.factory().string(format!("{}{text}", self.prefix))
    }
}

#[derive(Clone)]
struct TrueFn;

impl Object for TrueFn {
    fn display(&self, _cx: &mut Cx) -> sim_kernel::Result<String> {
        Ok("#<function true>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl sim_kernel::ObjectCompat for TrueFn {
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

impl Callable for TrueFn {
    fn call(&self, cx: &mut Cx, _args: Args) -> sim_kernel::Result<Value> {
        cx.factory().bool(true)
    }
}

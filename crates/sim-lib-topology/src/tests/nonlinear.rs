use std::sync::Arc;

use sim_kernel::{
    Args, CORE_FUNCTION_CLASS_ID, Callable, ClassRef, Cx, DefaultFactory, EagerPolicy, Error, Expr,
    Object, Symbol, Value,
};

use crate::{
    Edge, Graph, Node, Port, PortMode, PortRef, compile_graph,
    run::{TopologyRun, run_graph},
    topology_run_capability,
};

#[test]
fn nonlinear_star_fanout_and_merge_count() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "left", "left:");
    register_prefix(&mut cx, "right", "right:");
    let graph = two_branch_merge_graph("fanout-count", "count", None);
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let output =
        run_graph(&mut cx, &graph, &plan, Expr::String("seed".to_owned())).expect("run graph");

    assert_eq!(
        output,
        Expr::List(vec![
            Expr::String("left:seed".to_owned()),
            Expr::String("right:seed".to_owned()),
        ])
    );
}

#[test]
fn nonlinear_merge_all_waits_for_all_inputs() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "left", "left:");
    register_prefix(&mut cx, "right", "right:");
    register_prefix(&mut cx, "third", "third:");
    let graph = three_branch_merge_graph("merge-all", "all");
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");
    let mut run = TopologyRun::new(&graph, &plan, Expr::String("seed".to_owned())).expect("run");

    run.run(&mut cx).expect("run graph");

    assert_eq!(run.outputs().len(), 1);
    assert_eq!(
        run.output_expr(),
        Expr::List(vec![
            Expr::String("left:seed".to_owned()),
            Expr::String("right:seed".to_owned()),
            Expr::String("third:seed".to_owned()),
        ])
    );
}

#[test]
fn nonlinear_merge_any_uses_first_arrival() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "left", "left:");
    register_prefix(&mut cx, "right", "right:");
    let graph = two_branch_merge_graph("merge-any", "any", None);
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");
    let mut run = TopologyRun::new(&graph, &plan, Expr::String("seed".to_owned())).expect("run");

    run.run(&mut cx).expect("run graph");

    assert_eq!(run.outputs(), &[Expr::String("left:seed".to_owned())]);
}

#[test]
fn nonlinear_merge_latest_emits_latest_map() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "left", "left:");
    register_prefix(&mut cx, "right", "right:");
    let graph = latest_merge_graph();
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");
    let mut run = TopologyRun::new(&graph, &plan, Expr::String("seed".to_owned())).expect("run");

    run.run(&mut cx).expect("run graph");

    assert_eq!(
        run.outputs().last(),
        Some(&Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("left")),
                Expr::String("left:seed".to_owned()),
            ),
            (
                Expr::Symbol(Symbol::new("right")),
                Expr::String("right:seed".to_owned()),
            ),
        ]))
    );
}

#[test]
fn nonlinear_race_picks_first_accepted_candidate() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "left", "reject:");
    register_prefix(&mut cx, "right", "accept:");
    register_accept_prefix(&mut cx, "accepted", "accept:");
    let mut graph = two_branch_terminal_graph("race-flow", "race");
    graph.nodes[4]
        .options
        .push((Symbol::new("accept"), Expr::Symbol(test_symbol("accepted"))));
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let output =
        run_graph(&mut cx, &graph, &plan, Expr::String("seed".to_owned())).expect("run graph");

    assert_eq!(output, Expr::String("accept:seed".to_owned()));
}

#[test]
fn nonlinear_quorum_accepts_two_equal_results() {
    let mut cx = runtime_cx();
    register_prefix(&mut cx, "left", "same:");
    register_prefix(&mut cx, "right", "same:");
    register_prefix(&mut cx, "third", "other:");
    let mut graph = three_branch_terminal_graph("quorum-flow", "quorum");
    graph.nodes[5]
        .options
        .push((Symbol::new("n"), number_expr(2)));
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");
    let mut run = TopologyRun::new(&graph, &plan, Expr::String("seed".to_owned())).expect("run");

    run.run(&mut cx).expect("run graph");

    assert_eq!(run.outputs(), &[Expr::String("same:seed".to_owned())]);
}

#[test]
fn nonlinear_reduce_folds_three_values() {
    let mut cx = runtime_cx();
    register_const(&mut cx, "left", "a");
    register_const(&mut cx, "right", "b");
    register_const(&mut cx, "third", "c");
    register_concat_reducer(&mut cx, "concat");
    let mut graph = three_branch_terminal_graph("reduce-flow", "reduce");
    graph.nodes[5].target = Some(Expr::Symbol(test_symbol("concat")));
    graph.nodes[5]
        .options
        .push((Symbol::new("initial"), Expr::String(String::new())));
    let plan = compile_graph(&mut cx, &graph).expect("compiled graph");

    let output =
        run_graph(&mut cx, &graph, &plan, Expr::String("ignored".to_owned())).expect("run graph");

    assert_eq!(output, Expr::String("abc".to_owned()));
}

fn two_branch_merge_graph(name: &str, mode: &str, count: Option<u32>) -> Graph {
    let mut graph = two_branch_terminal_graph(name, "merge");
    graph.nodes[4]
        .options
        .push((Symbol::new("mode"), Expr::Symbol(Symbol::new(mode))));
    if let Some(count) = count {
        graph.nodes[4]
            .options
            .push((Symbol::new("count"), number_expr(count)));
    } else if mode == "count" {
        graph.nodes[4]
            .options
            .push((Symbol::new("count"), number_expr(2)));
    }
    graph
}

fn three_branch_merge_graph(name: &str, mode: &str) -> Graph {
    let mut graph = three_branch_terminal_graph(name, "merge");
    graph.nodes[5]
        .options
        .push((Symbol::new("mode"), Expr::Symbol(Symbol::new(mode))));
    graph
}

fn two_branch_terminal_graph(name: &str, terminal_verb: &str) -> Graph {
    let mut graph = Graph::minimal(name);
    let left = call_node("left", "left");
    let right = call_node("right", "right");
    graph.nodes = vec![
        Node::named("in", "in"),
        Node::named("tee", "tee"),
        left,
        right,
        Node::named("join", terminal_verb),
        Node::named("out", "out"),
    ];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("tee")),
        Edge::new(1, PortRef::output("tee"), PortRef::input("left")),
        Edge::new(2, PortRef::output("tee"), PortRef::input("right")),
        Edge::new(3, PortRef::output("left"), PortRef::input("join")),
        Edge::new(4, PortRef::output("right"), PortRef::input("join")),
        Edge::new(5, PortRef::output("join"), PortRef::input("out")),
    ];
    graph
}

fn three_branch_terminal_graph(name: &str, terminal_verb: &str) -> Graph {
    let mut graph = Graph::minimal(name);
    graph.nodes = vec![
        Node::named("in", "in"),
        Node::named("tee", "tee"),
        call_node("left", "left"),
        call_node("right", "right"),
        call_node("third", "third"),
        Node::named("join", terminal_verb),
        Node::named("out", "out"),
    ];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("tee")),
        Edge::new(1, PortRef::output("tee"), PortRef::input("left")),
        Edge::new(2, PortRef::output("tee"), PortRef::input("right")),
        Edge::new(3, PortRef::output("tee"), PortRef::input("third")),
        Edge::new(4, PortRef::output("left"), PortRef::input("join")),
        Edge::new(5, PortRef::output("right"), PortRef::input("join")),
        Edge::new(6, PortRef::output("third"), PortRef::input("join")),
        Edge::new(7, PortRef::output("join"), PortRef::input("out")),
    ];
    graph
}

fn latest_merge_graph() -> Graph {
    let mut graph = two_branch_terminal_graph("merge-latest", "merge");
    graph.nodes[4] = Node::with_ports(
        "join",
        Symbol::new("merge"),
        vec![
            Port::new(Symbol::new("left"), PortMode::Value, true),
            Port::new(Symbol::new("right"), PortMode::Value, true),
        ],
        vec![Port::value("out", true)],
    );
    graph.nodes[4]
        .options
        .push((Symbol::new("mode"), Expr::Symbol(Symbol::new("latest"))));
    graph.edges[3].to = PortRef::named("join", "left");
    graph.edges[4].to = PortRef::named("join", "right");
    graph
}

fn call_node(id: &str, target: &str) -> Node {
    let mut node = Node::named(id, "call");
    node.target = Some(Expr::Symbol(test_symbol(target)));
    node
}

fn runtime_cx() -> Cx {
    let mut cx = Cx::new(Arc::new(EagerPolicy), Arc::new(DefaultFactory));
    cx.grant(topology_run_capability());
    cx
}

fn register_prefix(cx: &mut Cx, name: &str, prefix: &'static str) {
    register_callable(cx, name, PrefixFn { prefix });
}

fn register_const(cx: &mut Cx, name: &str, value: &'static str) {
    register_callable(cx, name, ConstFn { value });
}

fn register_accept_prefix(cx: &mut Cx, name: &str, prefix: &'static str) {
    register_callable(cx, name, AcceptPrefixFn { prefix });
}

fn register_concat_reducer(cx: &mut Cx, name: &str) {
    register_callable(cx, name, ConcatReducer);
}

fn register_callable<T>(cx: &mut Cx, name: &str, callable: T)
where
    T: Object + sim_kernel::ObjectCompat + Send + Sync + 'static,
{
    let value = cx.factory().opaque(Arc::new(callable)).unwrap();
    cx.registry_mut()
        .register_value(test_symbol(name), value)
        .unwrap();
}

fn test_symbol(name: &str) -> Symbol {
    Symbol::qualified("test", name)
}

fn number_expr(value: u32) -> Expr {
    Expr::Number(sim_kernel::NumberLiteral {
        domain: Symbol::new("u32"),
        canonical: value.to_string(),
    })
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
        function_class(cx)
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }
}

impl Callable for PrefixFn {
    fn call(&self, cx: &mut Cx, args: Args) -> sim_kernel::Result<Value> {
        let text = string_arg(cx, &args)?;
        cx.factory().string(format!("{}{text}", self.prefix))
    }
}

#[derive(Clone)]
struct ConstFn {
    value: &'static str,
}

impl Object for ConstFn {
    fn display(&self, _cx: &mut Cx) -> sim_kernel::Result<String> {
        Ok("#<function test/const>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl sim_kernel::ObjectCompat for ConstFn {
    fn class(&self, cx: &mut Cx) -> sim_kernel::Result<ClassRef> {
        function_class(cx)
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }
}

impl Callable for ConstFn {
    fn call(&self, cx: &mut Cx, _args: Args) -> sim_kernel::Result<Value> {
        cx.factory().string(self.value.to_owned())
    }
}

#[derive(Clone)]
struct AcceptPrefixFn {
    prefix: &'static str,
}

impl Object for AcceptPrefixFn {
    fn display(&self, _cx: &mut Cx) -> sim_kernel::Result<String> {
        Ok("#<function test/accept-prefix>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl sim_kernel::ObjectCompat for AcceptPrefixFn {
    fn class(&self, cx: &mut Cx) -> sim_kernel::Result<ClassRef> {
        function_class(cx)
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }
}

impl Callable for AcceptPrefixFn {
    fn call(&self, cx: &mut Cx, args: Args) -> sim_kernel::Result<Value> {
        let text = string_arg(cx, &args)?;
        cx.factory().bool(text.starts_with(self.prefix))
    }
}

#[derive(Clone)]
struct ConcatReducer;

impl Object for ConcatReducer {
    fn display(&self, _cx: &mut Cx) -> sim_kernel::Result<String> {
        Ok("#<function test/concat>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl sim_kernel::ObjectCompat for ConcatReducer {
    fn class(&self, cx: &mut Cx) -> sim_kernel::Result<ClassRef> {
        function_class(cx)
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }
}

impl Callable for ConcatReducer {
    fn call(&self, cx: &mut Cx, args: Args) -> sim_kernel::Result<Value> {
        let Some(first) = args.values().first() else {
            return cx.factory().string(String::new());
        };
        let Expr::List(items) = first.object().as_expr(cx)? else {
            return Err(Error::TypeMismatch {
                expected: "reducer pair",
                found: "non-list",
            });
        };
        let [acc, value] = items.as_slice() else {
            return Err(Error::TypeMismatch {
                expected: "reducer pair",
                found: "list",
            });
        };
        let acc = match acc {
            Expr::Nil => String::new(),
            Expr::String(text) => text.clone(),
            _ => {
                return Err(Error::TypeMismatch {
                    expected: "string accumulator",
                    found: "non-string",
                });
            }
        };
        let Expr::String(value) = value else {
            return Err(Error::TypeMismatch {
                expected: "string value",
                found: "non-string",
            });
        };
        cx.factory().string(format!("{acc}{value}"))
    }
}

fn function_class(cx: &mut Cx) -> sim_kernel::Result<ClassRef> {
    cx.factory().class_stub(
        CORE_FUNCTION_CLASS_ID,
        Symbol::qualified("core", "Function"),
    )
}

fn string_arg(cx: &mut Cx, args: &Args) -> sim_kernel::Result<String> {
    let Some(first) = args.values().first() else {
        return Ok(String::new());
    };
    let Expr::String(text) = first.object().as_expr(cx)? else {
        return Err(Error::TypeMismatch {
            expected: "string",
            found: "non-string",
        });
    };
    Ok(text)
}

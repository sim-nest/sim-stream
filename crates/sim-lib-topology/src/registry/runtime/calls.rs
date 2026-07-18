use std::sync::Arc;

use sim_kernel::{
    Args, CORE_FUNCTION_CLASS_ID, Callable, ClassRef, Cx, Error, Expr, Object, ObjectCompat,
    RawArgs, Result, Symbol, Value,
};

use crate::{
    Graph, TopologyConnection, TopologyPatch, compile_graph, connection_from_graph,
    cookbook::{embodied_intelligence_trace_demo, tiny_graph_demo},
    counterfactual_replay,
    diagram::{draw, from_diagram},
    package::{TopologyPackageSource, parse_package},
    parse_graph,
    patch::patched_connection,
    replay_report,
    run::run_graph,
    text::{from_text, graph_from_text, graph_to_expr, parse_text},
    topology_explain, topology_reflect_graph, topology_reflect_run,
};

use super::SharedTopologyRegistry;
use super::reports::{compiled_graph_expr, counterfactual_from_expr, run_embedded_tests};
use crate::registry::{
    TopologyRegistry, topology_def, topology_get, topology_list, topology_load_file,
    topology_load_source, topology_reload, topology_remove,
};

#[derive(Clone, Copy)]
enum TopologyFnKind {
    Graph,
    Compile,
    Run,
    ParseText,
    FromText,
    Draw,
    FromDiagram,
    Reflect,
    Explain,
    Replay,
    Counterfactual,
    Def,
    Get,
    List,
    Remove,
    LoadFile,
    LoadSource,
    Reload,
    Patch,
    Test,
    TinyGraphDemo,
    EmbodiedIntelligenceTraceDemo,
}

#[derive(Clone)]
pub(super) struct TopologyFunction {
    symbol: Symbol,
    kind: TopologyFnKind,
    registry: SharedTopologyRegistry,
}

impl TopologyFunction {
    pub(super) fn symbol(&self) -> &Symbol {
        &self.symbol
    }
}

impl Object for TopologyFunction {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok(format!("#<function {}>", self.symbol))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for TopologyFunction {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        if let Some(value) = cx
            .registry()
            .class_by_symbol(&Symbol::qualified("core", "Function"))
        {
            return Ok(value.clone());
        }
        cx.factory().class_stub(
            CORE_FUNCTION_CLASS_ID,
            Symbol::qualified("core", "Function"),
        )
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }
}

impl Callable for TopologyFunction {
    fn call(&self, cx: &mut Cx, args: Args) -> Result<Value> {
        match self.kind {
            TopologyFnKind::Patch => return self.call_patch_values(cx, args.values()),
            TopologyFnKind::LoadSource => return self.call_load_source_values(cx, args.values()),
            _ => {}
        }
        let exprs = args
            .values()
            .iter()
            .map(|value| value.object().as_expr(cx))
            .collect::<Result<Vec<_>>>()?;
        self.call_exprs(cx, RawArgs::new(exprs))
    }

    fn call_exprs(&self, cx: &mut Cx, args: RawArgs) -> Result<Value> {
        match self.kind {
            TopologyFnKind::Graph => self.call_graph(cx, args.into_exprs()),
            TopologyFnKind::Compile => self.call_compile(cx, args.into_exprs()),
            TopologyFnKind::Run => self.call_run(cx, args.into_exprs()),
            TopologyFnKind::ParseText => self.call_parse_text(cx, args.into_exprs()),
            TopologyFnKind::FromText => self.call_from_text(cx, args.into_exprs()),
            TopologyFnKind::Draw => self.call_draw(cx, args.into_exprs()),
            TopologyFnKind::FromDiagram => self.call_from_diagram(cx, args.into_exprs()),
            TopologyFnKind::Reflect => self.call_reflect(cx, args.into_exprs()),
            TopologyFnKind::Explain => self.call_explain(cx, args.into_exprs()),
            TopologyFnKind::Replay => self.call_replay(cx, args.into_exprs()),
            TopologyFnKind::Counterfactual => self.call_counterfactual(cx, args.into_exprs()),
            TopologyFnKind::Def => self.call_def(cx, args.into_exprs()),
            TopologyFnKind::Get => self.call_get(cx, args.into_exprs()),
            TopologyFnKind::List => self.call_list(cx, args.into_exprs()),
            TopologyFnKind::Remove => self.call_remove(cx, args.into_exprs()),
            TopologyFnKind::LoadFile => self.call_load_file(cx, args.into_exprs()),
            TopologyFnKind::LoadSource => self.call_load_source_exprs(args.into_exprs()),
            TopologyFnKind::Reload => self.call_reload(cx, args.into_exprs()),
            TopologyFnKind::Patch => self.call_patch_exprs(cx, args.into_exprs()),
            TopologyFnKind::Test => self.call_test(cx, args.into_exprs()),
            TopologyFnKind::TinyGraphDemo => self.call_tiny_graph_demo(cx, args.into_exprs()),
            TopologyFnKind::EmbodiedIntelligenceTraceDemo => {
                self.call_embodied_intelligence_trace_demo(cx, args.into_exprs())
            }
        }
    }
}

impl TopologyFunction {
    fn call_graph(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 1)?;
        let graph = graph_arg(cx, args[0].clone())?;
        let connection = connection_from_graph(cx, &graph)?;
        cx.factory().opaque(Arc::new(connection))
    }

    fn call_compile(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 1)?;
        let graph = graph_arg(cx, args[0].clone())?;
        let plan = compile_graph(cx, &graph)?;
        cx.factory().expr(compiled_graph_expr(&plan))
    }

    fn call_run(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 2)?;
        let graph = graph_from_source_expr(cx, &self.registry, &args[0])?;
        let plan = compile_graph(cx, &graph)?;
        let output = run_graph(cx, &graph, &plan, args[1].clone())?;
        cx.factory().expr(output)
    }

    fn call_parse_text(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 1)?;
        cx.factory().expr(parse_text(&string_arg(&args[0])?)?)
    }

    fn call_from_text(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 1)?;
        let source = string_arg(&args[0])?;
        let connection = from_text(cx, &source)?;
        cx.factory().opaque(Arc::new(connection))
    }

    fn call_draw(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 1)?;
        cx.factory().expr(draw(&string_arg(&args[0])?)?)
    }

    fn call_from_diagram(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 1)?;
        let source = string_arg(&args[0])?;
        let connection = from_diagram(cx, &source)?;
        cx.factory().opaque(Arc::new(connection))
    }

    fn call_reflect(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 1)?;
        let graph = graph_from_source_expr(cx, &self.registry, &args[0])?;
        cx.factory().expr(topology_reflect_graph(cx, &graph))
    }

    fn call_explain(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 2)?;
        let graph = graph_from_source_expr(cx, &self.registry, &args[0])?;
        let report = topology_reflect_run(cx, &graph, args[1].clone())?;
        cx.factory().expr(topology_explain(&report))
    }

    fn call_replay(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 2)?;
        let graph = graph_from_source_expr(cx, &self.registry, &args[0])?;
        let report = topology_reflect_run(cx, &graph, args[1].clone())?;
        cx.factory().expr(replay_report(&report)?)
    }

    fn call_counterfactual(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 3)?;
        let graph = graph_from_source_expr(cx, &self.registry, &args[0])?;
        let report = topology_reflect_run(cx, &graph, args[1].clone())?;
        let change = counterfactual_from_expr(&args[2])?;
        let output = counterfactual_replay(cx, &report, change)?;
        cx.factory().expr(output)
    }

    fn call_def(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 2)?;
        let name = symbol_arg(&args[0])?;
        let graph = graph_arg(cx, args[1].clone())?;
        let mut registry = lock_registry(&self.registry)?;
        let entry = topology_def(cx, &mut registry, name, graph)?;
        graph_value(cx, &entry.graph)
    }

    fn call_get(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 1)?;
        let name = symbol_arg(&args[0])?;
        let registry = lock_registry(&self.registry)?;
        match topology_get(&registry, &name) {
            Some(entry) => cx.factory().expr(topology_reflect_graph(cx, &entry.graph)),
            None => cx.factory().nil(),
        }
    }

    fn call_list(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 0)?;
        let registry = lock_registry(&self.registry)?;
        let values = topology_list(&registry)
            .into_iter()
            .map(|symbol| cx.factory().symbol(symbol))
            .collect::<Result<Vec<_>>>()?;
        cx.factory().list(values)
    }

    fn call_remove(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 1)?;
        let name = symbol_arg(&args[0])?;
        let mut registry = lock_registry(&self.registry)?;
        match topology_remove(cx, &mut registry, &name)? {
            Some(entry) => graph_value(cx, &entry.graph),
            None => cx.factory().nil(),
        }
    }

    fn call_load_file(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 1)?;
        let path = string_arg(&args[0])?;
        let mut registry = lock_registry(&self.registry)?;
        let entry = topology_load_file(cx, &mut registry, path)?;
        graph_value(cx, &entry.graph)
    }

    fn call_load_source_values(&self, cx: &mut Cx, args: &[Value]) -> Result<Value> {
        expect_value_len(&self.symbol, args, 2)?;
        let key_expr = args[1].object().as_expr(cx)?;
        let key = symbol_arg(&key_expr)?;
        let source = TopologyPackageSource::table_entry(args[0].clone(), key);
        let mut registry = lock_registry(&self.registry)?;
        let entry = topology_load_source(cx, &mut registry, source)?;
        graph_value(cx, &entry.graph)
    }

    fn call_load_source_exprs(&self, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 2)?;
        Err(Error::TypeMismatch {
            expected: "evaluated table source value",
            found: expr_type(&args[0]),
        })
    }

    fn call_reload(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 1)?;
        let name = symbol_arg(&args[0])?;
        let mut registry = lock_registry(&self.registry)?;
        let entry = topology_reload(cx, &mut registry, &name)?;
        graph_value(cx, &entry.graph)
    }

    fn call_patch_values(&self, cx: &mut Cx, args: &[Value]) -> Result<Value> {
        expect_value_len(&self.symbol, args, 2)?;
        let graph = graph_from_source_value(cx, &self.registry, &args[0])?;
        let patch_expr = args[1].object().as_expr(cx)?;
        let patch = TopologyPatch::from_expr(&patch_expr)?;
        let connection = patched_connection(cx, &graph, &patch)?;
        cx.factory().opaque(Arc::new(connection))
    }

    fn call_patch_exprs(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 2)?;
        let graph = graph_from_source_expr(cx, &self.registry, &args[0])?;
        let patch = TopologyPatch::from_expr(&args[1])?;
        let connection = patched_connection(cx, &graph, &patch)?;
        cx.factory().opaque(Arc::new(connection))
    }

    fn call_test(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 1)?;
        let graph = test_graph_arg(cx, &self.registry, &args[0])?;
        let report = run_embedded_tests(cx, &graph)?;
        cx.factory().expr(report)
    }

    fn call_tiny_graph_demo(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 0)?;
        cx.factory().expr(tiny_graph_demo())
    }

    fn call_embodied_intelligence_trace_demo(&self, cx: &mut Cx, args: Vec<Expr>) -> Result<Value> {
        expect_len(&self.symbol, &args, 0)?;
        cx.factory().expr(embodied_intelligence_trace_demo())
    }
}

pub(super) fn topology_functions(registry: SharedTopologyRegistry) -> Vec<TopologyFunction> {
    function_specs()
        .into_iter()
        .map(|(symbol, kind)| TopologyFunction {
            symbol,
            kind,
            registry: registry.clone(),
        })
        .collect()
}

pub(super) fn topology_function_symbols() -> Vec<Symbol> {
    function_specs()
        .into_iter()
        .map(|(symbol, _)| symbol)
        .collect()
}

fn function_specs() -> Vec<(Symbol, TopologyFnKind)> {
    [
        ("graph", TopologyFnKind::Graph),
        ("compile", TopologyFnKind::Compile),
        ("run", TopologyFnKind::Run),
        ("parse-text", TopologyFnKind::ParseText),
        ("from-text", TopologyFnKind::FromText),
        ("draw", TopologyFnKind::Draw),
        ("from-diagram", TopologyFnKind::FromDiagram),
        ("reflect", TopologyFnKind::Reflect),
        ("explain", TopologyFnKind::Explain),
        ("replay", TopologyFnKind::Replay),
        ("counterfactual", TopologyFnKind::Counterfactual),
        ("def", TopologyFnKind::Def),
        ("get", TopologyFnKind::Get),
        ("list", TopologyFnKind::List),
        ("remove", TopologyFnKind::Remove),
        ("load-file", TopologyFnKind::LoadFile),
        ("load-source", TopologyFnKind::LoadSource),
        ("reload", TopologyFnKind::Reload),
        ("patch", TopologyFnKind::Patch),
        ("test", TopologyFnKind::Test),
        ("tiny-graph-demo", TopologyFnKind::TinyGraphDemo),
        (
            "embodied-intelligence-trace-demo",
            TopologyFnKind::EmbodiedIntelligenceTraceDemo,
        ),
    ]
    .into_iter()
    .map(|(name, kind)| (Symbol::qualified("topology", name), kind))
    .collect()
}

fn lock_registry(
    registry: &SharedTopologyRegistry,
) -> Result<std::sync::MutexGuard<'_, TopologyRegistry>> {
    registry
        .lock()
        .map_err(|_| Error::PoisonedLock("topology registry"))
}

fn graph_arg(cx: &mut Cx, expr: Expr) -> Result<Graph> {
    let value = cx.factory().expr(expr)?;
    parse_graph(cx, value)
}

fn graph_value(cx: &mut Cx, graph: &Graph) -> Result<Value> {
    cx.factory().expr(graph_to_expr(graph))
}

fn graph_from_source_value(
    cx: &mut Cx,
    registry: &SharedTopologyRegistry,
    value: &Value,
) -> Result<Graph> {
    if let Some(connection) = value.object().downcast_ref::<TopologyConnection>() {
        return Ok(connection.source_graph().clone());
    }
    let expr = value.object().as_expr(cx)?;
    graph_from_source_expr(cx, registry, &expr)
}

fn graph_from_source_expr(
    cx: &mut Cx,
    registry: &SharedTopologyRegistry,
    expr: &Expr,
) -> Result<Graph> {
    if let Expr::Symbol(name) = expr {
        let registry = lock_registry(registry)?;
        if let Some(entry) = topology_get(&registry, name) {
            return Ok(entry.graph.clone());
        }
    }
    graph_arg(cx, expr.clone())
}

fn test_graph_arg(cx: &mut Cx, registry: &SharedTopologyRegistry, expr: &Expr) -> Result<Graph> {
    match expr {
        Expr::String(source)
            if source.contains("\ngraph:") || source.trim_start().starts_with("graph:") =>
        {
            Ok(parse_package(source)?.graph)
        }
        Expr::String(source) => graph_from_text(source),
        _ => graph_from_source_expr(cx, registry, expr),
    }
}

fn expect_len(symbol: &Symbol, args: &[Expr], expected: usize) -> Result<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(Error::Eval(format!(
            "{symbol} expects {expected} argument(s), got {}",
            args.len()
        )))
    }
}

fn expect_value_len(symbol: &Symbol, args: &[Value], expected: usize) -> Result<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(Error::Eval(format!(
            "{symbol} expects {expected} argument(s), got {}",
            args.len()
        )))
    }
}

fn symbol_arg(expr: &Expr) -> Result<Symbol> {
    match expr {
        Expr::Symbol(value) => Ok(value.clone()),
        Expr::String(value) => Ok(symbol_from_text(value)),
        other => Err(Error::TypeMismatch {
            expected: "symbol or string",
            found: expr_type(other),
        }),
    }
}

fn string_arg(expr: &Expr) -> Result<String> {
    match expr {
        Expr::String(value) => Ok(value.clone()),
        Expr::Symbol(value) => Ok(value.to_string()),
        other => Err(Error::TypeMismatch {
            expected: "string or symbol",
            found: expr_type(other),
        }),
    }
}

fn expr_type(expr: &Expr) -> &'static str {
    match expr {
        Expr::Nil => "nil",
        Expr::Bool(_) => "bool",
        Expr::Number(_) => "number",
        Expr::Symbol(_) => "symbol",
        Expr::Local(_) => "local",
        Expr::String(_) => "string",
        Expr::Bytes(_) => "bytes",
        Expr::List(_) => "list",
        Expr::Vector(_) => "vector",
        Expr::Map(_) => "map",
        Expr::Set(_) => "set",
        Expr::Call { .. } => "call",
        Expr::Infix { .. } => "infix",
        Expr::Prefix { .. } => "prefix",
        Expr::Postfix { .. } => "postfix",
        Expr::Block(_) => "block",
        Expr::Quote { .. } => "quote",
        Expr::Annotated { .. } => "annotated",
        Expr::Extension { .. } => "extension",
    }
}

fn symbol_from_text(text: &str) -> Symbol {
    if let Some((namespace, name)) = text.split_once('/')
        && !namespace.is_empty()
        && !name.is_empty()
    {
        return Symbol::qualified(namespace.to_owned(), name.to_owned());
    }
    if let Some((namespace, name)) = text.split_once(':')
        && !namespace.is_empty()
        && !name.is_empty()
    {
        return Symbol::qualified(namespace.to_owned(), name.to_owned());
    }
    Symbol::new(text.to_owned())
}

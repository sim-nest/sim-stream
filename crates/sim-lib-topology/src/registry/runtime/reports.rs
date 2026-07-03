use sim_kernel::{Cx, Error, Expr, Result, Symbol};

use crate::{CompiledGraph, Graph, TopologyCounterfactual, compile_graph, run::run_graph};

pub(super) fn run_embedded_tests(cx: &mut Cx, graph: &Graph) -> Result<Expr> {
    let plan = compile_graph(cx, graph)?;
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut rows = Vec::new();
    for test in &graph.tests {
        let output = run_graph(cx, graph, &plan, test.input.clone());
        let row = match output {
            Ok(output) if output == test.expect => {
                passed += 1;
                test_row(
                    &test.name,
                    true,
                    &test.input,
                    &test.expect,
                    Some(output),
                    None,
                )
            }
            Ok(output) => {
                failed += 1;
                test_row(
                    &test.name,
                    false,
                    &test.input,
                    &test.expect,
                    Some(output),
                    Some("output did not match expected expression".to_owned()),
                )
            }
            Err(error) => {
                failed += 1;
                test_row(
                    &test.name,
                    false,
                    &test.input,
                    &test.expect,
                    None,
                    Some(error.to_string()),
                )
            }
        };
        rows.push(row);
    }
    Ok(Expr::Map(vec![
        entry(
            "kind",
            Expr::Symbol(Symbol::qualified("topology", "test-report")),
        ),
        entry("graph", Expr::Symbol(graph.name.clone())),
        entry("passed", Expr::Bool(failed == 0)),
        entry("total", number_expr(graph.tests.len() as u64)),
        entry("ok", number_expr(passed as u64)),
        entry("failed", number_expr(failed as u64)),
        entry("tests", Expr::List(rows)),
    ]))
}

fn test_row(
    name: &Symbol,
    passed: bool,
    input: &Expr,
    expected: &Expr,
    output: Option<Expr>,
    detail: Option<String>,
) -> Expr {
    Expr::Map(vec![
        entry("name", Expr::Symbol(name.clone())),
        entry("passed", Expr::Bool(passed)),
        entry("input", input.clone()),
        entry("expected", expected.clone()),
        entry("output", output.unwrap_or(Expr::Nil)),
        entry("detail", detail.map(Expr::String).unwrap_or(Expr::Nil)),
    ])
}

pub(super) fn compiled_graph_expr(plan: &CompiledGraph) -> Expr {
    Expr::Map(vec![
        entry(
            "kind",
            Expr::Symbol(Symbol::qualified("topology", "compiled")),
        ),
        entry("graph", Expr::Symbol(plan.name.clone())),
        entry("nodes", Expr::List(compiled_nodes_expr(plan))),
        entry("edges", Expr::List(compiled_edges_expr(plan))),
        entry("inputs", Expr::List(index_exprs(&plan.input_nodes))),
        entry("outputs", Expr::List(index_exprs(&plan.output_nodes))),
    ])
}

fn compiled_nodes_expr(plan: &CompiledGraph) -> Vec<Expr> {
    plan.nodes
        .iter()
        .map(|node| {
            Expr::Map(vec![
                entry("index", number_expr(node.source_index as u64)),
                entry("id", Expr::Symbol(node.id.as_symbol().clone())),
                entry("verb", Expr::Symbol(node.verb.clone())),
            ])
        })
        .collect()
}

fn compiled_edges_expr(plan: &CompiledGraph) -> Vec<Expr> {
    plan.edges
        .iter()
        .map(|edge| {
            Expr::Map(vec![
                entry("index", number_expr(edge.source_index as u64)),
                entry("id", number_expr(u64::from(edge.id.0))),
                entry("from-node", number_expr(edge.from_node as u64)),
                entry("to-node", number_expr(edge.to_node as u64)),
                entry("priority", number_literal("i64", edge.priority.to_string())),
            ])
        })
        .collect()
}

pub(super) fn counterfactual_from_expr(expr: &Expr) -> Result<TopologyCounterfactual> {
    let kind = symbol_field(expr, "kind")?;
    match kind.name.as_ref() {
        "replace-target" => Ok(TopologyCounterfactual::ReplaceTarget {
            node: symbol_field(expr, "node")?,
            target: field_value(expr, "target")?.clone(),
        }),
        "disable-edge" => Ok(TopologyCounterfactual::DisableEdge {
            edge: crate::EdgeId(u32_field(expr, "edge")?),
        }),
        "force-predicate" => Ok(TopologyCounterfactual::ForcePredicate {
            node: symbol_field(expr, "node")?,
            result: bool_field(expr, "result")?,
        }),
        other => Err(Error::Eval(format!(
            "topology/counterfactual: unsupported change kind {other}"
        ))),
    }
}

fn symbol_field(expr: &Expr, name: &str) -> Result<Symbol> {
    match field_value(expr, name)? {
        Expr::Symbol(symbol) => Ok(symbol.clone()),
        Expr::String(text) => Ok(symbol_from_text(text)),
        other => Err(Error::TypeMismatch {
            expected: "symbol or string",
            found: expr_type(other),
        }),
    }
}

fn u32_field(expr: &Expr, name: &str) -> Result<u32> {
    match field_value(expr, name)? {
        Expr::Number(number) => number
            .canonical
            .parse::<u32>()
            .map_err(|_| Error::Eval(format!("field {name} must be a u32 number"))),
        Expr::String(text) => text
            .parse::<u32>()
            .map_err(|_| Error::Eval(format!("field {name} must be a u32 number"))),
        other => Err(Error::TypeMismatch {
            expected: "number or string",
            found: expr_type(other),
        }),
    }
}

fn bool_field(expr: &Expr, name: &str) -> Result<bool> {
    match field_value(expr, name)? {
        Expr::Bool(value) => Ok(*value),
        other => Err(Error::TypeMismatch {
            expected: "bool",
            found: expr_type(other),
        }),
    }
}

fn field_value<'a>(expr: &'a Expr, name: &str) -> Result<&'a Expr> {
    let Expr::Map(entries) = expr else {
        return Err(Error::TypeMismatch {
            expected: "map",
            found: expr_type(expr),
        });
    };
    entries
        .iter()
        .find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == name => {
                Some(value)
            }
            _ => None,
        })
        .ok_or_else(|| Error::Eval(format!("missing field {name}")))
}

fn index_exprs(values: &[usize]) -> Vec<Expr> {
    values
        .iter()
        .map(|value| number_expr(*value as u64))
        .collect()
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

use sim_value::build::{entry, uint as number_expr};

fn number_literal(domain: &str, canonical: String) -> Expr {
    sim_value::build::num(domain, &canonical)
}

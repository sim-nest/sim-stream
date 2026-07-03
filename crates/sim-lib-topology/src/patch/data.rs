use sim_kernel::{Error, Expr, Result, Symbol};

use crate::{
    Budget, Cell, Edge, Graph, Node, NodeId, PortRef, Scheduler, parse::parse_graph_expr,
    text::graph_to_expr,
};

use super::PatchOp;
use crate::record::{data_expr, port_ref_symbol};

pub(super) fn parse_patch_ops(expr: &Expr) -> Result<Vec<PatchOp>> {
    match data_expr(expr) {
        Expr::List(items) | Expr::Vector(items) => parse_list_ops(items),
        other => Err(patch_error(format!(
            "expected patch operation list, found {}",
            expr_kind(other)
        ))),
    }
}

pub(super) fn patch_ops_to_expr(ops: &[PatchOp]) -> Expr {
    Expr::List(ops.iter().map(patch_op_to_expr).collect())
}

fn parse_list_ops(items: &[Expr]) -> Result<Vec<PatchOp>> {
    let Some(first) = items.first().map(data_expr) else {
        return Ok(Vec::new());
    };
    if is_patch_wrapper(first) {
        return items[1..].iter().map(parse_op).collect();
    }
    if op_name(first).is_some() {
        return parse_op(&Expr::List(items.to_vec())).map(|op| vec![op]);
    }
    items.iter().map(parse_op).collect()
}

fn parse_op(expr: &Expr) -> Result<PatchOp> {
    let (Expr::List(items) | Expr::Vector(items)) = data_expr(expr) else {
        return Err(patch_error("expected patch operation form"));
    };
    let Some((head, args)) = items.split_first() else {
        return Err(patch_error("empty patch operation"));
    };
    let name = op_name(data_expr(head)).ok_or_else(|| patch_error("invalid patch operation"))?;
    match name {
        "add-node" => {
            expect_args(name, args, 1)?;
            Ok(PatchOp::AddNode(parse_node(&args[0])?))
        }
        "remove-node" => {
            expect_args(name, args, 1)?;
            Ok(PatchOp::RemoveNode(NodeId::from(symbol_arg(&args[0])?)))
        }
        "replace-node" => {
            expect_args(name, args, 2)?;
            Ok(PatchOp::ReplaceNode {
                id: NodeId::from(symbol_arg(&args[0])?),
                node: parse_node(&args[1])?,
            })
        }
        "add-edge" => {
            expect_args(name, args, 1)?;
            Ok(PatchOp::AddEdge {
                edge: parse_edge(&args[0])?,
                explicit_id: edge_has_id(&args[0]),
            })
        }
        "remove-edge" => {
            expect_args(name, args, 2)?;
            Ok(PatchOp::RemoveEdge {
                from: parse_port_ref(&args[0], "out")?,
                to: parse_port_ref(&args[1], "in")?,
            })
        }
        "replace-edge" => {
            expect_args(name, args, 3)?;
            Ok(PatchOp::ReplaceEdge {
                from: parse_port_ref(&args[0], "out")?,
                to: parse_port_ref(&args[1], "in")?,
                edge: parse_edge(&args[2])?,
                explicit_id: edge_has_id(&args[2]),
            })
        }
        "add-cell" => {
            expect_args(name, args, 1)?;
            Ok(PatchOp::AddCell(parse_cell(&args[0])?))
        }
        "set-budget" => {
            expect_args(name, args, 1)?;
            Ok(PatchOp::SetBudget(parse_budget(&args[0])?))
        }
        "set-scheduler" => {
            expect_args(name, args, 1)?;
            Ok(PatchOp::SetScheduler(parse_scheduler(&args[0])?))
        }
        "set-metadata" => {
            expect_args(name, args, 2)?;
            Ok(PatchOp::SetMetadata {
                key: symbol_arg(&args[0])?,
                value: data_expr(&args[1]).clone(),
            })
        }
        other => Err(patch_error(format!("unknown patch operation {other}"))),
    }
}

fn parse_node(expr: &Expr) -> Result<Node> {
    let mut graph = parse_graph_with("nodes", Expr::List(vec![data_expr(expr).clone()]))?;
    graph
        .nodes
        .pop()
        .ok_or_else(|| patch_error("add-node did not parse a node"))
}

fn parse_edge(expr: &Expr) -> Result<Edge> {
    let mut graph = parse_graph_with("edges", Expr::List(vec![data_expr(expr).clone()]))?;
    graph
        .edges
        .pop()
        .ok_or_else(|| patch_error("add-edge did not parse an edge"))
}

fn parse_cell(expr: &Expr) -> Result<Cell> {
    let mut graph = parse_graph_with("cells", Expr::List(vec![data_expr(expr).clone()]))?;
    graph
        .cells
        .pop()
        .ok_or_else(|| patch_error("add-cell did not parse a cell"))
}

fn parse_budget(expr: &Expr) -> Result<Budget> {
    parse_graph_with("budget", data_expr(expr).clone()).map(|graph| graph.budget)
}

fn parse_scheduler(expr: &Expr) -> Result<Scheduler> {
    parse_graph_with("scheduler", data_expr(expr).clone()).map(|graph| graph.scheduler)
}

fn parse_graph_with(field: &str, value: Expr) -> Result<Graph> {
    parse_graph_expr(&Expr::Map(vec![
        (
            Expr::Symbol(Symbol::new("name")),
            Expr::Symbol(Symbol::new("patch")),
        ),
        (Expr::Symbol(Symbol::new(field)), value),
    ]))
}

fn parse_port_ref(expr: &Expr, default_port: &str) -> Result<PortRef> {
    let text = symbol_text(expr)?;
    let parts = text.split(':').collect::<Vec<_>>();
    match parts.as_slice() {
        [node] if !node.is_empty() => Ok(PortRef::named(*node, default_port)),
        [node, port] if !node.is_empty() && !port.is_empty() => Ok(PortRef::named(*node, *port)),
        _ => Err(patch_error("expected port reference node or node:port")),
    }
}

fn symbol_arg(expr: &Expr) -> Result<Symbol> {
    let text = symbol_text(expr)?;
    if let Some((namespace, name)) = text.split_once('/')
        && !namespace.is_empty()
        && !name.is_empty()
    {
        return Ok(Symbol::qualified(namespace.to_owned(), name.to_owned()));
    }
    if let Some((namespace, name)) = text.split_once(':')
        && !namespace.is_empty()
        && !name.is_empty()
    {
        return Ok(Symbol::qualified(namespace.to_owned(), name.to_owned()));
    }
    Ok(Symbol::new(text))
}

fn symbol_text(expr: &Expr) -> Result<String> {
    match data_expr(expr) {
        Expr::Symbol(symbol) => Ok(symbol.to_string()),
        Expr::String(text) => Ok(text.clone()),
        other => Err(patch_error(format!(
            "expected symbol or string, found {}",
            expr_kind(other)
        ))),
    }
}

fn patch_op_to_expr(op: &PatchOp) -> Expr {
    match op {
        PatchOp::AddNode(node) => op_expr("add-node", vec![single_node_expr(node)]),
        PatchOp::RemoveNode(id) => {
            op_expr("remove-node", vec![Expr::Symbol(id.as_symbol().clone())])
        }
        PatchOp::ReplaceNode { id, node } => op_expr(
            "replace-node",
            vec![Expr::Symbol(id.as_symbol().clone()), single_node_expr(node)],
        ),
        PatchOp::AddEdge { edge, explicit_id } => {
            op_expr("add-edge", vec![single_edge_expr(edge, *explicit_id)])
        }
        PatchOp::RemoveEdge { from, to } => op_expr(
            "remove-edge",
            vec![
                Expr::Symbol(port_ref_symbol(from, "out")),
                Expr::Symbol(port_ref_symbol(to, "in")),
            ],
        ),
        PatchOp::ReplaceEdge {
            from,
            to,
            edge,
            explicit_id,
        } => op_expr(
            "replace-edge",
            vec![
                Expr::Symbol(port_ref_symbol(from, "out")),
                Expr::Symbol(port_ref_symbol(to, "in")),
                single_edge_expr(edge, *explicit_id),
            ],
        ),
        PatchOp::AddCell(cell) => op_expr("add-cell", vec![single_cell_expr(cell)]),
        PatchOp::SetBudget(budget) => op_expr("set-budget", vec![budget_expr(budget)]),
        PatchOp::SetScheduler(scheduler) => {
            op_expr("set-scheduler", vec![scheduler_expr(scheduler)])
        }
        PatchOp::SetMetadata { key, value } => op_expr(
            "set-metadata",
            vec![Expr::Symbol(key.clone()), value.clone()],
        ),
    }
}

fn single_node_expr(node: &Node) -> Expr {
    let mut graph = Graph::minimal("patch");
    graph.nodes.push(node.clone());
    list_field(&graph, "nodes")
        .into_iter()
        .next()
        .unwrap_or(Expr::Nil)
}

fn single_edge_expr(edge: &Edge, include_id: bool) -> Expr {
    let mut graph = Graph::minimal("patch");
    graph.edges.push(edge.clone());
    let expr = list_field(&graph, "edges")
        .into_iter()
        .next()
        .unwrap_or(Expr::Nil);
    if include_id {
        expr
    } else {
        without_field(expr, "id")
    }
}

fn single_cell_expr(cell: &Cell) -> Expr {
    let mut graph = Graph::minimal("patch");
    graph.cells.push(cell.clone());
    list_field(&graph, "cells")
        .into_iter()
        .next()
        .unwrap_or(Expr::Nil)
}

fn budget_expr(budget: &Budget) -> Expr {
    let mut graph = Graph::minimal("patch");
    graph.budget = budget.clone();
    graph_field(&graph, "budget").unwrap_or(Expr::Nil)
}

fn scheduler_expr(scheduler: &Scheduler) -> Expr {
    let mut graph = Graph::minimal("patch");
    graph.scheduler = scheduler.clone();
    graph_field(&graph, "scheduler").unwrap_or(Expr::Nil)
}

fn list_field(graph: &Graph, name: &str) -> Vec<Expr> {
    match graph_field(graph, name) {
        Some(Expr::List(items)) => items,
        _ => Vec::new(),
    }
}

fn graph_field(graph: &Graph, name: &str) -> Option<Expr> {
    let Expr::Map(entries) = graph_to_expr(graph) else {
        return None;
    };
    entries.into_iter().find_map(|(key, value)| match key {
        Expr::Symbol(symbol) if symbol.name.as_ref() == name => Some(value),
        _ => None,
    })
}

fn without_field(expr: Expr, name: &str) -> Expr {
    match expr {
        Expr::Map(entries) => Expr::Map(
            entries
                .into_iter()
                .filter(|(key, _)| key_name(key).as_deref() != Some(name))
                .collect(),
        ),
        other => other,
    }
}

fn op_expr(name: &str, args: Vec<Expr>) -> Expr {
    let mut items = Vec::with_capacity(args.len() + 1);
    items.push(Expr::Symbol(Symbol::new(name)));
    items.extend(args);
    Expr::List(items)
}

fn edge_has_id(expr: &Expr) -> bool {
    match data_expr(expr) {
        Expr::Map(entries) => entries
            .iter()
            .any(|(key, _)| key_name(key).as_deref() == Some("id")),
        Expr::List(items) | Expr::Vector(items) => items
            .get(3..)
            .unwrap_or(&[])
            .chunks_exact(2)
            .any(|pair| key_name(&pair[0]).as_deref() == Some("id")),
        _ => false,
    }
}

fn key_name(expr: &Expr) -> Option<String> {
    match data_expr(expr) {
        Expr::Symbol(symbol) => Some(symbol.name.trim_start_matches(':').replace('-', "_")),
        Expr::String(text) => Some(text.trim_start_matches(':').replace('-', "_")),
        _ => None,
    }
}

fn is_patch_wrapper(expr: &Expr) -> bool {
    matches!(expr, Expr::Symbol(symbol) if symbol.name.as_ref() == "patch")
}

fn op_name(expr: &Expr) -> Option<&str> {
    let Expr::Symbol(symbol) = expr else {
        return None;
    };
    Some(symbol.name.as_ref())
}

fn expect_args(name: &str, args: &[Expr], expected: usize) -> Result<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(patch_error(format!(
            "{name} expects {expected} argument(s), got {}",
            args.len()
        )))
    }
}

fn patch_error(message: impl Into<String>) -> Error {
    Error::Eval(format!("topology patch error: {}", message.into()))
}

use sim_value::kind::expr_kind;

use sim_kernel::{Expr, Symbol};

use crate::{BudgetExhausted, Cell, Edge, Graph, GraphTest, Node, Port, PortMode, SchedulerMode};

use super::value::number_expr;

/// Converts a graph into the canonical map expression used by topology data.
pub fn graph_to_expr(graph: &Graph) -> Expr {
    Expr::Map(vec![
        entry("kind", Expr::Symbol(Symbol::new("topology"))),
        entry("name", Expr::Symbol(graph.name.clone())),
        entry("version", Expr::String(graph.version.clone())),
        entry("api", Expr::String(graph.api.clone())),
        entry("input", optional_expr(graph.input.as_ref())),
        entry("output", optional_expr(graph.output.as_ref())),
        entry(
            "nodes",
            Expr::List(graph.nodes.iter().map(node_to_expr).collect()),
        ),
        entry(
            "edges",
            Expr::List(graph.edges.iter().map(edge_to_expr).collect()),
        ),
        entry(
            "cells",
            Expr::List(graph.cells.iter().map(cell_to_expr).collect()),
        ),
        entry("scheduler", scheduler_to_expr(graph)),
        entry("budget", budget_to_expr(graph)),
        entry(
            "capabilities",
            Expr::List(
                graph
                    .capabilities
                    .iter()
                    .cloned()
                    .map(Expr::Symbol)
                    .collect(),
            ),
        ),
        entry("metadata", symbol_expr_map(&graph.metadata)),
        entry(
            "tests",
            Expr::List(graph.tests.iter().map(test_to_expr).collect()),
        ),
    ])
}

fn node_to_expr(node: &Node) -> Expr {
    Expr::Map(vec![
        entry("id", Expr::Symbol(node.id.as_symbol().clone())),
        entry("verb", Expr::Symbol(node.verb.clone())),
        entry(
            "in",
            Expr::List(node.inputs.iter().map(port_to_expr).collect()),
        ),
        entry(
            "out",
            Expr::List(node.outputs.iter().map(port_to_expr).collect()),
        ),
        entry("target", optional_expr(node.target.as_ref())),
        entry(
            "role",
            node.role
                .as_ref()
                .cloned()
                .map(Expr::Symbol)
                .unwrap_or(Expr::Nil),
        ),
        entry("input", optional_expr(node.input.as_ref())),
        entry("output", optional_expr(node.output.as_ref())),
        entry("options", symbol_expr_map(&node.options)),
    ])
}

fn port_to_expr(port: &Port) -> Expr {
    Expr::Map(vec![
        entry("name", Expr::Symbol(port.name.clone())),
        entry("shape", optional_expr(port.shape.as_ref())),
        entry(
            "mode",
            Expr::Symbol(Symbol::new(match port.mode {
                PortMode::Value => "value",
                PortMode::Stream => "stream",
            })),
        ),
        entry("required", Expr::Bool(port.required)),
    ])
}

fn edge_to_expr(edge: &Edge) -> Expr {
    Expr::Map(vec![
        entry("id", number_expr(&edge.id.0.to_string())),
        entry("from", Expr::Symbol(port_ref_symbol(&edge.from, "out"))),
        entry("to", Expr::Symbol(port_ref_symbol(&edge.to, "in"))),
        entry("when", optional_expr(edge.when.as_ref())),
        entry("transform", optional_expr(edge.transform.as_ref())),
        entry(
            "as",
            edge.as_name
                .as_ref()
                .cloned()
                .map(Expr::Symbol)
                .unwrap_or(Expr::Nil),
        ),
        entry("priority", number_expr(&edge.priority.to_string())),
        entry(
            "max_visits",
            edge.max_visits
                .map(|value| number_expr(&value.to_string()))
                .unwrap_or(Expr::Nil),
        ),
        entry("buffer", optional_expr(edge.buffer.as_ref())),
        entry("metadata", symbol_expr_map(&edge.metadata)),
    ])
}

fn cell_to_expr(cell: &Cell) -> Expr {
    Expr::Map(vec![
        entry("name", Expr::Symbol(cell.name.clone())),
        entry("shape", optional_expr(cell.shape.as_ref())),
        entry("initial", cell.initial.clone()),
        entry(
            "merge",
            cell.merge
                .as_ref()
                .cloned()
                .map(Expr::Symbol)
                .unwrap_or(Expr::Nil),
        ),
        entry("private", Expr::Bool(cell.private)),
    ])
}

fn scheduler_to_expr(graph: &Graph) -> Expr {
    Expr::Map(vec![
        entry(
            "mode",
            Expr::Symbol(Symbol::new(match graph.scheduler.mode {
                SchedulerMode::Sequential => "sequential",
            })),
        ),
        entry(
            "seed",
            graph
                .scheduler
                .seed
                .map(|value| number_expr(&value.to_string()))
                .unwrap_or(Expr::Nil),
        ),
        entry(
            "max_concurrency",
            number_expr(&graph.scheduler.max_concurrency.to_string()),
        ),
        entry("deterministic", Expr::Bool(graph.scheduler.deterministic)),
    ])
}

fn budget_to_expr(graph: &Graph) -> Expr {
    Expr::Map(vec![
        entry(
            "max_steps",
            number_expr(&graph.budget.max_steps.to_string()),
        ),
        entry(
            "max_node_visits",
            number_expr(&graph.budget.max_node_visits.to_string()),
        ),
        entry(
            "max_edge_visits",
            number_expr(&graph.budget.max_edge_visits.to_string()),
        ),
        entry(
            "max_outputs",
            number_expr(&graph.budget.max_outputs.to_string()),
        ),
        entry(
            "max_child_runs",
            number_expr(&graph.budget.max_child_runs.to_string()),
        ),
        entry(
            "deadline_ms",
            graph
                .budget
                .deadline_ms
                .map(|value| number_expr(&value.to_string()))
                .unwrap_or(Expr::Nil),
        ),
        entry(
            "on_exhausted",
            Expr::Symbol(Symbol::new(match graph.budget.on_exhausted {
                BudgetExhausted::Fail => "fail",
                BudgetExhausted::Partial => "partial",
            })),
        ),
    ])
}

fn test_to_expr(test: &GraphTest) -> Expr {
    Expr::Map(vec![
        entry("name", Expr::Symbol(test.name.clone())),
        entry("input", test.input.clone()),
        entry("expect", test.expect.clone()),
        entry("fixtures", symbol_expr_map(&test.fixtures)),
    ])
}

fn symbol_expr_map(entries: &[(Symbol, Expr)]) -> Expr {
    Expr::Map(
        entries
            .iter()
            .map(|(key, value)| (Expr::Symbol(key.clone()), value.clone()))
            .collect(),
    )
}

use crate::record::port_ref_symbol;
use sim_value::build::entry;

fn optional_expr(value: Option<&Expr>) -> Expr {
    value.cloned().unwrap_or(Expr::Nil)
}

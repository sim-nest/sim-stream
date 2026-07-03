//! Parsing of canonical expression and list forms into topology graph data.

mod common;
mod fields;
mod util;

use sim_kernel::{Cx, Expr, Result, Symbol, Value};

use crate::{
    Budget, BudgetExhausted, Cell, Edge, EdgeId, Graph, Node, Port, PortMode, Scheduler,
    SchedulerMode, model::TOPOLOGY_API,
};

use self::{common::*, fields::Fields, util::*};

/// Parses a runtime value into canonical topology graph data.
pub fn parse_graph(cx: &mut Cx, value: Value) -> Result<Graph> {
    let expr = value.object().as_expr(cx)?;
    parse_graph_expr(&expr)
}

/// Parses a runtime value into a [`Graph`]; alias for [`parse_graph`].
pub fn graph_from_value(cx: &mut Cx, value: Value) -> Result<Graph> {
    parse_graph(cx, value)
}

pub(crate) fn parse_graph_expr(expr: &Expr) -> Result<Graph> {
    match data_expr(expr) {
        Expr::Map(entries) => parse_graph_fields(Fields::from_map(entries, "graph")?),
        Expr::List(items) | Expr::Vector(items) => parse_graph_list(items),
        other => Err(parse_error(
            "graph",
            format!(
                "expected graph map or keyword list, found {}",
                expr_kind(other)
            ),
        )),
    }
}

fn parse_graph_list(items: &[Expr]) -> Result<Graph> {
    let body = match items.first().map(data_expr) {
        Some(Expr::Symbol(symbol)) if symbol.as_qualified_str() == "topology/graph" => &items[1..],
        _ => items,
    };
    parse_graph_fields(Fields::from_keywords(body, "graph")?)
}

fn parse_graph_fields(fields: Fields<'_>) -> Result<Graph> {
    if let Some(kind) = fields.get("kind") {
        expect_named(kind, "topology", "graph.kind")?;
    }

    let name = parse_symbol(fields.required("name", "graph.name")?, "graph.name")?;
    let mut graph = Graph::new(name);

    if let Some(version) = fields.get("version") {
        graph.version = parse_string(version, "graph.version")?;
    }
    if let Some(api) = fields.get("api") {
        let api = parse_string(api, "graph.api")?;
        if api != TOPOLOGY_API {
            return Err(parse_error(
                "graph.api",
                format!("expected {TOPOLOGY_API}, found {api}"),
            ));
        }
        graph.api = api;
    }
    if let Some(input) = fields.get("input") {
        graph.input = parse_optional_expr(input);
    }
    if let Some(output) = fields.get("output") {
        graph.output = parse_optional_expr(output);
    }
    if let Some(nodes) = fields.get("nodes") {
        graph.nodes = parse_nodes(nodes, "graph.nodes")?;
    }
    if let Some(edges) = fields.get("edges") {
        graph.edges = parse_edges(edges, "graph.edges")?;
    }
    if let Some(cells) = fields.get("cells") {
        graph.cells = parse_cells(cells, "graph.cells")?;
    }
    if let Some(scheduler) = fields.get("scheduler") {
        graph.scheduler = parse_scheduler(scheduler, "graph.scheduler")?;
    }
    if let Some(budget) = fields.get("budget") {
        graph.budget = parse_budget(budget, "graph.budget")?;
    }
    if let Some(capabilities) = fields.get("capabilities") {
        graph.capabilities = parse_symbol_list(capabilities, "graph.capabilities")?;
    }
    if let Some(metadata) = fields.get("metadata") {
        graph.metadata = parse_symbol_expr_map(metadata, "graph.metadata")?;
    }
    if let Some(tests) = fields.get("tests") {
        graph.tests = parse_tests(tests, "graph.tests")?;
    }

    fields.reject_unknown(&[
        "kind",
        "name",
        "version",
        "api",
        "input",
        "output",
        "nodes",
        "edges",
        "cells",
        "scheduler",
        "budget",
        "capabilities",
        "metadata",
        "tests",
    ])?;

    Ok(graph)
}

fn parse_nodes(expr: &Expr, path: &str) -> Result<Vec<Node>> {
    sequence_items(expr, path)?
        .iter()
        .enumerate()
        .map(|(index, item)| parse_node(item, &format!("{path}[{index}]")))
        .collect()
}

fn parse_node(expr: &Expr, path: &str) -> Result<Node> {
    match data_expr(expr) {
        Expr::Map(entries) => parse_node_fields(Fields::from_map(entries, path)?, path),
        Expr::List(items) | Expr::Vector(items) => {
            let Some((first, rest)) = items.split_first() else {
                return Err(parse_error(path, "expected node id"));
            };
            let id = parse_symbol(first, &format!("{path}.id"))?;
            let fields = Fields::from_keywords(rest, path)?;
            parse_node_with_id(id, fields, path)
        }
        Expr::Symbol(symbol) => {
            let id = symbol.clone();
            Ok(Node::new(id.clone(), id))
        }
        other => Err(parse_error(
            path,
            format!(
                "expected node map, list, or symbol, found {}",
                expr_kind(other)
            ),
        )),
    }
}

fn parse_node_fields(fields: Fields<'_>, path: &str) -> Result<Node> {
    let id = parse_symbol(
        fields.required("id", &format!("{path}.id"))?,
        &format!("{path}.id"),
    )?;
    parse_node_with_id(id, fields, path)
}

fn parse_node_with_id(id: Symbol, fields: Fields<'_>, path: &str) -> Result<Node> {
    let verb = match fields.get("verb") {
        Some(value) => parse_symbol(value, &format!("{path}.verb"))?,
        None => id.clone(),
    };
    let mut node = Node::new(id, verb);

    if let Some(inputs) = fields.get("in") {
        node.inputs = parse_ports(inputs, &format!("{path}.in"))?;
    }
    if let Some(outputs) = fields.get("out") {
        node.outputs = parse_ports(outputs, &format!("{path}.out"))?;
    }
    if let Some(target) = fields.get("target") {
        node.target = parse_optional_expr(target);
    }
    if let Some(role) = fields.get("role") {
        node.role = parse_optional_symbol(role, &format!("{path}.role"))?;
    }
    if let Some(input) = fields.get("input") {
        node.input = parse_optional_expr(input);
    }
    if let Some(output) = fields.get("output") {
        node.output = parse_optional_expr(output);
    }
    if let Some(options) = fields.get("options") {
        node.options = parse_symbol_expr_map(options, &format!("{path}.options"))?;
    }
    node.options.extend(fields.unknown_pairs(&[
        "id", "verb", "in", "out", "target", "role", "input", "output", "options",
    ]));

    Ok(node)
}

fn parse_ports(expr: &Expr, path: &str) -> Result<Vec<Port>> {
    sequence_items(expr, path)?
        .iter()
        .enumerate()
        .map(|(index, item)| parse_port(item, &format!("{path}[{index}]")))
        .collect()
}

fn parse_port(expr: &Expr, path: &str) -> Result<Port> {
    match data_expr(expr) {
        Expr::Map(entries) => parse_port_fields(Fields::from_map(entries, path)?, path),
        Expr::List(items) | Expr::Vector(items) => {
            let Some((first, rest)) = items.split_first() else {
                return Err(parse_error(path, "expected port name"));
            };
            let name = parse_symbol(first, &format!("{path}.name"))?;
            let fields = Fields::from_keywords(rest, path)?;
            parse_port_with_name(name, fields, path)
        }
        Expr::Symbol(symbol) => Ok(Port::new(symbol.clone(), PortMode::Value, true)),
        other => Err(parse_error(
            path,
            format!(
                "expected port map, list, or symbol, found {}",
                expr_kind(other)
            ),
        )),
    }
}

fn parse_port_fields(fields: Fields<'_>, path: &str) -> Result<Port> {
    let name = parse_symbol(
        fields.required("name", &format!("{path}.name"))?,
        &format!("{path}.name"),
    )?;
    parse_port_with_name(name, fields, path)
}

fn parse_port_with_name(name: Symbol, fields: Fields<'_>, path: &str) -> Result<Port> {
    let mode = match fields.get("mode") {
        Some(value) => parse_port_mode(value, &format!("{path}.mode"))?,
        None => PortMode::Value,
    };
    let required = match fields.get("required") {
        Some(value) => parse_bool(value, &format!("{path}.required"))?,
        None => true,
    };
    let mut port = Port::new(name, mode, required);
    if let Some(shape) = fields.get("shape") {
        port.shape = parse_optional_expr(shape);
    }
    fields.reject_unknown(&["name", "shape", "mode", "required"])?;
    Ok(port)
}

fn parse_edges(expr: &Expr, path: &str) -> Result<Vec<Edge>> {
    sequence_items(expr, path)?
        .iter()
        .enumerate()
        .map(|(index, item)| {
            parse_edge(item, EdgeId::new(index as u32), &format!("{path}[{index}]"))
        })
        .collect()
}

fn parse_edge(expr: &Expr, fallback_id: EdgeId, path: &str) -> Result<Edge> {
    match data_expr(expr) {
        Expr::Map(entries) => {
            parse_edge_fields(Fields::from_map(entries, path)?, fallback_id, path)
        }
        Expr::List(items) | Expr::Vector(items) => parse_edge_list(items, fallback_id, path),
        other => Err(parse_error(
            path,
            format!("expected edge map or list, found {}", expr_kind(other)),
        )),
    }
}

fn parse_edge_list(items: &[Expr], fallback_id: EdgeId, path: &str) -> Result<Edge> {
    if items.len() < 3 || !is_arrow(&items[1]) {
        return Err(parse_error(path, "expected edge form: from -> to"));
    }
    let mut fields = Fields::from_keywords(&items[3..], path)?;
    fields.push("from", &items[0]);
    fields.push("to", &items[2]);
    parse_edge_fields(fields, fallback_id, path)
}

fn parse_edge_fields(fields: Fields<'_>, fallback_id: EdgeId, path: &str) -> Result<Edge> {
    let id = match fields.get("id") {
        Some(value) => EdgeId::new(parse_u32(value, &format!("{path}.id"))?),
        None => fallback_id,
    };
    let from = parse_port_ref(
        fields.required("from", &format!("{path}.from"))?,
        "out",
        &format!("{path}.from"),
    )?;
    let to = parse_port_ref(
        fields.required("to", &format!("{path}.to"))?,
        "in",
        &format!("{path}.to"),
    )?;
    let mut edge = Edge::new(id, from, to);

    if let Some(when) = fields.get("when") {
        edge.when = parse_optional_expr(when);
    }
    if let Some(transform) = fields.get("transform") {
        edge.transform = parse_optional_expr(transform);
    }
    if let Some(as_name) = fields.get("as") {
        edge.as_name = parse_optional_symbol(as_name, &format!("{path}.as"))?;
    }
    if let Some(priority) = fields.get("priority") {
        edge.priority = parse_i64(priority, &format!("{path}.priority"))?;
    }
    if let Some(max_visits) = fields.get("max_visits") {
        edge.max_visits = parse_optional_u32(max_visits, &format!("{path}.max_visits"))?;
    }
    if let Some(buffer) = fields.get("buffer") {
        edge.buffer = parse_optional_expr(buffer);
    }
    if let Some(metadata) = fields.get("metadata") {
        edge.metadata = parse_symbol_expr_map(metadata, &format!("{path}.metadata"))?;
    }
    edge.metadata.extend(fields.unknown_pairs(&[
        "id",
        "from",
        "to",
        "when",
        "transform",
        "as",
        "priority",
        "max_visits",
        "buffer",
        "metadata",
    ]));

    Ok(edge)
}

fn parse_cells(expr: &Expr, path: &str) -> Result<Vec<Cell>> {
    sequence_items(expr, path)?
        .iter()
        .enumerate()
        .map(|(index, item)| parse_cell(item, &format!("{path}[{index}]")))
        .collect()
}

fn parse_cell(expr: &Expr, path: &str) -> Result<Cell> {
    match data_expr(expr) {
        Expr::Map(entries) => parse_cell_fields(Fields::from_map(entries, path)?, path),
        Expr::List(items) | Expr::Vector(items) => {
            let Some((first, rest)) = items.split_first() else {
                return Err(parse_error(path, "expected cell name"));
            };
            let name = parse_symbol(first, &format!("{path}.name"))?;
            parse_cell_with_name(name, Fields::from_keywords(rest, path)?, path)
        }
        other => Err(parse_error(
            path,
            format!("expected cell map or list, found {}", expr_kind(other)),
        )),
    }
}

fn parse_cell_fields(fields: Fields<'_>, path: &str) -> Result<Cell> {
    let name = parse_symbol(
        fields.required("name", &format!("{path}.name"))?,
        &format!("{path}.name"),
    )?;
    parse_cell_with_name(name, fields, path)
}

fn parse_cell_with_name(name: Symbol, fields: Fields<'_>, path: &str) -> Result<Cell> {
    let initial = fields
        .required("initial", &format!("{path}.initial"))?
        .clone();
    let mut cell = Cell::new(name, initial);
    if let Some(shape) = fields.get("shape") {
        cell.shape = parse_optional_expr(shape);
    }
    if let Some(merge) = fields.get("merge") {
        cell.merge = parse_optional_symbol(merge, &format!("{path}.merge"))?;
    }
    if let Some(private) = fields.get("private") {
        cell.private = parse_bool(private, &format!("{path}.private"))?;
    }
    fields.reject_unknown(&["name", "shape", "initial", "merge", "private"])?;
    Ok(cell)
}

fn parse_scheduler(expr: &Expr, path: &str) -> Result<Scheduler> {
    let fields = parse_field_set(expr, path)?;
    let mut scheduler = Scheduler::default();
    if let Some(mode) = fields.get("mode") {
        expect_named(mode, "sequential", &format!("{path}.mode"))?;
        scheduler.mode = SchedulerMode::Sequential;
    }
    if let Some(seed) = fields.get("seed") {
        scheduler.seed = parse_optional_u64(seed, &format!("{path}.seed"))?;
    }
    if let Some(max_concurrency) = fields.get("max_concurrency") {
        scheduler.max_concurrency = parse_u32(max_concurrency, &format!("{path}.max_concurrency"))?;
    }
    if let Some(deterministic) = fields.get("deterministic") {
        scheduler.deterministic = parse_bool(deterministic, &format!("{path}.deterministic"))?;
    }
    fields.reject_unknown(&["mode", "seed", "max_concurrency", "deterministic"])?;
    Ok(scheduler)
}

fn parse_budget(expr: &Expr, path: &str) -> Result<Budget> {
    let fields = parse_field_set(expr, path)?;
    let mut budget = Budget::default();
    if let Some(max_steps) = fields.get("max_steps") {
        budget.max_steps = parse_u32(max_steps, &format!("{path}.max_steps"))?;
    }
    if let Some(max_node_visits) = fields.get("max_node_visits") {
        budget.max_node_visits = parse_u32(max_node_visits, &format!("{path}.max_node_visits"))?;
    }
    if let Some(max_edge_visits) = fields.get("max_edge_visits") {
        budget.max_edge_visits = parse_u32(max_edge_visits, &format!("{path}.max_edge_visits"))?;
    }
    if let Some(max_outputs) = fields.get("max_outputs") {
        budget.max_outputs = parse_u32(max_outputs, &format!("{path}.max_outputs"))?;
    }
    if let Some(max_child_runs) = fields.get("max_child_runs") {
        budget.max_child_runs = parse_u32(max_child_runs, &format!("{path}.max_child_runs"))?;
    }
    if let Some(deadline_ms) = fields.get("deadline_ms") {
        budget.deadline_ms = parse_optional_u64(deadline_ms, &format!("{path}.deadline_ms"))?;
    }
    if let Some(on_exhausted) = fields.get("on_exhausted") {
        budget.on_exhausted =
            match symbolish_name(on_exhausted, &format!("{path}.on_exhausted"))?.as_str() {
                "fail" => BudgetExhausted::Fail,
                "partial" => BudgetExhausted::Partial,
                other => {
                    return Err(parse_error(
                        format!("{path}.on_exhausted"),
                        format!("expected fail or partial, found {other}"),
                    ));
                }
            };
    }
    fields.reject_unknown(&[
        "max_steps",
        "max_node_visits",
        "max_edge_visits",
        "max_outputs",
        "max_child_runs",
        "deadline_ms",
        "on_exhausted",
    ])?;
    Ok(budget)
}

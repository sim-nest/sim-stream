//! Static topology graph validation.

use std::collections::{BTreeMap, BTreeSet};

use sim_kernel::{Cx, Expr, Result, Symbol};
use sim_shape::parse_shape_expr;

use crate::{
    capability::capability_name_from_symbol,
    error::validation_error,
    model::{Edge, EdgeId, Graph, Node, NodeId, Port, PortRef},
};

mod cycle;

/// Validates topology graph data before it is compiled or executed.
pub fn validate_graph(_cx: &mut Cx, graph: &Graph) -> Result<()> {
    validate_budget(graph)?;
    validate_capabilities(graph)?;
    validate_shapes(graph)?;

    let index = GraphIndex::build(graph)?;
    validate_node_declarations(graph)?;
    validate_public_boundary(graph)?;
    validate_edges(graph, &index)?;
    validate_required_ports(graph)?;
    validate_verbs(graph)?;
    validate_reachability(graph, &index)?;
    cycle::validate_bounded_cycles(graph, &index)?;

    Ok(())
}

#[derive(Clone, Copy)]
enum PortDirection {
    Input,
    Output,
}

struct GraphIndex {
    nodes: BTreeMap<NodeId, usize>,
}

impl GraphIndex {
    fn build(graph: &Graph) -> Result<Self> {
        let mut nodes = BTreeMap::new();
        for (index, node) in graph.nodes.iter().enumerate() {
            if nodes.insert(node.id.clone(), index).is_some() {
                return Err(validation_error(
                    &graph.name,
                    node_context(node),
                    "duplicate node id",
                ));
            }
        }
        Ok(Self { nodes })
    }

    fn node<'a>(&self, graph: &'a Graph, id: &NodeId) -> Option<&'a Node> {
        self.nodes.get(id).and_then(|index| graph.nodes.get(*index))
    }

    fn position(&self, id: &NodeId) -> Option<usize> {
        self.nodes.get(id).copied()
    }
}

fn validate_budget(graph: &Graph) -> Result<()> {
    check_positive(graph, "budget.max_steps", graph.budget.max_steps)?;
    check_positive(
        graph,
        "budget.max_node_visits",
        graph.budget.max_node_visits,
    )?;
    check_positive(
        graph,
        "budget.max_edge_visits",
        graph.budget.max_edge_visits,
    )?;
    check_positive(graph, "budget.max_outputs", graph.budget.max_outputs)?;
    check_positive(graph, "budget.max_child_runs", graph.budget.max_child_runs)?;
    check_positive(
        graph,
        "scheduler.max_concurrency",
        graph.scheduler.max_concurrency,
    )?;

    if graph.budget.deadline_ms == Some(0) {
        return Err(validation_error(
            &graph.name,
            "budget.deadline_ms",
            "deadline must be positive when present",
        ));
    }

    Ok(())
}

fn check_positive(graph: &Graph, context: &str, value: u32) -> Result<()> {
    if value == 0 {
        return Err(validation_error(
            &graph.name,
            context,
            "value must be positive",
        ));
    }
    Ok(())
}

fn validate_capabilities(graph: &Graph) -> Result<()> {
    let mut seen = BTreeSet::new();
    for capability in &graph.capabilities {
        let name = capability_name_from_symbol(capability)
            .map_err(|err| validation_error(&graph.name, "capabilities", err.to_string()))?;
        if !seen.insert(name.clone()) {
            return Err(validation_error(
                &graph.name,
                "capabilities",
                format!("duplicate capability name {name}"),
            ));
        }
    }
    Ok(())
}

fn validate_shapes(graph: &Graph) -> Result<()> {
    validate_shape(graph, "graph.input", graph.input.as_ref())?;
    validate_shape(graph, "graph.output", graph.output.as_ref())?;

    for node in &graph.nodes {
        validate_shape(
            graph,
            format!("{}.input", node_context(node)),
            node.input.as_ref(),
        )?;
        validate_shape(
            graph,
            format!("{}.output", node_context(node)),
            node.output.as_ref(),
        )?;
        for port in &node.inputs {
            validate_shape(
                graph,
                format!("{} input port {}", node_context(node), port.name),
                port.shape.as_ref(),
            )?;
        }
        for port in &node.outputs {
            validate_shape(
                graph,
                format!("{} output port {}", node_context(node), port.name),
                port.shape.as_ref(),
            )?;
        }
    }

    for cell in &graph.cells {
        validate_shape(
            graph,
            format!("cell {} shape", cell.name),
            cell.shape.as_ref(),
        )?;
    }

    Ok(())
}

fn validate_shape(graph: &Graph, context: impl AsRef<str>, shape: Option<&Expr>) -> Result<()> {
    let Some(shape) = shape else {
        return Ok(());
    };
    parse_shape_expr(shape).map_err(|error| {
        validation_error(
            &graph.name,
            context.as_ref(),
            format!("invalid shape value: {error}"),
        )
    })?;
    Ok(())
}

fn validate_node_declarations(graph: &Graph) -> Result<()> {
    for node in &graph.nodes {
        if !valid_symbol(node.id.as_symbol()) {
            return Err(validation_error(
                &graph.name,
                node_context(node),
                "node id must be a non-keyword symbol",
            ));
        }
        if !valid_symbol(&node.verb) {
            return Err(validation_error(
                &graph.name,
                node_context(node),
                "node verb must be a non-keyword symbol",
            ));
        }
        validate_ports(graph, node, PortDirection::Input, &node.inputs)?;
        validate_ports(graph, node, PortDirection::Output, &node.outputs)?;
    }
    Ok(())
}

fn validate_ports(
    graph: &Graph,
    node: &Node,
    direction: PortDirection,
    ports: &[Port],
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for port in ports {
        if !valid_symbol(&port.name) {
            return Err(validation_error(
                &graph.name,
                format!(
                    "{} {} port {}",
                    node_context(node),
                    direction_name(direction),
                    port.name
                ),
                "port name must be a non-keyword symbol",
            ));
        }
        if !seen.insert(port.name.clone()) {
            return Err(validation_error(
                &graph.name,
                format!(
                    "{} {} port {}",
                    node_context(node),
                    direction_name(direction),
                    port.name
                ),
                "duplicate port name",
            ));
        }
    }
    Ok(())
}

fn validate_public_boundary(graph: &Graph) -> Result<()> {
    if !graph
        .nodes
        .iter()
        .any(|node| node.verb.name.as_ref() == "in")
    {
        return Err(validation_error(
            &graph.name,
            "graph",
            "missing input node with verb in",
        ));
    }
    if !graph
        .nodes
        .iter()
        .any(|node| node.verb.name.as_ref() == "out")
    {
        return Err(validation_error(
            &graph.name,
            "graph",
            "missing output node with verb out",
        ));
    }
    Ok(())
}

fn validate_edges(graph: &Graph, index: &GraphIndex) -> Result<()> {
    let mut seen = BTreeSet::<EdgeId>::new();
    for edge in &graph.edges {
        if !seen.insert(edge.id) {
            return Err(validation_error(
                &graph.name,
                edge_context(edge),
                "duplicate edge id",
            ));
        }
        if edge.max_visits == Some(0) {
            return Err(validation_error(
                &graph.name,
                edge_context(edge),
                "max_visits must be positive when present",
            ));
        }
        validate_endpoint(graph, index, edge, &edge.from, PortDirection::Output)?;
        validate_endpoint(graph, index, edge, &edge.to, PortDirection::Input)?;
    }
    Ok(())
}

fn validate_endpoint(
    graph: &Graph,
    index: &GraphIndex,
    edge: &Edge,
    endpoint: &PortRef,
    direction: PortDirection,
) -> Result<()> {
    let Some(node) = index.node(graph, &endpoint.node) else {
        return Err(validation_error(
            &graph.name,
            edge_context(edge),
            format!(
                "unknown {} endpoint node {}",
                direction_name(direction),
                endpoint.node.as_symbol()
            ),
        ));
    };

    let ports = match direction {
        PortDirection::Input => &node.inputs,
        PortDirection::Output => &node.outputs,
    };
    if !ports.iter().any(|port| port.name == endpoint.port) {
        return Err(validation_error(
            &graph.name,
            edge_context(edge),
            format!(
                "unknown {} endpoint port {}:{}",
                direction_name(direction),
                endpoint.node.as_symbol(),
                endpoint.port
            ),
        ));
    }
    Ok(())
}

fn validate_required_ports(graph: &Graph) -> Result<()> {
    let incoming = connected_counts(graph, PortDirection::Input);
    let outgoing = connected_counts(graph, PortDirection::Output);

    for node in &graph.nodes {
        for port in &node.inputs {
            if port.required && connected_count(&incoming, node, port) == 0 {
                return Err(validation_error(
                    &graph.name,
                    format!("{} input port {}", node_context(node), port.name),
                    "required input port is not connected",
                ));
            }
        }
        for port in &node.outputs {
            if port.required && connected_count(&outgoing, node, port) == 0 {
                return Err(validation_error(
                    &graph.name,
                    format!("{} output port {}", node_context(node), port.name),
                    "required output port is not connected",
                ));
            }
        }
    }
    Ok(())
}

fn connected_counts(graph: &Graph, direction: PortDirection) -> BTreeMap<(NodeId, Symbol), usize> {
    let mut counts = BTreeMap::new();
    for edge in &graph.edges {
        let endpoint = match direction {
            PortDirection::Input => &edge.to,
            PortDirection::Output => &edge.from,
        };
        *counts
            .entry((endpoint.node.clone(), endpoint.port.clone()))
            .or_insert(0) += 1;
    }
    counts
}

fn connected_count(counts: &BTreeMap<(NodeId, Symbol), usize>, node: &Node, port: &Port) -> usize {
    counts
        .get(&(node.id.clone(), port.name.clone()))
        .copied()
        .unwrap_or(0)
}

fn validate_verbs(graph: &Graph) -> Result<()> {
    for node in &graph.nodes {
        if node.verb.name.as_ref() == "call" && node.target.is_none() {
            return Err(validation_error(
                &graph.name,
                node_context(node),
                "call node requires target",
            ));
        }
    }
    Ok(())
}

fn validate_reachability(graph: &Graph, index: &GraphIndex) -> Result<()> {
    let adjacency = adjacency(graph, index);
    let mut visited = vec![false; graph.nodes.len()];
    for (node_index, node) in graph.nodes.iter().enumerate() {
        if node.verb.name.as_ref() == "in" {
            visit_reachable(node_index, &adjacency, &mut visited);
        }
    }

    for (node_index, node) in graph.nodes.iter().enumerate() {
        if node.verb.name.as_ref() == "out" && !visited[node_index] {
            return Err(validation_error(
                &graph.name,
                node_context(node),
                "output is unreachable from graph input",
            ));
        }
    }

    Ok(())
}

fn visit_reachable(node: usize, adjacency: &[Vec<(usize, usize)>], visited: &mut [bool]) {
    if visited[node] {
        return;
    }
    visited[node] = true;
    for (next, _) in &adjacency[node] {
        visit_reachable(*next, adjacency, visited);
    }
}

fn adjacency(graph: &Graph, index: &GraphIndex) -> Vec<Vec<(usize, usize)>> {
    let mut adjacency = vec![Vec::new(); graph.nodes.len()];
    for (edge_index, edge) in graph.edges.iter().enumerate() {
        let Some(from) = index.position(&edge.from.node) else {
            continue;
        };
        let Some(to) = index.position(&edge.to.node) else {
            continue;
        };
        adjacency[from].push((to, edge_index));
    }
    adjacency
}

fn valid_symbol(symbol: &Symbol) -> bool {
    !symbol.name.is_empty() && !symbol.name.starts_with(':')
}

fn direction_name(direction: PortDirection) -> &'static str {
    match direction {
        PortDirection::Input => "input",
        PortDirection::Output => "output",
    }
}

fn node_context(node: &Node) -> String {
    format!("node {}", node.id.as_symbol())
}

fn edge_context(edge: &Edge) -> String {
    format!("edge {}", edge.id.0)
}

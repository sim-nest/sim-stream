//! Graph validation and lowering to deterministic compile plans.
//!
//! `compile_graph` validates topology graph data and lowers it to a
//! `CompiledGraph` with stable node and edge indexes.

use std::collections::BTreeMap;

use sim_kernel::{Cx, Result, Symbol};

use crate::{EdgeId, Graph, NodeId, PortRef, validate::validate_graph};

/// Deterministic graph plan produced from validated topology data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledGraph {
    /// Graph name copied from source data.
    pub name: Symbol,
    /// Compiled nodes in source declaration order.
    pub nodes: Vec<CompiledNode>,
    /// Compiled edges in source declaration order.
    pub edges: Vec<CompiledEdge>,
    /// Stable lookup from node id to source-order node index.
    pub node_index_by_id: BTreeMap<NodeId, usize>,
    /// Stable lookup from edge id to source-order edge index.
    pub edge_index_by_id: BTreeMap<EdgeId, usize>,
    /// Public input node indexes in source order.
    pub input_nodes: Vec<usize>,
    /// Public output node indexes in source order.
    pub output_nodes: Vec<usize>,
    /// Incoming edge indexes for each node, preserving source edge order.
    pub incoming_edges: Vec<Vec<usize>>,
    /// Outgoing edge indexes for each node, sorted by priority then source order.
    pub outgoing_edges: Vec<Vec<usize>>,
    /// Whether each node is reachable from any public input node.
    pub reachable_from_inputs: Vec<bool>,
    /// Whether each node participates in a directed cycle.
    pub cyclic_nodes: Vec<bool>,
    /// Whether each edge participates in a directed cycle.
    pub cycle_edges: Vec<bool>,
}

/// Compiled node metadata with a stable source index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledNode {
    /// Source-order node index.
    pub source_index: usize,
    /// Source node id.
    pub id: NodeId,
    /// Source node verb.
    pub verb: Symbol,
}

/// Compiled edge metadata with stable endpoint indexes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledEdge {
    /// Source-order edge index.
    pub source_index: usize,
    /// Source edge id.
    pub id: EdgeId,
    /// Source port reference.
    pub from: PortRef,
    /// Destination port reference.
    pub to: PortRef,
    /// Source-order node index for `from`.
    pub from_node: usize,
    /// Source-order node index for `to`.
    pub to_node: usize,
    /// Routing priority copied from source data.
    pub priority: i64,
}

/// Validates `graph` and lowers it to a [`CompiledGraph`] with stable node and
/// edge indexes.
///
/// This is the compile stage of the topology pipeline (see the crate-level
/// documentation); the resulting plan is deterministic for a given graph.
pub fn compile_graph(cx: &mut Cx, graph: &Graph) -> Result<CompiledGraph> {
    validate_graph(cx, graph)?;

    let node_index_by_id = node_indexes(graph);
    let input_nodes = boundary_nodes(graph, "in");
    let output_nodes = boundary_nodes(graph, "out");
    let nodes = compile_nodes(graph);
    let (edges, edge_index_by_id) = compile_edges(graph, &node_index_by_id);
    let (incoming_edges, outgoing_edges) = edge_lists(graph.nodes.len(), &edges);
    let reachable_from_inputs =
        reachable_from_inputs(graph.nodes.len(), &edges, &outgoing_edges, &input_nodes);
    let (cyclic_nodes, cycle_edges) = cycle_metadata(graph.nodes.len(), &edges, &outgoing_edges);

    Ok(CompiledGraph {
        name: graph.name.clone(),
        nodes,
        edges,
        node_index_by_id,
        edge_index_by_id,
        input_nodes,
        output_nodes,
        incoming_edges,
        outgoing_edges,
        reachable_from_inputs,
        cyclic_nodes,
        cycle_edges,
    })
}

fn node_indexes(graph: &Graph) -> BTreeMap<NodeId, usize> {
    graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), index))
        .collect()
}

fn boundary_nodes(graph: &Graph, verb: &str) -> Vec<usize> {
    graph
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| (node.verb.name.as_ref() == verb).then_some(index))
        .collect()
}

fn compile_nodes(graph: &Graph) -> Vec<CompiledNode> {
    graph
        .nodes
        .iter()
        .enumerate()
        .map(|(source_index, node)| CompiledNode {
            source_index,
            id: node.id.clone(),
            verb: node.verb.clone(),
        })
        .collect()
}

fn compile_edges(
    graph: &Graph,
    node_index_by_id: &BTreeMap<NodeId, usize>,
) -> (Vec<CompiledEdge>, BTreeMap<EdgeId, usize>) {
    let mut edge_index_by_id = BTreeMap::new();
    let edges = graph
        .edges
        .iter()
        .enumerate()
        .map(|(source_index, edge)| {
            edge_index_by_id.insert(edge.id, source_index);
            CompiledEdge {
                source_index,
                id: edge.id,
                from: edge.from.clone(),
                to: edge.to.clone(),
                from_node: node_index_by_id[&edge.from.node],
                to_node: node_index_by_id[&edge.to.node],
                priority: edge.priority,
            }
        })
        .collect();
    (edges, edge_index_by_id)
}

fn edge_lists(node_count: usize, edges: &[CompiledEdge]) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let mut incoming = vec![Vec::new(); node_count];
    let mut outgoing = vec![Vec::new(); node_count];

    for edge in edges {
        incoming[edge.to_node].push(edge.source_index);
        outgoing[edge.from_node].push(edge.source_index);
    }

    for edge_list in &mut outgoing {
        edge_list.sort_by_key(|edge_index| (edges[*edge_index].priority, *edge_index));
    }

    (incoming, outgoing)
}

fn reachable_from_inputs(
    node_count: usize,
    edges: &[CompiledEdge],
    outgoing_edges: &[Vec<usize>],
    input_nodes: &[usize],
) -> Vec<bool> {
    let mut reachable = vec![false; node_count];
    for input in input_nodes {
        visit_reachable(*input, edges, outgoing_edges, &mut reachable);
    }
    reachable
}

fn visit_reachable(
    node: usize,
    edges: &[CompiledEdge],
    outgoing_edges: &[Vec<usize>],
    reachable: &mut [bool],
) {
    if reachable[node] {
        return;
    }
    reachable[node] = true;
    for edge_index in &outgoing_edges[node] {
        visit_reachable(edges[*edge_index].to_node, edges, outgoing_edges, reachable);
    }
}

fn cycle_metadata(
    node_count: usize,
    edges: &[CompiledEdge],
    outgoing_edges: &[Vec<usize>],
) -> (Vec<bool>, Vec<bool>) {
    let reachability = transitive_reachability(node_count, edges, outgoing_edges);
    let mut cyclic_nodes = vec![false; node_count];
    let mut cycle_edges = vec![false; edges.len()];

    for edge in edges {
        if edge.from_node == edge.to_node || reachability[edge.to_node][edge.from_node] {
            cycle_edges[edge.source_index] = true;
            cyclic_nodes[edge.from_node] = true;
            cyclic_nodes[edge.to_node] = true;
        }
    }

    (cyclic_nodes, cycle_edges)
}

fn transitive_reachability(
    node_count: usize,
    edges: &[CompiledEdge],
    outgoing_edges: &[Vec<usize>],
) -> Vec<Vec<bool>> {
    let mut reachability = vec![vec![false; node_count]; node_count];
    for start in 0..node_count {
        visit_from(start, start, edges, outgoing_edges, &mut reachability);
    }
    reachability
}

fn visit_from(
    start: usize,
    node: usize,
    edges: &[CompiledEdge],
    outgoing_edges: &[Vec<usize>],
    reachability: &mut [Vec<bool>],
) {
    for edge_index in &outgoing_edges[node] {
        let next = edges[*edge_index].to_node;
        if reachability[start][next] {
            continue;
        }
        reachability[start][next] = true;
        visit_from(start, next, edges, outgoing_edges, reachability);
    }
}

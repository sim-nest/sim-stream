//! Latency propagation for topology placement reports.

use std::collections::{BTreeMap, VecDeque};

use sim_lib_stream_core::BridgeLatency;

use crate::{
    CompiledGraph,
    place::{DomainBridge, PortLatency, SiteMap},
};

pub(crate) fn latency_budget(
    graph: &CompiledGraph,
    sites: &SiteMap,
    bridges: &[DomainBridge],
) -> Vec<PortLatency> {
    let bridge_latency = bridges
        .iter()
        .map(|bridge| (bridge.edge, bridge.descriptor.latency()))
        .collect::<BTreeMap<_, _>>();
    let node_latency = graph
        .nodes
        .iter()
        .map(|node| sites.profile_for(&node.id).latency())
        .collect::<Vec<_>>();
    let mut budgets = node_latency.clone();

    propagate_acyclic_latency(graph, &node_latency, &bridge_latency, &mut budgets);
    propagate_bounded_cycle_latency(graph, &node_latency, &bridge_latency, &mut budgets);

    graph
        .output_nodes
        .iter()
        .map(|node_index| {
            let node = &graph.nodes[*node_index];
            let profile = sites.profile_for(&node.id);
            PortLatency {
                node: node.id.clone(),
                site: sites.site_for(&node.id).clone(),
                latency: budgets[*node_index],
                latency_class: profile.rate_contract().latency_class(),
            }
        })
        .collect()
}

fn propagate_acyclic_latency(
    graph: &CompiledGraph,
    node_latency: &[BridgeLatency],
    bridge_latency: &BTreeMap<crate::EdgeId, BridgeLatency>,
    budgets: &mut [BridgeLatency],
) {
    for node_index in acyclic_topological_order(graph) {
        for edge_index in &graph.outgoing_edges[node_index] {
            if graph.cycle_edges[*edge_index] {
                continue;
            }
            relax_edge(graph, node_latency, bridge_latency, budgets, *edge_index);
        }
    }
}

fn propagate_bounded_cycle_latency(
    graph: &CompiledGraph,
    node_latency: &[BridgeLatency],
    bridge_latency: &BTreeMap<crate::EdgeId, BridgeLatency>,
    budgets: &mut [BridgeLatency],
) {
    for _ in 0..bounded_cycle_passes(graph) {
        let mut changed = false;
        for edge_index in 0..graph.edges.len() {
            changed |= relax_edge(graph, node_latency, bridge_latency, budgets, edge_index);
        }
        if !changed {
            break;
        }
    }
}

fn relax_edge(
    graph: &CompiledGraph,
    node_latency: &[BridgeLatency],
    bridge_latency: &BTreeMap<crate::EdgeId, BridgeLatency>,
    budgets: &mut [BridgeLatency],
    edge_index: usize,
) -> bool {
    let edge = &graph.edges[edge_index];
    let candidate = budgets[edge.from_node]
        .plus(
            *bridge_latency
                .get(&edge.id)
                .unwrap_or(&BridgeLatency::zero()),
        )
        .plus(node_latency[edge.to_node]);
    let updated = max_latency(budgets[edge.to_node], candidate);
    if updated != budgets[edge.to_node] {
        budgets[edge.to_node] = updated;
        return true;
    }
    false
}

fn acyclic_topological_order(graph: &CompiledGraph) -> Vec<usize> {
    let mut incoming = vec![0usize; graph.nodes.len()];
    for (edge_index, edge) in graph.edges.iter().enumerate() {
        if !graph.cycle_edges[edge_index] {
            incoming[edge.to_node] += 1;
        }
    }

    let mut queue = incoming
        .iter()
        .enumerate()
        .filter_map(|(node_index, count)| (*count == 0).then_some(node_index))
        .collect::<VecDeque<_>>();
    let mut order = Vec::with_capacity(graph.nodes.len());

    while let Some(node_index) = queue.pop_front() {
        order.push(node_index);
        for edge_index in &graph.outgoing_edges[node_index] {
            if graph.cycle_edges[*edge_index] {
                continue;
            }
            let to_node = graph.edges[*edge_index].to_node;
            incoming[to_node] -= 1;
            if incoming[to_node] == 0 {
                queue.push_back(to_node);
            }
        }
    }

    for node_index in 0..graph.nodes.len() {
        if !order.contains(&node_index) {
            order.push(node_index);
        }
    }

    order
}

fn bounded_cycle_passes(graph: &CompiledGraph) -> usize {
    graph
        .edges
        .iter()
        .enumerate()
        .filter(|(edge_index, _)| graph.cycle_edges[*edge_index])
        .map(|(_, edge)| edge.max_visits.unwrap_or(1) as usize)
        .sum()
}

fn max_latency(left: BridgeLatency, right: BridgeLatency) -> BridgeLatency {
    BridgeLatency::frames_and_packets(
        left.frame_count().max(right.frame_count()),
        left.packet_count().max(right.packet_count()),
    )
}

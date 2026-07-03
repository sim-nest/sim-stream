use sim_kernel::Result;

use crate::{error::validation_error, model::Graph};

use super::{GraphIndex, adjacency, edge_context};

pub(super) fn validate_bounded_cycles(graph: &Graph, index: &GraphIndex) -> Result<()> {
    let adjacency = adjacency(graph, index);
    let mut state = vec![VisitState::New; graph.nodes.len()];
    let mut path_nodes = Vec::new();
    let mut path_edges = Vec::new();

    for node in 0..graph.nodes.len() {
        if state[node] == VisitState::New {
            find_unbounded_cycle(
                graph,
                node,
                &adjacency,
                &mut state,
                &mut path_nodes,
                &mut path_edges,
            )?;
        }
    }

    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    New,
    Active,
    Done,
}

fn find_unbounded_cycle(
    graph: &Graph,
    node: usize,
    adjacency: &[Vec<(usize, usize)>],
    state: &mut [VisitState],
    path_nodes: &mut Vec<usize>,
    path_edges: &mut Vec<usize>,
) -> Result<()> {
    state[node] = VisitState::Active;
    path_nodes.push(node);

    for (next, edge_index) in &adjacency[node] {
        match state[*next] {
            VisitState::New => {
                path_edges.push(*edge_index);
                find_unbounded_cycle(graph, *next, adjacency, state, path_nodes, path_edges)?;
                path_edges.pop();
            }
            VisitState::Active => {
                validate_cycle_bound(graph, *next, *edge_index, path_nodes, path_edges)?
            }
            VisitState::Done => {}
        }
    }

    path_nodes.pop();
    state[node] = VisitState::Done;
    Ok(())
}

fn validate_cycle_bound(
    graph: &Graph,
    cycle_start: usize,
    back_edge: usize,
    path_nodes: &[usize],
    path_edges: &[usize],
) -> Result<()> {
    let Some(start) = path_nodes.iter().position(|node| *node == cycle_start) else {
        return Ok(());
    };

    let mut cycle_edges = path_edges[start..].to_vec();
    cycle_edges.push(back_edge);
    if cycle_edges
        .iter()
        .any(|edge_index| graph.edges[*edge_index].max_visits.unwrap_or(0) > 0)
    {
        return Ok(());
    }

    let mut cycle_nodes = path_nodes[start..]
        .iter()
        .map(|node_index| graph.nodes[*node_index].id.as_symbol().to_string())
        .collect::<Vec<_>>();
    cycle_nodes.push(graph.nodes[cycle_start].id.as_symbol().to_string());

    Err(validation_error(
        &graph.name,
        edge_context(&graph.edges[back_edge]),
        format!(
            "unbounded cycle requires a positive max_visits edge: {}",
            cycle_nodes.join(" -> ")
        ),
    ))
}

//! Run-local nonlinear topology node state.

use std::collections::{BTreeMap, VecDeque};

use sim_kernel::{Expr, Symbol};

/// Mutable nonlinear node state for one topology run.
#[derive(Clone, Debug)]
pub struct TopologyNonlinearState {
    nodes: Vec<NodeNonlinearState>,
}

impl TopologyNonlinearState {
    /// Creates empty nonlinear state for a compiled graph.
    pub fn new(node_count: usize) -> Self {
        Self {
            nodes: vec![NodeNonlinearState::default(); node_count],
        }
    }

    /// Buffers one merge value and returns the current buffer length.
    pub fn push_merge(&mut self, node_index: usize, _port: Symbol, expr: Expr) -> usize {
        let state = &mut self.nodes[node_index];
        state.merge_buffer.push_back(PortValue { expr });
        state.merge_buffer.len()
    }

    /// Drains buffered merge values in arrival order.
    pub fn drain_merge(&mut self, node_index: usize, count: usize) -> Vec<Expr> {
        let state = &mut self.nodes[node_index];
        (0..count)
            .filter_map(|_| state.merge_buffer.pop_front())
            .map(|value| value.expr)
            .collect()
    }

    /// Marks a merge-any node as complete. Returns false if already complete.
    pub fn mark_merge_any_complete(&mut self, node_index: usize) -> bool {
        let state = &mut self.nodes[node_index];
        if state.merge_any_done {
            false
        } else {
            state.merge_any_done = true;
            true
        }
    }

    /// Records a latest-value merge arrival and returns a deterministic map.
    pub fn update_latest(&mut self, node_index: usize, port: Symbol, expr: Expr) -> Expr {
        let state = &mut self.nodes[node_index];
        state.latest.insert(port, expr);
        Expr::Map(
            state
                .latest
                .iter()
                .map(|(port, expr)| (Expr::Symbol(port.clone()), expr.clone()))
                .collect(),
        )
    }

    /// Returns the current reduce accumulator or the provided initial value.
    pub fn reduce_current(&self, node_index: usize, initial: Expr) -> Expr {
        self.nodes[node_index].reduce_acc.clone().unwrap_or(initial)
    }

    /// Records the next reduce accumulator and returns the new arrival count.
    pub fn record_reduce(&mut self, node_index: usize, next: Expr) -> usize {
        let state = &mut self.nodes[node_index];
        state.reduce_acc = Some(next);
        state.reduce_count += 1;
        state.reduce_count
    }

    /// Resets reduce accumulator state for the next group.
    pub fn reset_reduce(&mut self, node_index: usize) {
        let state = &mut self.nodes[node_index];
        state.reduce_acc = None;
        state.reduce_count = 0;
    }

    /// Marks a race node as complete. Returns false if it was already complete.
    pub fn mark_race_complete(&mut self, node_index: usize) -> bool {
        let state = &mut self.nodes[node_index];
        if state.race_done {
            false
        } else {
            state.race_done = true;
            true
        }
    }

    /// Records one quorum key and returns the agreement count for that key.
    pub fn record_quorum(&mut self, node_index: usize, key: Expr, value: Expr) -> (Expr, u32) {
        let state = &mut self.nodes[node_index];
        if let Some(entry) = state
            .quorum
            .iter_mut()
            .find(|entry| entry.key.canonical_eq(&key))
        {
            entry.count = entry.count.saturating_add(1);
            return (entry.value.clone(), entry.count);
        }

        state.quorum.push(QuorumEntry {
            key,
            value: value.clone(),
            count: 1,
        });
        (value, 1)
    }

    /// Marks a quorum node as complete. Returns false if it was already complete.
    pub fn mark_quorum_complete(&mut self, node_index: usize) -> bool {
        let state = &mut self.nodes[node_index];
        if state.quorum_done {
            false
        } else {
            state.quorum_done = true;
            true
        }
    }
}

#[derive(Clone, Debug, Default)]
struct NodeNonlinearState {
    merge_buffer: VecDeque<PortValue>,
    merge_any_done: bool,
    latest: BTreeMap<Symbol, Expr>,
    reduce_acc: Option<Expr>,
    reduce_count: usize,
    race_done: bool,
    quorum_done: bool,
    quorum: Vec<QuorumEntry>,
}

#[derive(Clone, Debug)]
struct PortValue {
    expr: Expr,
}

#[derive(Clone, Debug)]
struct QuorumEntry {
    key: Expr,
    value: Expr,
    count: u32,
}

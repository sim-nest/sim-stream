//! Compact scheduler-event constructors.

use super::*;

impl TopologyEvent {
    pub(super) fn node(kind: TopologyEventKind, node_index: usize) -> Self {
        Self {
            kind,
            node_index,
            port: None,
            edge_index: None,
            expr: None,
        }
    }

    pub(super) fn node_expr(kind: TopologyEventKind, node_index: usize, expr: Expr) -> Self {
        Self {
            kind,
            node_index,
            port: None,
            edge_index: None,
            expr: Some(expr),
        }
    }

    pub(super) fn port(
        kind: TopologyEventKind,
        node_index: usize,
        port: Symbol,
        expr: Expr,
    ) -> Self {
        Self {
            kind,
            node_index,
            port: Some(port),
            edge_index: None,
            expr: Some(expr),
        }
    }

    pub(super) fn edge(node_index: usize, port: Symbol, edge_index: usize, expr: Expr) -> Self {
        Self {
            kind: TopologyEventKind::EdgeRouted,
            node_index,
            port: Some(port),
            edge_index: Some(edge_index),
            expr: Some(expr),
        }
    }
}

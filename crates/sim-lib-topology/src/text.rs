//! Tiny line-oriented topology DSL.

use sim_kernel::{Cx, Expr, Result};

use crate::{Graph, TopologyConnection, connection_from_graph};

mod data;
mod parser;
mod value;

pub use data::graph_to_expr;
pub use parser::graph_from_text;

/// Parses topology text and returns canonical graph data.
pub fn parse_text(source: &str) -> Result<Expr> {
    graph_from_text(source).map(|graph| graph_to_expr(&graph))
}

/// Parses topology text and returns a runnable topology connection.
pub fn from_text(cx: &mut Cx, source: &str) -> Result<TopologyConnection> {
    let graph: Graph = graph_from_text(source)?;
    connection_from_graph(cx, &graph)
}

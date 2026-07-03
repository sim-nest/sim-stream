//! Shared topology record helpers.
//!
//! The graph/patch/text codecs independently re-grew the same two helpers: a
//! recursive quote-unwrap (`data_expr`) and a port-reference-to-symbol encoder
//! (`port_ref_symbol`). This module is their single owner.

use sim_kernel::{Expr, Symbol};

use crate::PortRef;

/// Strip any surrounding quote wrappers, returning the inner data expression.
pub(crate) fn data_expr(expr: &Expr) -> &Expr {
    match expr {
        Expr::Quote { expr, .. } => data_expr(expr),
        other => other,
    }
}

/// Encode a port reference as a symbol: the bare node symbol when the port is
/// the default, otherwise `node:port`.
pub(crate) fn port_ref_symbol(port: &PortRef, default_port: &str) -> Symbol {
    if port.port.name.as_ref() == default_port {
        port.node.as_symbol().clone()
    } else {
        Symbol::new(format!("{}:{}", port.node.as_symbol(), port.port))
    }
}

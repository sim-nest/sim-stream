//! Shape adapters for rank grammars.
//!
//! A rank grammar is the structural contract for rank nodes. This module
//! exposes that contract through the shared kernel [`Shape`] protocol so rank
//! spaces can participate in the same matcher and overload machinery as other
//! runtime values.

use sim_kernel::{
    Cx, Expr, MatchScore, Result as KernelResult, Shape, ShapeDoc, ShapeMatch, Symbol, Value,
};

use crate::{
    RankCodec, RankGrammar, RankGroupCodec, RankNode, RankPrimitiveCodec,
    read_construct::rank_node_from_expr, space::rank_node_from_value,
};

/// Kernel [`Shape`] implementation backed by a [`RankGrammar`].
#[derive(Clone, Debug)]
pub struct RankNodeShape {
    symbol: Symbol,
    grammar: RankGrammar,
}

impl RankNodeShape {
    /// Creates a shape named `symbol` that accepts nodes matching `grammar`.
    pub fn new(symbol: Symbol, grammar: RankGrammar) -> Self {
        Self { symbol, grammar }
    }

    /// Returns the rank grammar this shape checks.
    pub fn grammar(&self) -> &RankGrammar {
        &self.grammar
    }

    fn check_node(&self, node: &RankNode) -> ShapeMatch {
        let primitive = RankPrimitiveCodec::new(self.grammar.clone()).rank_node(node);
        if primitive.is_ok() {
            return ShapeMatch::accept(MatchScore::exact(40));
        }

        let grouped = RankGroupCodec::new(self.grammar.clone()).rank_node(node);
        if grouped.is_ok() {
            return ShapeMatch::accept(MatchScore::exact(35));
        }

        ShapeMatch::reject(format!(
            "rank node does not match {} grammar: {}; grouped check: {}",
            self.grammar.kind_name(),
            primitive.unwrap_err(),
            grouped.unwrap_err()
        ))
    }
}

impl Shape for RankNodeShape {
    fn symbol(&self) -> Option<Symbol> {
        Some(self.symbol.clone())
    }

    fn check_value(&self, _cx: &mut Cx, value: Value) -> KernelResult<ShapeMatch> {
        let node = match rank_node_from_value(&value) {
            Ok(node) => node,
            Err(error) => return Ok(ShapeMatch::reject(error.to_string())),
        };
        Ok(self.check_node(&node))
    }

    fn check_expr(&self, _cx: &mut Cx, expr: &Expr) -> KernelResult<ShapeMatch> {
        let node = match rank_node_from_expr(expr.clone()) {
            Ok(node) => node,
            Err(error) => return Ok(ShapeMatch::reject(error.to_string())),
        };
        Ok(self.check_node(&node))
    }

    fn describe(&self, _cx: &mut Cx) -> KernelResult<ShapeDoc> {
        Ok(ShapeDoc::new("rank node")
            .with_detail(format!("grammar kind: {}", self.grammar.kind_name())))
    }
}

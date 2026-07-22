use std::collections::BTreeSet;

use sim_kernel::{Expr, Symbol};

use crate::{RankError, RankResult};

use super::{MAX_GENERATED_EXPRS, RankExprSpec};

pub(crate) fn atom_exprs(spec: &RankExprSpec) -> Vec<Expr> {
    let mut exprs = vec![Expr::Nil, Expr::Bool(false), Expr::Bool(true)];
    exprs.extend(spec.symbols.iter().cloned().map(Expr::Symbol));
    exprs.extend(spec.strings.iter().cloned().map(Expr::String));
    exprs
}

pub(super) fn unique_symbols(items: impl IntoIterator<Item = Symbol>) -> RankResult<Vec<Symbol>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for item in items {
        if !seen.insert(item.clone()) {
            return Err(invalid_node(format!(
                "duplicate rank expression symbol {item}"
            )));
        }
        out.push(item);
    }
    Ok(out)
}

pub(super) fn unique_strings(items: impl IntoIterator<Item = String>) -> RankResult<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for item in items {
        if !seen.insert(item.clone()) {
            return Err(invalid_node(format!(
                "duplicate rank expression string {item:?}"
            )));
        }
        out.push(item);
    }
    Ok(out)
}

pub(super) fn push_unique(out: &mut Vec<Expr>, expr: Expr) -> RankResult<()> {
    push_expr_unique(out, expr);
    if out.len() > MAX_GENERATED_EXPRS {
        return Err(invalid_node(
            "restricted rank expression space exceeds generation bound",
        ));
    }
    Ok(())
}

pub(crate) fn push_expr_unique(out: &mut Vec<Expr>, expr: Expr) {
    if !out.contains(&expr) {
        out.push(expr);
    }
}

pub(super) fn symbol_index(spec: &RankExprSpec, symbol: &Symbol) -> RankResult<usize> {
    spec.symbols
        .iter()
        .position(|candidate| candidate == symbol)
        .ok_or_else(|| {
            invalid_node(format!(
                "symbol {symbol} is outside the expression alphabet"
            ))
        })
}

pub(super) fn string_index(spec: &RankExprSpec, value: &str) -> RankResult<usize> {
    spec.strings
        .iter()
        .position(|candidate| candidate == value)
        .ok_or_else(|| {
            invalid_node(format!(
                "string {value:?} is outside the expression alphabet"
            ))
        })
}

pub(super) fn symbol_alphabet_id() -> Symbol {
    Symbol::qualified("rank-expr", "symbol")
}

pub(super) fn string_alphabet_id() -> Symbol {
    Symbol::qualified("rank-expr", "string")
}

pub(super) fn invalid_node(message: impl Into<String>) -> RankError {
    RankError::InvalidNode {
        message: message.into(),
    }
}

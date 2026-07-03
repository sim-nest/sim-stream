//! Topology predicate evaluation.

use sim_kernel::{Cx, Expr, Result};
use sim_shape::parse_shape_expr;

use crate::adapter::{call_target_expr, resolve_target};

/// Evaluates a topology predicate against one expression.
pub fn predicate_accepts(cx: &mut Cx, predicate: &Expr, input: &Expr) -> Result<bool> {
    match predicate {
        Expr::Bool(value) => return Ok(*value),
        Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == "true" => {
            return Ok(true);
        }
        Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == "false" => {
            return Ok(false);
        }
        _ => {}
    }

    if let Ok(target) = resolve_target(cx, predicate)
        && let Some(shape) = target.object().as_shape()
    {
        let value = cx.factory().expr(input.clone())?;
        return Ok(shape.check_value(cx, value)?.accepted);
    }

    if let Ok(target) = resolve_target(cx, predicate) {
        let output = call_target_expr(cx, target, input.clone())?;
        return Ok(expr_truth(&output));
    }

    let shape = parse_shape_expr(predicate)?;
    Ok(shape.check_expr(cx, input)?.accepted)
}

fn expr_truth(expr: &Expr) -> bool {
    match expr {
        Expr::Nil | Expr::Bool(false) => false,
        Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == "false" => {
            false
        }
        _ => true,
    }
}

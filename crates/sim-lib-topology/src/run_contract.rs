//! Shared runtime contract checks for topology values.

use std::fmt;

use sim_kernel::{Cx, Error, Expr, Result, ShapeRef, Value};
use sim_shape::parse_shape_expr;

/// Checks an expression against an optional topology shape expression.
pub(crate) fn check_expr_shape(
    cx: &mut Cx,
    context: impl fmt::Display,
    shape_expr: Option<&Expr>,
    value: &Expr,
) -> Result<()> {
    let Some(shape_expr) = shape_expr else {
        return Ok(());
    };
    let shape = parse_shape_expr(shape_expr)?;
    let matched = shape.check_expr(cx, value)?;
    if matched.accepted {
        Ok(())
    } else {
        Err(rejected(context))
    }
}

/// Checks a runtime value against an optional live kernel shape reference.
pub(crate) fn check_value_shape(
    cx: &mut Cx,
    context: impl fmt::Display,
    shape_ref: Option<&ShapeRef>,
    value: Value,
) -> Result<()> {
    let Some(shape_ref) = shape_ref else {
        return Ok(());
    };
    let shape = shape_ref.object().as_shape().ok_or(Error::TypeMismatch {
        expected: "shape",
        found: "non-shape",
    })?;
    let matched = shape.check_value(cx, value)?;
    if matched.accepted {
        Ok(())
    } else {
        Err(rejected(context))
    }
}

fn rejected(context: impl fmt::Display) -> Error {
    Error::Eval(format!(
        "topology runtime contract rejected value for {context}"
    ))
}

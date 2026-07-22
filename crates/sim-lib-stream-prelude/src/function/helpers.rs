use sim_kernel::{Cx, Error, Expr, NumberLiteral, Result, Symbol, Value};
use sim_value::kind::expr_kind;

use crate::handle::{RunReport, StreamHandle};

pub(super) fn handle_arg(cx: &mut Cx, expr: &Expr) -> Result<StreamHandle> {
    let value = eval_value(cx, expr)?;
    value
        .object()
        .downcast_ref::<StreamHandle>()
        .cloned()
        .ok_or(Error::TypeMismatch {
            expected: "stream handle",
            found: "non-stream-handle",
        })
}

pub(super) fn symbol_arg(cx: &mut Cx, expr: &Expr) -> Result<Symbol> {
    match literal_expr(cx, expr)? {
        Expr::Symbol(symbol) => Ok(symbol),
        Expr::String(value) => Ok(Symbol::new(value)),
        other => Err(Error::TypeMismatch {
            expected: "symbol or string",
            found: expr_kind(&other),
        }),
    }
}

pub(super) fn usize_arg(cx: &mut Cx, expr: &Expr) -> Result<usize> {
    let expr = literal_expr(cx, expr)?;
    let canonical = match expr {
        Expr::Number(NumberLiteral { canonical, .. }) | Expr::String(canonical) => canonical,
        Expr::Symbol(symbol) => symbol.to_string(),
        other => {
            return Err(Error::TypeMismatch {
                expected: "integer or integer string",
                found: expr_kind(&other),
            });
        }
    };
    canonical
        .parse::<usize>()
        .map_err(|err| Error::Eval(format!("invalid stream/window count {canonical}: {err}")))
}

pub(super) fn eval_value(cx: &mut Cx, expr: &Expr) -> Result<Value> {
    cx.eval_expr(unquote(expr))
}

pub(super) fn data_expr(cx: &mut Cx, expr: &Expr) -> Result<Expr> {
    let expr = unquote(expr);
    match expr {
        Expr::Map(_) | Expr::List(_) | Expr::Vector(_) | Expr::Bytes(_) => Ok(expr),
        other => cx.eval_expr(other)?.object().as_expr(cx),
    }
}

pub(super) fn run_report_value(cx: &mut Cx, report: RunReport) -> Result<Value> {
    cx.factory().table(vec![
        (
            Symbol::new("packets"),
            cx.factory().string(report.packets.to_string())?,
        ),
        (
            Symbol::new("written"),
            cx.factory().string(report.written.to_string())?,
        ),
    ])
}

fn literal_expr(cx: &mut Cx, expr: &Expr) -> Result<Expr> {
    let expr = unquote(expr);
    match expr {
        Expr::Nil
        | Expr::Bool(_)
        | Expr::Number(_)
        | Expr::Symbol(_)
        | Expr::String(_)
        | Expr::Bytes(_)
        | Expr::Map(_)
        | Expr::List(_)
        | Expr::Vector(_) => Ok(expr),
        other => cx.eval_expr(other)?.object().as_expr(cx),
    }
}

fn unquote(expr: &Expr) -> Expr {
    match expr {
        Expr::Quote {
            mode: sim_kernel::QuoteMode::Quote,
            expr,
        } => (**expr).clone(),
        other => other.clone(),
    }
}

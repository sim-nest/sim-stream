use sim_kernel::{Error, Expr, Result, Symbol};

use crate::{PortMode, PortRef};

pub(super) fn sequence_items<'a>(expr: &'a Expr, path: &str) -> Result<&'a [Expr]> {
    match data_expr(expr) {
        Expr::List(items) | Expr::Vector(items) => Ok(items),
        other => Err(parse_error(
            path,
            format!("expected list, found {}", expr_kind(other)),
        )),
    }
}

pub(super) fn parse_port_ref(expr: &Expr, default_port: &str, path: &str) -> Result<PortRef> {
    let text = symbolish_name(expr, path)?;
    let parts = text.split(':').collect::<Vec<_>>();
    match parts.as_slice() {
        [node] if !node.is_empty() => Ok(PortRef::named(*node, default_port)),
        [node, port] if !node.is_empty() && !port.is_empty() => Ok(PortRef::named(*node, *port)),
        _ => Err(parse_error(
            path,
            "expected port reference `node` or `node:port`",
        )),
    }
}

pub(super) fn parse_optional_expr(expr: &Expr) -> Option<Expr> {
    match data_expr(expr) {
        Expr::Nil => None,
        value => Some(value.clone()),
    }
}

pub(super) fn parse_optional_symbol(expr: &Expr, path: &str) -> Result<Option<Symbol>> {
    match data_expr(expr) {
        Expr::Nil => Ok(None),
        _ => parse_symbol(expr, path).map(Some),
    }
}

pub(super) fn parse_port_mode(expr: &Expr, path: &str) -> Result<PortMode> {
    match symbolish_name(expr, path)?.as_str() {
        "value" => Ok(PortMode::Value),
        "stream" => Ok(PortMode::Stream),
        other => Err(parse_error(
            path,
            format!("expected value or stream, found {other}"),
        )),
    }
}

pub(super) fn expect_named(expr: &Expr, expected: &str, path: &str) -> Result<()> {
    let actual = symbolish_name(expr, path)?;
    if actual == expected {
        Ok(())
    } else {
        Err(parse_error(
            path,
            format!("expected {expected}, found {actual}"),
        ))
    }
}

pub(super) fn parse_symbol(expr: &Expr, path: &str) -> Result<Symbol> {
    match data_expr(expr) {
        Expr::Symbol(symbol) => Ok(symbol.clone()),
        other => Err(parse_error(
            path,
            format!("expected symbol, found {}", expr_kind(other)),
        )),
    }
}

pub(super) fn parse_string(expr: &Expr, path: &str) -> Result<String> {
    match data_expr(expr) {
        Expr::String(value) => Ok(value.clone()),
        other => Err(parse_error(
            path,
            format!("expected string, found {}", expr_kind(other)),
        )),
    }
}

pub(super) fn parse_bool(expr: &Expr, path: &str) -> Result<bool> {
    match data_expr(expr) {
        Expr::Bool(value) => Ok(*value),
        other => Err(parse_error(
            path,
            format!("expected bool, found {}", expr_kind(other)),
        )),
    }
}

pub(super) fn parse_optional_u32(expr: &Expr, path: &str) -> Result<Option<u32>> {
    match data_expr(expr) {
        Expr::Nil => Ok(None),
        _ => parse_u32(expr, path).map(Some),
    }
}

pub(super) fn parse_optional_u64(expr: &Expr, path: &str) -> Result<Option<u64>> {
    match data_expr(expr) {
        Expr::Nil => Ok(None),
        _ => parse_u64(expr, path).map(Some),
    }
}

pub(super) fn parse_u32(expr: &Expr, path: &str) -> Result<u32> {
    parse_u64(expr, path).and_then(|value| {
        u32::try_from(value)
            .map_err(|_| parse_error(path, format!("integer {value} is out of range for u32")))
    })
}

pub(super) fn parse_u64(expr: &Expr, path: &str) -> Result<u64> {
    match data_expr(expr) {
        Expr::Number(number) => number.canonical.parse::<u64>().map_err(|_| {
            parse_error(
                path,
                format!("expected unsigned integer, found {}", number.canonical),
            )
        }),
        other => Err(parse_error(
            path,
            format!("expected unsigned integer, found {}", expr_kind(other)),
        )),
    }
}

pub(super) fn parse_i64(expr: &Expr, path: &str) -> Result<i64> {
    match data_expr(expr) {
        Expr::Number(number) => number.canonical.parse::<i64>().map_err(|_| {
            parse_error(
                path,
                format!("expected integer, found {}", number.canonical),
            )
        }),
        other => Err(parse_error(
            path,
            format!("expected integer, found {}", expr_kind(other)),
        )),
    }
}

pub(super) fn symbolish_name(expr: &Expr, path: &str) -> Result<String> {
    match data_expr(expr) {
        Expr::Symbol(symbol) => Ok(symbol.name.to_string()),
        Expr::String(value) => Ok(value.clone()),
        other => Err(parse_error(
            path,
            format!("expected symbol or string, found {}", expr_kind(other)),
        )),
    }
}

pub(super) fn parse_symbolish_key(expr: &Expr, path: &str) -> Result<Symbol> {
    match data_expr(expr) {
        Expr::Symbol(symbol) => Ok(Symbol::new(normalize_key(symbol.name.as_ref()))),
        Expr::String(value) => Ok(Symbol::new(normalize_key(value))),
        other => Err(parse_error(
            path,
            format!("expected symbol key, found {}", expr_kind(other)),
        )),
    }
}

pub(super) fn is_arrow(expr: &Expr) -> bool {
    matches!(data_expr(expr), Expr::Symbol(symbol) if symbol.name.as_ref() == "->")
}

pub(super) use crate::record::data_expr;
pub(super) use sim_value::kind::expr_kind;

pub(super) fn parse_error(path: impl AsRef<str>, message: impl Into<String>) -> Error {
    Error::Eval(format!(
        "topology parse error at {}: {}",
        path.as_ref(),
        message.into()
    ))
}

pub(super) fn normalize_key(name: &str) -> String {
    name.strip_prefix(':')
        .unwrap_or(name)
        .replace('-', "_")
        .to_owned()
}

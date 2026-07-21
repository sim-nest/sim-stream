//! Strict expression-map helpers for XR samples.

use std::collections::BTreeSet;

use sim_kernel::{Expr, Symbol};
use sim_lib_stream_device::{DeviceSampleError, DeviceSampleResult, device_sample_record_symbol};
use sim_value::access;

pub(crate) fn map_entries<'a>(
    expr: &'a Expr,
    expected: &'static str,
) -> DeviceSampleResult<&'a [(Expr, Expr)]> {
    access::map_entries(expr, expected).map_err(kernel_error)
}

pub(crate) fn expect_only_fields(
    entries: &[(Expr, Expr)],
    known: &[&str],
    context: &str,
) -> DeviceSampleResult<()> {
    let mut seen = BTreeSet::new();
    for (key, _) in entries {
        let Expr::Symbol(symbol) = key else {
            return Err(DeviceSampleError::new(format!(
                "{context} fields must use bare symbol keys"
            )));
        };
        if symbol.namespace.is_some() || !known.contains(&symbol.name.as_ref()) {
            return Err(DeviceSampleError::new(format!(
                "{context} has unexpected field {symbol}"
            )));
        }
        if !seen.insert(symbol.name.as_ref()) {
            return Err(DeviceSampleError::new(format!(
                "{context} has duplicate field {symbol}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn expect_sample_tags(
    entries: &[(Expr, Expr)],
    sample: &Symbol,
    context: &str,
) -> DeviceSampleResult<()> {
    expect_symbol_field(entries, "kind", &device_sample_record_symbol(), context)?;
    expect_symbol_field(entries, "sample", sample, context)
}

pub(crate) fn expect_symbol_field(
    entries: &[(Expr, Expr)],
    name: &str,
    expected: &Symbol,
    context: &str,
) -> DeviceSampleResult<()> {
    let actual = symbol_field(entries, name, context)?;
    if actual == expected {
        Ok(())
    } else {
        Err(DeviceSampleError::new(format!(
            "{context} field {name} must be {expected}, found {actual}"
        )))
    }
}

pub(crate) fn symbol_field<'a>(
    entries: &'a [(Expr, Expr)],
    name: &str,
    context: &str,
) -> DeviceSampleResult<&'a Symbol> {
    match field(entries, name, context)? {
        Expr::Symbol(symbol) => Ok(symbol),
        other => Err(DeviceSampleError::new(format!(
            "{context} field {name} must be a symbol, found {}",
            sim_value::kind::expr_kind(other)
        ))),
    }
}

pub(crate) fn field<'a>(
    entries: &'a [(Expr, Expr)],
    name: &str,
    context: &str,
) -> DeviceSampleResult<&'a Expr> {
    entries
        .iter()
        .find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == name => {
                Some(value)
            }
            _ => None,
        })
        .ok_or_else(|| DeviceSampleError::new(format!("{context} missing field {name}")))
}

pub(crate) fn bool_field(
    entries: &[(Expr, Expr)],
    name: &str,
    context: &str,
) -> DeviceSampleResult<bool> {
    match field(entries, name, context)? {
        Expr::Bool(value) => Ok(*value),
        other => Err(DeviceSampleError::new(format!(
            "{context} field {name} must be a bool, found {}",
            sim_value::kind::expr_kind(other)
        ))),
    }
}

pub(crate) fn u64_field(
    entries: &[(Expr, Expr)],
    name: &str,
    context: &str,
) -> DeviceSampleResult<u64> {
    let value = field(entries, name, context)?;
    let Expr::Number(number) = value else {
        return Err(DeviceSampleError::new(format!(
            "{context} field {name} must be a u64 number, found {}",
            sim_value::kind::expr_kind(value)
        )));
    };
    if !matches!(number.domain.name.as_ref(), "i64" | "u64") {
        return Err(DeviceSampleError::new(format!(
            "{context} field {name} must use an integer domain, found {}",
            number.domain
        )));
    }
    number
        .canonical
        .parse::<u64>()
        .map_err(|err| DeviceSampleError::new(format!("invalid {context} field {name}: {err}")))
}

pub(crate) fn u32_field(
    entries: &[(Expr, Expr)],
    name: &str,
    context: &str,
) -> DeviceSampleResult<u32> {
    u64_field(entries, name, context)?
        .try_into()
        .map_err(|_| DeviceSampleError::new(format!("{context} field {name} is out of range")))
}

pub(crate) fn u16_field(
    entries: &[(Expr, Expr)],
    name: &str,
    context: &str,
) -> DeviceSampleResult<u16> {
    u64_field(entries, name, context)?
        .try_into()
        .map_err(|_| DeviceSampleError::new(format!("{context} field {name} is out of range")))
}

pub(crate) fn u8_field(
    entries: &[(Expr, Expr)],
    name: &str,
    context: &str,
) -> DeviceSampleResult<u8> {
    u64_field(entries, name, context)?
        .try_into()
        .map_err(|_| DeviceSampleError::new(format!("{context} field {name} is out of range")))
}

pub(crate) fn f64_array_field<const N: usize>(
    entries: &[(Expr, Expr)],
    name: &str,
    context: &str,
) -> DeviceSampleResult<[f64; N]> {
    f64_array_expr(field(entries, name, context)?, name, context)
}

pub(crate) fn optional_f64_array_field<const N: usize>(
    entries: &[(Expr, Expr)],
    name: &str,
    context: &str,
) -> DeviceSampleResult<Option<[f64; N]>> {
    match field(entries, name, context)? {
        Expr::Nil => Ok(None),
        expr => f64_array_expr(expr, name, context).map(Some),
    }
}

pub(crate) fn f64_array_expr<const N: usize>(
    expr: &Expr,
    name: &str,
    context: &str,
) -> DeviceSampleResult<[f64; N]> {
    let Expr::Vector(items) = expr else {
        return Err(DeviceSampleError::new(format!(
            "{context} field {name} must be a {N}-item vector, found {}",
            sim_value::kind::expr_kind(expr)
        )));
    };
    if items.len() != N {
        return Err(DeviceSampleError::new(format!(
            "{context} field {name} must contain {N} items"
        )));
    }
    let values = items
        .iter()
        .map(|item| f64_expr(item, name, context))
        .collect::<DeviceSampleResult<Vec<_>>>()?;
    values
        .try_into()
        .map_err(|_| DeviceSampleError::new(format!("{context} field {name} is malformed")))
}

pub(crate) fn f64_vector(values: &[f64]) -> Expr {
    sim_value::build::vector(
        values
            .iter()
            .copied()
            .map(sim_value::build::float)
            .collect(),
    )
}

fn f64_expr(expr: &Expr, name: &str, context: &str) -> DeviceSampleResult<f64> {
    let Expr::Number(number) = expr else {
        return Err(DeviceSampleError::new(format!(
            "{context} field {name} entries must be f64 numbers, found {}",
            sim_value::kind::expr_kind(expr)
        )));
    };
    if number.domain.name.as_ref() != "f64" {
        return Err(DeviceSampleError::new(format!(
            "{context} field {name} entries must use f64 domain, found {}",
            number.domain
        )));
    }
    let value = number.canonical.parse::<f64>().map_err(|err| {
        DeviceSampleError::new(format!("invalid {context} field {name} entry: {err}"))
    })?;
    if !value.is_finite() {
        return Err(DeviceSampleError::new(format!(
            "{context} field {name} entries must be finite"
        )));
    }
    Ok(value)
}

pub(crate) fn kernel_error(error: sim_kernel::Error) -> DeviceSampleError {
    DeviceSampleError::new(error.to_string())
}

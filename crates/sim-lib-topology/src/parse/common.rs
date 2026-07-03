use sim_kernel::{Expr, Result, Symbol};

use crate::GraphTest;

use super::{
    fields::Fields,
    util::{data_expr, expr_kind, parse_error, parse_symbol, parse_symbolish_key, sequence_items},
};

pub(super) fn parse_tests(expr: &Expr, path: &str) -> Result<Vec<GraphTest>> {
    sequence_items(expr, path)?
        .iter()
        .enumerate()
        .map(|(index, item)| parse_test(item, &format!("{path}[{index}]")))
        .collect()
}

pub(super) fn parse_field_set<'a>(expr: &'a Expr, path: &str) -> Result<Fields<'a>> {
    match data_expr(expr) {
        Expr::Map(entries) => Fields::from_map(entries, path),
        Expr::List(items) | Expr::Vector(items) => Fields::from_keywords(items, path),
        other => Err(parse_error(
            path,
            format!("expected map or keyword list, found {}", expr_kind(other)),
        )),
    }
}

pub(super) fn parse_symbol_list(expr: &Expr, path: &str) -> Result<Vec<Symbol>> {
    sequence_items(expr, path)?
        .iter()
        .enumerate()
        .map(|(index, item)| parse_symbol(item, &format!("{path}[{index}]")))
        .collect()
}

pub(super) fn parse_symbol_expr_map(expr: &Expr, path: &str) -> Result<Vec<(Symbol, Expr)>> {
    match data_expr(expr) {
        Expr::Map(entries) => entries
            .iter()
            .map(|(key, value)| Ok((parse_symbolish_key(key, path)?, data_expr(value).clone())))
            .collect(),
        Expr::List(items) | Expr::Vector(items) => {
            Fields::from_keywords(items, path).map(Fields::into_pairs)
        }
        other => Err(parse_error(
            path,
            format!("expected map or keyword list, found {}", expr_kind(other)),
        )),
    }
}

fn parse_test(expr: &Expr, path: &str) -> Result<GraphTest> {
    let fields = parse_field_set(expr, path)?;
    let name = parse_symbol(
        fields.required("name", &format!("{path}.name"))?,
        &format!("{path}.name"),
    )?;
    let input = fields.required("input", &format!("{path}.input"))?.clone();
    let expect = fields
        .required("expect", &format!("{path}.expect"))?
        .clone();
    let mut test = GraphTest::new(name, input, expect);
    if let Some(fixtures) = fields.get("fixtures") {
        test.fixtures = parse_symbol_expr_map(fixtures, &format!("{path}.fixtures"))?;
    }
    fields.reject_unknown(&["name", "input", "expect", "fixtures"])?;
    Ok(test)
}

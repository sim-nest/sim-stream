//! Conversions between rank nodes/coordinates and kernel expressions.
//!
//! Encodes a [`RankNode`] or coordinate into the kernel `Expr` form that
//! constructs runtime objects, and decodes the same extension form back into a
//! node.

use std::str::FromStr;

use num_bigint::BigInt;
use sim_kernel::{ContentId, Cx, Datum, DatumStore, Error, NumberLiteral, Result, Symbol, Value};

use crate::{Nat, RankNode};

/// Returns the constructor arguments encoding `node` as a kernel expression.
pub fn rank_node_constructor_args(node: &RankNode) -> Vec<sim_kernel::Expr> {
    vec![rank_node_expr(node)]
}

/// Returns the constructor arguments encoding a coordinate (space + ordinal).
pub fn rank_coordinate_constructor_args(
    cx: &mut Cx,
    coordinate: &sim_kernel::Coordinate,
) -> Result<Vec<sim_kernel::Expr>> {
    Ok(vec![
        sim_kernel::Expr::Symbol(coordinate.space.clone()),
        nat_expr(&nat_from_content_id(cx, &coordinate.ordinal)?),
    ])
}

pub(crate) fn rank_node_from_expr(expr: sim_kernel::Expr) -> Result<RankNode> {
    use sim_kernel::Expr;
    let Expr::Extension { tag, payload } = expr else {
        return Err(type_error("rank node extension", "non-extension"));
    };
    if tag != rank_node_tag() {
        return Err(Error::Eval(format!(
            "expected rank/node extension, found {tag}"
        )));
    }
    let Expr::Map(entries) = *payload else {
        return Err(type_error("rank node map", "non-map"));
    };
    let kind = symbol_field(&entries, "kind")?;
    match kind.name.as_ref() {
        "unit" => Ok(RankNode::Unit),
        "nat" => Ok(RankNode::Nat(nat_field(&entries, "value")?)),
        "int" => Ok(RankNode::Int(int_field(&entries, "value")?)),
        "bool" => Ok(RankNode::Bool(bool_field(&entries, "value")?)),
        "enum" => Ok(RankNode::Enum {
            id: symbol_field(&entries, "id")?,
            index: nat_field(&entries, "index")?,
        }),
        "ref" => Ok(RankNode::Ref {
            space: symbol_field(&entries, "space")?,
            ordinal: nat_field(&entries, "ordinal")?,
        }),
        "sum" => Ok(RankNode::sum(
            u32::try_from(u64_field(&entries, "tag")?)
                .map_err(|_| Error::Eval("rank sum tag does not fit u32".to_owned()))?,
            rank_node_from_expr(required_field(&entries, "value")?.clone())?,
        )),
        "product" => Ok(RankNode::Product(node_items(&entries)?)),
        "list" => Ok(RankNode::List(node_items(&entries)?)),
        "set" => Ok(RankNode::Set(node_items(&entries)?)),
        "map" => Ok(RankNode::Map(map_items(&entries)?)),
        other => Err(Error::Eval(format!("unknown rank node kind {other}"))),
    }
}

pub(crate) fn value_exprs(cx: &mut Cx, args: Vec<Value>) -> Result<Vec<sim_kernel::Expr>> {
    args.into_iter()
        .map(|value| value.object().as_expr(cx))
        .collect()
}

pub(crate) fn nat_from_value(cx: &mut Cx, value: &Value) -> Result<Nat> {
    match value.object().as_expr(cx)? {
        sim_kernel::Expr::Number(number) => Nat::from_number_literal(&number).map_err(Error::from),
        sim_kernel::Expr::String(value) => value.parse::<Nat>().map_err(Error::from),
        _ => Err(type_error("rank ordinal", "non-number")),
    }
}

pub(crate) fn usize_from_value(cx: &mut Cx, value: &Value, context: &'static str) -> Result<usize> {
    nat_from_value(cx, value)?
        .to_decimal_string()
        .parse::<usize>()
        .map_err(|_| Error::Eval(format!("{context} does not fit usize")))
}

pub(crate) fn u64_from_value(cx: &mut Cx, value: &Value, context: &'static str) -> Result<u64> {
    nat_from_value(cx, value)?
        .to_decimal_string()
        .parse::<u64>()
        .map_err(|_| Error::Eval(format!("{context} does not fit u64")))
}

pub(crate) fn symbol_from_value(
    cx: &mut Cx,
    value: &Value,
    context: &'static str,
) -> Result<Symbol> {
    match value.object().as_expr(cx)? {
        sim_kernel::Expr::Symbol(symbol) => Ok(symbol),
        _ => Err(type_error(context, "non-symbol")),
    }
}

fn rank_node_expr(node: &RankNode) -> sim_kernel::Expr {
    use sim_kernel::Expr;
    match node {
        RankNode::Unit => node_expr("unit", Vec::new()),
        RankNode::Nat(value) => node_expr("nat", vec![(field("value"), nat_expr(value))]),
        RankNode::Int(value) => node_expr("int", vec![(field("value"), int_expr(value))]),
        RankNode::Bool(value) => node_expr("bool", vec![(field("value"), Expr::Bool(*value))]),
        RankNode::Enum { id, index } => node_expr(
            "enum",
            vec![
                (field("id"), Expr::Symbol(id.clone())),
                (field("index"), nat_expr(index)),
            ],
        ),
        RankNode::Ref { space, ordinal } => node_expr(
            "ref",
            vec![
                (field("space"), Expr::Symbol(space.clone())),
                (field("ordinal"), nat_expr(ordinal)),
            ],
        ),
        RankNode::Sum { tag, value } => node_expr(
            "sum",
            vec![
                (field("tag"), u64_expr(u64::from(*tag))),
                (field("value"), rank_node_expr(value)),
            ],
        ),
        RankNode::Product(values) => {
            node_expr("product", vec![(field("items"), node_list(values))])
        }
        RankNode::List(values) => node_expr("list", vec![(field("items"), node_list(values))]),
        RankNode::Set(values) => node_expr("set", vec![(field("items"), node_list(values))]),
        RankNode::Map(entries) => node_expr(
            "map",
            vec![(
                field("entries"),
                Expr::List(
                    entries
                        .iter()
                        .map(|(key, value)| {
                            Expr::Map(vec![
                                (Expr::Symbol(field("key")), rank_node_expr(key)),
                                (Expr::Symbol(field("value")), rank_node_expr(value)),
                            ])
                        })
                        .collect(),
                ),
            )],
        ),
    }
}

fn node_expr(name: &'static str, mut fields: Vec<(Symbol, sim_kernel::Expr)>) -> sim_kernel::Expr {
    use sim_kernel::Expr;
    fields.insert(
        0,
        (
            field("kind"),
            Expr::Symbol(Symbol::qualified("rank-node", name)),
        ),
    );
    Expr::Extension {
        tag: rank_node_tag(),
        payload: Box::new(Expr::Map(
            fields
                .into_iter()
                .map(|(name, value)| (Expr::Symbol(name), value))
                .collect(),
        )),
    }
}

fn node_list(values: &[RankNode]) -> sim_kernel::Expr {
    sim_kernel::Expr::List(values.iter().map(rank_node_expr).collect())
}

fn node_items(entries: &[(sim_kernel::Expr, sim_kernel::Expr)]) -> Result<Vec<RankNode>> {
    let sim_kernel::Expr::List(items) = required_field(entries, "items")? else {
        return Err(type_error("rank node list", "non-list"));
    };
    items.iter().cloned().map(rank_node_from_expr).collect()
}

fn map_items(
    entries: &[(sim_kernel::Expr, sim_kernel::Expr)],
) -> Result<Vec<(RankNode, RankNode)>> {
    let sim_kernel::Expr::List(items) = required_field(entries, "entries")? else {
        return Err(type_error("rank node map entries", "non-list"));
    };
    items
        .iter()
        .map(|item| {
            let sim_kernel::Expr::Map(pair) = item else {
                return Err(type_error("rank node map entry", "non-map"));
            };
            Ok((
                rank_node_from_expr(required_field(pair, "key")?.clone())?,
                rank_node_from_expr(required_field(pair, "value")?.clone())?,
            ))
        })
        .collect()
}

fn required_field<'a>(
    entries: &'a [(sim_kernel::Expr, sim_kernel::Expr)],
    name: &str,
) -> Result<&'a sim_kernel::Expr> {
    let key = field(name);
    entries
        .iter()
        .find_map(|(candidate, value)| {
            let sim_kernel::Expr::Symbol(candidate) = candidate else {
                return None;
            };
            (candidate == &key).then_some(value)
        })
        .ok_or_else(|| Error::Eval(format!("missing rank node field {name}")))
}

fn symbol_field(entries: &[(sim_kernel::Expr, sim_kernel::Expr)], name: &str) -> Result<Symbol> {
    match required_field(entries, name)? {
        sim_kernel::Expr::Symbol(symbol) => Ok(symbol.clone()),
        _ => Err(type_error("symbol", "non-symbol")),
    }
}

fn bool_field(entries: &[(sim_kernel::Expr, sim_kernel::Expr)], name: &str) -> Result<bool> {
    match required_field(entries, name)? {
        sim_kernel::Expr::Bool(value) => Ok(*value),
        _ => Err(type_error("bool", "non-bool")),
    }
}

fn nat_field(entries: &[(sim_kernel::Expr, sim_kernel::Expr)], name: &str) -> Result<Nat> {
    match required_field(entries, name)? {
        sim_kernel::Expr::Number(number) => Nat::from_number_literal(number).map_err(Error::from),
        sim_kernel::Expr::String(value) => value.parse::<Nat>().map_err(Error::from),
        _ => Err(type_error("rank nat", "non-number")),
    }
}

fn int_field(entries: &[(sim_kernel::Expr, sim_kernel::Expr)], name: &str) -> Result<BigInt> {
    match required_field(entries, name)? {
        sim_kernel::Expr::Number(number) if number.domain == crate::bigint_number_domain() => {
            BigInt::from_str(&number.canonical)
                .map_err(|_| Error::Eval(format!("invalid rank int {}", number.canonical)))
        }
        sim_kernel::Expr::String(value) => {
            BigInt::from_str(value).map_err(|_| Error::Eval(format!("invalid rank int {value}")))
        }
        _ => Err(type_error("rank int", "non-number")),
    }
}

fn u64_field(entries: &[(sim_kernel::Expr, sim_kernel::Expr)], name: &str) -> Result<u64> {
    let value = nat_field(entries, name)?;
    value
        .to_decimal_string()
        .parse::<u64>()
        .map_err(|_| Error::Eval(format!("rank field {name} does not fit u64")))
}

fn nat_expr(value: &Nat) -> sim_kernel::Expr {
    sim_kernel::Expr::Number(value.to_number_literal())
}

fn int_expr(value: &BigInt) -> sim_kernel::Expr {
    sim_kernel::Expr::Number(NumberLiteral {
        domain: crate::bigint_number_domain(),
        canonical: value.to_string(),
    })
}

fn u64_expr(value: u64) -> sim_kernel::Expr {
    sim_kernel::Expr::Number(NumberLiteral {
        domain: crate::bigint_number_domain(),
        canonical: value.to_string(),
    })
}

fn nat_from_content_id(cx: &mut Cx, id: &ContentId) -> Result<Nat> {
    let datum = cx
        .datum_store()
        .get(id)?
        .cloned()
        .ok_or_else(|| Error::Eval(format!("rank ordinal content id {id:?} is missing")))?;
    let Datum::Number(number) = datum else {
        return Err(Error::Eval(
            "rank coordinate ordinal is not a number datum".to_owned(),
        ));
    };
    Nat::from_number_literal(&number).map_err(Error::from)
}

fn rank_node_tag() -> Symbol {
    Symbol::qualified("rank", "node")
}

fn field(name: &str) -> Symbol {
    Symbol::new(name.to_owned())
}

fn type_error(expected: &'static str, found: &'static str) -> Error {
    Error::TypeMismatch { expected, found }
}

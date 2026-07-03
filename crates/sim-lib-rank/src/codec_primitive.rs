//! Primitive rank codec for finite, non-recursive grammars.
//!
//! Ranks units, naturals, integers, bools, enums, refs, sums, products, and
//! bounded collections directly to ordinals using mixed-radix and combinatorial
//! arithmetic, without grade grouping.

use sim_kernel::Symbol;

use crate::{
    codec::RankCodec,
    codec_finite::{
        binomial_u64, combination_rank, combination_unrank, find_len_group, find_map_len_group,
        find_set_len_group, list_offset, map_offset, mixed_rank, mixed_unrank, pow_nat, set_offset,
    },
    codec_integer::{rank_int, unrank_int},
    error::{RankError, RankResult},
    grammar::{RankAlt, RankField, RankGrammar},
    nat::Nat,
    node::RankNode,
    version::RankVersion,
};

/// Direct [`RankCodec`] for a finite, non-recursive grammar.
///
/// Computes ordinals straight from the grammar's combinatorial structure; it
/// rejects unbounded or recursive constructs rather than grouping by grade.
#[derive(Clone, Debug)]
pub struct RankPrimitiveCodec {
    grammar: RankGrammar,
}

impl RankPrimitiveCodec {
    /// Creates a primitive codec for the given grammar.
    pub fn new(grammar: RankGrammar) -> Self {
        Self { grammar }
    }
}

impl RankCodec for RankPrimitiveCodec {
    fn id(&self) -> Symbol {
        Symbol::qualified("rank-codec", "primitive")
    }

    fn version(&self) -> RankVersion {
        RankVersion::v1()
    }

    fn count(&self) -> Option<Nat> {
        count_of(&self.grammar)
    }

    fn r_ok(&self, r: &Nat) -> bool {
        self.count().is_none_or(|count| r < &count)
    }

    fn rank_node(&self, node: &RankNode) -> RankResult<Nat> {
        rank_with(&self.grammar, node)
    }

    fn unrank_node(&self, r: &Nat) -> RankResult<RankNode> {
        unrank_with(&self.grammar, r)
    }
}

fn count_of(grammar: &RankGrammar) -> Option<Nat> {
    match grammar {
        RankGrammar::Empty => Some(Nat::zero()),
        RankGrammar::Unit => Some(Nat::one()),
        RankGrammar::Nat | RankGrammar::Int | RankGrammar::Ref { .. } => None,
        RankGrammar::Bool => Some(Nat::from(2_u64)),
        RankGrammar::Enum { items, .. } => Some(Nat::from(items.len())),
        RankGrammar::RecursiveRef { .. } => None,
        RankGrammar::Sum { alts, .. } => count_sum(alts),
        RankGrammar::Product { fields, .. } => count_product(fields),
        RankGrammar::List {
            element,
            min_len,
            max_len,
            ..
        } => count_list(element, *min_len, *max_len),
        RankGrammar::Set {
            element, max_len, ..
        } => count_set(element, *max_len),
        RankGrammar::Map {
            key,
            value,
            max_len,
            ..
        } => count_map(key, value, *max_len),
        RankGrammar::Guard { inner, .. } => count_of(inner),
    }
}

fn rank_with(grammar: &RankGrammar, node: &RankNode) -> RankResult<Nat> {
    match (grammar, node) {
        (RankGrammar::Empty, _) => Err(RankError::UnsupportedCodec { kind: "empty" }),
        (RankGrammar::Unit, RankNode::Unit) => Ok(Nat::zero()),
        (RankGrammar::Nat, RankNode::Nat(value)) => Ok(value.clone()),
        (RankGrammar::Int, RankNode::Int(value)) => Ok(rank_int(value)),
        (RankGrammar::Bool, RankNode::Bool(value)) => Ok(Nat::from(u64::from(*value))),
        (RankGrammar::Enum { id, items }, RankNode::Enum { id: node_id, index })
            if id == node_id =>
        {
            let limit = Nat::from(items.len());
            require_below(index, &limit)?;
            Ok(index.clone())
        }
        (RankGrammar::Ref { id }, RankNode::Ref { space, ordinal }) if id == space => {
            Ok(ordinal.clone())
        }
        (RankGrammar::Sum { alts, .. }, RankNode::Sum { tag, value }) => {
            rank_sum(alts, *tag, value)
        }
        (RankGrammar::Product { fields, .. }, RankNode::Product(values)) => {
            rank_product(fields, values)
        }
        (
            RankGrammar::List {
                element,
                min_len,
                max_len,
                ..
            },
            RankNode::List(values),
        ) => rank_list(element, *min_len, *max_len, values),
        (
            RankGrammar::Set {
                element, max_len, ..
            },
            RankNode::Set(values),
        ) => rank_set(element, *max_len, values),
        (
            RankGrammar::Map {
                key,
                value,
                max_len,
                ..
            },
            RankNode::Map(entries),
        ) => rank_map(key, value, *max_len, entries),
        (RankGrammar::Guard { inner, .. }, _) => rank_with(inner, node),
        (expected, found) => Err(RankError::NodeGrammarMismatch {
            expected: expected.kind_name(),
            found: found.kind_name(),
        }),
    }
}

fn unrank_with(grammar: &RankGrammar, r: &Nat) -> RankResult<RankNode> {
    match grammar {
        RankGrammar::Empty => Err(RankError::OrdinalOutOfRange {
            ordinal: r.to_string(),
            count: "0".to_owned(),
        }),
        RankGrammar::Unit => {
            require_below(r, &Nat::one())?;
            Ok(RankNode::Unit)
        }
        RankGrammar::Nat => Ok(RankNode::Nat(r.clone())),
        RankGrammar::Int => Ok(RankNode::Int(unrank_int(r)?)),
        RankGrammar::Bool => {
            require_below(r, &Nat::from(2_u64))?;
            Ok(RankNode::Bool(!r.is_zero()))
        }
        RankGrammar::Enum { id, items } => {
            let count = Nat::from(items.len());
            require_below(r, &count)?;
            Ok(RankNode::Enum {
                id: id.clone(),
                index: r.clone(),
            })
        }
        RankGrammar::Ref { id } => Ok(RankNode::Ref {
            space: id.clone(),
            ordinal: r.clone(),
        }),
        RankGrammar::RecursiveRef { .. } => Err(RankError::UnsupportedCodec {
            kind: "recursive-ref",
        }),
        RankGrammar::Sum { alts, .. } => unrank_sum(alts, r),
        RankGrammar::Product { fields, .. } => unrank_product(fields, r),
        RankGrammar::List {
            element,
            min_len,
            max_len,
            ..
        } => unrank_list(element, *min_len, *max_len, r),
        RankGrammar::Set {
            element, max_len, ..
        } => unrank_set(element, *max_len, r),
        RankGrammar::Map {
            key,
            value,
            max_len,
            ..
        } => unrank_map(key, value, *max_len, r),
        RankGrammar::Guard { inner, .. } => unrank_with(inner, r),
    }
}

fn count_sum(alts: &[RankAlt]) -> Option<Nat> {
    let mut total = Nat::zero();
    for alt in alts {
        total = total.checked_add(&count_of(&alt.grammar)?);
    }
    Some(total)
}

fn count_product(fields: &[RankField]) -> Option<Nat> {
    let mut total = Nat::one();
    for field in fields {
        total = total.checked_mul(&count_of(&field.grammar)?);
    }
    Some(total)
}

fn count_list(element: &RankGrammar, min_len: u64, max_len: Option<u64>) -> Option<Nat> {
    let base = count_of(element)?;
    let max_len = max_len?;
    let mut total = Nat::zero();
    for len in min_len..=max_len {
        total = total.checked_add(&pow_nat(&base, len));
    }
    Some(total)
}

fn count_set(element: &RankGrammar, max_len: Option<u64>) -> Option<Nat> {
    let element_count = nat_to_u64(&count_of(element)?, "set element count").ok()?;
    let max_len = max_len.unwrap_or(element_count).min(element_count);
    let mut total = Nat::zero();
    for len in 0..=max_len {
        total = total.checked_add(&binomial_u64(element_count, len));
    }
    Some(total)
}

fn count_map(key: &RankGrammar, value: &RankGrammar, max_len: Option<u64>) -> Option<Nat> {
    let key_count = nat_to_u64(&count_of(key)?, "map key count").ok()?;
    let value_count = count_of(value)?;
    let max_len = max_len.unwrap_or(key_count).min(key_count);
    let mut total = Nat::zero();
    for len in 0..=max_len {
        let keys = binomial_u64(key_count, len);
        let values = pow_nat(&value_count, len);
        total = total.checked_add(&keys.checked_mul(&values));
    }
    Some(total)
}

fn rank_sum(alts: &[RankAlt], tag: u32, value: &RankNode) -> RankResult<Nat> {
    let index = usize::try_from(tag).map_err(|_| invalid_node("sum tag does not fit in usize"))?;
    let mut offset = Nat::zero();
    for (current, alt) in alts.iter().enumerate() {
        let count = require_count(&alt.grammar, "sum alternative")?;
        if current == index {
            return Ok(offset.checked_add(&rank_with(&alt.grammar, value)?));
        }
        offset = offset.checked_add(&count);
    }
    Err(invalid_node(
        "sum tag is outside the grammar alternative range",
    ))
}

fn unrank_sum(alts: &[RankAlt], r: &Nat) -> RankResult<RankNode> {
    let mut remaining = r.clone();
    for (tag, alt) in alts.iter().enumerate() {
        let count = require_count(&alt.grammar, "sum alternative")?;
        if remaining < count {
            return Ok(RankNode::sum(
                u32::try_from(tag).map_err(|_| invalid_node("sum tag does not fit in u32"))?,
                unrank_with(&alt.grammar, &remaining)?,
            ));
        }
        remaining = remaining.checked_sub(&count)?;
    }
    Err(out_of_range(r, &count_sum(alts).unwrap_or_else(Nat::zero)))
}

fn rank_product(fields: &[RankField], values: &[RankNode]) -> RankResult<Nat> {
    if fields.len() != values.len() {
        return Err(invalid_node(
            "product node arity does not match grammar fields",
        ));
    }
    let mut ranks = Vec::with_capacity(fields.len());
    let mut counts = Vec::with_capacity(fields.len());
    for (field, value) in fields.iter().zip(values) {
        let count = require_count(&field.grammar, "product field")?;
        let rank = rank_with(&field.grammar, value)?;
        require_below(&rank, &count)?;
        ranks.push(rank);
        counts.push(count);
    }
    mixed_rank(&ranks, &counts)
}

fn unrank_product(fields: &[RankField], r: &Nat) -> RankResult<RankNode> {
    let counts = fields
        .iter()
        .map(|field| require_count(&field.grammar, "product field"))
        .collect::<RankResult<Vec<_>>>()?;
    let ranks = mixed_unrank(r, &counts)?;
    let values = fields
        .iter()
        .zip(ranks)
        .map(|(field, rank)| unrank_with(&field.grammar, &rank))
        .collect::<RankResult<Vec<_>>>()?;
    Ok(RankNode::Product(values))
}

fn rank_list(
    element: &RankGrammar,
    min_len: u64,
    max_len: Option<u64>,
    values: &[RankNode],
) -> RankResult<Nat> {
    let len = checked_len(values.len())?;
    validate_len(len, min_len, max_len)?;
    let max_len = max_len.ok_or(RankError::UnsupportedCodec {
        kind: "unbounded-list",
    })?;
    let base = require_count(element, "list element")?;
    let offset = list_offset(&base, min_len, len, max_len)?;
    let ranks = values
        .iter()
        .map(|value| rank_with(element, value))
        .collect::<RankResult<Vec<_>>>()?;
    let counts = vec![base; values.len()];
    Ok(offset.checked_add(&mixed_rank(&ranks, &counts)?))
}

fn unrank_list(
    element: &RankGrammar,
    min_len: u64,
    max_len: Option<u64>,
    r: &Nat,
) -> RankResult<RankNode> {
    let max_len = max_len.ok_or(RankError::UnsupportedCodec {
        kind: "unbounded-list",
    })?;
    let base = require_count(element, "list element")?;
    let (len, remaining) = find_len_group(&base, min_len, max_len, r)?;
    let counts = vec![
        base;
        usize::try_from(len)
            .map_err(|_| invalid_node("list length does not fit in usize"))?
    ];
    let ranks = mixed_unrank(&remaining, &counts)?;
    let values = ranks
        .iter()
        .map(|rank| unrank_with(element, rank))
        .collect::<RankResult<Vec<_>>>()?;
    Ok(RankNode::List(values))
}

fn rank_set(element: &RankGrammar, max_len: Option<u64>, values: &[RankNode]) -> RankResult<Nat> {
    let element_count = require_count_u64(element, "set element count")?;
    let max_len = max_len.unwrap_or(element_count).min(element_count);
    let len = checked_len(values.len())?;
    validate_len(len, 0, Some(max_len))?;
    let mut ranks = values
        .iter()
        .map(|value| nat_to_u64(&rank_with(element, value)?, "set element rank"))
        .collect::<RankResult<Vec<_>>>()?;
    ranks.sort_unstable();
    reject_duplicate_u64(&ranks, "set node contains duplicate element ranks")?;
    let offset = set_offset(element_count, max_len, len);
    Ok(offset.checked_add(&combination_rank(element_count, &ranks)?))
}

fn unrank_set(element: &RankGrammar, max_len: Option<u64>, r: &Nat) -> RankResult<RankNode> {
    let element_count = require_count_u64(element, "set element count")?;
    let max_len = max_len.unwrap_or(element_count).min(element_count);
    let (len, remaining) = find_set_len_group(element_count, max_len, r)?;
    let ranks = combination_unrank(element_count, len, &remaining)?;
    let values = ranks
        .iter()
        .map(|rank| unrank_with(element, &Nat::from(*rank)))
        .collect::<RankResult<Vec<_>>>()?;
    Ok(RankNode::Set(values))
}

fn rank_map(
    key: &RankGrammar,
    value: &RankGrammar,
    max_len: Option<u64>,
    entries: &[(RankNode, RankNode)],
) -> RankResult<Nat> {
    let key_count = require_count_u64(key, "map key count")?;
    let value_count = require_count(value, "map value count")?;
    let max_len = max_len.unwrap_or(key_count).min(key_count);
    let len = checked_len(entries.len())?;
    validate_len(len, 0, Some(max_len))?;

    let mut ranked = Vec::with_capacity(entries.len());
    for (entry_key, entry_value) in entries {
        ranked.push((
            nat_to_u64(&rank_with(key, entry_key)?, "map key rank")?,
            rank_with(value, entry_value)?,
        ));
    }
    ranked.sort_by_key(|(key_rank, _)| *key_rank);

    let key_ranks = ranked
        .iter()
        .map(|(key_rank, _)| *key_rank)
        .collect::<Vec<_>>();
    reject_duplicate_u64(&key_ranks, "map node contains duplicate key ranks")?;
    let value_ranks = ranked
        .iter()
        .map(|(_, value_rank)| value_rank.clone())
        .collect::<Vec<_>>();
    let value_counts = vec![value_count.clone(); entries.len()];
    let value_space = pow_nat(&value_count, len);
    let group_rank = combination_rank(key_count, &key_ranks)?
        .checked_mul(&value_space)
        .checked_add(&mixed_rank(&value_ranks, &value_counts)?);
    Ok(map_offset(key_count, &value_count, max_len, len).checked_add(&group_rank))
}

fn unrank_map(
    key: &RankGrammar,
    value: &RankGrammar,
    max_len: Option<u64>,
    r: &Nat,
) -> RankResult<RankNode> {
    let key_count = require_count_u64(key, "map key count")?;
    let value_count = require_count(value, "map value count")?;
    let max_len = max_len.unwrap_or(key_count).min(key_count);
    let (len, remaining) = find_map_len_group(key_count, &value_count, max_len, r)?;
    let value_space = pow_nat(&value_count, len);
    let (combo_rank, values_rank) = remaining.div_mod(&value_space)?;
    let key_ranks = combination_unrank(key_count, len, &combo_rank)?;
    let value_counts = vec![
        value_count;
        usize::try_from(len)
            .map_err(|_| invalid_node("map length does not fit in usize"))?
    ];
    let value_ranks = mixed_unrank(&values_rank, &value_counts)?;
    let entries = key_ranks
        .iter()
        .zip(value_ranks)
        .map(|(key_rank, value_rank)| {
            Ok((
                unrank_with(key, &Nat::from(*key_rank))?,
                unrank_with(value, &value_rank)?,
            ))
        })
        .collect::<RankResult<Vec<_>>>()?;
    Ok(RankNode::Map(entries))
}

fn require_count(grammar: &RankGrammar, kind: &'static str) -> RankResult<Nat> {
    count_of(grammar).ok_or(RankError::UnsupportedCodec { kind })
}

fn require_count_u64(grammar: &RankGrammar, kind: &'static str) -> RankResult<u64> {
    nat_to_u64(&require_count(grammar, kind)?, kind)
}

fn require_below(value: &Nat, count: &Nat) -> RankResult<()> {
    if value < count {
        Ok(())
    } else {
        Err(out_of_range(value, count))
    }
}

fn out_of_range(ordinal: &Nat, count: &Nat) -> RankError {
    RankError::OrdinalOutOfRange {
        ordinal: ordinal.to_string(),
        count: count.to_string(),
    }
}

fn nat_to_u64(value: &Nat, kind: &'static str) -> RankResult<u64> {
    value
        .to_decimal_string()
        .parse()
        .map_err(|_| RankError::UnsupportedCodec { kind })
}

fn checked_len(len: usize) -> RankResult<u64> {
    u64::try_from(len).map_err(|_| invalid_node("collection length does not fit in u64"))
}

fn validate_len(len: u64, min_len: u64, max_len: Option<u64>) -> RankResult<()> {
    if len < min_len || max_len.is_some_and(|max_len| len > max_len) {
        return Err(invalid_node(
            "collection node length is outside grammar bounds",
        ));
    }
    Ok(())
}

fn reject_duplicate_u64(values: &[u64], message: &'static str) -> RankResult<()> {
    if values.windows(2).any(|window| window[0] == window[1]) {
        return Err(invalid_node(message));
    }
    Ok(())
}

fn invalid_node(message: &'static str) -> RankError {
    RankError::InvalidNode {
        message: message.into(),
    }
}

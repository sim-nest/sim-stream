use sim_kernel::Symbol;

use crate::{
    error::{RankError, RankResult},
    grade::RankGrade,
    grammar::RankGrammar,
    nat::Nat,
};

pub(crate) fn one_at_zero(grade: RankGrade) -> Nat {
    if grade == 0 { Nat::one() } else { Nat::zero() }
}

pub(crate) fn grammar_id(grammar: &RankGrammar) -> Option<&Symbol> {
    match grammar {
        RankGrammar::Enum { id, .. }
        | RankGrammar::Sum { id, .. }
        | RankGrammar::Product { id, .. }
        | RankGrammar::List { id, .. }
        | RankGrammar::Set { id, .. }
        | RankGrammar::Map { id, .. }
        | RankGrammar::Guard { id, .. } => Some(id),
        RankGrammar::Empty
        | RankGrammar::Unit
        | RankGrammar::Nat
        | RankGrammar::Int
        | RankGrammar::Bool
        | RankGrammar::Ref { .. }
        | RankGrammar::RecursiveRef { .. } => None,
    }
}

pub(crate) fn count_key(grammar: &RankGrammar, root: Option<(&Symbol, &RankGrammar)>) -> String {
    match root {
        Some((root_id, root_grammar)) => format!("{root_id:?}:{root_grammar:?}|{grammar:?}"),
        None => format!("{grammar:?}"),
    }
}

pub(crate) fn nat_to_grade(value: &Nat) -> RankResult<RankGrade> {
    value
        .to_decimal_string()
        .parse()
        .map_err(|_| RankError::GradeOverflow {
            value: value.to_decimal_string(),
        })
}

pub(crate) fn int_to_grade(value: &num_bigint::BigInt) -> RankResult<RankGrade> {
    value
        .to_string()
        .trim_start_matches('-')
        .parse()
        .map_err(|_| RankError::GradeOverflow {
            value: value.to_string(),
        })
}

pub(crate) fn add_grade(lhs: RankGrade, rhs: RankGrade) -> RankResult<RankGrade> {
    lhs.checked_add(rhs)
        .ok_or_else(|| RankError::GradeOverflow {
            value: format!("{lhs}+{rhs}"),
        })
}

pub(crate) fn validate_node_len(len: usize, min_len: u64, max_len: Option<u64>) -> RankResult<()> {
    let len = u64::try_from(len).map_err(|_| RankError::GradeOverflow {
        value: len.to_string(),
    })?;
    if len < min_len || max_len.is_some_and(|max_len| len > max_len) {
        return Err(RankError::InvalidNode {
            message: "collection node length is outside grammar bounds".to_owned(),
        });
    }
    Ok(())
}

pub(crate) fn grammar_kind(grammar: &RankGrammar) -> &'static str {
    match grammar {
        RankGrammar::Empty => "empty",
        RankGrammar::Unit => "unit",
        RankGrammar::Nat => "nat",
        RankGrammar::Int => "int",
        RankGrammar::Bool => "bool",
        RankGrammar::Enum { .. } => "enum",
        RankGrammar::Ref { .. } => "ref",
        RankGrammar::RecursiveRef { .. } => "recursive-ref",
        RankGrammar::Sum { .. } => "sum",
        RankGrammar::Product { .. } => "product",
        RankGrammar::List { .. } => "list",
        RankGrammar::Set { .. } => "set",
        RankGrammar::Map { .. } => "map",
        RankGrammar::Guard { .. } => "guard",
    }
}

use num_bigint::{BigInt, Sign};
use sim_kernel::Symbol;

use crate::{
    codec_group_list::{rank_list, unrank_list},
    codec_group_product::{rank_product, unrank_product},
    error::{RankError, RankResult},
    grade::RankGrade,
    grade_compile::GradeCompiler,
    grammar::{RankAlt, RankGrammar},
    limits::RankLimits,
    nat::Nat,
    node::RankNode,
};

pub(crate) fn rank_in_group_with(
    grammar: &RankGrammar,
    node: &RankNode,
    grade: RankGrade,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<Nat> {
    match (grammar, node) {
        (RankGrammar::Empty, _) => Err(RankError::UnsupportedCodec { kind: "empty" }),
        (RankGrammar::Unit, RankNode::Unit) => zero_rank_at_grade(grade),
        (RankGrammar::Nat, RankNode::Nat(value)) if value == &Nat::from(grade) => Ok(Nat::zero()),
        (RankGrammar::Nat, RankNode::Nat(_)) => Err(invalid_node("nat node grade mismatch")),
        (RankGrammar::Int, RankNode::Int(value)) => rank_int_at_grade(value, grade),
        (RankGrammar::Bool, RankNode::Bool(value)) => {
            zero_rank_at_grade(grade)?;
            Ok(Nat::from(u64::from(*value)))
        }
        (RankGrammar::Enum { items, .. }, RankNode::Enum { index, .. }) => {
            zero_rank_at_grade(grade)?;
            require_below(index, &Nat::from(items.len()))?;
            Ok(index.clone())
        }
        (RankGrammar::Ref { id }, RankNode::Ref { space, ordinal }) if id == space => {
            if ordinal == &Nat::from(grade) {
                Ok(Nat::zero())
            } else {
                Err(invalid_node("ref node grade mismatch"))
            }
        }
        (RankGrammar::RecursiveRef { id }, _) => {
            let Some((root_id, root_grammar)) = root else {
                return Err(RankError::UnresolvedRecursiveRef { id: id.clone() });
            };
            if id != root_id {
                return Err(RankError::UnresolvedRecursiveRef { id: id.clone() });
            }
            rank_in_group_with(root_grammar, node, grade, root, compiler)
        }
        (RankGrammar::Sum { alts, .. }, RankNode::Sum { tag, value }) => {
            rank_sum(alts, *tag, value, grade, root, compiler)
        }
        (RankGrammar::Product { fields, .. }, RankNode::Product(values)) => {
            rank_product(fields, values, grade, root, compiler)
        }
        (
            RankGrammar::List {
                element,
                min_len,
                max_len,
                ..
            },
            RankNode::List(values),
        ) => rank_list(element, *min_len, *max_len, values, grade, root, compiler),
        (RankGrammar::Guard { inner, .. }, _) => {
            rank_in_group_with(inner, node, grade, root, compiler)
        }
        (RankGrammar::Set { .. } | RankGrammar::Map { .. }, _) => {
            Err(RankError::UnsupportedCodec {
                kind: grammar.kind_name(),
            })
        }
        (expected, found) => Err(RankError::NodeGrammarMismatch {
            expected: expected.kind_name(),
            found: found.kind_name(),
        }),
    }
}

pub(crate) fn unrank_in_group_with(
    grammar: &RankGrammar,
    grade: RankGrade,
    r: &Nat,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<RankNode> {
    match grammar {
        RankGrammar::Empty => Err(RankError::UnsupportedCodec { kind: "empty" }),
        RankGrammar::Unit => {
            require_below(r, &one_at_zero(grade))?;
            Ok(RankNode::Unit)
        }
        RankGrammar::Nat => {
            require_below(r, &Nat::one())?;
            Ok(RankNode::Nat(Nat::from(grade)))
        }
        RankGrammar::Int => unrank_int_at_grade(grade, r),
        RankGrammar::Bool => {
            zero_rank_at_grade(grade)?;
            require_below(r, &Nat::from(2_u64))?;
            Ok(RankNode::Bool(!r.is_zero()))
        }
        RankGrammar::Enum { id, items } => {
            zero_rank_at_grade(grade)?;
            require_below(r, &Nat::from(items.len()))?;
            Ok(RankNode::Enum {
                id: id.clone(),
                index: r.clone(),
            })
        }
        RankGrammar::Ref { id } => {
            require_below(r, &Nat::one())?;
            Ok(RankNode::Ref {
                space: id.clone(),
                ordinal: Nat::from(grade),
            })
        }
        RankGrammar::RecursiveRef { id } => {
            let Some((root_id, root_grammar)) = root else {
                return Err(RankError::UnresolvedRecursiveRef { id: id.clone() });
            };
            if id != root_id {
                return Err(RankError::UnresolvedRecursiveRef { id: id.clone() });
            }
            unrank_in_group_with(root_grammar, grade, r, root, compiler)
        }
        RankGrammar::Sum { alts, .. } => unrank_sum(alts, grade, r, root, compiler),
        RankGrammar::Product { fields, .. } => unrank_product(fields, grade, r, root, compiler),
        RankGrammar::List {
            element,
            min_len,
            max_len,
            ..
        } => unrank_list(element, *min_len, *max_len, grade, r, root, compiler),
        RankGrammar::Guard { inner, .. } => unrank_in_group_with(inner, grade, r, root, compiler),
        RankGrammar::Set { .. } | RankGrammar::Map { .. } => Err(RankError::UnsupportedCodec {
            kind: grammar.kind_name(),
        }),
    }
}

fn rank_sum(
    alts: &[RankAlt],
    tag: u32,
    value: &RankNode,
    grade: RankGrade,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<Nat> {
    let tag = usize::try_from(tag).map_err(|_| invalid_node("sum tag does not fit in usize"))?;
    let mut offset = Nat::zero();
    for (index, alt) in alts.iter().enumerate() {
        if grade >= alt.grade_cost {
            let child_grade = grade - alt.grade_cost;
            let count = count_group(&alt.grammar, child_grade, root, compiler)?;
            if index == tag {
                let rank = rank_in_group_with(&alt.grammar, value, child_grade, root, compiler)?;
                return Ok(offset.checked_add(&rank));
            }
            offset = offset.checked_add(&count);
        } else if index == tag {
            return Err(invalid_node("sum alternative grade exceeds group grade"));
        }
    }
    Err(invalid_node(
        "sum tag is outside the grammar alternative range",
    ))
}

fn unrank_sum(
    alts: &[RankAlt],
    grade: RankGrade,
    r: &Nat,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<RankNode> {
    let mut remaining = r.clone();
    for (index, alt) in alts.iter().enumerate() {
        if grade < alt.grade_cost {
            continue;
        }
        let child_grade = grade - alt.grade_cost;
        let count = count_group(&alt.grammar, child_grade, root, compiler)?;
        if remaining < count {
            let tag =
                u32::try_from(index).map_err(|_| invalid_node("sum tag does not fit in u32"))?;
            return Ok(RankNode::sum(
                tag,
                unrank_in_group_with(&alt.grammar, child_grade, &remaining, root, compiler)?,
            ));
        }
        remaining = remaining.checked_sub(&count)?;
    }
    Err(out_of_range(
        r,
        &count_group_sum(alts, grade, root, compiler)?,
    ))
}

pub(crate) fn count_group(
    grammar: &RankGrammar,
    grade: RankGrade,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<Nat> {
    compiler.count_at_grade_with_root(grammar, grade, root)
}

fn count_group_sum(
    alts: &[RankAlt],
    grade: RankGrade,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<Nat> {
    let mut total = Nat::zero();
    for alt in alts {
        if grade >= alt.grade_cost {
            let child = count_group(&alt.grammar, grade - alt.grade_cost, root, compiler)?;
            total = total.checked_add(&child);
        }
    }
    Ok(total)
}

pub(crate) fn group_offset(
    grammar: &RankGrammar,
    grade: RankGrade,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<Nat> {
    let mut offset = Nat::zero();
    for prior_grade in 0..grade {
        offset = offset.checked_add(&count_group(grammar, prior_grade, root, compiler)?);
    }
    Ok(offset)
}

pub(crate) fn find_group(
    grammar: &RankGrammar,
    r: &Nat,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
    limits: &RankLimits,
) -> RankResult<(RankGrade, Nat)> {
    let mut remaining = r.clone();
    let mut search_limits = limits.clone();
    let mut grade = 0;
    loop {
        search_limits.consume(1, "rank.group.find-grade")?;
        let count = count_group(grammar, grade, root, compiler)?;
        if remaining < count {
            return Ok((grade, remaining));
        }
        remaining = remaining.checked_sub(&count)?;
        grade = grade
            .checked_add(1)
            .ok_or_else(|| RankError::GradeOverflow {
                value: grade.to_string(),
            })?;
    }
}

fn rank_int_at_grade(value: &BigInt, grade: RankGrade) -> RankResult<Nat> {
    if grade == 0 {
        return if value == &BigInt::from(0) {
            Ok(Nat::zero())
        } else {
            Err(invalid_node("int node grade mismatch"))
        };
    }
    if value == &int_at_grade(grade, Sign::Minus) {
        Ok(Nat::zero())
    } else if value == &int_at_grade(grade, Sign::Plus) {
        Ok(Nat::one())
    } else {
        Err(invalid_node("int node grade mismatch"))
    }
}

fn unrank_int_at_grade(grade: RankGrade, r: &Nat) -> RankResult<RankNode> {
    let count = if grade == 0 {
        Nat::one()
    } else {
        Nat::from(2_u64)
    };
    require_below(r, &count)?;
    let value = if grade == 0 {
        BigInt::from(0)
    } else if r.is_zero() {
        int_at_grade(grade, Sign::Minus)
    } else {
        int_at_grade(grade, Sign::Plus)
    };
    Ok(RankNode::Int(value))
}

fn int_at_grade(grade: RankGrade, sign: Sign) -> BigInt {
    BigInt::from_biguint(sign, Nat::from(grade).as_biguint().clone())
}

pub(crate) fn one_at_zero(grade: RankGrade) -> Nat {
    if grade == 0 { Nat::one() } else { Nat::zero() }
}

pub(crate) fn root_for(grammar: &RankGrammar) -> Option<(&Symbol, &RankGrammar)> {
    grammar_id(grammar).map(|id| (id, grammar))
}

fn grammar_id(grammar: &RankGrammar) -> Option<&Symbol> {
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

pub(crate) fn require_below(value: &Nat, count: &Nat) -> RankResult<()> {
    if value < count {
        Ok(())
    } else {
        Err(out_of_range(value, count))
    }
}

pub(crate) fn out_of_range(ordinal: &Nat, count: &Nat) -> RankError {
    RankError::OrdinalOutOfRange {
        ordinal: ordinal.to_string(),
        count: count.to_string(),
    }
}

pub(crate) fn checked_len(len: usize) -> RankResult<u64> {
    u64::try_from(len).map_err(|_| invalid_node("collection length does not fit in u64"))
}

pub(crate) fn validate_len(len: u64, min_len: u64, max_len: Option<u64>) -> RankResult<()> {
    if len < min_len || max_len.is_some_and(|max_len| len > max_len) {
        return Err(invalid_node(
            "collection node length is outside grammar bounds",
        ));
    }
    Ok(())
}

pub(crate) fn invalid_node(message: &'static str) -> RankError {
    RankError::InvalidNode {
        message: message.into(),
    }
}

fn zero_rank_at_grade(grade: RankGrade) -> RankResult<Nat> {
    require_below(&Nat::zero(), &one_at_zero(grade))?;
    Ok(Nat::zero())
}

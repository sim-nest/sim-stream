use sim_kernel::Symbol;

use crate::{
    codec_group_core::{
        checked_len, count_group, invalid_node, one_at_zero, out_of_range, rank_in_group_with,
        require_below, unrank_in_group_with, validate_len,
    },
    error::RankResult,
    grade::RankGrade,
    grade_compile::GradeCompiler,
    grammar::{RankField, RankGrammar},
    nat::Nat,
    node::RankNode,
};

pub(crate) fn rank_list(
    element: &RankGrammar,
    min_len: u64,
    max_len: Option<u64>,
    values: &[RankNode],
    grade: RankGrade,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<Nat> {
    let len = checked_len(values.len())?;
    let max = max_len.unwrap_or(grade).min(grade);
    validate_len(len, min_len, Some(max))?;
    if grade < len {
        return Err(invalid_node("list length exceeds group grade"));
    }
    let mut offset = Nat::zero();
    for prior_len in min_len..len {
        offset = offset.checked_add(&count_repeated(
            element,
            prior_len,
            grade - prior_len,
            root,
            compiler,
        )?);
    }
    Ok(offset.checked_add(&rank_repeated(
        element,
        values,
        grade - len,
        root,
        compiler,
    )?))
}

pub(crate) fn unrank_list(
    element: &RankGrammar,
    min_len: u64,
    max_len: Option<u64>,
    grade: RankGrade,
    r: &Nat,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<RankNode> {
    let max = max_len.unwrap_or(grade).min(grade);
    let mut remaining = r.clone();
    for len in min_len..=max {
        let count = count_repeated(element, len, grade - len, root, compiler)?;
        if remaining < count {
            return Ok(RankNode::List(unrank_repeated(
                element,
                len,
                grade - len,
                &remaining,
                root,
                compiler,
            )?));
        }
        remaining = remaining.checked_sub(&count)?;
    }
    Err(out_of_range(
        r,
        &count_list_group(element, min_len, max_len, grade, root, compiler)?,
    ))
}

fn rank_repeated(
    element: &RankGrammar,
    values: &[RankNode],
    grade: RankGrade,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<Nat> {
    let Some((first, rest)) = values.split_first() else {
        return zero_rank_at_grade(grade);
    };
    let first_grade = compiler.grade_of_node_with_root(element, first, root)?;
    if first_grade > grade {
        return Err(invalid_node("list element grade exceeds group grade"));
    }
    let mut offset = Nat::zero();
    for prior_grade in 0..first_grade {
        let first_count = count_group(element, prior_grade, root, compiler)?;
        let rest_count = count_repeated(
            element,
            checked_len(rest.len())?,
            grade - prior_grade,
            root,
            compiler,
        )?;
        offset = offset.checked_add(&first_count.checked_mul(&rest_count));
    }
    let rest_grade = grade - first_grade;
    let first_rank = rank_in_group_with(element, first, first_grade, root, compiler)?;
    let rest_rank = rank_repeated(element, rest, rest_grade, root, compiler)?;
    let rest_count = count_repeated(
        element,
        checked_len(rest.len())?,
        rest_grade,
        root,
        compiler,
    )?;
    Ok(offset
        .checked_add(&first_rank.checked_mul(&rest_count))
        .checked_add(&rest_rank))
}

fn unrank_repeated(
    element: &RankGrammar,
    len: u64,
    grade: RankGrade,
    r: &Nat,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<Vec<RankNode>> {
    if len == 0 {
        require_below(r, &one_at_zero(grade))?;
        return Ok(Vec::new());
    }
    let mut remaining = r.clone();
    for first_grade in 0..=grade {
        let first_count = count_group(element, first_grade, root, compiler)?;
        let rest_count = count_repeated(element, len - 1, grade - first_grade, root, compiler)?;
        let group_count = first_count.checked_mul(&rest_count);
        if group_count.is_zero() {
            continue;
        }
        if remaining < group_count {
            let (first_rank, rest_rank) = remaining.div_mod(&rest_count)?;
            let mut values = vec![unrank_in_group_with(
                element,
                first_grade,
                &first_rank,
                root,
                compiler,
            )?];
            values.extend(unrank_repeated(
                element,
                len - 1,
                grade - first_grade,
                &rest_rank,
                root,
                compiler,
            )?);
            return Ok(values);
        }
        remaining = remaining.checked_sub(&group_count)?;
    }
    Err(out_of_range(
        r,
        &count_repeated(element, len, grade, root, compiler)?,
    ))
}

fn count_list_group(
    element: &RankGrammar,
    min_len: u64,
    max_len: Option<u64>,
    grade: RankGrade,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<Nat> {
    let max = max_len.unwrap_or(grade).min(grade);
    let mut total = Nat::zero();
    for len in min_len..=max {
        total = total.checked_add(&count_repeated(element, len, grade - len, root, compiler)?);
    }
    Ok(total)
}

fn count_repeated(
    element: &RankGrammar,
    len: u64,
    grade: RankGrade,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<Nat> {
    let repeated = RankGrammar::Product {
        id: Symbol::qualified("rank-group", "repeat"),
        fields: (0..len)
            .map(|_| RankField::new(Symbol::new("item"), element.clone(), 0))
            .collect(),
    };
    compiler.count_at_grade_with_root(&repeated, grade, root)
}

fn zero_rank_at_grade(grade: RankGrade) -> RankResult<Nat> {
    require_below(&Nat::zero(), &one_at_zero(grade))?;
    Ok(Nat::zero())
}

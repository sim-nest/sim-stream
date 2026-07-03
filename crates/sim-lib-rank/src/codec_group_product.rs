use sim_kernel::Symbol;

use crate::{
    codec_group_core::{
        count_group, invalid_node, one_at_zero, out_of_range, rank_in_group_with, require_below,
        unrank_in_group_with,
    },
    error::RankResult,
    grade::RankGrade,
    grade_compile::GradeCompiler,
    grammar::{RankField, RankGrammar},
    nat::Nat,
    node::RankNode,
};

pub(crate) fn rank_product(
    fields: &[RankField],
    values: &[RankNode],
    grade: RankGrade,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<Nat> {
    if fields.len() != values.len() {
        return Err(invalid_node(
            "product node arity does not match grammar fields",
        ));
    }
    rank_product_from(fields, values, 0, grade, root, compiler)
}

fn rank_product_from(
    fields: &[RankField],
    values: &[RankNode],
    index: usize,
    grade: RankGrade,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<Nat> {
    let Some(field) = fields.get(index) else {
        return zero_rank_at_grade(grade);
    };
    if grade < field.grade_cost {
        return Err(invalid_node("product field grade cost exceeds group grade"));
    }
    let remaining = grade - field.grade_cost;
    let child_grade = compiler.grade_of_node_with_root(&field.grammar, &values[index], root)?;
    if child_grade > remaining {
        return Err(invalid_node("product child grade exceeds group grade"));
    }

    let mut offset = Nat::zero();
    for prior_grade in 0..child_grade {
        let child_count = count_group(&field.grammar, prior_grade, root, compiler)?;
        let rest_count =
            count_product_suffix(fields, index + 1, remaining - prior_grade, root, compiler)?;
        offset = offset.checked_add(&child_count.checked_mul(&rest_count));
    }

    let rest_grade = remaining - child_grade;
    let child_rank =
        rank_in_group_with(&field.grammar, &values[index], child_grade, root, compiler)?;
    let rest_rank = rank_product_from(fields, values, index + 1, rest_grade, root, compiler)?;
    let rest_count = count_product_suffix(fields, index + 1, rest_grade, root, compiler)?;
    Ok(offset
        .checked_add(&child_rank.checked_mul(&rest_count))
        .checked_add(&rest_rank))
}

pub(crate) fn unrank_product(
    fields: &[RankField],
    grade: RankGrade,
    r: &Nat,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<RankNode> {
    Ok(RankNode::Product(unrank_product_from(
        fields, 0, grade, r, root, compiler,
    )?))
}

fn unrank_product_from(
    fields: &[RankField],
    index: usize,
    grade: RankGrade,
    r: &Nat,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<Vec<RankNode>> {
    let Some(field) = fields.get(index) else {
        require_below(r, &one_at_zero(grade))?;
        return Ok(Vec::new());
    };
    if grade < field.grade_cost {
        return Err(out_of_range(r, &Nat::zero()));
    }
    let remaining_grade = grade - field.grade_cost;
    let mut remaining_rank = r.clone();
    for child_grade in 0..=remaining_grade {
        let child_count = count_group(&field.grammar, child_grade, root, compiler)?;
        let rest_grade = remaining_grade - child_grade;
        let rest_count = count_product_suffix(fields, index + 1, rest_grade, root, compiler)?;
        let group_count = child_count.checked_mul(&rest_count);
        if group_count.is_zero() {
            continue;
        }
        if remaining_rank < group_count {
            let (child_rank, rest_rank) = remaining_rank.div_mod(&rest_count)?;
            let mut values = vec![unrank_in_group_with(
                &field.grammar,
                child_grade,
                &child_rank,
                root,
                compiler,
            )?];
            values.extend(unrank_product_from(
                fields,
                index + 1,
                rest_grade,
                &rest_rank,
                root,
                compiler,
            )?);
            return Ok(values);
        }
        remaining_rank = remaining_rank.checked_sub(&group_count)?;
    }
    Err(out_of_range(
        r,
        &count_product_suffix(fields, index, grade, root, compiler)?,
    ))
}

fn count_product_suffix(
    fields: &[RankField],
    index: usize,
    grade: RankGrade,
    root: Option<(&Symbol, &RankGrammar)>,
    compiler: &mut GradeCompiler,
) -> RankResult<Nat> {
    let suffix = RankGrammar::Product {
        id: Symbol::qualified("rank-group", "product-suffix"),
        fields: fields.get(index..).unwrap_or_default().to_vec(),
    };
    compiler.count_at_grade_with_root(&suffix, grade, root)
}

fn zero_rank_at_grade(grade: RankGrade) -> RankResult<Nat> {
    require_below(&Nat::zero(), &one_at_zero(grade))?;
    Ok(Nat::zero())
}

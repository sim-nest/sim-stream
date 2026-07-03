//! Grade compiler for rankable spaces.
//!
//! [`GradeCompiler`] computes per-grade value counts and the grade of a given
//! node over a [`RankGrammar`], memoizing intermediate results and guarding
//! against unproductive recursion and resource-limit overruns.

use std::collections::{BTreeMap, BTreeSet};

use sim_kernel::Symbol;

use crate::{
    error::{RankError, RankResult},
    grade::{GradeMemoStats, RankGrade},
    grade_util::{
        add_grade, count_key, grammar_id, grammar_kind, int_to_grade, nat_to_grade, one_at_zero,
        validate_node_len,
    },
    grammar::{RankField, RankGrammar},
    limits::RankLimits,
    nat::Nat,
    node::RankNode,
};

/// Counts and grades values of a [`RankGrammar`], with memoization and limits.
#[derive(Clone, Debug)]
pub struct GradeCompiler {
    limits: RankLimits,
    count_memo: BTreeMap<(String, RankGrade), Nat>,
    grade_memo: BTreeMap<(String, String), RankGrade>,
    active_counts: BTreeSet<(String, RankGrade)>,
    count_hits: u64,
    grade_hits: u64,
}

impl GradeCompiler {
    /// Builds a compiler bounded by the given resource limits.
    pub fn new(limits: RankLimits) -> Self {
        Self {
            limits,
            count_memo: BTreeMap::new(),
            grade_memo: BTreeMap::new(),
            active_counts: BTreeSet::new(),
            count_hits: 0,
            grade_hits: 0,
        }
    }

    /// Returns the resource limits in force for this compiler.
    pub fn limits(&self) -> &RankLimits {
        &self.limits
    }

    pub(crate) fn reset_limits(&mut self, limits: RankLimits) {
        self.limits = limits;
    }

    /// Returns memoization statistics accumulated so far.
    pub fn memo_stats(&self) -> GradeMemoStats {
        GradeMemoStats {
            count_entries: self.count_memo.len(),
            grade_entries: self.grade_memo.len(),
            count_hits: self.count_hits,
            grade_hits: self.grade_hits,
        }
    }

    /// Counts the values of `grammar` at exactly `grade`.
    pub fn count_at_grade(&mut self, grammar: &RankGrammar, grade: RankGrade) -> RankResult<Nat> {
        let root = grammar_id(grammar).map(|id| (id, grammar));
        self.count_inner(grammar, grade, root)
    }

    pub(crate) fn count_at_grade_with_root(
        &mut self,
        grammar: &RankGrammar,
        grade: RankGrade,
        root: Option<(&Symbol, &RankGrammar)>,
    ) -> RankResult<Nat> {
        self.count_inner(grammar, grade, root)
    }

    /// Reports whether the count at `grade` is finite (computes successfully).
    pub fn count_is_finite_at_grade(
        &mut self,
        grammar: &RankGrammar,
        grade: RankGrade,
    ) -> RankResult<bool> {
        self.count_at_grade(grammar, grade).map(|_| true)
    }

    /// Computes the grade of `node` interpreted under `grammar`.
    pub fn grade_of_node(
        &mut self,
        grammar: &RankGrammar,
        node: &RankNode,
    ) -> RankResult<RankGrade> {
        self.grade_inner(grammar, node, grammar_id(grammar).map(|id| (id, grammar)))
    }

    pub(crate) fn grade_of_node_with_root(
        &mut self,
        grammar: &RankGrammar,
        node: &RankNode,
        root: Option<(&Symbol, &RankGrammar)>,
    ) -> RankResult<RankGrade> {
        self.grade_inner(grammar, node, root)
    }

    fn count_inner(
        &mut self,
        grammar: &RankGrammar,
        grade: RankGrade,
        root: Option<(&Symbol, &RankGrammar)>,
    ) -> RankResult<Nat> {
        let key = (count_key(grammar, root), grade);
        if let Some(cached) = self.count_memo.get(&key) {
            self.count_hits += 1;
            return Ok(cached.clone());
        }
        self.limits.consume(1, "rank.grade.count")?;
        if !self.active_counts.insert(key.clone()) {
            return Err(RankError::UnproductiveRecursion {
                id: grammar_id(grammar)
                    .cloned()
                    .unwrap_or_else(|| Symbol::new("unknown")),
            });
        }

        let result = self.count_uncached(grammar, grade, root);
        self.active_counts.remove(&key);
        let result = result?;
        self.limits.check_nat(&result, "rank.grade.count")?;
        self.count_memo.insert(key, result.clone());
        Ok(result)
    }

    fn count_uncached(
        &mut self,
        grammar: &RankGrammar,
        grade: RankGrade,
        root: Option<(&Symbol, &RankGrammar)>,
    ) -> RankResult<Nat> {
        match grammar {
            RankGrammar::Empty => Ok(Nat::zero()),
            RankGrammar::Unit => Ok(one_at_zero(grade)),
            RankGrammar::Nat => Ok(Nat::one()),
            RankGrammar::Int if grade == 0 => Ok(Nat::one()),
            RankGrammar::Int => Ok(Nat::from(2_u64)),
            RankGrammar::Bool => Ok(if grade == 0 {
                Nat::from(2_u64)
            } else {
                Nat::zero()
            }),
            RankGrammar::Enum { items, .. } => Ok(if grade == 0 {
                Nat::from(items.len())
            } else {
                Nat::zero()
            }),
            RankGrammar::Ref { .. } => Ok(Nat::one()),
            RankGrammar::RecursiveRef { id } => {
                let Some((root_id, root_grammar)) = root else {
                    return Err(RankError::UnresolvedRecursiveRef { id: id.clone() });
                };
                if id != root_id {
                    return Err(RankError::UnresolvedRecursiveRef { id: id.clone() });
                }
                self.count_inner(root_grammar, grade, root)
            }
            RankGrammar::Sum { alts, .. } => {
                let mut total = Nat::zero();
                for alt in alts {
                    if grade >= alt.grade_cost {
                        let child = self.count_inner(&alt.grammar, grade - alt.grade_cost, root)?;
                        total = total.checked_add(&child);
                    }
                }
                Ok(total)
            }
            RankGrammar::Product { fields, .. } => self.count_product(fields, grade, root),
            RankGrammar::List {
                element,
                min_len,
                max_len,
                ..
            } => self.count_list(element, *min_len, *max_len, grade, root),
            RankGrammar::Set {
                element, max_len, ..
            } => self.count_list(element, 0, *max_len, grade, root),
            RankGrammar::Map {
                key,
                value,
                max_len,
                ..
            } => {
                let entry = RankGrammar::Product {
                    id: Symbol::qualified("rank", "map-entry"),
                    fields: vec![
                        RankField::new(Symbol::new("key"), *key.clone(), 0),
                        RankField::new(Symbol::new("value"), *value.clone(), 0),
                    ],
                };
                self.count_list(&entry, 0, *max_len, grade, root)
            }
            RankGrammar::Guard { inner, .. } => self.count_inner(inner, grade, root),
        }
    }

    fn count_product(
        &mut self,
        fields: &[RankField],
        grade: RankGrade,
        root: Option<(&Symbol, &RankGrammar)>,
    ) -> RankResult<Nat> {
        self.count_product_from(fields, 0, grade, root)
    }

    fn count_product_from(
        &mut self,
        fields: &[RankField],
        index: usize,
        grade: RankGrade,
        root: Option<(&Symbol, &RankGrammar)>,
    ) -> RankResult<Nat> {
        let Some(field) = fields.get(index) else {
            return Ok(one_at_zero(grade));
        };
        if grade < field.grade_cost {
            return Ok(Nat::zero());
        }

        let remaining = grade - field.grade_cost;
        let mut total = Nat::zero();
        for child_grade in 0..=remaining {
            let child_count = self.count_inner(&field.grammar, child_grade, root)?;
            if child_count.is_zero() {
                continue;
            }
            let rest = self.count_product_from(fields, index + 1, remaining - child_grade, root)?;
            if rest.is_zero() {
                continue;
            }
            total = total.checked_add(&child_count.checked_mul(&rest));
        }
        Ok(total)
    }

    fn count_list(
        &mut self,
        element: &RankGrammar,
        min_len: u64,
        max_len: Option<u64>,
        grade: RankGrade,
        root: Option<(&Symbol, &RankGrammar)>,
    ) -> RankResult<Nat> {
        let max_len = max_len.unwrap_or(grade).min(grade);
        let mut total = Nat::zero();
        for len in min_len..=max_len {
            let remaining = grade - len;
            total = total.checked_add(&self.count_repeated(element, len, remaining, root)?);
        }
        Ok(total)
    }

    fn count_repeated(
        &mut self,
        element: &RankGrammar,
        len: u64,
        grade: RankGrade,
        root: Option<(&Symbol, &RankGrammar)>,
    ) -> RankResult<Nat> {
        if len == 0 {
            return Ok(one_at_zero(grade));
        }

        let mut total = Nat::zero();
        for first_grade in 0..=grade {
            let first = self.count_inner(element, first_grade, root)?;
            if first.is_zero() {
                continue;
            }
            let rest = self.count_repeated(element, len - 1, grade - first_grade, root)?;
            if rest.is_zero() {
                continue;
            }
            total = total.checked_add(&first.checked_mul(&rest));
        }
        Ok(total)
    }

    fn grade_inner(
        &mut self,
        grammar: &RankGrammar,
        node: &RankNode,
        root: Option<(&Symbol, &RankGrammar)>,
    ) -> RankResult<RankGrade> {
        let key = (count_key(grammar, root), format!("{node:?}"));
        if let Some(cached) = self.grade_memo.get(&key) {
            self.grade_hits += 1;
            return Ok(*cached);
        }
        self.limits.consume(1, "rank.grade.node")?;

        let grade = self.grade_uncached(grammar, node, root)?;
        self.grade_memo.insert(key, grade);
        Ok(grade)
    }

    fn grade_uncached(
        &mut self,
        grammar: &RankGrammar,
        node: &RankNode,
        root: Option<(&Symbol, &RankGrammar)>,
    ) -> RankResult<RankGrade> {
        match (grammar, node) {
            (RankGrammar::Unit, RankNode::Unit) => Ok(0),
            (RankGrammar::Nat, RankNode::Nat(value)) => nat_to_grade(value),
            (RankGrammar::Int, RankNode::Int(value)) => int_to_grade(value),
            (RankGrammar::Bool, RankNode::Bool(_)) => Ok(0),
            (RankGrammar::Enum { items, .. }, RankNode::Enum { index, .. }) => {
                let index = nat_to_grade(index)?;
                if index < u64::try_from(items.len()).unwrap_or(u64::MAX) {
                    Ok(0)
                } else {
                    Err(RankError::InvalidNode {
                        message: "enum node index is outside the grammar item range".to_owned(),
                    })
                }
            }
            (RankGrammar::Ref { id }, RankNode::Ref { space, ordinal }) if id == space => {
                nat_to_grade(ordinal)
            }
            (RankGrammar::RecursiveRef { id }, _) => {
                let Some((root_id, root_grammar)) = root else {
                    return Err(RankError::UnresolvedRecursiveRef { id: id.clone() });
                };
                if id != root_id {
                    return Err(RankError::UnresolvedRecursiveRef { id: id.clone() });
                }
                self.grade_inner(root_grammar, node, root)
            }
            (RankGrammar::Sum { alts, .. }, RankNode::Sum { tag, value }) => {
                let alt = alts
                    .get(usize::try_from(*tag).map_err(|_| RankError::InvalidNode {
                        message: "sum node tag does not fit in usize".to_owned(),
                    })?)
                    .ok_or_else(|| RankError::InvalidNode {
                        message: "sum node tag is outside the grammar alternative range".to_owned(),
                    })?;
                add_grade(alt.grade_cost, self.grade_inner(&alt.grammar, value, root)?)
            }
            (RankGrammar::Product { fields, .. }, RankNode::Product(values)) => {
                if fields.len() != values.len() {
                    return Err(RankError::InvalidNode {
                        message: "product node arity does not match grammar fields".to_owned(),
                    });
                }
                let mut total = 0;
                for (field, value) in fields.iter().zip(values) {
                    let field_grade = add_grade(
                        field.grade_cost,
                        self.grade_inner(&field.grammar, value, root)?,
                    )?;
                    total = add_grade(total, field_grade)?;
                }
                Ok(total)
            }
            (
                RankGrammar::List {
                    element,
                    min_len,
                    max_len,
                    ..
                },
                RankNode::List(values),
            ) => {
                validate_node_len(values.len(), *min_len, *max_len)?;
                self.grade_sequence(element, values, root)
            }
            (
                RankGrammar::Set {
                    element, max_len, ..
                },
                RankNode::Set(values),
            ) => {
                validate_node_len(values.len(), 0, *max_len)?;
                self.grade_sequence(element, values, root)
            }
            (
                RankGrammar::Map {
                    key,
                    value,
                    max_len,
                    ..
                },
                RankNode::Map(entries),
            ) => {
                validate_node_len(entries.len(), 0, *max_len)?;
                let mut total =
                    u64::try_from(entries.len()).map_err(|_| RankError::GradeOverflow {
                        value: entries.len().to_string(),
                    })?;
                for (entry_key, entry_value) in entries {
                    total = add_grade(total, self.grade_inner(key, entry_key, root)?)?;
                    total = add_grade(total, self.grade_inner(value, entry_value, root)?)?;
                }
                Ok(total)
            }
            (RankGrammar::Guard { inner, .. }, _) => self.grade_inner(inner, node, root),
            (expected, found) => Err(RankError::NodeGrammarMismatch {
                expected: grammar_kind(expected),
                found: found.kind_name(),
            }),
        }
    }

    fn grade_sequence(
        &mut self,
        element: &RankGrammar,
        values: &[RankNode],
        root: Option<(&Symbol, &RankGrammar)>,
    ) -> RankResult<RankGrade> {
        let mut total = u64::try_from(values.len()).map_err(|_| RankError::GradeOverflow {
            value: values.len().to_string(),
        })?;
        for value in values {
            total = add_grade(total, self.grade_inner(element, value, root)?)?;
        }
        Ok(total)
    }
}

impl Default for GradeCompiler {
    fn default() -> Self {
        Self::new(RankLimits::default())
    }
}

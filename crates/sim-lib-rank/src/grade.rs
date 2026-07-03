//! Grade entry points for rankable spaces.
//!
//! A grade is the size class that stratifies a space into finite slices: each
//! value has a grade, and each grade holds a finite, countable set of values.
//! These helpers wrap a default [`GradeCompiler`] for one-shot grading and
//! counting.

use crate::{
    error::RankResult, grade_compile::GradeCompiler, grammar::RankGrammar, nat::Nat, node::RankNode,
};

/// The grade (size class) of a value, as a natural-number index.
pub type RankGrade = u64;

/// Memoization statistics for a grade computation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GradeMemoStats {
    /// Number of distinct count-memo entries retained.
    pub count_entries: usize,
    /// Number of distinct grade-memo entries retained.
    pub grade_entries: usize,
    /// Number of count-memo cache hits.
    pub count_hits: u64,
    /// Number of grade-memo cache hits.
    pub grade_hits: u64,
}

/// Computes the grade of a single node within a grammar.
pub fn grade_of_node(grammar: &RankGrammar, node: &RankNode) -> RankResult<RankGrade> {
    GradeCompiler::default().grade_of_node(grammar, node)
}

/// Counts the values of a grammar at exactly the given grade.
pub fn count_at_grade(grammar: &RankGrammar, grade: RankGrade) -> RankResult<Nat> {
    GradeCompiler::default().count_at_grade(grammar, grade)
}

/// Reports whether the count at the given grade is finite.
pub fn grade_count_is_finite(grammar: &RankGrammar, grade: RankGrade) -> RankResult<bool> {
    GradeCompiler::default().count_is_finite_at_grade(grammar, grade)
}

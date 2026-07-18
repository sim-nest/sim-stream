//! Grade-grouped rank codec for recursive and unbounded grammars.
//!
//! Partitions a space into grade groups (by structural size), ranks within each
//! group, and concatenates the groups into one global ordinal so even infinite
//! grammars become a single retrieval index.

use std::sync::{Arc, Mutex, MutexGuard};

use sim_kernel::Symbol;

use crate::{
    codec::RankCodec,
    codec_group_core::{
        count_group, find_group, group_offset, invalid_node, rank_in_group_with, require_below,
        root_for, unrank_in_group_with,
    },
    error::RankResult,
    grade::RankGrade,
    grade_compile::GradeCompiler,
    grammar::RankGrammar,
    limits::RankLimits,
    nat::Nat,
    node::RankNode,
    version::RankVersion,
};

/// Codec that ranks nodes within grade groups before flattening to a global ordinal.
///
/// Splits a space by grade (structural size) so that each group is finite and
/// individually rankable, the prerequisite for ranking recursive grammars.
pub trait GroupCodec: Send + Sync + std::fmt::Debug {
    /// Returns the number of grade groups, or `None` when unbounded.
    fn group_count(&self) -> Option<Nat>;
    /// Returns the grade group that a node belongs to.
    fn group_of_node(&self, node: &RankNode) -> RankResult<RankGrade>;
    /// Returns the inhabitant count of the group at the given grade.
    fn group_count_at(&self, key: RankGrade) -> RankResult<Nat>;
    /// Ranks a node to its ordinal within the group at the given grade.
    fn rank_in_group(&self, key: RankGrade, node: &RankNode) -> RankResult<Nat>;
    /// Unranks an ordinal back to its node within the group at the given grade.
    fn unrank_in_group(&self, key: RankGrade, r: &Nat) -> RankResult<RankNode>;
}

/// Grade-grouped [`RankCodec`] over a single grammar with bounded resources.
///
/// Holds the grammar, resource limits, and a shared grade compiler that caches
/// the per-grade counts for offsetting and locating ordinals.
#[derive(Debug)]
pub struct RankGroupCodec {
    grammar: RankGrammar,
    limits: RankLimits,
    compiler: Arc<Mutex<GradeCompiler>>,
}

impl Clone for RankGroupCodec {
    fn clone(&self) -> Self {
        Self::with_limits(self.grammar.clone(), self.limits.clone())
    }
}

impl RankGroupCodec {
    /// Creates a codec for the grammar using default resource limits.
    pub fn new(grammar: RankGrammar) -> Self {
        Self::with_limits(grammar, RankLimits::default())
    }

    /// Creates a codec for the grammar with explicit resource limits.
    pub fn with_limits(grammar: RankGrammar, limits: RankLimits) -> Self {
        Self {
            grammar,
            limits: limits.clone(),
            compiler: Arc::new(Mutex::new(GradeCompiler::new(limits))),
        }
    }

    /// Returns the grammar this codec ranks against.
    pub fn grammar(&self) -> &RankGrammar {
        &self.grammar
    }

    fn compiler(&self) -> RankResult<MutexGuard<'_, GradeCompiler>> {
        let mut compiler = self
            .compiler
            .lock()
            .map_err(|_| invalid_node("rank group compiler lock is poisoned"))?;
        compiler.reset_limits(self.limits.clone());
        Ok(compiler)
    }
}

impl GroupCodec for RankGroupCodec {
    fn group_count(&self) -> Option<Nat> {
        None
    }

    fn group_of_node(&self, node: &RankNode) -> RankResult<RankGrade> {
        let mut compiler = self.compiler()?;
        compiler.grade_of_node(&self.grammar, node)
    }

    fn group_count_at(&self, key: RankGrade) -> RankResult<Nat> {
        let mut compiler = self.compiler()?;
        compiler.count_at_grade(&self.grammar, key)
    }

    fn rank_in_group(&self, key: RankGrade, node: &RankNode) -> RankResult<Nat> {
        let mut compiler = self.compiler()?;
        rank_in_group_with(
            &self.grammar,
            node,
            key,
            root_for(&self.grammar),
            &mut compiler,
        )
    }

    fn unrank_in_group(&self, key: RankGrade, r: &Nat) -> RankResult<RankNode> {
        let mut compiler = self.compiler()?;
        unrank_in_group_with(
            &self.grammar,
            key,
            r,
            root_for(&self.grammar),
            &mut compiler,
        )
    }
}

impl RankCodec for RankGroupCodec {
    fn id(&self) -> Symbol {
        Symbol::qualified("rank-codec", "group")
    }

    fn version(&self) -> RankVersion {
        RankVersion::v1()
    }

    fn count(&self) -> Option<Nat> {
        None
    }

    fn r_ok(&self, _r: &Nat) -> bool {
        true
    }

    fn rank_node(&self, node: &RankNode) -> RankResult<Nat> {
        let root = root_for(&self.grammar);
        let mut compiler = self.compiler()?;
        let grade = compiler.grade_of_node_with_root(&self.grammar, node, root)?;
        let offset = group_offset(&self.grammar, grade, root, &mut compiler)?;
        let rank = rank_in_group_with(&self.grammar, node, grade, root, &mut compiler)?;
        let count = count_group(&self.grammar, grade, root, &mut compiler)?;
        require_below(&rank, &count)?;
        Ok(offset.checked_add(&rank))
    }

    fn unrank_node(&self, r: &Nat) -> RankResult<RankNode> {
        let root = root_for(&self.grammar);
        let mut compiler = self.compiler()?;
        let (grade, rank) = find_group(&self.grammar, r, root, &mut compiler, &self.limits)?;
        unrank_in_group_with(&self.grammar, grade, &rank, root, &mut compiler)
    }
}

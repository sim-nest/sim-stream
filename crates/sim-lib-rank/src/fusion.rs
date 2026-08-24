//! Deterministic, inspectable reciprocal-rank fusion.

use std::{collections::BTreeMap, fmt::Debug};

use crate::{EmbeddingIndex, RankError, RankLimits, RankResult, retrieve_limited};

/// A bounded ranking supplied by one stable source.
#[derive(Clone, Debug, PartialEq)]
pub struct RankedList<K> {
    /// Stable source identifier.
    pub source_id: String,
    /// Nonnegative multiplier applied to this source.
    pub source_weight: f64,
    /// Items in best-first source order.
    pub items: Vec<K>,
}

impl<K> RankedList<K> {
    /// Constructs a source list, validating its identity and weight.
    pub fn new(
        source_id: impl Into<String>,
        source_weight: f64,
        items: Vec<K>,
    ) -> RankResult<Self> {
        let source_id = source_id.into();
        if source_id.is_empty() {
            return Err(invalid_fusion("source id must not be empty"));
        }
        if !source_weight.is_finite() || source_weight < 0.0 {
            return Err(invalid_fusion(
                "source weight must be finite and nonnegative",
            ));
        }
        Ok(Self {
            source_id,
            source_weight,
            items,
        })
    }
}

/// Hard bounds for a fusion operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FusionLimits {
    /// Maximum rows inspected from each source.
    pub per_source: usize,
    /// Maximum distinct candidates admitted across all sources.
    pub candidates: usize,
    /// Maximum fused results returned.
    pub output: usize,
}

impl FusionLimits {
    /// Constructs positive fusion bounds. An output of zero is allowed.
    pub fn new(per_source: usize, candidates: usize, output: usize) -> RankResult<Self> {
        if per_source == 0 || candidates == 0 {
            return Err(invalid_fusion(
                "per-source and candidate limits must be positive",
            ));
        }
        Ok(Self {
            per_source,
            candidates,
            output,
        })
    }
}

/// Why a row did not contribute to the fused score.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RankDropReason {
    /// The same key already appeared earlier in this source.
    Duplicate,
    /// The source row was beyond the per-source limit.
    SourceCutoff,
    /// The key was new after the aggregate candidate limit was full.
    CandidateCutoff,
}

/// A row excluded from scoring, retained for inspection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankDrop<K> {
    /// Stable item key.
    pub key: K,
    /// Stable source identifier.
    pub source_id: String,
    /// One-based rank in the supplied source list.
    pub source_rank: usize,
    /// Exact exclusion reason.
    pub reason: RankDropReason,
}

/// One source's auditable contribution to a fused item.
#[derive(Clone, Debug, PartialEq)]
pub struct RankContribution<K> {
    /// Stable item key.
    pub key: K,
    /// Stable source identifier.
    pub source_id: String,
    /// Configured source multiplier.
    pub source_weight: f64,
    /// One-based rank in that source.
    pub source_rank: usize,
    /// Reciprocal-rank constant used for this operation.
    pub rrf_k: usize,
    /// Exact value added to the fused score.
    pub value: f64,
}

/// A fused output item and the evidence for its score.
#[derive(Clone, Debug, PartialEq)]
pub struct FusedRank<K> {
    /// Stable item key.
    pub key: K,
    /// Sum of all recorded contribution values.
    pub score: f64,
    /// Best one-based rank attained in any source.
    pub best_source_rank: usize,
    /// Contributions in stable source-id order.
    pub contributions: Vec<RankContribution<K>>,
}

/// Tie-break comparison recorded between adjacent equal-score results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankTieBreak<K> {
    /// Item ordered first.
    pub winner: K,
    /// Item ordered second.
    pub runner_up: K,
    /// Deciding field: `best_source_rank` or `stable_key`.
    pub decided_by: &'static str,
}

/// Complete reciprocal-rank-fusion result and decision receipt.
#[derive(Clone, Debug, PartialEq)]
pub struct RankedFusion<K> {
    /// Reciprocal-rank constant used by every contribution.
    pub rrf_k: usize,
    /// Bounds applied by the operation.
    pub limits: FusionLimits,
    /// Best-first fused output, bounded by [`FusionLimits::output`].
    pub items: Vec<FusedRank<K>>,
    /// Every accepted contribution, including candidates beyond output cutoff.
    pub contributions: Vec<RankContribution<K>>,
    /// Rows omitted through duplicate or cutoff policy.
    pub drops: Vec<RankDrop<K>>,
    /// Equal-score ordering decisions between adjacent output items.
    pub tie_breaks: Vec<RankTieBreak<K>>,
    /// Number of valid fused candidates omitted by the output cutoff.
    pub output_cutoff: usize,
    /// Stable keys omitted by the output cutoff, in fused order.
    pub output_drops: Vec<K>,
}

/// Fuses source rankings using weighted reciprocal-rank fusion.
///
/// Sources are canonicalized by id, making the result and receipt independent
/// of input source order. Source ids must be unique. Only identical keys are
/// deduplicated; callers remain responsible for alias policy.
pub fn reciprocal_rank_fusion<K>(
    mut sources: Vec<RankedList<K>>,
    rrf_k: usize,
    limits: FusionLimits,
) -> RankResult<RankedFusion<K>>
where
    K: Clone + Debug + Ord,
{
    if rrf_k == 0 {
        return Err(invalid_fusion("rrf_k must be positive"));
    }
    sources.sort_by(|left, right| left.source_id.cmp(&right.source_id));
    if sources
        .windows(2)
        .any(|pair| pair[0].source_id == pair[1].source_id)
    {
        return Err(invalid_fusion("source ids must be unique"));
    }

    let mut candidates: BTreeMap<K, Vec<RankContribution<K>>> = BTreeMap::new();
    let mut drops = Vec::new();
    for source in sources {
        let mut source_keys = std::collections::BTreeSet::new();
        for (offset, key) in source.items.into_iter().enumerate() {
            let source_rank = offset + 1;
            let reason = if source_rank > limits.per_source {
                Some(RankDropReason::SourceCutoff)
            } else if !source_keys.insert(key.clone()) {
                Some(RankDropReason::Duplicate)
            } else if !candidates.contains_key(&key) && candidates.len() == limits.candidates {
                Some(RankDropReason::CandidateCutoff)
            } else {
                None
            };
            if let Some(reason) = reason {
                drops.push(RankDrop {
                    key,
                    source_id: source.source_id.clone(),
                    source_rank,
                    reason,
                });
                continue;
            }
            let denominator = rrf_k
                .checked_add(source_rank)
                .ok_or_else(|| invalid_fusion("rrf_k plus source rank overflows"))?;
            let value = source.source_weight / denominator as f64;
            candidates
                .entry(key.clone())
                .or_default()
                .push(RankContribution {
                    key,
                    source_id: source.source_id.clone(),
                    source_weight: source.source_weight,
                    source_rank,
                    rrf_k,
                    value,
                });
        }
    }
    let mut fused: Vec<_> = candidates
        .into_iter()
        .map(|(key, mut contributions)| {
            contributions.sort_by(|a, b| a.source_id.cmp(&b.source_id));
            let score = contributions.iter().map(|item| item.value).sum();
            let best_source_rank = contributions
                .iter()
                .map(|item| item.source_rank)
                .min()
                .unwrap_or(usize::MAX);
            FusedRank {
                key,
                score,
                best_source_rank,
                contributions,
            }
        })
        .collect();
    fused.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.best_source_rank.cmp(&b.best_source_rank))
            .then_with(|| a.key.cmp(&b.key))
    });
    let tie_breaks = fused
        .windows(2)
        .filter(|pair| pair[0].score == pair[1].score)
        .map(|pair| RankTieBreak {
            winner: pair[0].key.clone(),
            runner_up: pair[1].key.clone(),
            decided_by: if pair[0].best_source_rank != pair[1].best_source_rank {
                "best_source_rank"
            } else {
                "stable_key"
            },
        })
        .collect();
    let contributions = fused
        .iter()
        .flat_map(|item| item.contributions.iter().cloned())
        .collect();
    let output_drops = fused
        .iter()
        .skip(limits.output)
        .map(|item| item.key.clone())
        .collect::<Vec<_>>();
    let output_cutoff = output_drops.len();
    fused.truncate(limits.output);
    Ok(RankedFusion {
        rrf_k,
        limits,
        items: fused,
        contributions,
        drops,
        tie_breaks,
        output_cutoff,
        output_drops,
    })
}

/// Adapts bounded cosine retrieval into a fusion source list.
///
/// `key_for_id` supplies the caller's stable key; no embedding or key policy is
/// copied into the rank library.
pub fn ranked_list_from_embeddings<K, F>(
    source_id: impl Into<String>,
    source_weight: f64,
    index: &impl EmbeddingIndex,
    query: &[f32],
    limit: usize,
    mut key_for_id: F,
) -> RankResult<RankedList<K>>
where
    F: FnMut(&str) -> K,
{
    let mut traversal = RankLimits::default();
    let hits = retrieve_limited(index, query, limit, &mut traversal)?;
    RankedList::new(
        source_id,
        source_weight,
        hits.iter().map(|hit| key_for_id(&hit.id)).collect(),
    )
}

fn invalid_fusion(message: impl Into<String>) -> RankError {
    RankError::InvalidNode {
        message: message.into(),
    }
}

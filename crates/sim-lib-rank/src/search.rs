//! Local search over a ranked space by hill-climbing and beam search.
//!
//! Walks the ordinals of a [`RankCodec`] guided by a caller-supplied score and
//! a [`RankNeighborhood`], seeking the highest-scoring node reachable from a
//! start ordinal.

use std::collections::BTreeSet;

use crate::{Nat, RankCodec, RankNeighborhood, RankNode, RankResult, limits::RankLimits};

/// Score assigned to a ranked node during search; higher is better.
pub type RankSearchScore = i128;

/// A scored position in the search: an ordinal paired with its score.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankSearchState {
    /// Ordinal of the ranked node at this position.
    pub ordinal: Nat,
    /// Score of the node at `ordinal`.
    pub score: RankSearchScore,
}

/// Outcome of a hill-climb: the best state and the path taken to reach it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankSearchResult {
    /// Highest-scoring state found.
    pub best: RankSearchState,
    /// States visited in order from start to best.
    pub path: Vec<RankSearchState>,
}

/// Outcome of a beam search: the best state and the surviving frontier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankBeamSearchResult {
    /// Highest-scoring state found across all depths.
    pub best: RankSearchState,
    /// Final beam of states retained after the last expansion.
    pub frontier: Vec<RankSearchState>,
}

/// Hill-climbs from `start`, moving to the best neighbor while score improves.
///
/// At each step it scores every neighbor of the current ordinal and advances to
/// the strictly better one (ties broken by smaller ordinal), stopping when no
/// neighbor improves. Each step consumes one unit of `limits` fuel.
pub fn hill_climb<N, F>(
    neighborhood: &N,
    codec: &dyn RankCodec,
    start: &Nat,
    limits: &mut RankLimits,
    mut score: F,
) -> RankResult<RankSearchResult>
where
    N: RankNeighborhood + ?Sized,
    F: FnMut(&Nat, &RankNode) -> RankResult<RankSearchScore>,
{
    limits.check_nat(start, "rank.search.hill-climb")?;
    let mut current = score_state(codec, start, &mut score)?;
    let mut path = vec![current.clone()];
    loop {
        limits.consume(1, "rank.search.hill-climb")?;
        let mut best = current.clone();
        for neighbor in neighborhood.neighbors(codec, &current.ordinal, limits)? {
            let candidate = score_state(codec, &neighbor, &mut score)?;
            if better(&candidate, &best) {
                best = candidate;
            }
        }
        if best.score <= current.score {
            return Ok(RankSearchResult {
                best: current,
                path,
            });
        }
        current = best;
        path.push(current.clone());
    }
}

/// Beam-searches from `start`, keeping the top `width` states for `depth` rounds.
///
/// Each round scores all neighbors of the current frontier, deduplicates seen
/// ordinals, keeps the best `width` candidates, and tracks the global best.
/// A `width` of zero returns just the scored start with an empty frontier; each
/// round consumes one unit of `limits` fuel.
pub fn beam_search<N, F>(
    neighborhood: &N,
    codec: &dyn RankCodec,
    start: &Nat,
    width: usize,
    depth: usize,
    limits: &mut RankLimits,
    mut score: F,
) -> RankResult<RankBeamSearchResult>
where
    N: RankNeighborhood + ?Sized,
    F: FnMut(&Nat, &RankNode) -> RankResult<RankSearchScore>,
{
    limits.check_nat(start, "rank.search.beam")?;
    limits.check_count(width, "rank.search.beam.width")?;
    limits.check_count(depth, "rank.search.beam.depth")?;
    if width == 0 {
        return Ok(RankBeamSearchResult {
            best: score_state(codec, start, &mut score)?,
            frontier: Vec::new(),
        });
    }

    let start = score_state(codec, start, &mut score)?;
    let mut best = start.clone();
    let mut frontier = vec![start];
    let mut seen = BTreeSet::new();

    for _ in 0..depth {
        limits.consume(1, "rank.search.beam")?;
        let mut candidates = Vec::new();
        for state in &frontier {
            seen.insert(state.ordinal.clone());
            for neighbor in neighborhood.neighbors(codec, &state.ordinal, limits)? {
                if seen.insert(neighbor.clone()) {
                    candidates.push(score_state(codec, &neighbor, &mut score)?);
                }
            }
        }
        if candidates.is_empty() {
            break;
        }
        candidates.sort_by(compare_states);
        candidates.truncate(width);
        if better(&candidates[0], &best) {
            best = candidates[0].clone();
        }
        frontier = candidates;
    }

    Ok(RankBeamSearchResult { best, frontier })
}

fn score_state<F>(
    codec: &dyn RankCodec,
    ordinal: &Nat,
    score: &mut F,
) -> RankResult<RankSearchState>
where
    F: FnMut(&Nat, &RankNode) -> RankResult<RankSearchScore>,
{
    let node = codec.unrank_node(ordinal)?;
    Ok(RankSearchState {
        ordinal: ordinal.clone(),
        score: score(ordinal, &node)?,
    })
}

fn better(candidate: &RankSearchState, incumbent: &RankSearchState) -> bool {
    candidate.score > incumbent.score
        || (candidate.score == incumbent.score && candidate.ordinal < incumbent.ordinal)
}

fn compare_states(left: &RankSearchState, right: &RankSearchState) -> std::cmp::Ordering {
    right
        .score
        .cmp(&left.score)
        .then_with(|| left.ordinal.cmp(&right.ordinal))
}

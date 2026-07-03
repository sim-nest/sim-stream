//! Scoring functions and scored frontiers over a ranked ordinal space.
//!
//! Defines the [`RankScore`] contract that assigns a numeric score to each
//! ordinal, plus the frontier machinery -- band, beam, and novelty -- that
//! scores candidates and collects the top-ranked ones into a [`RankFrontier`]
//! for retrieval and downstream streaming.

use std::sync::{Arc, Mutex};

use sim_kernel::{Cx, Error as KernelError, Event, EventKind, EventSource, Ref, Symbol, Tick};

use crate::{
    Nat, RankCodec, RankError, RankNeighborhood, RankNode, RankResult, RankVersion,
    coordinate_for_nat, intern_ordinal,
    limits::RankLimits,
    search::{RankSearchScore, beam_search},
};

/// Numeric score assigned to a ranked ordinal.
///
/// Higher values rank ahead; this is the comparable scalar produced by every
/// [`RankScore`] and carried through frontiers.
pub type ScoreValue = RankSearchScore;

#[cfg(feature = "rank-learn")]
#[path = "order_learn.rs"]
mod order_learn;
#[cfg(feature = "rank-learn")]
pub use order_learn::{
    RankFrozenLearnedModel, learned_frequency_order, publish_learned_model_claims,
    rank_learned_model_card, rank_learned_model_digest_predicate, rank_learned_model_id_predicate,
    rank_learned_model_kind, rank_learned_model_version_predicate,
};
#[path = "order_score_stream.rs"]
mod order_score_stream;
#[cfg(feature = "rank-scatter")]
pub(crate) use order_score_stream::{key_nat, rank_coordinate_packet, rank_data_metadata};
pub use order_score_stream::{
    rank_coordinate_data_kind, rank_frontier_data_kind, rank_frontier_stream,
    rank_ordinal_data_kind,
};

/// Scoring function over a ranked ordinal space.
///
/// Implementors assign a [`ScoreValue`] to each ordinal (given its decoded
/// node), driving how candidates are ranked. The `id` and `version` identify
/// the scorer for provenance and reproducibility.
pub trait RankScore: Send + Sync + std::fmt::Debug {
    /// Returns the symbol identifying this scorer.
    fn id(&self) -> Symbol;
    /// Returns the version of this scorer's scoring behavior.
    fn version(&self) -> RankVersion;
    /// Scores one ordinal, given the codec and the ordinal's decoded node.
    fn score(
        &self,
        codec: &dyn RankCodec,
        ordinal: &Nat,
        node: &RankNode,
    ) -> RankResult<ScoreValue>;
}

/// Adapter that turns a closure into a [`RankScore`].
///
/// Wraps a scoring function `F` under an identifying symbol; the version is
/// fixed at v1.
pub struct RankScoreFn<F> {
    id: Symbol,
    score: F,
}

impl<F> RankScoreFn<F> {
    /// Builds a closure-backed scorer with the given identifier.
    pub fn new(id: Symbol, score: F) -> Self {
        Self { id, score }
    }
}

impl<F> std::fmt::Debug for RankScoreFn<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RankScoreFn")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl<F> RankScore for RankScoreFn<F>
where
    F: Fn(&dyn RankCodec, &Nat, &RankNode) -> RankResult<ScoreValue> + Send + Sync,
{
    fn id(&self) -> Symbol {
        self.id.clone()
    }

    fn version(&self) -> RankVersion {
        RankVersion::v1()
    }

    fn score(
        &self,
        codec: &dyn RankCodec,
        ordinal: &Nat,
        node: &RankNode,
    ) -> RankResult<ScoreValue> {
        (self.score)(codec, ordinal, node)
    }
}

/// An ordinal paired with the score it received.
///
/// The unit of a [`RankFrontier`]: a candidate ordinal and its [`ScoreValue`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoredOrdinal {
    /// The ranked ordinal.
    pub ordinal: Nat,
    /// The score assigned to the ordinal.
    pub score: ScoreValue,
}

/// Half-open range of ordinals `[start, end)` over the ranked space.
///
/// An absent `end` means the range runs to the codec's count (open-ended), and
/// requires a finite codec count to resolve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankOrdinalRange {
    /// Inclusive lower bound of the range.
    pub start: Nat,
    /// Exclusive upper bound, or `None` to run to the codec count.
    pub end: Option<Nat>,
}

impl RankOrdinalRange {
    /// Builds the closed-open range `[start, end)`.
    pub fn closed_open(start: Nat, end: Nat) -> Self {
        Self {
            start,
            end: Some(end),
        }
    }

    /// Builds an open-ended range starting at `start` and running to the count.
    pub fn from_start(start: Nat) -> Self {
        Self { start, end: None }
    }

    fn end_for(&self, codec: &dyn RankCodec) -> RankResult<Nat> {
        self.end
            .clone()
            .or_else(|| codec.count())
            .ok_or_else(|| RankError::InvalidNode {
                message: "open rank frontier range requires a finite codec count".to_owned(),
            })
    }
}

/// An ordered set of scored ordinals -- the ranked retrieval result.
///
/// A frontier is the top band/beam/novelty selection produced by the frontier
/// constructors: a deduplicated, length-capped sequence of [`ScoredOrdinal`]
/// that can be replayed as events or projected into a data stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankFrontier {
    id: Symbol,
    items: Vec<ScoredOrdinal>,
}

impl RankFrontier {
    /// Builds a frontier from scored items, deduplicating by ordinal and
    /// truncating to `max_events`.
    pub fn new(id: Symbol, mut items: Vec<ScoredOrdinal>, max_events: usize) -> Self {
        items.dedup_by(|left, right| left.ordinal == right.ordinal);
        items.truncate(max_events);
        Self { id, items }
    }

    /// Returns the symbol identifying this frontier.
    pub fn id(&self) -> &Symbol {
        &self.id
    }

    /// Returns the scored ordinals in rank order.
    pub fn items(&self) -> &[ScoredOrdinal] {
        &self.items
    }

    /// Iterates the frontier's ordinals in rank order, dropping scores.
    pub fn ordinals(&self) -> impl Iterator<Item = &Nat> {
        self.items.iter().map(|item| &item.ordinal)
    }

    /// Returns the rank position of an ordinal within this frontier.
    ///
    /// A frontier does not maintain an ordinal-to-position index, so this
    /// always fails with a position-unavailable error.
    pub fn position_of(&self, _ordinal: &Nat) -> RankResult<Nat> {
        Err(RankError::PositionUnavailable {
            id: self.id.clone(),
        })
    }

    /// Builds an event source that replays the frontier's ordinals as events.
    ///
    /// Each ordinal becomes a chunk event under `run`, carrying the payload form
    /// selected by `payload`, followed by a terminal done event.
    pub fn event_source(&self, run: Ref, payload: RankFrontierPayload) -> Arc<dyn EventSource> {
        Arc::new(RankFrontierEventSource::new(
            run,
            self.items.iter().map(|item| item.ordinal.clone()).collect(),
            payload,
        ))
    }

    /// Projects the frontier into a data stream of summary and item packets.
    pub fn data_stream(&self, payload: RankFrontierPayload) -> sim_lib_stream_core::StreamValue {
        rank_frontier_stream(self, payload)
    }
}

/// Specification for a band frontier: ordinals whose score falls in a range.
///
/// Scans `range` and keeps each ordinal whose score lies within `[min, max]`,
/// up to `max_events` results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandFrontierSpec {
    /// Identifier for the resulting frontier.
    pub id: Symbol,
    /// Ordinal range to scan.
    pub range: RankOrdinalRange,
    /// Inclusive minimum score to admit.
    pub min: ScoreValue,
    /// Inclusive maximum score to admit.
    pub max: ScoreValue,
    /// Maximum number of ordinals to collect.
    pub max_events: usize,
}

impl BandFrontierSpec {
    /// Builds a band frontier specification from its fields.
    pub fn new(
        id: Symbol,
        range: RankOrdinalRange,
        min: ScoreValue,
        max: ScoreValue,
        max_events: usize,
    ) -> Self {
        Self {
            id,
            range,
            min,
            max,
            max_events,
        }
    }
}

/// Specification for a beam frontier: top-scored ordinals from a beam search.
///
/// Runs a width-limited, depth-limited beam search over the neighborhood from
/// `start`, keeping the highest-scoring ordinals up to `max_events`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeamFrontierSpec {
    /// Identifier for the resulting frontier.
    pub id: Symbol,
    /// Ordinal to seed the beam search from.
    pub start: Nat,
    /// Beam width -- candidates retained at each step.
    pub width: usize,
    /// Search depth -- expansion steps from the start.
    pub depth: usize,
    /// Maximum number of ordinals to collect.
    pub max_events: usize,
}

impl BeamFrontierSpec {
    /// Builds a beam frontier specification from its fields.
    pub fn new(id: Symbol, start: Nat, width: usize, depth: usize, max_events: usize) -> Self {
        Self {
            id,
            start,
            width,
            depth,
            max_events,
        }
    }
}

/// Payload form carried for each ordinal when a frontier is emitted.
///
/// Selects how an ordinal is rendered as event/stream content -- as its
/// interned ordinal content, or as a coordinate within a named space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RankFrontierPayload {
    /// Emit the interned content of the ordinal itself.
    OrdinalContent,
    /// Emit the ordinal as a coordinate within the named space.
    Coordinate {
        /// Symbol naming the coordinate space.
        space: Symbol,
    },
}

/// Builds a band frontier: ordinals in `range` whose score lies within bounds.
///
/// Scans the range, scoring each ordinal and admitting those in `[spec.min,
/// spec.max]` until `max_events` is reached; results are sorted by ordinal.
/// Errors when the bounds or range are inverted.
pub fn band_frontier(
    spec: BandFrontierSpec,
    codec: &dyn RankCodec,
    score: &dyn RankScore,
    limits: &mut RankLimits,
) -> RankResult<RankFrontier> {
    if spec.min > spec.max {
        return Err(RankError::InvalidNode {
            message: "rank band frontier minimum score exceeds maximum".to_owned(),
        });
    }
    let end = spec.range.end_for(codec)?;
    if spec.range.start > end {
        return Err(RankError::InvalidNode {
            message: "rank frontier range start exceeds end".to_owned(),
        });
    }

    let mut values = Vec::new();
    let mut ordinal = spec.range.start;
    while ordinal < end {
        limits.consume(1, "rank.frontier.band")?;
        limits.check_nat(&ordinal, "rank.frontier.band")?;
        let node = codec.unrank_node(&ordinal)?;
        let value = score.score(codec, &ordinal, &node)?;
        if (spec.min..=spec.max).contains(&value) {
            values.push(ScoredOrdinal {
                ordinal: ordinal.clone(),
                score: value,
            });
            if values.len() >= spec.max_events {
                break;
            }
        }
        ordinal = ordinal.checked_add(&Nat::one());
    }
    values.sort_by(|left, right| left.ordinal.cmp(&right.ordinal));
    Ok(RankFrontier::new(spec.id, values, spec.max_events))
}

/// Builds a beam frontier by beam-searching the neighborhood from `start`.
///
/// Expands candidates by `width` over `depth` steps, scoring each, and collects
/// the highest-scoring ordinals (ties broken by ordinal) up to `max_events`.
pub fn beam_frontier(
    spec: BeamFrontierSpec,
    codec: &dyn RankCodec,
    neighborhood: &dyn RankNeighborhood,
    score: &dyn RankScore,
    limits: &mut RankLimits,
) -> RankResult<RankFrontier> {
    let result = beam_search(
        neighborhood,
        codec,
        &spec.start,
        spec.width,
        spec.depth,
        limits,
        |ordinal, node| score.score(codec, ordinal, node),
    )?;
    let mut values = result
        .frontier
        .into_iter()
        .map(|state| ScoredOrdinal {
            ordinal: state.ordinal,
            score: state.score,
        })
        .collect::<Vec<_>>();
    values.sort_by(compare_scored);
    Ok(RankFrontier::new(spec.id, values, spec.max_events))
}

/// Builds a novelty frontier scoring candidates by distance to an archive.
///
/// Scores each candidate by its minimum neighborhood distance to any archived
/// ordinal -- higher distance means more novel -- and ranks the most novel
/// first, up to `max_events`.
pub fn novelty_frontier(
    id: Symbol,
    codec: &dyn RankCodec,
    neighborhood: &dyn RankNeighborhood,
    candidates: impl IntoIterator<Item = Nat>,
    archive: &[Nat],
    limits: &mut RankLimits,
    max_events: usize,
) -> RankResult<RankFrontier> {
    let mut values = Vec::new();
    for ordinal in candidates {
        limits.consume(1, "rank.frontier.novelty")?;
        let score = novelty_score(codec, neighborhood, &ordinal, archive, limits)?;
        values.push(ScoredOrdinal { ordinal, score });
    }
    values.sort_by(compare_scored);
    Ok(RankFrontier::new(id, values, max_events))
}

fn novelty_score(
    codec: &dyn RankCodec,
    neighborhood: &dyn RankNeighborhood,
    ordinal: &Nat,
    archive: &[Nat],
    limits: &mut RankLimits,
) -> RankResult<ScoreValue> {
    let mut best = None;
    for reference in archive {
        let Some(distance) = neighborhood.distance(codec, ordinal, reference, limits)? else {
            continue;
        };
        best = Some(best.map_or(distance.clone(), |current: Nat| current.min(distance)));
    }
    best.map(|distance| {
        distance
            .to_decimal_string()
            .parse()
            .unwrap_or(ScoreValue::MAX)
    })
    .ok_or_else(|| RankError::InvalidNode {
        message: "rank novelty frontier requires at least one defined archive distance".to_owned(),
    })
}

fn compare_scored(left: &ScoredOrdinal, right: &ScoredOrdinal) -> std::cmp::Ordering {
    right
        .score
        .cmp(&left.score)
        .then_with(|| left.ordinal.cmp(&right.ordinal))
}

struct RankFrontierEventSource {
    run: Ref,
    ordinals: Vec<Nat>,
    payload: RankFrontierPayload,
    state: Mutex<RankFrontierEventState>,
}

struct RankFrontierEventState {
    index: usize,
    seq: u64,
    done_sent: bool,
}

impl RankFrontierEventSource {
    fn new(run: Ref, ordinals: Vec<Nat>, payload: RankFrontierPayload) -> Self {
        Self {
            run,
            ordinals,
            payload,
            state: Mutex::new(RankFrontierEventState {
                index: 0,
                seq: 0,
                done_sent: false,
            }),
        }
    }

    fn payload_ref(&self, cx: &mut Cx, ordinal: &Nat) -> sim_kernel::Result<Ref> {
        match &self.payload {
            RankFrontierPayload::OrdinalContent => Ok(Ref::Content(intern_ordinal(cx, ordinal)?)),
            RankFrontierPayload::Coordinate { space } => {
                coordinate_for_nat(cx, space.clone(), ordinal)
            }
        }
    }
}

impl EventSource for RankFrontierEventSource {
    fn next(&self, cx: &mut Cx) -> sim_kernel::Result<Option<Event>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| KernelError::PoisonedLock("rank frontier event source"))?;
        let seq = state.seq;
        state.seq = state.seq.saturating_add(1);

        if let Some(ordinal) = self.ordinals.get(state.index) {
            let tick = Tick::new(
                Symbol::qualified("rank/order", "position"),
                Ref::Content(intern_ordinal(cx, &Nat::from(state.index))?),
            );
            let event = Event::new(
                self.run.clone(),
                seq,
                vec![tick],
                EventKind::Chunk {
                    payload: self.payload_ref(cx, ordinal)?,
                },
            )?;
            state.index += 1;
            return Ok(Some(event));
        }

        if state.done_sent {
            Ok(None)
        } else {
            state.done_sent = true;
            Ok(Some(Event::done(self.run.clone(), seq)?))
        }
    }
}

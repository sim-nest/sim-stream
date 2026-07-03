//! Bounded music spaces for RANK.
//!
//! R6.15 keeps these as finite, deterministic rank examples over structural
//! `RankNode` values: pitch classes, rational durations, fixed-total rhythms,
//! bounded melodies, progressions, music orders, and a melody neighborhood.

use sim_kernel::Symbol;

use crate::{
    Nat, RankCodec, RankExactOrder, RankNeighborhood, RankNode, RankResult, RankVersion,
    limits::RankLimits, order::nat_to_index,
};

#[path = "music_support.rs"]
mod music_support;
use music_support::{
    articulation_symbol, cadence_score, gcd, generate_melodies, generate_progressions,
    generate_rhythms, invalid_node, melody_crossover_nodes, melody_motion, melody_neighbor_nodes,
    melody_node_distance, melody_span, ordinals, pitch_class_symbol, progression_motion,
    rhythm_simplicity, valid_ordinals,
};

const REST_TAG: u32 = 0;
const NOTE_TAG: u32 = 1;

/// Reduced rational note duration in the music rank space.
///
/// Stored in lowest terms; both fields are positive after construction through
/// [`RankDuration::new`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RankDuration {
    /// Numerator of the reduced duration fraction.
    pub numerator: u64,
    /// Denominator of the reduced duration fraction.
    pub denominator: u64,
}

/// Note articulation marking ranked as a small ordinal alphabet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RankArticulation {
    /// Detached, shortened articulation (ordinal 0).
    Staccato,
    /// Default articulation (ordinal 1).
    Normal,
    /// Smooth, connected articulation (ordinal 2).
    Legato,
}

/// Bounds that close a finite music rank space.
///
/// Caps the rational duration alphabet and the lengths of rhythms, melodies,
/// and progressions, optionally fixing a total rhythm duration. These bounds
/// make the generated coordinate space finite and deterministic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankMusicSpec {
    max_denominator: u64,
    max_duration_numerator: u64,
    max_rhythm_len: usize,
    fixed_total: Option<RankDuration>,
    max_melody_len: usize,
    max_progression_len: usize,
}

/// Finite rank codec over bounded rhythms (sequences of rests and notes).
///
/// Enumerates every rhythm allowed by its [`RankMusicSpec`], orders them
/// lexicographically as the canonical ordinal layout, and precomputes a
/// duration-simplicity ordering.
#[derive(Clone, Debug)]
pub struct RankRhythmCodec {
    spec: RankMusicSpec,
    nodes: Vec<RankNode>,
    duration_simple: Vec<Nat>,
}

/// Finite rank codec over bounded melodies (pitched note sequences).
///
/// Enumerates every melody allowed by its [`RankMusicSpec`] and precomputes a
/// low-motion ordering that favors small adjacent pitch motion.
#[derive(Clone, Debug)]
pub struct RankMelodyCodec {
    spec: RankMusicSpec,
    nodes: Vec<RankNode>,
    low_motion: Vec<Nat>,
}

/// Finite rank codec over bounded chord progressions.
///
/// Enumerates every progression allowed by its [`RankMusicSpec`] and
/// precomputes both a cadence-first and a low-motion alternate ordering.
#[derive(Clone, Debug)]
pub struct RankProgressionCodec {
    spec: RankMusicSpec,
    nodes: Vec<RankNode>,
    cadence_first: Vec<Nat>,
    low_motion: Vec<Nat>,
}

/// Search neighborhood over the bounded melody rank space.
///
/// Implements [`RankNeighborhood`] to provide neighbors, distance, mutation,
/// and crossover for melody ordinals during local search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankMelodyNeighborhood {
    spec: RankMusicSpec,
}

impl RankDuration {
    /// Builds a reduced duration, failing closed on a zero numerator or
    /// denominator.
    pub fn new(numerator: u64, denominator: u64) -> RankResult<Self> {
        if numerator == 0 || denominator == 0 {
            return Err(invalid_node(
                "music duration numerator and denominator must be positive",
            ));
        }
        let gcd = gcd(numerator, denominator);
        Ok(Self {
            numerator: numerator / gcd,
            denominator: denominator / gcd,
        })
    }
}

impl RankArticulation {
    fn index(self) -> u64 {
        match self {
            Self::Staccato => 0,
            Self::Normal => 1,
            Self::Legato => 2,
        }
    }

    fn from_index(index: u64) -> RankResult<Self> {
        match index {
            0 => Ok(Self::Staccato),
            1 => Ok(Self::Normal),
            2 => Ok(Self::Legato),
            _ => Err(invalid_node("music articulation index is outside range")),
        }
    }
}

impl RankMusicSpec {
    /// Builds a music spec, validating that the duration and sequence bounds
    /// are positive and that any fixed total fits the duration bound.
    pub fn new(
        max_denominator: u64,
        max_duration_numerator: u64,
        max_rhythm_len: usize,
        fixed_total: Option<RankDuration>,
        max_melody_len: usize,
        max_progression_len: usize,
    ) -> RankResult<Self> {
        if max_denominator == 0 || max_duration_numerator == 0 {
            return Err(invalid_node("music duration bounds must be positive"));
        }
        if max_rhythm_len == 0 || max_melody_len == 0 || max_progression_len == 0 {
            return Err(invalid_node("music sequence bounds must be positive"));
        }
        if let Some(total) = fixed_total
            && total.denominator > max_denominator
        {
            return Err(invalid_node(
                "fixed rhythm total denominator exceeds duration bound",
            ));
        }
        Ok(Self {
            max_denominator,
            max_duration_numerator,
            max_rhythm_len,
            fixed_total,
            max_melody_len,
            max_progression_len,
        })
    }

    /// Returns the fixed total rhythm duration, if one is required.
    pub fn fixed_total(&self) -> Option<RankDuration> {
        self.fixed_total
    }
}

impl RankRhythmCodec {
    /// Builds the codec by generating and lexicographically ordering all
    /// rhythms allowed by `spec`.
    pub fn new(spec: RankMusicSpec) -> RankResult<Self> {
        let mut nodes = generate_rhythms(&spec)?;
        nodes.sort_by_key(rank_music_lex_key);
        let mut duration_simple = ordinals(nodes.len());
        duration_simple.sort_by_key(|ordinal| {
            let node = &nodes[nat_to_index(ordinal, nodes.len(), "rhythm ordinal").unwrap()];
            (rhythm_simplicity(node), rank_music_lex_key(node))
        });
        Ok(Self {
            spec,
            nodes,
            duration_simple,
        })
    }

    /// Returns the bounding spec for this codec.
    pub fn spec(&self) -> &RankMusicSpec {
        &self.spec
    }
}

impl RankMelodyCodec {
    /// Builds the codec by generating and lexicographically ordering all
    /// melodies allowed by `spec`, and precomputing the low-motion order.
    pub fn new(spec: RankMusicSpec) -> RankResult<Self> {
        let mut nodes = generate_melodies(&spec)?;
        nodes.sort_by_key(rank_music_lex_key);
        let mut low_motion = ordinals(nodes.len());
        low_motion.sort_by_key(|ordinal| {
            let node = &nodes[nat_to_index(ordinal, nodes.len(), "melody ordinal").unwrap()];
            (
                melody_motion(node),
                melody_span(node),
                rank_music_lex_key(node),
            )
        });
        Ok(Self {
            spec,
            nodes,
            low_motion,
        })
    }

    /// Returns the bounding spec for this codec.
    pub fn spec(&self) -> &RankMusicSpec {
        &self.spec
    }
}

impl RankProgressionCodec {
    /// Builds the codec by generating and lexicographically ordering all
    /// progressions allowed by `spec`, precomputing the cadence-first and
    /// low-motion orders.
    pub fn new(spec: RankMusicSpec) -> RankResult<Self> {
        let mut nodes = generate_progressions(&spec)?;
        nodes.sort_by_key(rank_music_lex_key);
        let mut cadence_first = ordinals(nodes.len());
        cadence_first.sort_by_key(|ordinal| {
            let node = &nodes[nat_to_index(ordinal, nodes.len(), "progression ordinal").unwrap()];
            (
                cadence_score(node),
                progression_motion(node),
                rank_music_lex_key(node),
            )
        });
        let mut low_motion = ordinals(nodes.len());
        low_motion.sort_by_key(|ordinal| {
            let node = &nodes[nat_to_index(ordinal, nodes.len(), "progression ordinal").unwrap()];
            (
                progression_motion(node),
                cadence_score(node),
                rank_music_lex_key(node),
            )
        });
        Ok(Self {
            spec,
            nodes,
            cadence_first,
            low_motion,
        })
    }

    /// Returns the bounding spec for this codec.
    pub fn spec(&self) -> &RankMusicSpec {
        &self.spec
    }
}

macro_rules! impl_finite_music_codec {
    ($ty:ty, $id:literal) => {
        impl RankCodec for $ty {
            fn id(&self) -> Symbol {
                Symbol::qualified("rank-codec/music", $id)
            }

            fn version(&self) -> RankVersion {
                RankVersion::v1()
            }

            fn count(&self) -> Option<Nat> {
                Some(Nat::from(self.nodes.len()))
            }

            fn r_ok(&self, r: &Nat) -> bool {
                nat_to_index(r, self.nodes.len(), "music ordinal").is_ok()
            }

            fn rank_node(&self, node: &RankNode) -> RankResult<Nat> {
                self.nodes
                    .iter()
                    .position(|candidate| candidate == node)
                    .map(Nat::from)
                    .ok_or_else(|| invalid_node("music node is outside the bounded space"))
            }

            fn unrank_node(&self, r: &Nat) -> RankResult<RankNode> {
                let index = nat_to_index(r, self.nodes.len(), "music ordinal")?;
                Ok(self.nodes[index].clone())
            }
        }
    };
}

impl_finite_music_codec!(RankRhythmCodec, "rhythm");
impl_finite_music_codec!(RankMelodyCodec, "melody");
impl_finite_music_codec!(RankProgressionCodec, "progression");

impl RankMelodyNeighborhood {
    /// Builds a melody neighborhood bounded by `spec`.
    pub fn new(spec: RankMusicSpec) -> Self {
        Self { spec }
    }
}

impl RankNeighborhood for RankMelodyNeighborhood {
    fn id(&self) -> Symbol {
        Symbol::qualified("rank-metric/music", "melody")
    }

    fn version(&self) -> RankVersion {
        RankVersion::v1()
    }

    fn neighbors(
        &self,
        codec: &dyn RankCodec,
        ordinal: &Nat,
        limits: &mut RankLimits,
    ) -> RankResult<Vec<Nat>> {
        limits.consume(1, "rank.music.neighbors")?;
        let node = codec.unrank_node(ordinal)?;
        let candidates = melody_neighbor_nodes(&self.spec, &node)?;
        valid_ordinals(codec, ordinal, candidates, limits)
    }

    fn distance(
        &self,
        codec: &dyn RankCodec,
        a: &Nat,
        b: &Nat,
        limits: &mut RankLimits,
    ) -> RankResult<Option<Nat>> {
        limits.consume(1, "rank.music.distance")?;
        if a == b {
            return Ok(Some(Nat::zero()));
        }
        let left = codec.unrank_node(a)?;
        let right = codec.unrank_node(b)?;
        Ok(Some(Nat::from(melody_node_distance(&left, &right).max(1))))
    }

    fn mutate(
        &self,
        codec: &dyn RankCodec,
        ordinal: &Nat,
        seed: u64,
        limits: &mut RankLimits,
    ) -> RankResult<Nat> {
        let neighbors = self.neighbors(codec, ordinal, limits)?;
        if neighbors.is_empty() {
            return Ok(ordinal.clone());
        }
        Ok(neighbors[(seed as usize) % neighbors.len()].clone())
    }

    fn crossover(
        &self,
        codec: &dyn RankCodec,
        a: &Nat,
        b: &Nat,
        seed: u64,
        limits: &mut RankLimits,
    ) -> RankResult<Nat> {
        limits.consume(1, "rank.music.crossover")?;
        let left = codec.unrank_node(a)?;
        let right = codec.unrank_node(b)?;
        for candidate in melody_crossover_nodes(&left, &right, seed)? {
            let rank = codec.rank_node(&candidate);
            if let Ok(rank) = rank
                && &rank != a
                && codec.r_ok(&rank)
            {
                return Ok(rank);
            }
        }
        self.mutate(codec, a, seed, limits)
    }
}

/// Builds the duration-simplicity exact order for a rhythm codec, placing
/// rhythms with fewer, simpler rational durations first.
pub fn rank_rhythm_duration_simple_order(codec: &RankRhythmCodec) -> RankResult<RankExactOrder> {
    RankExactOrder::new(
        Symbol::qualified("rank-music-order", "duration-simple"),
        codec.duration_simple.clone(),
    )
}

/// Builds the low-motion exact order for a melody codec, placing melodies with
/// the least adjacent pitch motion first.
pub fn rank_melody_low_motion_order(codec: &RankMelodyCodec) -> RankResult<RankExactOrder> {
    RankExactOrder::new(
        Symbol::qualified("rank-music-order", "low-motion"),
        codec.low_motion.clone(),
    )
}

/// Builds the cadence-first exact order for a progression codec, placing
/// progressions with stronger dominant-to-tonic cadences first.
pub fn rank_progression_cadence_first_order(
    codec: &RankProgressionCodec,
) -> RankResult<RankExactOrder> {
    RankExactOrder::new(
        Symbol::qualified("rank-music-order", "cadence-first"),
        codec.cadence_first.clone(),
    )
}

/// Builds the low-motion exact order for a progression codec, placing
/// progressions with the least root motion between chords first.
pub fn rank_progression_low_motion_order(
    codec: &RankProgressionCodec,
) -> RankResult<RankExactOrder> {
    RankExactOrder::new(
        Symbol::qualified("rank-music-order", "progression-low-motion"),
        codec.low_motion.clone(),
    )
}

/// Returns a human-readable explanation of a named music order, or `None` for
/// an unknown order symbol.
pub fn rank_music_order_explanation(order: &Symbol) -> Option<&'static str> {
    match order.as_qualified_str().as_str() {
        "rank-music-order/duration-simple" => {
            Some("early rhythms use fewer, simpler rational durations")
        }
        "rank-music-order/low-motion" => Some("early melodies minimize adjacent pitch motion"),
        "rank-music-order/cadence-first" => {
            Some("early progressions end with stronger dominant-to-tonic cadence")
        }
        "rank-music-order/progression-low-motion" => {
            Some("early progressions minimize root motion between chords")
        }
        _ => None,
    }
}

/// Builds a `RankNode` for a pitch class, requiring `pitch` in the range
/// `0..12`.
pub fn rank_pitch_class_node(pitch: u8) -> RankResult<RankNode> {
    if pitch >= 12 {
        return Err(invalid_node("pitch class must be in 0..12"));
    }
    Ok(RankNode::Enum {
        id: pitch_class_symbol(),
        index: Nat::from(u64::from(pitch)),
    })
}

/// Builds a `RankNode` encoding a duration as a numerator/denominator product.
pub fn rank_duration_node(duration: RankDuration) -> RankNode {
    RankNode::Product(vec![
        RankNode::Nat(Nat::from(duration.numerator)),
        RankNode::Nat(Nat::from(duration.denominator)),
    ])
}

/// Builds a `RankNode` for a rest of the given duration (sum tag for rests).
pub fn rank_rest_node(duration: RankDuration) -> RankNode {
    RankNode::sum(REST_TAG, rank_duration_node(duration))
}

/// Builds a `RankNode` for a note from its pitch class, duration, and
/// articulation (sum tag for notes).
pub fn rank_note_node(
    pitch: u8,
    duration: RankDuration,
    articulation: RankArticulation,
) -> RankResult<RankNode> {
    Ok(RankNode::sum(
        NOTE_TAG,
        RankNode::Product(vec![
            rank_pitch_class_node(pitch)?,
            rank_duration_node(duration),
            RankNode::Enum {
                id: articulation_symbol(),
                index: Nat::from(articulation.index()),
            },
        ]),
    ))
}

/// Returns the lexicographic sort key used to canonically order music nodes.
pub fn rank_music_lex_key(node: &RankNode) -> String {
    format!("{node:?}")
}

#[cfg(test)]
#[path = "music_tests.rs"]
mod music_tests;

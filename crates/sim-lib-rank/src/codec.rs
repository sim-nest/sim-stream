//! Core `RankCodec` trait: the rank/unrank contract for a coordinate space.
//!
//! A rank codec is a bijection between structured nodes and natural ordinals,
//! letting any conforming grammar serve as an indexable retrieval space.

use sim_kernel::Symbol;

use crate::{RankNode, RankResult, RankVersion, nat::Nat};

/// Bijection between [`RankNode`] values and natural ordinals for one space.
///
/// Implementors map each inhabitant of a grammar to a unique natural number and
/// back, providing the indexing primitive behind retrieval and ordering.
pub trait RankCodec: Send + Sync + std::fmt::Debug {
    /// Returns the stable symbol identifying this codec.
    fn id(&self) -> Symbol;
    /// Returns the codec version, used to gate compatibility of stored ordinals.
    fn version(&self) -> RankVersion;
    /// Returns the count of inhabitants, or `None` if the space is unbounded.
    fn count(&self) -> Option<Nat>;
    /// Reports whether ordinal `r` is in range for this codec.
    fn r_ok(&self, r: &Nat) -> bool;
    /// Ranks a node to its natural ordinal within this space.
    fn rank_node(&self, node: &RankNode) -> RankResult<Nat>;
    /// Unranks an ordinal back to its node within this space.
    fn unrank_node(&self, r: &Nat) -> RankResult<RankNode>;
}

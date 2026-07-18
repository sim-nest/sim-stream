//! Exact (fully materialized) orderings over a ranked ordinal space.
//!
//! Defines [`RankExactOrder`], a total ordering that maps positions to
//! canonical ordinals and back, imposing a learned or constructed rank order on
//! the candidates a codec enumerates.

use std::collections::BTreeSet;

use sim_kernel::Symbol;

use crate::{RankCodec, RankError, RankNode, RankResult, nat::Nat};

/// Exact total ordering over a finite ranked ordinal space.
///
/// Holds an explicit permutation: position `i` maps to a canonical ordinal,
/// with the inverse map cached so positions and canonical ordinals can be
/// looked up in either direction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankExactOrder {
    id: Symbol,
    canonical_by_position: Vec<Nat>,
    position_by_canonical: Vec<Nat>,
}

impl RankExactOrder {
    /// Builds an exact order from a position-to-canonical-ordinal mapping.
    ///
    /// Validates that every ordinal is in range and that no ordinal repeats,
    /// then precomputes the inverse (canonical-to-position) lookup.
    pub fn new(id: Symbol, canonical_by_position: Vec<Nat>) -> RankResult<Self> {
        let count = canonical_by_position.len();
        let mut seen = BTreeSet::new();
        for ordinal in &canonical_by_position {
            let index = nat_to_index(ordinal, count, "rank order canonical ordinal")?;
            if !seen.insert(index) {
                return Err(RankError::InvalidNode {
                    message: format!("rank exact order {id} contains duplicate ordinal {ordinal}"),
                });
            }
        }
        let mut position_by_canonical = vec![Nat::zero(); count];
        for (position, canonical) in canonical_by_position.iter().enumerate() {
            let canonical_index = nat_to_index(canonical, count, "rank order canonical ordinal")?;
            position_by_canonical[canonical_index] = Nat::from(position);
        }
        Ok(Self {
            id,
            canonical_by_position,
            position_by_canonical,
        })
    }

    /// Returns the symbol identifying this order.
    pub fn id(&self) -> &Symbol {
        &self.id
    }

    /// Returns the number of positions (and canonical ordinals) in the order.
    pub fn count(&self) -> Nat {
        Nat::from(self.canonical_by_position.len())
    }

    /// Reports whether the order contains no positions.
    pub fn is_empty(&self) -> bool {
        self.canonical_by_position.is_empty()
    }

    /// Returns the canonical ordinals indexed by position, in rank order.
    pub fn canonical_ordinals(&self) -> &[Nat] {
        &self.canonical_by_position
    }

    /// Returns the canonical ordinal occupying the given rank position.
    pub fn canonical_ordinal(&self, position: &Nat) -> RankResult<Nat> {
        let index = nat_to_index(
            position,
            self.canonical_by_position.len(),
            "rank order position",
        )?;
        Ok(self.canonical_by_position[index].clone())
    }

    /// Returns the rank position of a canonical ordinal (the inverse map).
    pub fn position_of(&self, canonical: &Nat) -> RankResult<Nat> {
        let index = nat_to_index(
            canonical,
            self.position_by_canonical.len(),
            "rank order canonical ordinal",
        )?;
        Ok(self.position_by_canonical[index].clone())
    }

    /// Decodes the node at the given rank position via the codec.
    ///
    /// Translates the position to its canonical ordinal, then asks the codec to
    /// unrank that ordinal into a [`RankNode`].
    pub fn unrank_node(&self, codec: &dyn RankCodec, position: &Nat) -> RankResult<RankNode> {
        codec.unrank_node(&self.canonical_ordinal(position)?)
    }

    /// Ranks a node into its position under this order via the codec.
    ///
    /// Asks the codec for the node's canonical ordinal, then maps that ordinal
    /// to its position in this order.
    pub fn rank_node(&self, codec: &dyn RankCodec, node: &RankNode) -> RankResult<Nat> {
        self.position_of(&codec.rank_node(node)?)
    }
}

/// Converts a [`Nat`] ordinal into a bounded `usize` index.
///
/// Fails with an out-of-range error when the value does not fit in `usize` or
/// reaches `upper_bound`; `what` names the value for the diagnostic message.
pub(crate) fn nat_to_index(
    value: &Nat,
    upper_bound: usize,
    what: &'static str,
) -> RankResult<usize> {
    let index = value
        .to_decimal_string()
        .parse::<usize>()
        .map_err(|_| RankError::InvalidNode {
            message: format!("{what} {value} does not fit in usize"),
        })?;
    if index >= upper_bound {
        return Err(RankError::OrdinalOutOfRange {
            ordinal: value.to_string(),
            count: upper_bound.to_string(),
        });
    }
    Ok(index)
}

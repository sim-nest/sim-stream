//! Composition of exact rank orders.
//!
//! Combines two [`RankExactOrder`] permutations over the same space into a
//! single order, so orderings can be layered (rank by `first`, then re-rank by
//! `second`).

use sim_kernel::Symbol;

use crate::{Nat, RankError, RankExactOrder, RankResult};

/// Composes two exact orders into one by chaining their permutations.
///
/// Applies `second` and then `first`: position `p` takes `second`'s canonical
/// ordinal at `p`, then `first`'s canonical ordinal at that value. Both orders
/// must cover the same number of ordinals.
pub fn then_order(
    id: Symbol,
    first: &RankExactOrder,
    second: &RankExactOrder,
) -> RankResult<RankExactOrder> {
    if first.count() != second.count() {
        return Err(RankError::InvalidNode {
            message: format!(
                "cannot compose rank exact orders with counts {} and {}",
                first.count(),
                second.count()
            ),
        });
    }
    let mut ordinals = Vec::with_capacity(second.canonical_ordinals().len());
    for position in 0..second.canonical_ordinals().len() {
        let position = Nat::from(position);
        let first_position = second.canonical_ordinal(&position)?;
        ordinals.push(first.canonical_ordinal(&first_position)?);
    }
    RankExactOrder::new(id, ordinals)
}

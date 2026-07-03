//! Built-in constructors for common exact rank orders.
//!
//! Provides ready-made orderings over a ranked ordinal space -- identity
//! permutations, grade-first traversals, reverse windows, round-robin
//! interleavings, and seeded shuffles -- each producing a [`RankExactOrder`].

use sim_kernel::Symbol;

use crate::{
    GroupCodec, RankError, RankExactOrder, RankResult, grade::RankGrade, nat::Nat,
    order::nat_to_index,
};

/// Builds an exact order from an explicit sequence of canonical ordinals.
///
/// The iterator yields, for each rank position in turn, the `usize` ordinal
/// that occupies it.
pub fn exact_permutation(
    id: Symbol,
    canonical_by_position: impl IntoIterator<Item = usize>,
) -> RankResult<RankExactOrder> {
    RankExactOrder::new(
        id,
        canonical_by_position.into_iter().map(Nat::from).collect(),
    )
}

/// Builds the identity order `0, 1, ..., count - 1`.
///
/// Position equals canonical ordinal, leaving the codec's native rank order
/// unchanged.
pub fn canonical_order(id: Symbol, count: usize) -> RankResult<RankExactOrder> {
    exact_permutation(id, 0..count)
}

/// Builds an order that visits ordinals grade by grade, low grade first.
///
/// Walks each grade up to `max_grade`, emitting that grade's group members in
/// ordinal order before advancing, so lower-grade candidates rank ahead.
pub fn grade_first_order(
    id: Symbol,
    codec: &dyn GroupCodec,
    max_grade: RankGrade,
) -> RankResult<RankExactOrder> {
    let mut ordinals = Vec::new();
    let mut offset = Nat::zero();
    for grade in 0..=max_grade {
        let count = codec.group_count_at(grade)?;
        let count_index = nat_to_index(&count, usize::MAX, "rank grade count")?;
        for index in 0..count_index {
            ordinals.push(offset.checked_add(&Nat::from(index)));
        }
        offset = offset.checked_add(&count);
    }
    RankExactOrder::new(id, ordinals)
}

/// Builds an order that reverses ordinals within each fixed-size window.
///
/// Splits `0..count` into consecutive blocks of `window` and reverses each
/// block, preserving block order. Errors when `window` is zero.
pub fn reverse_window_order(id: Symbol, count: usize, window: usize) -> RankResult<RankExactOrder> {
    if window == 0 {
        return Err(invalid_order(
            "reverse-window size must be greater than zero",
        ));
    }
    let mut ordinals = Vec::with_capacity(count);
    let mut start = 0;
    while start < count {
        let end = start.saturating_add(window).min(count);
        ordinals.extend((start..end).rev());
        start = end;
    }
    exact_permutation(id, ordinals)
}

/// Builds an order that interleaves ordinals across a fixed number of lanes.
///
/// Assigns ordinals to `lanes` by residue and emits lane 0 first, then lane 1,
/// and so on, so each lane's candidates are grouped contiguously. Errors when
/// `lanes` is zero.
pub fn round_robin_order(id: Symbol, count: usize, lanes: usize) -> RankResult<RankExactOrder> {
    if lanes == 0 {
        return Err(invalid_order(
            "round-robin lane count must be greater than zero",
        ));
    }
    let mut ordinals = Vec::with_capacity(count);
    for lane in 0..lanes.min(count.max(1)) {
        let mut index = lane;
        while index < count {
            ordinals.push(index);
            index = index.saturating_add(lanes);
        }
    }
    exact_permutation(id, ordinals)
}

/// Builds a deterministic pseudo-random permutation order from a seed.
///
/// Applies a seeded Fisher-Yates shuffle (a fixed LCG drives the swaps), so the
/// same `seed` and `count` always yield the same order.
pub fn seeded_shuffle_order(id: Symbol, count: usize, seed: u64) -> RankResult<RankExactOrder> {
    let mut ordinals = (0..count).collect::<Vec<_>>();
    let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
    for upper in (1..ordinals.len()).rev() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let swap_with = (state as usize) % (upper + 1);
        ordinals.swap(upper, swap_with);
    }
    exact_permutation(id, ordinals)
}

fn invalid_order(message: impl Into<String>) -> RankError {
    RankError::InvalidNode {
        message: message.into(),
    }
}

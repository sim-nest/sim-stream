//! Semantic stream time refs shared by clocks, combinators, and placement.
//!
//! Kernel [`Tick`] values carry a clock symbol plus a generic [`Ref`]. This
//! module defines the stream-core convention for numeric clock indexes in that
//! generic slot so stream operators can compare clock meaning directly without
//! ordering opaque content ids.

use sim_kernel::{Error, Ref, Result, Symbol, Tick};

/// Namespace used by stream clock-index refs.
pub const CLOCK_INDEX_REF_NAMESPACE: &str = "stream/clock-index";

/// A comparable clock index extracted from a stream [`Tick`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClockTickIndex(u64);

impl ClockTickIndex {
    /// Builds a comparable clock-index key from its raw index value.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw clock-index value.
    pub fn value(self) -> u64 {
        self.0
    }
}

/// Builds the semantic [`Ref`] representation for a stream clock index.
///
/// The tick's clock symbol carries the clock identity; the ref stores only the
/// index in a parseable namespace so byte ordering is never used as clock time.
pub fn clock_index_ref(index: u64) -> Ref {
    Ref::Symbol(clock_index_symbol(index))
}

/// Builds the symbol used by [`clock_index_ref`].
pub fn clock_index_symbol(index: u64) -> Symbol {
    Symbol::qualified(CLOCK_INDEX_REF_NAMESPACE, index.to_string())
}

/// Extracts a comparable index for `expected_clock` from `tick`.
///
/// Returns `Ok(None)` when the tick belongs to another clock. Returns an error
/// when the tick is on `expected_clock` but does not carry the semantic
/// stream-clock index ref representation.
pub fn tick_clock_index(tick: &Tick, expected_clock: &Symbol) -> Result<Option<ClockTickIndex>> {
    if &tick.clock != expected_clock {
        return Ok(None);
    }
    let Ref::Symbol(symbol) = &tick.index else {
        return Err(incomparable_tick_error(tick));
    };
    if symbol.namespace.as_deref() != Some(CLOCK_INDEX_REF_NAMESPACE) {
        return Err(incomparable_tick_error(tick));
    }
    let index = symbol.name.parse::<u64>().map_err(|err| {
        Error::Eval(format!(
            "stream tick on clock {} has invalid semantic index {}: {err}",
            tick.clock.as_qualified_str(),
            symbol.name
        ))
    })?;
    Ok(Some(ClockTickIndex::new(index)))
}

fn incomparable_tick_error(tick: &Tick) -> Error {
    Error::Eval(format!(
        "stream tick on clock {} has incomparable index {:?}; expected {CLOCK_INDEX_REF_NAMESPACE}/<u64>",
        tick.clock.as_qualified_str(),
        tick.index
    ))
}

#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Clock charts and tick conversion helpers for SIM streams.
//!
//! The crate keeps clock math in library space. Kernel events still carry only
//! kernel `Tick` values, with clock indexes interned as datum content refs.

mod citizen;
mod clock;
pub mod cookbook;
mod instant;
mod tempo;

pub use citizen::{StreamClockDescriptor, stream_clock_class_symbol};
pub use clock::{Clock, ClockChart, ClockIndex, IndexConversion};
pub use cookbook::tempo_chart_demo;
pub use instant::Instant;
pub use tempo::{TempoMap, TempoSegment};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

#[cfg(test)]
mod tests;

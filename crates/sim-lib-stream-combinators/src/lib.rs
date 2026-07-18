#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Lazy in-memory combinators for SIM stream packets.
//!
//! This crate composes stream-core packet spines without talking to devices,
//! files, or transports. Runtime surfaces lower graph forms into these
//! Rust-level combinators.

mod bridge;
mod cell;
pub mod cookbook;
mod event_algebra;
mod ops;
mod recording;
mod stream;

pub use bridge::{event_rate_gate, jitter_buffer, latency_comp_delay, resample_pcm};
pub use cell::{CellSnapshot, StreamCell, stream_cell};
pub use cookbook::pipeline_stages_demo;
pub use event_algebra::{
    event_join_data_kind, expr_path, filter_data_field_eq, join_data_on_field,
    model_event_data_kind, project_data_field, rank_data_by_i64_field, rank_frontier_data_kind,
    redact_data_field,
};
pub use ops::{
    ClockConvertedStream, StreamStage, clock_convert, fan, filter, filter_data_kind,
    filter_data_kind_stage, filter_data_shape, filter_data_shape_stage, filter_stage, identity,
    map, map_data_expr, map_data_expr_stage, map_stage, merge, merge_by_clock, pipe, run_bang,
    stream_window_data_kind, take, take_stage, tap, tap_diagnostics, tap_diagnostics_stage,
    tap_stage, window_by_count, window_by_count_stage,
};
pub use recording::{
    DEFAULT_RECORD_ITEM_LIMIT, SeekTarget, StreamRecording, record_bang, record_bang_bounded,
    record_cassette_bang, record_cassette_bang_bounded, record_events, record_ledger_run,
    record_ledger_slice, replay, replay_cassette, seek,
};
pub use stream::{Stream, StreamNode};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

#[cfg(test)]
mod bridge_tests;

#[cfg(test)]
mod event_algebra_tests;
#[cfg(test)]
mod tests;

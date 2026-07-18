#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Lisp-facing STREAM 6 prelude for memory streams.
//!
//! This is the umbrella crate that composes the streaming fabric (core, audio,
//! and combinators) into a single host-registered library. It installs
//! capability-gated public functions for opening deterministic memory MIDI and
//! PCM sources/sinks, running source-to-sink pipelines, applying combinator
//! stages, and browsing stream handles as Cards.
//!
//! Use [`install_stream_prelude_lib`] to install the prelude (and its
//! stream-core prerequisites) into a runtime; [`StreamPreludeLib`] is the
//! underlying [`sim_kernel::Lib`] for callers that manage loading directly. The
//! `stream_*_symbol` and `stream_*_capability` helpers name the functions,
//! values, and capabilities the library exports, and [`StreamHandle`] is the
//! live handle threaded through every stream operation.

mod cap;
mod card;
pub mod cookbook;
mod function;
mod handle;
mod live;
mod live_control;
mod spec;
mod transform;

pub use cap::{
    stream_cancel_capability, stream_control_capability, stream_open_capability,
    stream_push_capability, stream_read_capability, stream_stats_capability,
    stream_transform_capability, stream_write_capability,
};
pub use cookbook::memory_pipe_demo;
pub use function::{
    StreamPreludeLib, install_stream_prelude_lib, stream_card_symbol, stream_memory_specs_symbol,
    stream_open_symbol, stream_pipe_symbol, stream_sink_packets_symbol, stream_write_symbol,
};
pub use handle::{RunReport, StageHandle, StreamHandle};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

#[cfg(test)]
mod tests;

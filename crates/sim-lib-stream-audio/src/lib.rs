#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! PCM source and sink adapters for STREAM 6.
//!
//! This crate keeps PCM I/O in library space. The in-memory source and sink
//! are deterministic test backends; stream observation still goes through
//! `sim-lib-stream-core` packets and spines.

mod buffer;
mod citizen;
mod io;
mod spec;
mod spine;

pub use buffer::{
    PcmBuffer, f32_interleaved_to_planar, f32_planar_to_interleaved, f32_sample_to_i16,
    f32_samples_to_i16, i16_interleaved_to_planar, i16_planar_to_interleaved, i16_sample_to_f32,
    i16_samples_to_f32,
};
pub use citizen::{PcmFormatDescriptor, pcm_format_class_symbol};
pub use io::{MemoryPcmSink, MemoryPcmSource, PcmPumpSummary, PcmSink, PcmSource, pump_pcm};
pub use spec::{PcmSampleFormat, PcmSpec};
pub use spine::{pcm_source_to_stream, stream_to_pcm_sink};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

#[cfg(test)]
mod tests;

#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Worn-device sample contracts and deterministic modeled wrist sources.
//!
//! The crate keeps watch and wearable sensor events in library space. Each
//! event is a strict [`sim_lib_stream_device::DeviceSample`] with a stable
//! sensor tag, a confidence score, a monotone sequence number, and a payload
//! expression. Modeled sources are index-driven so tests and demos can exercise
//! wrist streams without clocks, random data, hardware, or network access.

pub mod citizen;
pub mod cookbook;
pub mod modeled;
pub mod quorum;
pub mod worn;

/// Cookbook recipes for this crate, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

pub use citizen::{
    WornEventValue, WristStreamLib, install_wrist_stream_lib, worn_event_class_symbol,
    wrist_stream_exports, wrist_stream_manifest_symbol,
};
pub use cookbook::worn_heart_rate_descriptor_demo;
pub use modeled::{
    ModeledBatterySource, ModeledConnectionSource, ModeledHeartRateSource, ModeledLocationSource,
    ModeledMotionSource,
};
pub use quorum::{HeartRateQuorum, QuorumSide, heart_rate_quorum};
pub use worn::{
    MicAudioFrame, WORN_CONFIDENCE_MAX, WornEvent, WornSensor, mic_audio_raw_frame_symbol,
    worn_event_sample_kind_symbol,
};

#[cfg(test)]
mod quorum_tests;

#[cfg(test)]
mod worn_tests;

#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Device sample contracts and deterministic modeled sources for SIM streams.
//!
//! The crate keeps physical-device samples in library space. Samples encode as
//! ordinary stream data packets with stable kind tags and monotone sequence
//! numbers, while modeled sources produce deterministic hardware-free fixtures
//! for CI and downstream device instances.

pub mod citizen;
pub mod cookbook;
pub mod modeled;
pub mod sample;

/// Cookbook recipes for this crate, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

pub use citizen::{
    DeviceSampleValue, DeviceStreamBaseLib, device_sample_class_symbol, device_stream_base_exports,
    device_stream_base_manifest_symbol, install_device_stream_base,
};
pub use cookbook::device_caps_descriptor_demo;
pub use modeled::{ModeledDeviceCapsSource, ModeledSource, seq_is_monotone};
pub use sample::{
    DeviceCaps, DeviceSample, DeviceSampleError, DeviceSampleResult,
    device_caps_sample_kind_symbol, device_sample_record_symbol, roundtrip_ok, sample_kind_symbol,
    sample_packet,
};

#[cfg(test)]
mod sample_tests;

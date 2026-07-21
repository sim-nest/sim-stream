#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! XR glasses sample contracts and deterministic modeled sources.
//!
//! The crate keeps glasses-side sensor inputs in library space. Each sample is
//! a strict [`sim_lib_stream_device::DeviceSample`] with a stable `xr/*` sample
//! kind and a monotone sequence number. Modeled sources are index-driven so
//! tests and demos can exercise Viture-style and Halo-style streams without
//! clocks, random data, hardware, or network access.

pub mod camera;
pub mod citizen;
pub mod cookbook;
pub mod hand;
pub mod mic;
pub mod modeled;
pub mod pose;
pub mod tap;

mod wire;

/// Cookbook recipes for this crate, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

pub use camera::{XrCameraFrameRef, xr_camera_frame_sample_kind_symbol};
pub use citizen::{
    XrSampleValue, XrStreamLib, install_xr_stream_lib, xr_sample_class_symbol, xr_stream_exports,
    xr_stream_manifest_symbol,
};
pub use cookbook::xr_modeled_descriptor_demo;
pub use hand::{XR_CONFIDENCE_MAX, XrHandSample, xr_hand_sample_kind_symbol};
pub use mic::{XrMicChunkRef, xr_mic_chunk_sample_kind_symbol};
pub use modeled::{
    ModeledHaloCameraSource, ModeledHaloMicSource, ModeledHaloMotionSource, ModeledHaloTapSource,
    ModeledVitureHandSource, ModeledViturePoseSource, ModeledVitureStereoCameraSource,
    halo_device_symbol, viture_device_symbol,
};
pub use pose::{XrPoseSample, XrTrackingStatus, xr_pose_sample_kind_symbol};
pub use tap::{XrTapSample, xr_tap_sample_kind_symbol};

#[cfg(test)]
mod xr_tests;

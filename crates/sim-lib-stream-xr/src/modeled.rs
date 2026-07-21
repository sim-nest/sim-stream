//! Deterministic modeled XR glasses sources.

use sim_kernel::Symbol;
use sim_lib_stream_device::ModeledSource;

use crate::{
    XrCameraFrameRef, XrHandSample, XrMicChunkRef, XrPoseSample, XrTapSample, XrTrackingStatus,
};

const VITURE_NS_STEP: u64 = 8_333_333;
const HALO_NS_STEP: u64 = 33_333_333;
const HALF_TURN_COMPONENT: f64 = std::f64::consts::FRAC_1_SQRT_2;

const VITURE_ORBIT: [([f64; 3], [f64; 4]); 8] = [
    ([0.18, 1.62, -0.55], [1.0, 0.0, 0.0, 0.0]),
    ([0.13, 1.64, -0.62], [0.9238795, 0.0, 0.3826834, 0.0]),
    (
        [0.0, 1.65, -0.66],
        [HALF_TURN_COMPONENT, 0.0, HALF_TURN_COMPONENT, 0.0],
    ),
    ([-0.13, 1.64, -0.62], [0.3826834, 0.0, 0.9238795, 0.0]),
    ([-0.18, 1.62, -0.55], [0.0, 0.0, 1.0, 0.0]),
    ([-0.13, 1.6, -0.48], [0.3826834, 0.0, -0.9238795, 0.0]),
    (
        [0.0, 1.59, -0.44],
        [HALF_TURN_COMPONENT, 0.0, -HALF_TURN_COMPONENT, 0.0],
    ),
    ([0.13, 1.6, -0.48], [0.9238795, 0.0, -0.3826834, 0.0]),
];

const HALO_ORIENTATION: [[f64; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.9987503, 0.0, 0.0499792, 0.0],
    [0.9950042, 0.0, 0.0998334, 0.0],
    [0.9987503, 0.0, -0.0499792, 0.0],
];

/// Returns the modeled Viture Luma Ultra device symbol.
pub fn viture_device_symbol() -> Symbol {
    Symbol::qualified("device/glasses", "viture-luma-ultra")
}

/// Returns the modeled Brilliant Labs Halo device symbol.
pub fn halo_device_symbol() -> Symbol {
    Symbol::qualified("device/glasses", "brilliant-halo")
}

/// Deterministic modeled Viture 6DoF pose source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeledViturePoseSource;

impl ModeledSource for ModeledViturePoseSource {
    type Sample = XrPoseSample;

    fn at(&self, index: u64) -> Self::Sample {
        let pose = orbit_pose(index);
        XrPoseSample::new(
            index,
            Some(pose.0),
            pose.1,
            index.saturating_mul(VITURE_NS_STEP),
            VITURE_NS_STEP,
            6,
            XrTrackingStatus::Tracked,
        )
        .expect("modeled Viture pose is valid")
    }
}

/// Deterministic modeled Viture stereo camera frame source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeledVitureStereoCameraSource;

impl ModeledSource for ModeledVitureStereoCameraSource {
    type Sample = XrCameraFrameRef;

    fn at(&self, index: u64) -> Self::Sample {
        XrCameraFrameRef::new(
            index,
            viture_device_symbol(),
            Symbol::qualified("stream/xr-camera", "viture-stereo"),
            frame_symbol("viture-stereo", index),
            [1280, 720],
            index.saturating_mul(VITURE_NS_STEP),
            true,
        )
        .expect("modeled Viture camera frame reference is valid")
    }
}

/// Deterministic modeled Viture hand-ray source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeledVitureHandSource;

impl ModeledSource for ModeledVitureHandSource {
    type Sample = XrHandSample;

    fn at(&self, index: u64) -> Self::Sample {
        let right = index.is_multiple_of(2);
        let side = if right { 0.18 } else { -0.18 };
        let hand = if right { "right" } else { "left" };
        let drift = f64::from(u8::try_from(index % 5).expect("bounded hand drift")) * 0.01;
        XrHandSample::new(
            index,
            viture_device_symbol(),
            Symbol::qualified("stream/xr-hand", hand),
            [side, 1.24 + drift, -0.36],
            [side * 0.2, -0.08, -1.0],
            9_400,
            index.saturating_mul(VITURE_NS_STEP),
        )
        .expect("modeled Viture hand sample is valid")
    }
}

/// Deterministic modeled Halo orientation-hint source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeledHaloMotionSource;

impl ModeledSource for ModeledHaloMotionSource {
    type Sample = XrPoseSample;

    fn at(&self, index: u64) -> Self::Sample {
        let orientation = HALO_ORIENTATION[index as usize % HALO_ORIENTATION.len()];
        XrPoseSample::new(
            index,
            None,
            orientation,
            index.saturating_mul(HALO_NS_STEP),
            0,
            3,
            XrTrackingStatus::Limited,
        )
        .expect("modeled Halo motion hint is valid")
    }
}

/// Deterministic modeled Halo tap train source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeledHaloTapSource;

impl ModeledSource for ModeledHaloTapSource {
    type Sample = XrTapSample;

    fn at(&self, index: u64) -> Self::Sample {
        XrTapSample::new(
            index,
            halo_device_symbol(),
            index,
            9_700,
            index.saturating_mul(HALO_NS_STEP),
        )
        .expect("modeled Halo tap sample is valid")
    }
}

/// Deterministic modeled Halo camera frame reference source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeledHaloCameraSource;

impl ModeledSource for ModeledHaloCameraSource {
    type Sample = XrCameraFrameRef;

    fn at(&self, index: u64) -> Self::Sample {
        XrCameraFrameRef::new(
            index,
            halo_device_symbol(),
            Symbol::qualified("stream/xr-camera", "halo-rgb"),
            frame_symbol("halo-rgb", index),
            [640, 480],
            index.saturating_mul(HALO_NS_STEP),
            false,
        )
        .expect("modeled Halo camera frame reference is valid")
    }
}

/// Deterministic modeled Halo microphone chunk reference source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeledHaloMicSource;

impl ModeledSource for ModeledHaloMicSource {
    type Sample = XrMicChunkRef;

    fn at(&self, index: u64) -> Self::Sample {
        XrMicChunkRef::new(index, mic_chunk_symbol(index), 40)
            .expect("modeled Halo mic chunk reference is valid")
    }
}

fn orbit_pose(index: u64) -> ([f64; 3], [f64; 4]) {
    VITURE_ORBIT[index as usize % VITURE_ORBIT.len()]
}

fn frame_symbol(prefix: &str, index: u64) -> Symbol {
    Symbol::qualified("stream/xr-frame", format!("{prefix}-{index:06}"))
}

fn mic_chunk_symbol(index: u64) -> Symbol {
    Symbol::qualified("stream/xr-mic-chunk", format!("halo-canned-{index:06}"))
}

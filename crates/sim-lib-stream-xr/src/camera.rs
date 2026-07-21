//! Strict XR camera frame reference samples.

use sim_kernel::{Expr, Symbol};
use sim_lib_stream_device::{
    DeviceSample, DeviceSampleError, DeviceSampleResult, device_sample_record_symbol,
    sample_kind_symbol,
};
use sim_value::build;

use crate::wire::{
    bool_field, expect_only_fields, expect_sample_tags, map_entries, symbol_field, u32_field,
    u64_field,
};

const XR_CAMERA_FIELDS: &[&str] = &[
    "kind",
    "sample",
    "seq",
    "device",
    "camera",
    "frame-key",
    "width-px",
    "height-px",
    "t-ns",
    "stereo",
];

/// A reference to an XR camera frame held by a stream or device backend.
///
/// The sample carries identity, dimensions, and timing metadata only. It never
/// embeds camera pixels in the sensor record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XrCameraFrameRef {
    seq: u64,
    device: Symbol,
    camera: Symbol,
    frame_key: Symbol,
    width_px: u32,
    height_px: u32,
    t_ns: u64,
    stereo: bool,
}

impl XrCameraFrameRef {
    /// Builds an XR camera frame reference.
    pub fn new(
        seq: u64,
        device: Symbol,
        camera: Symbol,
        frame_key: Symbol,
        size_px: [u32; 2],
        t_ns: u64,
        stereo: bool,
    ) -> DeviceSampleResult<Self> {
        let [width_px, height_px] = size_px;
        if width_px == 0 {
            return Err(DeviceSampleError::new("XR camera width-px must be nonzero"));
        }
        if height_px == 0 {
            return Err(DeviceSampleError::new(
                "XR camera height-px must be nonzero",
            ));
        }
        Ok(Self {
            seq,
            device,
            camera,
            frame_key,
            width_px,
            height_px,
            t_ns,
            stereo,
        })
    }

    /// Returns the monotone sequence number.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Returns the device identity.
    pub fn device(&self) -> &Symbol {
        &self.device
    }

    /// Returns the camera lane identity.
    pub fn camera(&self) -> &Symbol {
        &self.camera
    }

    /// Returns the backend frame reference key.
    pub fn frame_key(&self) -> &Symbol {
        &self.frame_key
    }

    /// Returns the frame width in pixels.
    pub fn width_px(&self) -> u32 {
        self.width_px
    }

    /// Returns the frame height in pixels.
    pub fn height_px(&self) -> u32 {
        self.height_px
    }

    /// Returns the sample timestamp in nanoseconds.
    pub fn t_ns(&self) -> u64 {
        self.t_ns
    }

    /// Reports whether the reference represents paired stereo frames.
    pub fn stereo(&self) -> bool {
        self.stereo
    }
}

impl DeviceSample for XrCameraFrameRef {
    fn sample_kind() -> &'static str {
        "xr/camera-frame"
    }

    fn seq(&self) -> u64 {
        self.seq
    }

    fn to_expr(&self) -> Expr {
        build::map(vec![
            ("kind", Expr::Symbol(device_sample_record_symbol())),
            ("sample", Expr::Symbol(xr_camera_frame_sample_kind_symbol())),
            ("seq", build::uint(self.seq)),
            ("device", Expr::Symbol(self.device.clone())),
            ("camera", Expr::Symbol(self.camera.clone())),
            ("frame-key", Expr::Symbol(self.frame_key.clone())),
            ("width-px", build::uint(u64::from(self.width_px))),
            ("height-px", build::uint(u64::from(self.height_px))),
            ("t-ns", build::uint(self.t_ns)),
            ("stereo", Expr::Bool(self.stereo)),
        ])
    }

    fn from_expr(expr: &Expr) -> DeviceSampleResult<Self> {
        let entries = map_entries(expr, "XR camera frame reference map")?;
        expect_only_fields(entries, XR_CAMERA_FIELDS, "XR camera frame")?;
        expect_sample_tags(
            entries,
            &xr_camera_frame_sample_kind_symbol(),
            "XR camera frame",
        )?;
        Self::new(
            u64_field(entries, "seq", "XR camera frame")?,
            symbol_field(entries, "device", "XR camera frame")?.clone(),
            symbol_field(entries, "camera", "XR camera frame")?.clone(),
            symbol_field(entries, "frame-key", "XR camera frame")?.clone(),
            [
                u32_field(entries, "width-px", "XR camera frame")?,
                u32_field(entries, "height-px", "XR camera frame")?,
            ],
            u64_field(entries, "t-ns", "XR camera frame")?,
            bool_field(entries, "stereo", "XR camera frame")?,
        )
    }
}

/// Returns the qualified sample-kind symbol for [`XrCameraFrameRef`].
pub fn xr_camera_frame_sample_kind_symbol() -> Symbol {
    sample_kind_symbol(XrCameraFrameRef::sample_kind())
}

pub(crate) fn decode_known_camera_frame(expr: &Expr) -> DeviceSampleResult<()> {
    XrCameraFrameRef::from_expr(expr).map(|_| ())
}

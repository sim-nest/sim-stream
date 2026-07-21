//! Strict XR tap input sample records.

use sim_kernel::{Expr, Symbol};
use sim_lib_stream_device::{
    DeviceSample, DeviceSampleError, DeviceSampleResult, device_sample_record_symbol,
    sample_kind_symbol,
};
use sim_value::build;

use crate::{
    hand::XR_CONFIDENCE_MAX,
    wire::{
        expect_only_fields, expect_sample_tags, map_entries, symbol_field, u16_field, u64_field,
    },
};

const XR_TAP_FIELDS: &[&str] = &[
    "kind",
    "sample",
    "seq",
    "device",
    "tap-index",
    "confidence",
    "t-ns",
];

/// A strict XR tap input sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XrTapSample {
    seq: u64,
    device: Symbol,
    tap_index: u64,
    confidence: u16,
    t_ns: u64,
}

impl XrTapSample {
    /// Builds an XR tap sample.
    pub fn new(
        seq: u64,
        device: Symbol,
        tap_index: u64,
        confidence: u16,
        t_ns: u64,
    ) -> DeviceSampleResult<Self> {
        if confidence > XR_CONFIDENCE_MAX {
            return Err(DeviceSampleError::new(format!(
                "XR tap confidence must be <= {XR_CONFIDENCE_MAX}"
            )));
        }
        Ok(Self {
            seq,
            device,
            tap_index,
            confidence,
            t_ns,
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

    /// Returns the tap index within the modeled or real tap train.
    pub fn tap_index(&self) -> u64 {
        self.tap_index
    }

    /// Returns confidence in ten-thousandths.
    pub fn confidence(&self) -> u16 {
        self.confidence
    }

    /// Returns the sample timestamp in nanoseconds.
    pub fn t_ns(&self) -> u64 {
        self.t_ns
    }
}

impl DeviceSample for XrTapSample {
    fn sample_kind() -> &'static str {
        "xr/tap"
    }

    fn seq(&self) -> u64 {
        self.seq
    }

    fn to_expr(&self) -> Expr {
        build::map(vec![
            ("kind", Expr::Symbol(device_sample_record_symbol())),
            ("sample", Expr::Symbol(xr_tap_sample_kind_symbol())),
            ("seq", build::uint(self.seq)),
            ("device", Expr::Symbol(self.device.clone())),
            ("tap-index", build::uint(self.tap_index)),
            ("confidence", build::uint(u64::from(self.confidence))),
            ("t-ns", build::uint(self.t_ns)),
        ])
    }

    fn from_expr(expr: &Expr) -> DeviceSampleResult<Self> {
        let entries = map_entries(expr, "XR tap sample map")?;
        expect_only_fields(entries, XR_TAP_FIELDS, "XR tap")?;
        expect_sample_tags(entries, &xr_tap_sample_kind_symbol(), "XR tap")?;
        Self::new(
            u64_field(entries, "seq", "XR tap")?,
            symbol_field(entries, "device", "XR tap")?.clone(),
            u64_field(entries, "tap-index", "XR tap")?,
            u16_field(entries, "confidence", "XR tap")?,
            u64_field(entries, "t-ns", "XR tap")?,
        )
    }
}

/// Returns the qualified sample-kind symbol for [`XrTapSample`].
pub fn xr_tap_sample_kind_symbol() -> Symbol {
    sample_kind_symbol(XrTapSample::sample_kind())
}

pub(crate) fn decode_known_tap(expr: &Expr) -> DeviceSampleResult<()> {
    XrTapSample::from_expr(expr).map(|_| ())
}

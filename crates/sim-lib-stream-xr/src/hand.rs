//! Strict XR hand ray sample records.

use sim_kernel::{Expr, Symbol};
use sim_lib_stream_device::{
    DeviceSample, DeviceSampleError, DeviceSampleResult, device_sample_record_symbol,
    sample_kind_symbol,
};
use sim_value::build;

use crate::wire::{
    expect_only_fields, expect_sample_tags, f64_array_field, f64_vector, map_entries, symbol_field,
    u16_field, u64_field,
};

/// Maximum accepted confidence value, expressed in ten-thousandths.
pub const XR_CONFIDENCE_MAX: u16 = 10_000;

const XR_HAND_FIELDS: &[&str] = &[
    "kind",
    "sample",
    "seq",
    "device",
    "hand",
    "origin-m",
    "direction",
    "confidence",
    "t-ns",
];

/// A strict XR hand ray sample.
#[derive(Clone, Debug, PartialEq)]
pub struct XrHandSample {
    seq: u64,
    device: Symbol,
    hand: Symbol,
    origin_m: [f64; 3],
    direction: [f64; 3],
    confidence: u16,
    t_ns: u64,
}

impl XrHandSample {
    /// Builds an XR hand ray sample.
    pub fn new(
        seq: u64,
        device: Symbol,
        hand: Symbol,
        origin_m: [f64; 3],
        direction: [f64; 3],
        confidence: u16,
        t_ns: u64,
    ) -> DeviceSampleResult<Self> {
        validate_finite("origin-m", origin_m)?;
        validate_finite("direction", direction)?;
        if direction.iter().all(|value| *value == 0.0) {
            return Err(DeviceSampleError::new(
                "XR hand direction must not be all zero",
            ));
        }
        if confidence > XR_CONFIDENCE_MAX {
            return Err(DeviceSampleError::new(format!(
                "XR hand confidence must be <= {XR_CONFIDENCE_MAX}"
            )));
        }
        Ok(Self {
            seq,
            device,
            hand,
            origin_m,
            direction,
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

    /// Returns the hand identity.
    pub fn hand(&self) -> &Symbol {
        &self.hand
    }

    /// Returns the hand-ray origin in meters.
    pub fn origin_m(&self) -> [f64; 3] {
        self.origin_m
    }

    /// Returns the hand-ray direction.
    pub fn direction(&self) -> [f64; 3] {
        self.direction
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

impl DeviceSample for XrHandSample {
    fn sample_kind() -> &'static str {
        "xr/hand"
    }

    fn seq(&self) -> u64 {
        self.seq
    }

    fn to_expr(&self) -> Expr {
        build::map(vec![
            ("kind", Expr::Symbol(device_sample_record_symbol())),
            ("sample", Expr::Symbol(xr_hand_sample_kind_symbol())),
            ("seq", build::uint(self.seq)),
            ("device", Expr::Symbol(self.device.clone())),
            ("hand", Expr::Symbol(self.hand.clone())),
            ("origin-m", f64_vector(&self.origin_m)),
            ("direction", f64_vector(&self.direction)),
            ("confidence", build::uint(u64::from(self.confidence))),
            ("t-ns", build::uint(self.t_ns)),
        ])
    }

    fn from_expr(expr: &Expr) -> DeviceSampleResult<Self> {
        let entries = map_entries(expr, "XR hand sample map")?;
        expect_only_fields(entries, XR_HAND_FIELDS, "XR hand")?;
        expect_sample_tags(entries, &xr_hand_sample_kind_symbol(), "XR hand")?;
        Self::new(
            u64_field(entries, "seq", "XR hand")?,
            symbol_field(entries, "device", "XR hand")?.clone(),
            symbol_field(entries, "hand", "XR hand")?.clone(),
            f64_array_field(entries, "origin-m", "XR hand")?,
            f64_array_field(entries, "direction", "XR hand")?,
            u16_field(entries, "confidence", "XR hand")?,
            u64_field(entries, "t-ns", "XR hand")?,
        )
    }
}

/// Returns the qualified sample-kind symbol for [`XrHandSample`].
pub fn xr_hand_sample_kind_symbol() -> Symbol {
    sample_kind_symbol(XrHandSample::sample_kind())
}

pub(crate) fn decode_known_hand(expr: &Expr) -> DeviceSampleResult<()> {
    XrHandSample::from_expr(expr).map(|_| ())
}

fn validate_finite(name: &str, values: impl IntoIterator<Item = f64>) -> DeviceSampleResult<()> {
    if values.into_iter().all(f64::is_finite) {
        Ok(())
    } else {
        Err(DeviceSampleError::new(format!(
            "XR hand field {name} entries must be finite"
        )))
    }
}

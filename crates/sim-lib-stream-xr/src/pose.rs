//! Strict XR pose sample records.

use sim_kernel::{Expr, Symbol};
use sim_lib_stream_device::{
    DeviceSample, DeviceSampleError, DeviceSampleResult, device_sample_record_symbol,
    sample_kind_symbol,
};
use sim_value::build;

use crate::wire::{
    expect_only_fields, expect_sample_tags, f64_array_field, f64_vector, map_entries,
    optional_f64_array_field, symbol_field, u8_field, u64_field,
};

const XR_TRACKING_NAMESPACE: &str = "stream/xr-tracking";
const XR_POSE_FIELDS: &[&str] = &[
    "kind",
    "sample",
    "seq",
    "position-m",
    "orientation",
    "t-ns",
    "predict-ns",
    "dof",
    "status",
];

/// Tracking state for an XR pose sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum XrTrackingStatus {
    /// Full tracking is active.
    Tracked,
    /// Tracking is degraded but still usable as a hint.
    Limited,
    /// Tracking is unavailable.
    Lost,
}

impl XrTrackingStatus {
    /// Returns every stable tracking status in wire order.
    pub fn all() -> &'static [Self] {
        &ALL_TRACKING_STATUSES
    }

    /// Returns the stable wire name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Tracked => "tracked",
            Self::Limited => "limited",
            Self::Lost => "lost",
        }
    }

    /// Returns the stable symbol for this status.
    pub fn symbol(self) -> Symbol {
        Symbol::qualified(XR_TRACKING_NAMESPACE, self.name())
    }

    /// Decodes a tracking status from its stable symbol.
    pub fn from_symbol(symbol: &Symbol) -> DeviceSampleResult<Self> {
        if symbol.namespace.as_deref() != Some(XR_TRACKING_NAMESPACE) {
            return Err(DeviceSampleError::new(format!(
                "XR tracking status must be a {XR_TRACKING_NAMESPACE}/* symbol, found {symbol}"
            )));
        }
        Self::all()
            .iter()
            .copied()
            .find(|status| status.name() == symbol.name.as_ref())
            .ok_or_else(|| DeviceSampleError::new(format!("unknown XR tracking status {symbol}")))
    }
}

const ALL_TRACKING_STATUSES: [XrTrackingStatus; 3] = [
    XrTrackingStatus::Tracked,
    XrTrackingStatus::Limited,
    XrTrackingStatus::Lost,
];

/// A strict XR pose sample.
///
/// `orientation` is a quaternion in `[w, x, y, z]` order. Six degree-of-freedom
/// samples carry `position_m`; three degree-of-freedom samples carry no
/// position and remain useful as orientation or motion hints.
#[derive(Clone, Debug, PartialEq)]
pub struct XrPoseSample {
    position_m: Option<[f64; 3]>,
    orientation: [f64; 4],
    t_ns: u64,
    predict_ns: u64,
    dof: u8,
    status: XrTrackingStatus,
    seq: u64,
}

impl XrPoseSample {
    /// Builds an XR pose sample.
    pub fn new(
        seq: u64,
        position_m: Option<[f64; 3]>,
        orientation: [f64; 4],
        t_ns: u64,
        predict_ns: u64,
        dof: u8,
        status: XrTrackingStatus,
    ) -> DeviceSampleResult<Self> {
        if !matches!(dof, 3 | 6) {
            return Err(DeviceSampleError::new("XR pose dof must be 3 or 6"));
        }
        if dof == 6 && position_m.is_none() {
            return Err(DeviceSampleError::new(
                "XR 6DoF pose must include position-m",
            ));
        }
        if dof == 3 && position_m.is_some() {
            return Err(DeviceSampleError::new(
                "XR 3DoF pose must not include position-m",
            ));
        }
        if let Some(position_m) = position_m {
            validate_finite("position-m", position_m)?;
        }
        validate_finite("orientation", orientation)?;
        if orientation.iter().all(|value| *value == 0.0) {
            return Err(DeviceSampleError::new(
                "XR pose orientation must not be all zero",
            ));
        }
        Ok(Self {
            position_m,
            orientation,
            t_ns,
            predict_ns,
            dof,
            status,
            seq,
        })
    }

    /// Returns the optional position in meters.
    pub fn position_m(&self) -> Option<[f64; 3]> {
        self.position_m
    }

    /// Returns the orientation quaternion in `[w, x, y, z]` order.
    pub fn orientation(&self) -> [f64; 4] {
        self.orientation
    }

    /// Returns the sample timestamp in nanoseconds.
    pub fn t_ns(&self) -> u64 {
        self.t_ns
    }

    /// Returns the prediction horizon in nanoseconds.
    pub fn predict_ns(&self) -> u64 {
        self.predict_ns
    }

    /// Returns the number of tracked degrees of freedom.
    pub fn dof(&self) -> u8 {
        self.dof
    }

    /// Returns the tracking status.
    pub fn status(&self) -> XrTrackingStatus {
        self.status
    }

    /// Returns the monotone sequence number.
    pub fn seq(&self) -> u64 {
        self.seq
    }
}

impl DeviceSample for XrPoseSample {
    fn sample_kind() -> &'static str {
        "xr/pose"
    }

    fn seq(&self) -> u64 {
        self.seq
    }

    fn to_expr(&self) -> Expr {
        build::map(vec![
            ("kind", Expr::Symbol(device_sample_record_symbol())),
            ("sample", Expr::Symbol(xr_pose_sample_kind_symbol())),
            ("seq", build::uint(self.seq)),
            ("position-m", position_expr(self.position_m)),
            ("orientation", f64_vector(&self.orientation)),
            ("t-ns", build::uint(self.t_ns)),
            ("predict-ns", build::uint(self.predict_ns)),
            ("dof", build::uint(u64::from(self.dof))),
            ("status", Expr::Symbol(self.status.symbol())),
        ])
    }

    fn from_expr(expr: &Expr) -> DeviceSampleResult<Self> {
        let entries = map_entries(expr, "XR pose sample map")?;
        expect_only_fields(entries, XR_POSE_FIELDS, "XR pose")?;
        expect_sample_tags(entries, &xr_pose_sample_kind_symbol(), "XR pose")?;
        Self::new(
            u64_field(entries, "seq", "XR pose")?,
            optional_f64_array_field(entries, "position-m", "XR pose")?,
            f64_array_field(entries, "orientation", "XR pose")?,
            u64_field(entries, "t-ns", "XR pose")?,
            u64_field(entries, "predict-ns", "XR pose")?,
            u8_field(entries, "dof", "XR pose")?,
            XrTrackingStatus::from_symbol(symbol_field(entries, "status", "XR pose")?)?,
        )
    }
}

/// Returns the qualified sample-kind symbol for [`XrPoseSample`].
pub fn xr_pose_sample_kind_symbol() -> Symbol {
    sample_kind_symbol(XrPoseSample::sample_kind())
}

pub(crate) fn decode_known_pose(expr: &Expr) -> DeviceSampleResult<()> {
    XrPoseSample::from_expr(expr).map(|_| ())
}

fn position_expr(position: Option<[f64; 3]>) -> Expr {
    position.map_or(Expr::Nil, |position| f64_vector(&position))
}

fn validate_finite(name: &str, values: impl IntoIterator<Item = f64>) -> DeviceSampleResult<()> {
    if values.into_iter().all(f64::is_finite) {
        Ok(())
    } else {
        Err(DeviceSampleError::new(format!(
            "XR pose field {name} entries must be finite"
        )))
    }
}

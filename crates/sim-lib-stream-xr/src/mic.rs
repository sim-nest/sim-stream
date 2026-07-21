//! Strict XR microphone chunk reference samples.

use sim_kernel::{Expr, Symbol};
use sim_lib_stream_device::{
    DeviceSample, DeviceSampleError, DeviceSampleResult, device_sample_record_symbol,
    sample_kind_symbol,
};
use sim_value::build;

use crate::wire::{
    expect_only_fields, expect_sample_tags, map_entries, symbol_field, u32_field, u64_field,
};

const XR_MIC_FIELDS: &[&str] = &["kind", "sample", "seq", "store-key", "ms"];

/// A reference to captured XR microphone audio.
///
/// The sample names an audio chunk held by a backend store and its duration in
/// milliseconds. It intentionally carries no transcript, command, or intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XrMicChunkRef {
    store_key: Symbol,
    ms: u32,
    seq: u64,
}

impl XrMicChunkRef {
    /// Builds a microphone chunk reference.
    pub fn new(seq: u64, store_key: Symbol, ms: u32) -> DeviceSampleResult<Self> {
        if ms == 0 {
            return Err(DeviceSampleError::new("XR mic chunk ms must be nonzero"));
        }
        Ok(Self { store_key, ms, seq })
    }

    /// Returns the backend store key for the captured audio chunk.
    pub fn store_key(&self) -> &Symbol {
        &self.store_key
    }

    /// Returns the captured chunk duration in milliseconds.
    pub fn ms(&self) -> u32 {
        self.ms
    }

    /// Returns the monotone sequence number.
    pub fn seq(&self) -> u64 {
        self.seq
    }
}

impl DeviceSample for XrMicChunkRef {
    fn sample_kind() -> &'static str {
        "xr/mic-chunk"
    }

    fn seq(&self) -> u64 {
        self.seq
    }

    fn to_expr(&self) -> Expr {
        build::map(vec![
            ("kind", Expr::Symbol(device_sample_record_symbol())),
            ("sample", Expr::Symbol(xr_mic_chunk_sample_kind_symbol())),
            ("seq", build::uint(self.seq)),
            ("store-key", Expr::Symbol(self.store_key.clone())),
            ("ms", build::uint(u64::from(self.ms))),
        ])
    }

    fn from_expr(expr: &Expr) -> DeviceSampleResult<Self> {
        let entries = map_entries(expr, "XR mic chunk reference map")?;
        expect_only_fields(entries, XR_MIC_FIELDS, "XR mic chunk")?;
        expect_sample_tags(entries, &xr_mic_chunk_sample_kind_symbol(), "XR mic chunk")?;
        Self::new(
            u64_field(entries, "seq", "XR mic chunk")?,
            symbol_field(entries, "store-key", "XR mic chunk")?.clone(),
            u32_field(entries, "ms", "XR mic chunk")?,
        )
    }
}

/// Returns the qualified sample-kind symbol for [`XrMicChunkRef`].
pub fn xr_mic_chunk_sample_kind_symbol() -> Symbol {
    sample_kind_symbol(XrMicChunkRef::sample_kind())
}

pub(crate) fn decode_known_mic_chunk(expr: &Expr) -> DeviceSampleResult<()> {
    XrMicChunkRef::from_expr(expr).map(|_| ())
}

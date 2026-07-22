//! Fail-closed device sample values and stream packet helpers.

use std::fmt;

use sim_kernel::{Expr, Symbol};
use sim_lib_stream_core::{DataPacket, StreamPacket};
use sim_value::{access, build};

/// Result type returned by device sample decoders.
pub type DeviceSampleResult<T> = std::result::Result<T, DeviceSampleError>;

/// A strict device sample decoding error.
///
/// The error intentionally carries a human-readable message only. Device sample
/// parsers reject malformed data at the first invalid field rather than trying
/// to coerce or infer a sample from partial input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSampleError {
    message: String,
}

impl DeviceSampleError {
    /// Builds a device sample error from a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the error message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for DeviceSampleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DeviceSampleError {}

impl From<DeviceSampleError> for sim_kernel::Error {
    fn from(error: DeviceSampleError) -> Self {
        sim_kernel::Error::Eval(format!("device sample error: {error}"))
    }
}

/// Trait implemented by concrete device sample records.
///
/// A sample has a stable kind tag, a monotone sequence number, and strict
/// expression round-tripping. [`from_expr`](Self::from_expr) is fail-closed: it
/// returns an error for missing fields, wrong field types, unknown kind tags, or
/// malformed sequence numbers.
pub trait DeviceSample: Sized + Clone {
    /// Stable bare sample kind, such as `device-caps`.
    fn sample_kind() -> &'static str;

    /// Monotone sequence number for ordering and aging samples.
    fn seq(&self) -> u64;

    /// Encodes the sample as a self-describing expression map.
    fn to_expr(&self) -> Expr;

    /// Decodes the sample from its expression map.
    fn from_expr(expr: &Expr) -> DeviceSampleResult<Self>;
}

/// Reports whether a sample survives a strict expression round trip.
pub fn roundtrip_ok<S>(sample: &S) -> bool
where
    S: DeviceSample + PartialEq,
{
    S::from_expr(&sample.to_expr())
        .map(|decoded| &decoded == sample)
        .unwrap_or(false)
}

/// Wraps a device sample as an ordinary stream data packet.
pub fn sample_packet<S: DeviceSample>(sample: &S) -> StreamPacket {
    StreamPacket::Data(DataPacket::new(
        sample_kind_symbol(S::sample_kind()),
        sample.to_expr(),
    ))
}

/// Returns the stable record tag for device sample maps.
pub fn device_sample_record_symbol() -> Symbol {
    Symbol::qualified("stream", "device-sample")
}

/// Returns the qualified sample-kind symbol for `kind`.
pub fn sample_kind_symbol(kind: &str) -> Symbol {
    Symbol::qualified("stream/device-sample", kind)
}

/// Returns the qualified sample-kind symbol for [`DeviceCaps`].
pub fn device_caps_sample_kind_symbol() -> Symbol {
    sample_kind_symbol(DeviceCaps::sample_kind())
}

/// A sample describing a device's stream-facing capabilities.
///
/// `DeviceCaps` is the base sample every concrete device instance can emit
/// before it starts producing richer sensor-specific sample kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceCaps {
    seq: u64,
    device: Symbol,
    streams: Vec<Symbol>,
    inputs: Vec<Symbol>,
    outputs: Vec<Symbol>,
}

impl DeviceCaps {
    /// Builds a device capabilities sample.
    pub fn new(
        seq: u64,
        device: Symbol,
        streams: Vec<Symbol>,
        inputs: Vec<Symbol>,
        outputs: Vec<Symbol>,
    ) -> Self {
        Self {
            seq,
            device,
            streams,
            inputs,
            outputs,
        }
    }

    /// Builds the deterministic demo capability sample used by tests and docs.
    pub fn demo(seq: u64) -> Self {
        Self::new(
            seq,
            Symbol::qualified("device", "modeled-edge"),
            vec![
                Symbol::qualified("device/stream", "battery"),
                Symbol::qualified("device/stream", "motion"),
            ],
            vec![Symbol::qualified("device/input", "button")],
            vec![
                Symbol::qualified("device/output", "screen"),
                Symbol::qualified("device/output", "haptic"),
            ],
        )
    }

    /// Returns the device identity symbol.
    pub fn device(&self) -> &Symbol {
        &self.device
    }

    /// Returns the advertised sample stream symbols.
    pub fn streams(&self) -> &[Symbol] {
        &self.streams
    }

    /// Returns the input capability symbols.
    pub fn inputs(&self) -> &[Symbol] {
        &self.inputs
    }

    /// Returns the output capability symbols.
    pub fn outputs(&self) -> &[Symbol] {
        &self.outputs
    }

    /// Wraps this capabilities sample as a stream data packet.
    pub fn to_stream_packet(&self) -> StreamPacket {
        sample_packet(self)
    }
}

impl DeviceSample for DeviceCaps {
    fn sample_kind() -> &'static str {
        "device-caps"
    }

    fn seq(&self) -> u64 {
        self.seq
    }

    fn to_expr(&self) -> Expr {
        build::map(vec![
            ("kind", Expr::Symbol(device_sample_record_symbol())),
            ("sample", Expr::Symbol(device_caps_sample_kind_symbol())),
            ("seq", build::uint(self.seq)),
            ("device", Expr::Symbol(self.device.clone())),
            ("streams", symbols_expr(&self.streams)),
            ("inputs", symbols_expr(&self.inputs)),
            ("outputs", symbols_expr(&self.outputs)),
        ])
    }

    fn from_expr(expr: &Expr) -> DeviceSampleResult<Self> {
        let entries = entries(expr)?;
        expect_record_tag(entries)?;
        expect_sample_kind(entries, &device_caps_sample_kind_symbol())?;
        Ok(Self::new(
            seq_field(entries)?,
            symbol_field(entries, "device")?.clone(),
            symbol_list_field(entries, "streams")?,
            symbol_list_field(entries, "inputs")?,
            symbol_list_field(entries, "outputs")?,
        ))
    }
}

pub(crate) fn decode_known_sample(expr: &Expr) -> DeviceSampleResult<()> {
    let entries = entries(expr)?;
    expect_record_tag(entries)?;
    let kind = symbol_field(entries, "sample")?;
    if kind == &device_caps_sample_kind_symbol() {
        DeviceCaps::from_expr(expr)?;
        return Ok(());
    }
    Err(DeviceSampleError::new(format!(
        "unknown device sample kind {kind}"
    )))
}

pub(crate) fn sample_constructor_args(expr: &Expr) -> DeviceSampleResult<Vec<Expr>> {
    decode_known_sample(expr)?;
    Ok(vec![expr.clone()])
}

fn symbols_expr(symbols: &[Symbol]) -> Expr {
    build::list(symbols.iter().cloned().map(Expr::Symbol).collect())
}

fn entries(expr: &Expr) -> DeviceSampleResult<&[(Expr, Expr)]> {
    access::map_entries(expr, "device sample map").map_err(kernel_error)
}

fn expect_record_tag(entries: &[(Expr, Expr)]) -> DeviceSampleResult<()> {
    let actual = symbol_field(entries, "kind")?;
    let expected = device_sample_record_symbol();
    if actual == &expected {
        Ok(())
    } else {
        Err(DeviceSampleError::new(format!(
            "device sample kind tag must be {expected}, found {actual}"
        )))
    }
}

fn expect_sample_kind(entries: &[(Expr, Expr)], expected: &Symbol) -> DeviceSampleResult<()> {
    let actual = symbol_field(entries, "sample")?;
    if actual == expected {
        Ok(())
    } else {
        Err(DeviceSampleError::new(format!(
            "device sample record must be {expected}, found {actual}"
        )))
    }
}

fn seq_field(entries: &[(Expr, Expr)]) -> DeviceSampleResult<u64> {
    let value = field(entries, "seq")?;
    let Expr::Number(number) = value else {
        return Err(DeviceSampleError::new(format!(
            "device sample seq must be a u64 number, found {}",
            sim_value::kind::expr_kind(value)
        )));
    };
    if !matches!(number.domain.name.as_ref(), "i64" | "u64") {
        return Err(DeviceSampleError::new(format!(
            "device sample seq must use an integer domain, found {}",
            number.domain
        )));
    }
    number
        .canonical
        .parse::<u64>()
        .map_err(|err| DeviceSampleError::new(format!("invalid device sample seq: {err}")))
}

fn symbol_field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> DeviceSampleResult<&'a Symbol> {
    access::entry_required_sym(entries, name, "device sample").map_err(kernel_error)
}

fn field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> DeviceSampleResult<&'a Expr> {
    access::entry_required(entries, name, "device sample").map_err(kernel_error)
}

fn symbol_list_field(entries: &[(Expr, Expr)], name: &str) -> DeviceSampleResult<Vec<Symbol>> {
    let items =
        access::entry_required_list(entries, name, "device sample").map_err(kernel_error)?;
    items
        .iter()
        .map(|item| match item {
            Expr::Symbol(symbol) => Ok(symbol.clone()),
            other => Err(DeviceSampleError::new(format!(
                "device sample {name} entries must be symbols, found {}",
                sim_value::kind::expr_kind(other)
            ))),
        })
        .collect()
}

fn kernel_error(error: sim_kernel::Error) -> DeviceSampleError {
    DeviceSampleError::new(error.to_string())
}

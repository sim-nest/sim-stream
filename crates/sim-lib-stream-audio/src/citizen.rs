use sim_citizen_derive::Citizen;
use sim_kernel::{Error, Expr, NumberLiteral, Result, Symbol};

use crate::{PcmSampleFormat, PcmSpec};

const LIB_NS: &str = "stream-audio";

/// Runtime citizen that carries a [`PcmSpec`] as a `stream/PcmFormat` object.
///
/// The descriptor stores the audio format as a kernel [`Expr`] map so it can be
/// exposed to the SIM runtime as a first-class object (registered under the
/// [`pcm_format_class_symbol`] class). [`spec`](PcmFormatDescriptor::spec)
/// decodes that expression back into a [`PcmSpec`].
///
/// # Examples
///
/// ```
/// use sim_lib_stream_audio::{PcmFormatDescriptor, PcmSpec};
///
/// let spec = PcmSpec::f32(2, 48_000)?;
/// let descriptor = PcmFormatDescriptor::new(spec);
/// assert_eq!(descriptor.spec()?, spec);
/// # Ok::<(), sim_kernel::Error>(())
/// ```
#[derive(Clone, Debug, PartialEq, Citizen)]
#[citizen(symbol = "stream/PcmFormat", version = 1)]
pub struct PcmFormatDescriptor {
    #[citizen(with = "pcm_spec_expr")]
    spec: Expr,
}

impl PcmFormatDescriptor {
    /// Builds a descriptor from an audio format.
    pub fn new(spec: PcmSpec) -> Self {
        Self {
            spec: spec_to_expr(spec),
        }
    }

    /// Builds a descriptor from an already-encoded format expression.
    ///
    /// Returns an error when `expr` is not a valid PCM format map.
    pub fn from_expr(expr: Expr) -> Result<Self> {
        pcm_spec_expr::decode(&expr)?;
        Ok(Self { spec: expr })
    }

    /// Decodes the stored expression back into a [`PcmSpec`].
    ///
    /// Returns an error when the stored expression is not a valid PCM format
    /// map.
    pub fn spec(&self) -> Result<PcmSpec> {
        spec_from_expr(&self.spec)
    }

    /// Returns the underlying format expression.
    pub fn as_expr(&self) -> &Expr {
        &self.spec
    }
}

impl Default for PcmFormatDescriptor {
    fn default() -> Self {
        Self::new(PcmSpec::f32(2, 48_000).expect("default PCM descriptor should be valid"))
    }
}

/// Returns the `stream/PcmFormat` class symbol under which
/// [`PcmFormatDescriptor`] is registered.
pub fn pcm_format_class_symbol() -> Symbol {
    Symbol::qualified("stream", "PcmFormat")
}

pub(crate) mod pcm_spec_expr {
    use sim_kernel::{Expr, Result};

    use super::spec_from_expr;

    pub fn encode(expr: &Expr) -> Expr {
        expr.clone()
    }

    pub fn decode(expr: &Expr) -> Result<Expr> {
        spec_from_expr(expr)?;
        Ok(expr.clone())
    }
}

fn spec_to_expr(spec: PcmSpec) -> Expr {
    Expr::Map(vec![
        (field("tag"), tag("pcm-format")),
        (field("channels"), number_usize(spec.channels())),
        (field("sample-rate-hz"), number_u32(spec.sample_rate_hz())),
        (
            field("sample-format"),
            Expr::Symbol(match spec.sample_format() {
                PcmSampleFormat::I16 => Symbol::qualified("pcm", "i16"),
                PcmSampleFormat::F32 => Symbol::qualified("pcm", "f32"),
            }),
        ),
    ])
}

fn spec_from_expr(expr: &Expr) -> Result<PcmSpec> {
    let map = sim_value::access::map_entries(expr, "PCM format descriptor")?;
    expect_tag(map, "pcm-format")?;
    let channels = expr_usize(lookup_required(map, "channels")?, "channels")?;
    let sample_rate_hz = expr_u32(lookup_required(map, "sample-rate-hz")?, "sample-rate-hz")?;
    match lookup_required(map, "sample-format")? {
        Expr::Symbol(symbol) if symbol.namespace.as_deref() == Some("pcm") => {
            match symbol.name.as_ref() {
                "i16" => PcmSpec::i16(channels, sample_rate_hz),
                "f32" => PcmSpec::f32(channels, sample_rate_hz),
                other => Err(Error::Eval(format!("unknown PCM sample format: {other}"))),
            }
        }
        _ => Err(Error::Eval(
            "PCM sample format must be a pcm/* symbol".to_owned(),
        )),
    }
}

fn field(name: &'static str) -> Expr {
    sim_value::build::qsym(LIB_NS, name)
}

fn tag(name: &'static str) -> Expr {
    Expr::Symbol(Symbol::qualified(LIB_NS, name))
}

fn number_u32(value: u32) -> Expr {
    number_usize(value as usize)
}

fn number_usize(value: usize) -> Expr {
    Expr::Number(NumberLiteral {
        domain: Symbol::qualified("numbers", "i64"),
        canonical: value.to_string(),
    })
}

fn expect_tag(map: &[(Expr, Expr)], expected: &str) -> Result<()> {
    match lookup_required(map, "tag")? {
        Expr::Symbol(symbol) if is_symbol(symbol, LIB_NS, expected) => Ok(()),
        _ => Err(Error::Eval(format!(
            "PCM descriptor tag must be {expected}"
        ))),
    }
}

fn expr_u32(expr: &Expr, context: &str) -> Result<u32> {
    expr_usize(expr, context)?
        .try_into()
        .map_err(|_| Error::Eval(format!("{context} is out of range for u32")))
}

fn expr_usize(expr: &Expr, context: &str) -> Result<usize> {
    let text = match expr {
        Expr::Number(number) => number.canonical.as_str(),
        Expr::String(text) => text,
        _ => return Err(Error::Eval(format!("{context} must be a number"))),
    };
    text.parse::<usize>()
        .map_err(|_| Error::Eval(format!("{context} must be an unsigned integer")))
}

fn lookup_required<'a>(map: &'a [(Expr, Expr)], name: &str) -> Result<&'a Expr> {
    map.iter()
        .find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if is_symbol(symbol, LIB_NS, name) => Some(value),
            _ => None,
        })
        .ok_or_else(|| Error::Eval(format!("PCM descriptor field is missing: {name}")))
}

fn is_symbol(symbol: &Symbol, namespace: &str, name: &str) -> bool {
    symbol.namespace.as_deref() == Some(namespace) && symbol.name.as_ref() == name
}

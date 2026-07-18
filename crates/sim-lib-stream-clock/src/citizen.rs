use sim_citizen_derive::Citizen;
use sim_kernel::{Error, Expr, NumberLiteral, Result, Symbol};
use sim_lib_stream_core::ClockDomain;

use crate::{Clock, ClockChart, TempoMap, TempoSegment};

const LIB_NS: &str = "stream-clock";

/// Citizen wrapper that carries a [`Clock`] as a self-describing `stream/Clock`
/// value.
///
/// The descriptor stores the clock in its encoded [`Expr`] form so it can move
/// across codec surfaces and the kernel object system, while still decoding
/// back to a [`Clock`] for clock math.
///
/// # Examples
///
/// ```
/// use sim_kernel::Symbol;
/// use sim_lib_stream_clock::StreamClockDescriptor;
///
/// let descriptor = StreamClockDescriptor::frame(Symbol::new("audio"), 48_000)?;
/// let clock = descriptor.clock()?;
/// assert_eq!(clock.id(), &Symbol::new("audio"));
/// # Ok::<(), sim_kernel::Error>(())
/// ```
#[derive(Clone, Debug, PartialEq, Citizen)]
#[citizen(symbol = "stream/Clock", version = 1)]
pub struct StreamClockDescriptor {
    #[citizen(with = "clock_expr")]
    clock: Expr,
}

impl StreamClockDescriptor {
    /// Builds a descriptor for a frame clock with the given id and rate.
    ///
    /// Returns an error when `frames_per_second` is zero.
    pub fn frame(id: Symbol, frames_per_second: u64) -> Result<Self> {
        let clock = Clock::frame(id, frames_per_second)?;
        Ok(Self::from_clock(&clock))
    }

    /// Builds a descriptor for a MIDI clock with the given id, resolution, and
    /// tempo map.
    ///
    /// Returns an error when `tpq` is zero.
    pub fn midi(id: Symbol, tpq: u32, tempo_map: TempoMap) -> Result<Self> {
        let clock = Clock::midi(id, tpq, tempo_map)?;
        Ok(Self::from_clock(&clock))
    }

    /// Builds a descriptor by encoding an existing [`Clock`].
    pub fn from_clock(clock: &Clock) -> Self {
        Self {
            clock: clock_to_expr(clock),
        }
    }

    /// Builds a descriptor from an already-encoded clock [`Expr`].
    ///
    /// Returns an error when `expr` is not a valid stream clock encoding.
    pub fn from_expr(expr: Expr) -> Result<Self> {
        clock_expr::decode(&expr)?;
        Ok(Self { clock: expr })
    }

    /// Decodes the wrapped [`Clock`].
    ///
    /// Returns an error when the stored encoding is invalid.
    pub fn clock(&self) -> Result<Clock> {
        clock_from_expr(&self.clock)
    }

    /// Returns the wrapped clock in its encoded [`Expr`] form.
    pub fn as_expr(&self) -> &Expr {
        &self.clock
    }
}

impl Default for StreamClockDescriptor {
    fn default() -> Self {
        Self::frame(Symbol::qualified("clock", "citizen"), 48_000)
            .expect("default stream clock descriptor should be valid")
    }
}

/// Returns the `stream/Clock` class symbol for registering and looking up
/// [`StreamClockDescriptor`] values.
pub fn stream_clock_class_symbol() -> Symbol {
    Symbol::qualified("stream", "Clock")
}

pub(crate) mod clock_expr {
    use sim_kernel::{Expr, Result};

    use super::clock_from_expr;

    pub fn encode(expr: &Expr) -> Expr {
        expr.clone()
    }

    pub fn decode(expr: &Expr) -> Result<Expr> {
        clock_from_expr(expr)?;
        Ok(expr.clone())
    }
}

fn clock_to_expr(clock: &Clock) -> Expr {
    match clock.chart() {
        ClockChart::Frames { frames_per_second } => Expr::Map(vec![
            (field("tag"), tag("clock")),
            (field("id"), Expr::Symbol(clock.id().clone())),
            (field("domain"), Expr::Symbol(clock.domain().symbol())),
            (field("kind"), tag("frame")),
            (field("frames-per-second"), number_u64(*frames_per_second)),
        ]),
        ClockChart::Midi { tpq, tempo_map } => Expr::Map(vec![
            (field("tag"), tag("clock")),
            (field("id"), Expr::Symbol(clock.id().clone())),
            (field("domain"), Expr::Symbol(clock.domain().symbol())),
            (field("kind"), tag("midi")),
            (field("tpq"), number_u32(*tpq)),
            (
                field("tempo-map"),
                Expr::Vector(
                    tempo_map
                        .segments()
                        .iter()
                        .map(tempo_segment_to_expr)
                        .collect(),
                ),
            ),
        ]),
    }
}

fn clock_from_expr(expr: &Expr) -> Result<Clock> {
    let map = sim_value::access::map_entries(expr, "clock descriptor")?;
    expect_tag(map, "clock")?;
    let id = expr_symbol(lookup_required(map, "id")?, "clock id")?;
    let domain = ClockDomain::from_symbol(&expr_symbol(
        lookup_required(map, "domain")?,
        "clock domain",
    )?)?;
    let kind = tag_name(lookup_required(map, "kind")?, "clock kind")?;
    match kind {
        "frame" => Clock::frame_with_domain(
            id,
            domain,
            expr_u64(
                lookup_required(map, "frames-per-second")?,
                "frames-per-second",
            )?,
        ),
        "midi" => {
            if domain != ClockDomain::MidiTick {
                return Err(Error::Eval(
                    "MIDI stream clock domain must be midi-tick".to_owned(),
                ));
            }
            Clock::midi(
                id,
                expr_u32(lookup_required(map, "tpq")?, "tpq")?,
                tempo_map_from_expr(lookup_required(map, "tempo-map")?)?,
            )
        }
        other => Err(Error::Eval(format!("unknown stream clock kind: {other}"))),
    }
}

fn tempo_segment_to_expr(segment: &TempoSegment) -> Expr {
    Expr::Map(vec![
        (field("start-tick"), number_u64(segment.start_tick)),
        (field("us-per-quarter"), number_u32(segment.us_per_quarter)),
    ])
}

fn tempo_map_from_expr(expr: &Expr) -> Result<TempoMap> {
    let Expr::Vector(items) = expr else {
        return Err(Error::Eval("tempo map must be a vector".to_owned()));
    };
    TempoMap::new(
        items
            .iter()
            .map(|item| {
                let map = sim_value::access::map_entries(item, "tempo segment")?;
                TempoSegment::new(
                    expr_u64(lookup_required(map, "start-tick")?, "start-tick")?,
                    expr_u32(lookup_required(map, "us-per-quarter")?, "us-per-quarter")?,
                )
            })
            .collect::<Result<Vec<_>>>()?,
    )
}

fn field(name: &'static str) -> Expr {
    sim_value::build::qsym(LIB_NS, name)
}

fn tag(name: &'static str) -> Expr {
    Expr::Symbol(Symbol::qualified(LIB_NS, name))
}

fn number_u32(value: u32) -> Expr {
    number_u64(u64::from(value))
}

fn number_u64(value: u64) -> Expr {
    Expr::Number(NumberLiteral {
        domain: Symbol::qualified("numbers", "i64"),
        canonical: value.to_string(),
    })
}

fn expect_tag(map: &[(Expr, Expr)], expected: &str) -> Result<()> {
    match lookup_required(map, "tag")? {
        Expr::Symbol(symbol) if is_symbol(symbol, LIB_NS, expected) => Ok(()),
        _ => Err(Error::Eval(format!("stream clock tag must be {expected}"))),
    }
}

fn expr_symbol(expr: &Expr, context: &str) -> Result<Symbol> {
    match expr {
        Expr::Symbol(symbol) => Ok(symbol.clone()),
        _ => Err(Error::Eval(format!("{context} must be a symbol"))),
    }
}

fn tag_name<'a>(expr: &'a Expr, context: &str) -> Result<&'a str> {
    match expr {
        Expr::Symbol(symbol) if symbol.namespace.as_deref() == Some(LIB_NS) => {
            Ok(symbol.name.as_ref())
        }
        _ => Err(Error::Eval(format!("{context} must be a {LIB_NS} symbol"))),
    }
}

fn expr_u32(expr: &Expr, context: &str) -> Result<u32> {
    expr_u64(expr, context)?
        .try_into()
        .map_err(|_| Error::Eval(format!("{context} is out of range for u32")))
}

fn expr_u64(expr: &Expr, context: &str) -> Result<u64> {
    let text = match expr {
        Expr::Number(number) => number.canonical.as_str(),
        Expr::String(text) => text,
        _ => return Err(Error::Eval(format!("{context} must be a number"))),
    };
    text.parse::<u64>()
        .map_err(|_| Error::Eval(format!("{context} must be an unsigned integer")))
}

fn lookup_required<'a>(map: &'a [(Expr, Expr)], name: &str) -> Result<&'a Expr> {
    map.iter()
        .find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if is_symbol(symbol, LIB_NS, name) => Some(value),
            _ => None,
        })
        .ok_or_else(|| Error::Eval(format!("stream clock field is missing: {name}")))
}

fn is_symbol(symbol: &Symbol, namespace: &str, name: &str) -> bool {
    symbol.namespace.as_deref() == Some(namespace) && symbol.name.as_ref() == name
}

use sim_kernel::{
    Cx, Datum, DatumStore, Diagnostic, Error, Expr, NumberLiteral, Ref, Result, Severity, Symbol,
    Tick,
};
use sim_lib_stream_core::ClockDomain;

use crate::{Instant, TempoMap, tempo::midi_tick_duration};

/// Position on a clock timeline, counted in that clock's own units (frames for
/// frame clocks, ticks for MIDI clocks).
///
/// # Examples
///
/// ```
/// use sim_lib_stream_clock::ClockIndex;
///
/// let index = ClockIndex::new(48_000);
/// assert_eq!(index.value(), 48_000);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClockIndex(u64);

impl ClockIndex {
    /// Wraps a raw count into a clock index.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the underlying count.
    pub fn value(self) -> u64 {
        self.0
    }
}

/// Result of converting an [`Instant`] to a [`ClockIndex`], paired with any
/// diagnostics raised by the conversion.
///
/// A conversion is exact when the instant lands on a clock boundary; otherwise
/// the index is rounded toward zero and a warning diagnostic records that the
/// instant was not on an exact boundary.
///
/// # Examples
///
/// ```
/// use sim_kernel::Symbol;
/// use sim_lib_stream_clock::{Clock, Instant};
///
/// let clock = Clock::frame(Symbol::new("audio"), 48_000)?;
/// let conversion = clock.index_for_instant(Instant::seconds(1))?;
/// assert_eq!(conversion.index().value(), 48_000);
/// assert!(conversion.is_exact());
/// # Ok::<(), sim_kernel::Error>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexConversion {
    index: ClockIndex,
    diagnostics: Vec<Diagnostic>,
}

impl IndexConversion {
    fn exact(index: u64) -> Self {
        Self {
            index: ClockIndex::new(index),
            diagnostics: Vec::new(),
        }
    }

    fn inexact(index: u64, message: impl Into<String>) -> Self {
        Self {
            index: ClockIndex::new(index),
            diagnostics: vec![Diagnostic {
                severity: Severity::Warning,
                message: message.into(),
                source: None,
                span: None,
                code: Some(Symbol::qualified("stream/clock", "inexact-conversion")),
                related: Vec::new(),
            }],
        }
    }

    /// Returns the converted index.
    pub fn index(&self) -> ClockIndex {
        self.index
    }

    /// Returns the diagnostics raised by the conversion, empty when exact.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns `true` when the instant landed exactly on a clock boundary.
    pub fn is_exact(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

/// Timing law that maps a clock's indexes to [`Instant`]s and back.
///
/// A chart is either a fixed-rate frame clock or a MIDI clock whose tempo is
/// governed by a [`TempoMap`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClockChart {
    /// Fixed-rate clock advancing at `frames_per_second` frames each second.
    Frames {
        /// Frames per second of the clock.
        frames_per_second: u64,
    },
    /// MIDI clock whose ticks are paced by `tempo_map` at `tpq` ticks per
    /// quarter note.
    Midi {
        /// Ticks per quarter note.
        tpq: u32,
        /// Tempo timeline driving tick pacing.
        tempo_map: TempoMap,
    },
}

/// Named clock with a [`ClockDomain`] and a [`ClockChart`], the unit of clock
/// math in this crate.
///
/// A clock converts between [`Instant`]s and [`ClockIndex`]es, and mints the
/// kernel [`Tick`] / [`Expr`] forms that carry a clock index across the
/// runtime.
///
/// # Examples
///
/// ```
/// use sim_kernel::Symbol;
/// use sim_lib_stream_clock::{Clock, ClockChart, ClockIndex};
///
/// let clock = Clock::frame(Symbol::new("audio"), 48_000)?;
/// assert!(matches!(clock.chart(), ClockChart::Frames { frames_per_second: 48_000 }));
/// let instant = clock.instant_for_index(ClockIndex::new(48_000))?;
/// assert_eq!(instant.numerator(), 1);
/// assert_eq!(instant.denominator(), 1);
/// # Ok::<(), sim_kernel::Error>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clock {
    id: Symbol,
    domain: ClockDomain,
    chart: ClockChart,
}

impl Clock {
    /// Builds a frame clock in the [`ClockDomain::Sample`] domain.
    ///
    /// Returns an error when `frames_per_second` is zero.
    pub fn frame(id: Symbol, frames_per_second: u64) -> Result<Self> {
        Self::frame_with_domain(id, ClockDomain::Sample, frames_per_second)
    }

    /// Builds a frame clock in an explicit `domain`.
    ///
    /// Returns an error when `domain` is [`ClockDomain::MidiTick`] (which
    /// requires a MIDI chart) or when `frames_per_second` is zero.
    pub fn frame_with_domain(
        id: Symbol,
        domain: ClockDomain,
        frames_per_second: u64,
    ) -> Result<Self> {
        if domain == ClockDomain::MidiTick {
            return Err(Error::Eval(
                "frame clock domain must not be midi-tick".to_owned(),
            ));
        }
        if frames_per_second == 0 {
            return Err(Error::Eval("frame clock rate must be non-zero".to_owned()));
        }
        Ok(Self {
            id,
            domain,
            chart: ClockChart::Frames { frames_per_second },
        })
    }

    /// Builds a MIDI clock in the [`ClockDomain::MidiTick`] domain.
    ///
    /// Returns an error when `tpq` (ticks per quarter note) is zero.
    pub fn midi(id: Symbol, tpq: u32, tempo_map: TempoMap) -> Result<Self> {
        if tpq == 0 {
            return Err(Error::Eval("midi clock TPQ must be non-zero".to_owned()));
        }
        Ok(Self {
            id,
            domain: ClockDomain::MidiTick,
            chart: ClockChart::Midi { tpq, tempo_map },
        })
    }

    /// Returns the clock's identifying symbol.
    pub fn id(&self) -> &Symbol {
        &self.id
    }

    /// Returns the clock's domain.
    pub fn domain(&self) -> ClockDomain {
        self.domain
    }

    /// Returns the clock's timing chart.
    pub fn chart(&self) -> &ClockChart {
        &self.chart
    }

    /// Converts `instant` to this clock's index, reporting whether the result
    /// is exact via the returned [`IndexConversion`].
    ///
    /// Returns an error when the conversion arithmetic overflows or the index
    /// does not fit in a `u64`.
    pub fn index_for_instant(&self, instant: Instant) -> Result<IndexConversion> {
        match &self.chart {
            ClockChart::Frames { frames_per_second } => {
                frame_index_for_instant(instant, *frames_per_second)
            }
            ClockChart::Midi { tpq, tempo_map } => midi_index_for_instant(instant, *tpq, tempo_map),
        }
    }

    /// Converts a clock `index` back to its [`Instant`].
    ///
    /// Returns an error when the conversion arithmetic overflows.
    pub fn instant_for_index(&self, index: ClockIndex) -> Result<Instant> {
        match &self.chart {
            ClockChart::Frames { frames_per_second } => {
                Instant::new(i128::from(index.value()), i128::from(*frames_per_second))
            }
            ClockChart::Midi { tpq, tempo_map } => midi_instant_for_index(index, *tpq, tempo_map),
        }
    }

    /// Interns `index` as datum content and returns the kernel [`Tick`] that
    /// carries it on this clock.
    ///
    /// The clock index is stored as a datum node referenced by content, so
    /// kernel events keep carrying only `Tick` values. Returns an error when
    /// interning into the datum store fails.
    pub fn tick_for_index(&self, cx: &mut Cx, index: ClockIndex) -> Result<Tick> {
        let id = cx.datum_store_mut().intern(Datum::Node {
            tag: Symbol::qualified("stream/clock", "index"),
            fields: vec![
                (Symbol::new("clock"), Datum::Symbol(self.id.clone())),
                (
                    Symbol::new("index"),
                    Datum::Number(NumberLiteral {
                        domain: Symbol::qualified("stream/clock", "index"),
                        canonical: index.value().to_string(),
                    }),
                ),
            ],
        })?;
        Ok(Tick::new(self.id.clone(), Ref::Content(id)))
    }

    /// Builds the codec [`Expr`] extension form encoding `index` on this clock.
    pub fn index_expr(&self, index: ClockIndex) -> Expr {
        Expr::Extension {
            tag: Symbol::qualified("stream/clock", "index"),
            payload: Box::new(Expr::Map(vec![
                (
                    Expr::Symbol(Symbol::new("clock")),
                    Expr::Symbol(self.id.clone()),
                ),
                (
                    Expr::Symbol(Symbol::new("index")),
                    Expr::Number(NumberLiteral {
                        domain: Symbol::qualified("stream/clock", "index"),
                        canonical: index.value().to_string(),
                    }),
                ),
            ])),
        }
    }
}

fn frame_index_for_instant(instant: Instant, frames_per_second: u64) -> Result<IndexConversion> {
    let numerator = instant
        .numerator()
        .checked_mul(i128::from(frames_per_second))
        .ok_or_else(|| Error::Eval("frame clock conversion overflowed".to_owned()))?;
    let denominator = instant.denominator();
    let index = checked_index(numerator / denominator)?;
    if numerator % denominator == 0 {
        Ok(IndexConversion::exact(index))
    } else {
        Ok(IndexConversion::inexact(
            index,
            "instant is not an exact frame boundary",
        ))
    }
}

fn midi_index_for_instant(
    instant: Instant,
    tpq: u32,
    tempo_map: &TempoMap,
) -> Result<IndexConversion> {
    let starts = tempo_map.segment_start_instants(tpq)?;
    let segment_index = starts.partition_point(|start| *start <= instant) - 1;
    let segment = tempo_map.segments()[segment_index];
    let elapsed = instant.checked_sub(starts[segment_index])?;
    let numerator = elapsed
        .numerator()
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_mul(i128::from(tpq)))
        .ok_or_else(|| Error::Eval("midi clock conversion overflowed".to_owned()))?;
    let denominator = elapsed
        .denominator()
        .checked_mul(i128::from(segment.us_per_quarter))
        .ok_or_else(|| Error::Eval("midi clock denominator overflowed".to_owned()))?;
    let delta_ticks = checked_index(numerator / denominator)?;
    let index = segment
        .start_tick
        .checked_add(delta_ticks)
        .ok_or_else(|| Error::Eval("midi clock index overflowed".to_owned()))?;
    if numerator % denominator == 0 {
        Ok(IndexConversion::exact(index))
    } else {
        Ok(IndexConversion::inexact(
            index,
            "instant is not an exact midi tick boundary",
        ))
    }
}

fn midi_instant_for_index(index: ClockIndex, tpq: u32, tempo_map: &TempoMap) -> Result<Instant> {
    let starts = tempo_map.segment_start_instants(tpq)?;
    let segments = tempo_map.segments();
    let segment_index = segments.partition_point(|segment| segment.start_tick <= index.value()) - 1;
    let segment = segments[segment_index];
    let elapsed_ticks = index.value() - segment.start_tick;
    starts[segment_index].checked_add(midi_tick_duration(
        elapsed_ticks,
        tpq,
        segment.us_per_quarter,
    )?)
}

fn checked_index(value: i128) -> Result<u64> {
    u64::try_from(value)
        .map_err(|_| Error::Eval("clock index must fit in an unsigned 64-bit value".to_owned()))
}

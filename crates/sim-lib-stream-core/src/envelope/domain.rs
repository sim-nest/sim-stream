use sim_kernel::{Error, Result, Symbol};

/// Clock a stream is timed against.
///
/// Each variant names one timeline a packet can ride; the kernel defines the
/// clock-domain contract as [`Symbol`]s, and this enum is the concrete set this
/// fabric understands. [`ClockDomain::symbol`] maps a variant to its kernel
/// symbol and [`ClockDomain::from_symbol`] parses it back, accepting the bare
/// label, the `clock/<label>` form, and the fully qualified
/// `stream/clock-domain/<label>` form.
///
/// # Examples
///
/// ```
/// use sim_lib_stream_core::ClockDomain;
///
/// let domain = ClockDomain::Sample;
/// assert_eq!(domain.wire_label(), "sample");
/// let parsed = ClockDomain::from_symbol(&domain.symbol()).unwrap();
/// assert_eq!(parsed, domain);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockDomain {
    /// Per-sample audio timeline (the finest audio clock).
    Sample,
    /// Per-block processing timeline (one tick per audio block).
    Block,
    /// Control-rate timeline for parameter and modulation updates.
    Control,
    /// MIDI tick timeline (musical clock pulses).
    MidiTick,
    /// Wall-clock (real-world) time.
    Wall,
    /// Transport timeline (musical position: bars/beats under play control).
    Transport,
    /// Server-side frame timeline.
    ServerFrame,
    /// Browser-side frame timeline (client render cadence).
    BrowserFrame,
    /// Trace-step timeline for stepped/replayed execution.
    TraceStep,
    /// Job timeline keyed to background job progress.
    Job,
}

impl ClockDomain {
    /// Returns the stable wire label for this domain (for example `"sample"`).
    pub fn wire_label(self) -> &'static str {
        match self {
            Self::Sample => "sample",
            Self::Block => "block",
            Self::Control => "control",
            Self::MidiTick => "midi-tick",
            Self::Wall => "wall",
            Self::Transport => "transport",
            Self::ServerFrame => "server-frame",
            Self::BrowserFrame => "browser-frame",
            Self::TraceStep => "trace-step",
            Self::Job => "job",
        }
    }

    /// Returns the kernel [`Symbol`] for this domain, namespaced under
    /// `stream/clock-domain`.
    pub fn symbol(self) -> Symbol {
        Symbol::qualified("stream/clock-domain", self.wire_label())
    }

    /// Parses a [`ClockDomain`] from a kernel [`Symbol`].
    ///
    /// Accepts the bare label, the compatibility `clock/<label>` form, and the
    /// fully qualified `stream/clock-domain/<label>` form. Returns an error for
    /// any unrecognized clock domain.
    pub fn from_symbol(symbol: &Symbol) -> Result<Self> {
        match symbol.as_qualified_str().as_str() {
            "sample" | "clock/sample" | "stream/clock-domain/sample" => Ok(Self::Sample),
            "block" | "clock/block" | "stream/clock-domain/block" => Ok(Self::Block),
            "control" | "clock/control" | "stream/clock-domain/control" => Ok(Self::Control),
            "midi"
            | "midi-tick"
            | "clock/midi"
            | "clock/midi-tick"
            | "stream/clock-domain/midi-tick" => Ok(Self::MidiTick),
            "wall" | "clock/wall" | "stream/clock-domain/wall" => Ok(Self::Wall),
            "transport" | "clock/transport" | "stream/clock-domain/transport" => {
                Ok(Self::Transport)
            }
            "server-frame" | "clock/server-frame" | "stream/clock-domain/server-frame" => {
                Ok(Self::ServerFrame)
            }
            "browser-frame" | "clock/browser-frame" | "stream/clock-domain/browser-frame" => {
                Ok(Self::BrowserFrame)
            }
            "trace-step" | "clock/trace-step" | "stream/clock-domain/trace-step" => {
                Ok(Self::TraceStep)
            }
            "job" | "clock/job" | "stream/clock-domain/job" => Ok(Self::Job),
            other => Err(Error::Eval(format!("unknown stream clock domain {other}"))),
        }
    }

    /// Resolves the clock domain for a stream's declared clock symbol.
    ///
    /// Returns an error when the declared clock is outside the canonical stream
    /// clock-domain aliases accepted by [`ClockDomain::from_symbol`].
    pub fn for_stream_clock(symbol: &Symbol) -> Result<Self> {
        Self::from_symbol(symbol)
    }
}

//! Stream boundary contract: the [`StreamEnvelope`] that wraps every packet
//! crossing the streaming fabric.
//!
//! An envelope binds a [`StreamPacket`] to the routing and timing metadata a
//! transport needs to carry it: the originating stream and packet ids, the
//! media and direction, a monotonic sequence number, the [`Tick`]s that locate
//! it on its clocks, the [`ClockDomain`]s it rides, the [`TransportProfile`]
//! that bounds what the transport may do, and any diagnostics raised along the
//! way.
//!
//! The kernel owns the protocol vocabulary referenced here -- [`Symbol`],
//! [`Expr`], [`Tick`], and the clock/capability contracts. This module supplies
//! the concrete envelope behavior: construction with validation, the wire form
//! ([`StreamEnvelope::to_expr`] / [`TryFrom<Expr>`]), and the mapping of
//! clock-domain symbols to the [`ClockDomain`] enum.

use std::str::FromStr;

#[path = "envelope/profile.rs"]
mod profile;
#[path = "envelope/ref_codec.rs"]
mod ref_codec;

use sim_kernel::{Error, Expr, Result, Symbol, Tick};

use crate::buffer::{expr_kind, field, string_field, symbol_field};
use crate::{StreamDirection, StreamItem, StreamMedia, StreamMetadata, StreamPacket};
pub use profile::{LatencyClass, StreamCapability, TransportProfile};
use ref_codec::{ref_expr, ref_from_expr};

/// Wire version of the [`StreamEnvelope`] map form.
///
/// Encoded into every [`StreamEnvelope::to_expr`] map and checked on decode;
/// envelopes carrying any other version are rejected.
pub const STREAM_ENVELOPE_VERSION: u32 = 1;

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
    /// Accepts the bare label, the legacy `clock/<label>` form, and the fully
    /// qualified `stream/clock-domain/<label>` form. Returns an error for any
    /// unrecognized clock domain.
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

    /// Resolves the clock domain for a stream's declared clock symbol, falling
    /// back to [`ClockDomain::ServerFrame`] when the symbol is unrecognized.
    pub fn for_stream_clock(symbol: &Symbol) -> Self {
        Self::from_symbol(symbol).unwrap_or(Self::ServerFrame)
    }
}

/// A single packet plus the routing and timing metadata that carries it across
/// the streaming fabric.
///
/// Every envelope is constructed through a validating constructor that checks
/// its [`Tick`]s, confirms the declared [`StreamMedia`] matches the wrapped
/// [`StreamPacket`], folds each tick's clock into the recorded clock-domain set,
/// and stamps the current [`STREAM_ENVELOPE_VERSION`]. The struct fields are
/// private; read access is through the accessor methods, and the wire form is
/// produced by [`StreamEnvelope::to_expr`] and recovered by [`TryFrom<Expr>`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamEnvelope {
    version: u32,
    stream_id: Symbol,
    packet_id: Symbol,
    media: StreamMedia,
    direction: StreamDirection,
    sequence: u64,
    ticks: Vec<Tick>,
    clock_domain: ClockDomain,
    clock_domains: Vec<ClockDomain>,
    profile: TransportProfile,
    diagnostics: Vec<Symbol>,
    packet: StreamPacket,
}

impl StreamEnvelope {
    /// Builds an envelope whose clock-domain set is seeded from the single
    /// primary `clock_domain`.
    ///
    /// Validates the ticks and checks that `media` matches the packet's media;
    /// see [`StreamEnvelope::new_with_clock_domains`] for the full contract.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        stream_id: Symbol,
        packet_id: Symbol,
        media: StreamMedia,
        direction: StreamDirection,
        sequence: u64,
        ticks: Vec<Tick>,
        clock_domain: ClockDomain,
        profile: TransportProfile,
        diagnostics: Vec<Symbol>,
        packet: StreamPacket,
    ) -> Result<Self> {
        Self::new_with_clock_domains(
            stream_id,
            packet_id,
            media,
            direction,
            sequence,
            ticks,
            clock_domain,
            vec![clock_domain],
            profile,
            diagnostics,
            packet,
        )
    }

    /// Builds an envelope with an explicit set of clock domains.
    ///
    /// Validates the ticks via the kernel, requires `media` to equal the
    /// wrapped packet's media (erroring otherwise), augments `clock_domains`
    /// with each tick's clock domain, and normalizes the result so the primary
    /// `clock_domain` leads and no domain repeats. The stored version is always
    /// [`STREAM_ENVELOPE_VERSION`].
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_clock_domains(
        stream_id: Symbol,
        packet_id: Symbol,
        media: StreamMedia,
        direction: StreamDirection,
        sequence: u64,
        ticks: Vec<Tick>,
        clock_domain: ClockDomain,
        clock_domains: Vec<ClockDomain>,
        profile: TransportProfile,
        diagnostics: Vec<Symbol>,
        packet: StreamPacket,
    ) -> Result<Self> {
        sim_kernel::validate_ticks(&ticks)?;
        let packet_media = packet.media();
        if packet_media != media {
            return Err(Error::Eval(format!(
                "stream envelope media {} does not match packet media {}",
                media.symbol(),
                packet_media.symbol()
            )));
        }
        let mut all_clock_domains = clock_domains;
        for tick in &ticks {
            all_clock_domains.push(ClockDomain::from_symbol(&tick.clock)?);
        }
        let clock_domains = normalize_clock_domains(clock_domain, all_clock_domains);
        Ok(Self {
            version: STREAM_ENVELOPE_VERSION,
            stream_id,
            packet_id,
            media,
            direction,
            sequence,
            ticks,
            clock_domain,
            clock_domains,
            profile,
            diagnostics,
            packet,
        })
    }

    /// Builds an envelope from stream `metadata` and one [`StreamItem`], using
    /// the in-memory-local [`TransportProfile`].
    ///
    /// Derives the packet id from the stream id and `sequence`, and resolves the
    /// clock domain from the metadata's declared clock. A convenience wrapper
    /// over [`StreamEnvelope::from_item_with_profile`].
    pub fn from_item(metadata: &StreamMetadata, sequence: u64, item: &StreamItem) -> Result<Self> {
        Self::from_item_with_profile(metadata, sequence, item, TransportProfile::memory_local())
    }

    /// Builds an envelope from stream `metadata` and one [`StreamItem`] under an
    /// explicit [`TransportProfile`].
    ///
    /// Derives the packet id from the stream id and `sequence`, copies the
    /// item's ticks and packet, and resolves the clock domain from the
    /// metadata's clock via [`ClockDomain::for_stream_clock`]. No diagnostics
    /// are attached.
    pub fn from_item_with_profile(
        metadata: &StreamMetadata,
        sequence: u64,
        item: &StreamItem,
        profile: TransportProfile,
    ) -> Result<Self> {
        Self::new(
            metadata.id().clone(),
            packet_id(metadata.id(), sequence),
            metadata.media(),
            metadata.direction(),
            sequence,
            item.ticks().to_vec(),
            ClockDomain::for_stream_clock(metadata.clock()),
            profile,
            Vec::new(),
            item.packet().clone(),
        )
    }

    /// Returns the envelope wire version (always [`STREAM_ENVELOPE_VERSION`]).
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Returns the id of the stream this envelope belongs to.
    pub fn stream_id(&self) -> &Symbol {
        &self.stream_id
    }

    /// Returns the id of the wrapped packet.
    pub fn packet_id(&self) -> &Symbol {
        &self.packet_id
    }

    /// Returns the media type carried by this envelope.
    pub fn media(&self) -> StreamMedia {
        self.media
    }

    /// Returns the direction of flow for this envelope.
    pub fn direction(&self) -> StreamDirection {
        self.direction
    }

    /// Returns the monotonic sequence number of this envelope within its stream.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the [`Tick`]s that locate this envelope on its clocks.
    pub fn ticks(&self) -> &[Tick] {
        &self.ticks
    }

    /// Returns the primary clock domain this envelope is timed against.
    pub fn clock_domain(&self) -> ClockDomain {
        self.clock_domain
    }

    /// Returns every clock domain this envelope rides.
    ///
    /// The primary [`ClockDomain::clock_domain`](StreamEnvelope::clock_domain)
    /// leads, followed by any additional domains contributed by the ticks, with
    /// no repeats.
    pub fn clock_domains(&self) -> &[ClockDomain] {
        &self.clock_domains
    }

    /// Returns the transport profile bounding what a carrier may do with this
    /// envelope.
    pub fn profile(&self) -> &TransportProfile {
        &self.profile
    }

    /// Returns the diagnostic symbols attached to this envelope.
    pub fn diagnostics(&self) -> &[Symbol] {
        &self.diagnostics
    }

    /// Returns the wrapped packet payload.
    pub fn packet(&self) -> &StreamPacket {
        &self.packet
    }

    /// Encodes this envelope into its [`Expr`] map wire form.
    ///
    /// The map is tagged with [`stream_envelope_tag_symbol`] and round-trips
    /// back through the [`TryFrom<Expr>`] implementation.
    pub fn to_expr(&self) -> Expr {
        Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("envelope")),
                Expr::Symbol(stream_envelope_tag_symbol()),
            ),
            (
                Expr::Symbol(Symbol::new("version")),
                Expr::String(self.version.to_string()),
            ),
            (
                Expr::Symbol(Symbol::new("stream-id")),
                Expr::Symbol(self.stream_id.clone()),
            ),
            (
                Expr::Symbol(Symbol::new("packet-id")),
                Expr::Symbol(self.packet_id.clone()),
            ),
            (
                Expr::Symbol(Symbol::new("media")),
                Expr::Symbol(self.media.symbol()),
            ),
            (
                Expr::Symbol(Symbol::new("direction")),
                Expr::Symbol(self.direction.symbol()),
            ),
            (
                Expr::Symbol(Symbol::new("sequence")),
                Expr::String(self.sequence.to_string()),
            ),
            (
                Expr::Symbol(Symbol::new("ticks")),
                Expr::List(self.ticks.iter().map(tick_expr).collect()),
            ),
            (
                Expr::Symbol(Symbol::new("clock-domain")),
                Expr::Symbol(self.clock_domain.symbol()),
            ),
            (
                Expr::Symbol(Symbol::new("clock-domains")),
                Expr::List(
                    self.clock_domains
                        .iter()
                        .map(|domain| Expr::Symbol(domain.symbol()))
                        .collect(),
                ),
            ),
            (Expr::Symbol(Symbol::new("profile")), self.profile.to_expr()),
            (
                Expr::Symbol(Symbol::new("diagnostics")),
                Expr::List(self.diagnostics.iter().cloned().map(Expr::Symbol).collect()),
            ),
            (Expr::Symbol(Symbol::new("packet")), self.packet.to_expr()),
        ])
    }
}

impl TryFrom<Expr> for StreamEnvelope {
    type Error = Error;

    fn try_from(expr: Expr) -> Result<Self> {
        let Expr::Map(entries) = &expr else {
            return Err(Error::TypeMismatch {
                expected: "stream envelope map",
                found: expr_kind(&expr),
            });
        };
        ensure_fields(
            entries,
            &[
                "envelope",
                "version",
                "stream-id",
                "packet-id",
                "media",
                "direction",
                "sequence",
                "ticks",
                "clock-domain",
                "clock-domains",
                "profile",
                "diagnostics",
                "packet",
            ],
        )?;
        let tag = symbol_field(entries, "envelope")?;
        if *tag != stream_envelope_tag_symbol() {
            return Err(Error::Eval(format!(
                "unknown stream envelope tag {}",
                tag.as_qualified_str()
            )));
        }
        let version = parse_string_field::<u32>(entries, "version")?;
        if version != STREAM_ENVELOPE_VERSION {
            return Err(Error::Eval(format!(
                "unsupported stream envelope version {version}"
            )));
        }
        let packet = StreamPacket::try_from(field(entries, "packet")?.clone())?;
        let ticks = tick_list(entries, "ticks")?;
        Self::new_with_clock_domains(
            symbol_field(entries, "stream-id")?.clone(),
            symbol_field(entries, "packet-id")?.clone(),
            StreamMedia::from_symbol(symbol_field(entries, "media")?)?,
            StreamDirection::from_symbol(symbol_field(entries, "direction")?)?,
            parse_string_field::<u64>(entries, "sequence")?,
            ticks,
            ClockDomain::from_symbol(symbol_field(entries, "clock-domain")?)?,
            clock_domain_list(entries, "clock-domains")?,
            TransportProfile::from_expr(field(entries, "profile")?)?,
            symbol_list(entries, "diagnostics")?.to_vec(),
            packet,
        )
    }
}

/// Returns the runtime tag [`Symbol`] that marks a map as a stream envelope.
///
/// Written under the `envelope` key by [`StreamEnvelope::to_expr`] and required
/// on decode by the [`TryFrom<Expr>`] implementation.
pub fn stream_envelope_tag_symbol() -> Symbol {
    Symbol::qualified("stream/envelope", "v1")
}

fn packet_id(stream_id: &Symbol, sequence: u64) -> Symbol {
    Symbol::qualified(
        "stream/packet-id",
        format!("{}#{sequence}", stream_id.as_qualified_str()),
    )
}

fn tick_expr(tick: &Tick) -> Expr {
    Expr::Map(vec![
        (
            Expr::Symbol(Symbol::new("clock")),
            Expr::Symbol(tick.clock.clone()),
        ),
        (Expr::Symbol(Symbol::new("index")), ref_expr(&tick.index)),
    ])
}

fn tick_from_expr(expr: &Expr) -> Result<Tick> {
    let Expr::Map(entries) = expr else {
        return Err(Error::TypeMismatch {
            expected: "stream tick map",
            found: expr_kind(expr),
        });
    };
    ensure_fields(entries, &["clock", "index"])?;
    Ok(Tick::new(
        symbol_field(entries, "clock")?.clone(),
        ref_from_expr(field(entries, "index")?)?,
    ))
}

fn parse_string_field<T>(entries: &[(Expr, Expr)], name: &str) -> Result<T>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    string_field(entries, name)?
        .parse::<T>()
        .map_err(|err| Error::Eval(format!("invalid stream envelope {name}: {err}")))
}

fn tick_list(entries: &[(Expr, Expr)], name: &str) -> Result<Vec<Tick>> {
    list_field(entries, name)?
        .iter()
        .map(tick_from_expr)
        .collect()
}

fn clock_domain_list(entries: &[(Expr, Expr)], name: &str) -> Result<Vec<ClockDomain>> {
    symbol_list(entries, name)?
        .iter()
        .map(ClockDomain::from_symbol)
        .collect()
}

fn symbol_list(entries: &[(Expr, Expr)], name: &str) -> Result<Vec<Symbol>> {
    list_field(entries, name)?
        .iter()
        .map(|expr| match expr {
            Expr::Symbol(symbol) => Ok(symbol.clone()),
            other => Err(Error::TypeMismatch {
                expected: "symbol list item",
                found: expr_kind(other),
            }),
        })
        .collect()
}

fn list_field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> Result<&'a [Expr]> {
    match field(entries, name)? {
        Expr::List(items) => Ok(items),
        other => Err(Error::TypeMismatch {
            expected: "list field",
            found: expr_kind(other),
        }),
    }
}

fn ensure_fields(entries: &[(Expr, Expr)], allowed: &[&str]) -> Result<()> {
    for (key, _) in entries {
        let Expr::Symbol(symbol) = key else {
            return Err(Error::TypeMismatch {
                expected: "symbol stream envelope field",
                found: expr_kind(key),
            });
        };
        if symbol.namespace.is_none() && allowed.contains(&symbol.name.as_ref()) {
            continue;
        }
        return Err(Error::Eval(format!(
            "unknown stream envelope field {}",
            symbol.as_qualified_str()
        )));
    }
    Ok(())
}

fn normalize_clock_domains(
    primary: ClockDomain,
    clock_domains: Vec<ClockDomain>,
) -> Vec<ClockDomain> {
    let mut domains = vec![primary];
    for domain in clock_domains {
        if !domains.contains(&domain) {
            domains.push(domain);
        }
    }
    domains
}

//! Stream metadata values and their publication into the runtime claim store.
//!
//! This module supplies the concrete description of a stream's identity:
//! [`StreamMetadata`] bundles a stream id, its [`StreamMedia`] kind,
//! [`StreamDirection`], clock symbol, and buffer policy. [`RateContract`]
//! captures the clock-domain/latency/rate agreement two ports must share to
//! connect.
//!
//! The kernel defines the claim/fact contract and the clock-domain surface;
//! this module supplies the streaming-fabric behavior on top of it. The
//! `stream_*_predicate` helpers name the predicate symbols, and
//! [`publish_metadata_claims`] writes a stream's metadata into the runtime as
//! public facts so other libraries can query a stream's shape.

use sim_kernel::{
    Claim, ClaimPattern, Cx, Error, Expr, Ref, Result, Symbol, stream_surface,
    stream_surface::publish_stream_metadata_claims,
};

use crate::buffer::{BufferPolicy, expr_kind, field, string_field, symbol_field};
use crate::{ClockDomain, LatencyClass};

/// Media kind carried by a stream.
///
/// Selects which packet profile an envelope on the stream is expected to
/// carry and which `stream/media/*` symbol identifies the stream in the
/// claim store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamMedia {
    /// Real-time PCM audio frames (see [`PcmPacket`](crate::PcmPacket)).
    Pcm,
    /// MIDI events (see [`MidiPacket`](crate::MidiPacket)).
    Midi,
    /// Diagnostic messages (see [`StreamDiagnostic`](crate::StreamDiagnostic)).
    Diagnostic,
    /// Opaque structured data payloads (see [`DataPacket`](crate::DataPacket)).
    Data,
}

impl StreamMedia {
    /// Returns the `stream/media/*` symbol identifying this media kind.
    pub fn symbol(self) -> Symbol {
        match self {
            Self::Pcm => Symbol::qualified("stream/media", "pcm"),
            Self::Midi => Symbol::qualified("stream/media", "midi"),
            Self::Diagnostic => Symbol::qualified("stream/media", "diagnostic"),
            Self::Data => Symbol::qualified("stream/media", "data"),
        }
    }

    /// Parses a [`StreamMedia`] from its `stream/media/*` symbol.
    ///
    /// Returns an error for any symbol outside the known media kinds.
    pub fn from_symbol(symbol: &Symbol) -> Result<Self> {
        match symbol.as_qualified_str().as_str() {
            "stream/media/pcm" => Ok(Self::Pcm),
            "stream/media/midi" => Ok(Self::Midi),
            "stream/media/diagnostic" => Ok(Self::Diagnostic),
            "stream/media/data" => Ok(Self::Data),
            other => Err(Error::Eval(format!("unknown stream media {other}"))),
        }
    }
}

/// Flow direction of a stream relative to its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamDirection {
    /// Produces envelopes (output).
    Source,
    /// Consumes envelopes (input).
    Sink,
    /// Both produces and consumes envelopes.
    Duplex,
}

impl StreamDirection {
    /// Returns the `stream/direction/*` symbol identifying this direction.
    pub fn symbol(self) -> Symbol {
        match self {
            Self::Source => Symbol::qualified("stream/direction", "source"),
            Self::Sink => Symbol::qualified("stream/direction", "sink"),
            Self::Duplex => Symbol::qualified("stream/direction", "duplex"),
        }
    }

    /// Parses a [`StreamDirection`] from its `stream/direction/*` symbol.
    ///
    /// Returns an error for any symbol outside the known directions.
    pub fn from_symbol(symbol: &Symbol) -> Result<Self> {
        match symbol.as_qualified_str().as_str() {
            "stream/direction/source" => Ok(Self::Source),
            "stream/direction/sink" => Ok(Self::Sink),
            "stream/direction/duplex" => Ok(Self::Duplex),
            other => Err(Error::Eval(format!("unknown stream direction {other}"))),
        }
    }
}

/// Timing agreement a stream port advertises and must share to connect.
///
/// Pairs a [`ClockDomain`] with a [`LatencyClass`] and an optional nominal
/// sample rate in hertz. Two ports may be wired together only when their
/// contracts are compatible (see [`is_compatible_with`](RateContract::is_compatible_with)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateContract {
    clock_domain: ClockDomain,
    latency_class: LatencyClass,
    nominal_rate_hz: Option<u32>,
}

impl RateContract {
    /// Builds a contract from an explicit clock domain, latency class, and
    /// optional nominal rate.
    pub fn new(
        clock_domain: ClockDomain,
        latency_class: LatencyClass,
        nominal_rate_hz: Option<u32>,
    ) -> Self {
        Self {
            clock_domain,
            latency_class,
            nominal_rate_hz,
        }
    }

    /// Contract for sample-exact audio: sample clock domain, sample-exact
    /// latency, and the given nominal rate.
    pub fn sample_exact(nominal_rate_hz: Option<u32>) -> Self {
        Self::new(
            ClockDomain::Sample,
            LatencyClass::SampleExact,
            nominal_rate_hz,
        )
    }

    /// Contract for block-local processing: block clock domain and block-local
    /// latency, with no fixed nominal rate.
    pub fn block_local() -> Self {
        Self::new(ClockDomain::Block, LatencyClass::BlockLocal, None)
    }

    /// Contract for interactive control traffic: control clock domain and
    /// interactive latency.
    pub fn control() -> Self {
        Self::new(ClockDomain::Control, LatencyClass::Interactive, None)
    }

    /// Contract for MIDI-tick traffic: MIDI-tick clock domain and interactive
    /// latency.
    pub fn midi_tick() -> Self {
        Self::new(ClockDomain::MidiTick, LatencyClass::Interactive, None)
    }

    /// Contract for offline trace stepping: trace-step clock domain and
    /// offline-render latency.
    pub fn trace_step() -> Self {
        Self::new(ClockDomain::TraceStep, LatencyClass::OfflineRender, None)
    }

    /// Returns the clock domain this contract runs in.
    pub fn clock_domain(self) -> ClockDomain {
        self.clock_domain
    }

    /// Returns the latency class this contract promises.
    pub fn latency_class(self) -> LatencyClass {
        self.latency_class
    }

    /// Returns the nominal sample rate in hertz, if one is fixed.
    pub fn nominal_rate_hz(self) -> Option<u32> {
        self.nominal_rate_hz
    }

    /// Reports whether `self` and `other` may be connected.
    ///
    /// Compatible means matching clock domain and latency class; nominal rates
    /// must agree only when both are fixed (an unset rate matches any rate).
    pub fn is_compatible_with(self, other: Self) -> bool {
        self.clock_domain == other.clock_domain
            && self.latency_class == other.latency_class
            && rates_are_compatible(self.nominal_rate_hz, other.nominal_rate_hz)
    }

    /// Checks compatibility with `other`, returning a descriptive error when
    /// the two contracts cannot be connected.
    pub fn ensure_compatible(self, other: Self) -> Result<()> {
        if self.is_compatible_with(other) {
            return Ok(());
        }
        Err(Error::Eval(format!(
            "incompatible port rate contracts: source {} {} {:?}, target {} {} {:?}",
            self.clock_domain.wire_label(),
            self.latency_class.wire_label(),
            self.nominal_rate_hz,
            other.clock_domain.wire_label(),
            other.latency_class.wire_label(),
            other.nominal_rate_hz
        )))
    }
}

fn rates_are_compatible(left: Option<u32>, right: Option<u32>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left == right,
        _ => true,
    }
}

/// Full identity of a stream: id, media kind, direction, clock, and buffer
/// policy.
///
/// This is the value other libraries inspect to learn a stream's shape, and
/// the value [`publish_metadata_claims`] turns into runtime facts. It
/// round-trips to and from an [`Expr`] map (`table_expr`/`from_table_expr`)
/// and to constructor arguments (`to_constructor_args`/`from_constructor_args`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamMetadata {
    id: Symbol,
    media: StreamMedia,
    direction: StreamDirection,
    clock: Symbol,
    buffer: BufferPolicy,
}

impl StreamMetadata {
    /// Builds metadata from its id, media kind, direction, clock symbol, and
    /// buffer policy.
    pub fn new(
        id: Symbol,
        media: StreamMedia,
        direction: StreamDirection,
        clock: Symbol,
        buffer: BufferPolicy,
    ) -> Self {
        Self {
            id,
            media,
            direction,
            clock,
            buffer,
        }
    }

    /// Returns the stream's identity symbol.
    pub fn id(&self) -> &Symbol {
        &self.id
    }

    /// Returns the stream's media kind.
    pub fn media(&self) -> StreamMedia {
        self.media
    }

    /// Returns the stream's flow direction.
    pub fn direction(&self) -> StreamDirection {
        self.direction
    }

    /// Returns the symbol naming the stream's clock.
    pub fn clock(&self) -> &Symbol {
        &self.clock
    }

    /// Returns the stream's buffer policy.
    pub fn buffer(&self) -> &BufferPolicy {
        &self.buffer
    }

    /// Returns the claim-store subject reference for this stream (its id as a
    /// symbol ref).
    pub fn subject_ref(&self) -> Ref {
        Ref::Symbol(self.id.clone())
    }

    /// Encodes the metadata as the ordered constructor argument expressions
    /// accepted by [`from_constructor_args`](StreamMetadata::from_constructor_args).
    pub fn to_constructor_args(&self) -> Vec<Expr> {
        vec![
            Expr::Symbol(self.id.clone()),
            Expr::Symbol(self.media.symbol()),
            Expr::Symbol(self.direction.symbol()),
            Expr::Symbol(self.clock.clone()),
            self.buffer.to_expr(),
        ]
    }

    /// Rebuilds metadata from the five constructor argument expressions
    /// produced by [`to_constructor_args`](StreamMetadata::to_constructor_args).
    ///
    /// Returns an error when the argument count or any argument shape is wrong.
    pub fn from_constructor_args(args: Vec<Expr>) -> Result<Self> {
        let [id, media, direction, clock, buffer] = args.as_slice() else {
            return Err(Error::Eval(
                "stream/Metadata expects five constructor arguments".to_owned(),
            ));
        };
        Ok(Self::new(
            symbol_expr(id, "stream id")?,
            StreamMedia::from_symbol(symbol_expr_ref(media, "stream media")?)?,
            StreamDirection::from_symbol(symbol_expr_ref(direction, "stream direction")?)?,
            symbol_expr(clock, "stream clock")?,
            BufferPolicy::from_expr(buffer)?,
        ))
    }

    /// Encodes the metadata as a self-describing `Expr` map keyed by field
    /// name (`kind`, `id`, `media`, `direction`, `clock`, `buffer`).
    pub fn table_expr(&self) -> Expr {
        Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("kind")),
                Expr::Symbol(stream_surface::stream_kind()),
            ),
            (
                Expr::Symbol(Symbol::new("id")),
                Expr::String(self.id.to_string()),
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
                Expr::Symbol(Symbol::new("clock")),
                Expr::Symbol(self.clock.clone()),
            ),
            (Expr::Symbol(Symbol::new("buffer")), self.buffer.to_expr()),
        ])
    }

    /// Rebuilds metadata from the `Expr` map produced by
    /// [`table_expr`](StreamMetadata::table_expr).
    ///
    /// Returns an error when the expression is not a map or a required field
    /// is missing or malformed.
    pub fn from_table_expr(expr: &Expr) -> Result<Self> {
        let Expr::Map(entries) = expr else {
            return Err(Error::TypeMismatch {
                expected: "stream metadata map",
                found: expr_kind(expr),
            });
        };
        Ok(Self::new(
            Symbol::new(string_field(entries, "id")?.to_owned()),
            StreamMedia::from_symbol(symbol_field(entries, "media")?)?,
            StreamDirection::from_symbol(symbol_field(entries, "direction")?)?,
            symbol_field(entries, "clock")?.clone(),
            BufferPolicy::from_expr(field(entries, "buffer")?)?,
        ))
    }
}

/// Returns the `stream/id` predicate symbol used for stream-identity facts.
pub fn stream_id_predicate() -> Symbol {
    Symbol::qualified("stream", "id")
}

/// Returns the `stream/media` predicate symbol used for media-kind facts.
pub fn stream_media_predicate() -> Symbol {
    Symbol::qualified("stream", "media")
}

/// Returns the `stream/direction` predicate symbol used for direction facts.
pub fn stream_direction_predicate() -> Symbol {
    Symbol::qualified("stream", "direction")
}

/// Returns the `stream/buffer` predicate symbol used for buffer-policy facts.
pub fn stream_buffer_predicate() -> Symbol {
    Symbol::qualified("stream", "buffer")
}

/// Publishes a stream's metadata into the runtime as public facts about
/// `subject`.
///
/// Writes one claim per field (id, media, direction, clock, buffer) through
/// the kernel's stream-metadata claim surface, then records the default
/// in-memory transport fact once. The kernel owns the claim/fact contract;
/// this function supplies the streaming-fabric mapping from [`StreamMetadata`]
/// onto it. The transport fact is inserted only if not already present.
pub fn publish_metadata_claims(cx: &mut Cx, subject: Ref, metadata: &StreamMetadata) -> Result<()> {
    publish_stream_metadata_claims(
        cx,
        subject.clone(),
        [
            (stream_id_predicate(), Ref::Symbol(metadata.id.clone())),
            (
                stream_media_predicate(),
                Ref::Symbol(metadata.media.symbol()),
            ),
            (
                stream_direction_predicate(),
                Ref::Symbol(metadata.direction.symbol()),
            ),
            (
                stream_surface::stream_clock_predicate(),
                Ref::Symbol(metadata.clock.clone()),
            ),
            (
                stream_buffer_predicate(),
                Ref::Symbol(metadata.buffer.symbol()),
            ),
        ],
    )?;
    insert_once(
        cx,
        subject,
        stream_surface::stream_transport_predicate(),
        Ref::Symbol(Symbol::qualified("stream", "memory")),
    )
}

fn insert_once(cx: &mut Cx, subject: Ref, predicate: Symbol, object: Ref) -> Result<()> {
    let exists = !cx
        .query_facts(ClaimPattern::exact(
            subject.clone(),
            predicate.clone(),
            object.clone(),
        ))?
        .is_empty();
    if !exists {
        cx.insert_fact(Claim::public(subject, predicate, object))?;
    }
    Ok(())
}

fn symbol_expr(expr: &Expr, expected: &'static str) -> Result<Symbol> {
    Ok(symbol_expr_ref(expr, expected)?.clone())
}

fn symbol_expr_ref<'a>(expr: &'a Expr, expected: &'static str) -> Result<&'a Symbol> {
    match expr {
        Expr::Symbol(symbol) => Ok(symbol),
        other => Err(Error::TypeMismatch {
            expected,
            found: expr_kind(other),
        }),
    }
}

//! Packet payloads carried by stream envelopes.
//!
//! [`StreamPacket`] is the umbrella over the concrete payload kinds an
//! envelope can hold: [`PcmPacket`] audio frames, [`MidiPacket`] events,
//! [`StreamDiagnostic`] messages, and opaque [`DataPacket`] values. Each
//! payload round-trips to and from a self-describing [`Expr`] map tagged with
//! a `stream/packet/*` symbol, so packets can be serialized, interned as
//! kernel data ([`StreamPacket::intern_ref`]), and reconstructed.
//!
//! The kernel defines the `Expr`/`Datum`/datum-store contract; this module
//! supplies the concrete streaming-fabric payload model on top of it.

use std::fmt::Display;
use std::str::FromStr;

use sim_kernel::{Cx, Datum, DatumStore, Error, Expr, Ref, Result, Symbol};
use sim_value::access;

use crate::buffer::{expr_kind, field, string_field, symbol_field};
use crate::metadata::StreamMedia;

#[path = "packet/pcm.rs"]
mod pcm;

pub use pcm::{PcmPacket, PcmSampleFormat};

/// A single timed MIDI event within a [`MidiPacket`].
///
/// Holds the event time in ticks, the ticks-per-quarter-note (TPQ) resolution
/// the ticks are measured against, and the raw MIDI message bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiPacketEvent {
    ticks: i64,
    tpq: u16,
    bytes: Vec<u8>,
}

impl MidiPacketEvent {
    /// Builds an event from its tick time, TPQ resolution, and message bytes.
    ///
    /// Returns an error when `tpq` is zero, since a zero resolution cannot
    /// time the event.
    pub fn new(ticks: i64, tpq: u16, bytes: Vec<u8>) -> Result<Self> {
        if tpq == 0 {
            return Err(Error::Eval(
                "MIDI packet TPQ must be greater than zero".to_owned(),
            ));
        }
        Ok(Self { ticks, tpq, bytes })
    }

    /// Returns the event time in ticks.
    pub fn ticks(&self) -> i64 {
        self.ticks
    }

    /// Returns the ticks-per-quarter-note resolution the ticks are measured in.
    pub fn tpq(&self) -> u16 {
        self.tpq
    }

    /// Returns the raw MIDI message bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A MIDI payload: an ordered run of [`MidiPacketEvent`]s sharing one TPQ
/// resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiPacket {
    tpq: u16,
    events: Vec<MidiPacketEvent>,
}

impl MidiPacket {
    /// Builds a packet from its events, adopting the TPQ of the first event.
    ///
    /// Returns an error when `events` is empty or any event uses a different
    /// TPQ than the first; a packet carries a single shared resolution.
    pub fn new(events: Vec<MidiPacketEvent>) -> Result<Self> {
        let Some(first) = events.first() else {
            return Err(Error::Eval(
                "MIDI packet must contain at least one event".to_owned(),
            ));
        };
        let tpq = first.tpq;
        if events.iter().any(|event| event.tpq != tpq) {
            return Err(Error::Eval(
                "MIDI packet events must use one shared TPQ".to_owned(),
            ));
        }
        Ok(Self { tpq, events })
    }

    /// Returns the shared ticks-per-quarter-note resolution of every event.
    pub fn tpq(&self) -> u16 {
        self.tpq
    }

    /// Returns the packet's events in order.
    pub fn events(&self) -> &[MidiPacketEvent] {
        &self.events
    }

    /// Encodes the packet as a `stream/packet/midi` [`Expr`] map.
    pub fn to_expr(&self) -> Expr {
        Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("packet")),
                Expr::Symbol(Symbol::qualified("stream/packet", "midi")),
            ),
            (
                Expr::Symbol(Symbol::new("tpq")),
                Expr::String(self.tpq.to_string()),
            ),
            (
                Expr::Symbol(Symbol::new("events")),
                Expr::List(self.events.iter().map(midi_event_expr).collect()),
            ),
        ])
    }
}

/// A diagnostic payload: a categorized human-readable message carried
/// in-band on a stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamDiagnostic {
    kind: Symbol,
    message: String,
}

impl StreamDiagnostic {
    /// Builds a diagnostic from its kind symbol and message text.
    pub fn new(kind: Symbol, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Returns the symbol categorizing this diagnostic.
    pub fn kind(&self) -> &Symbol {
        &self.kind
    }

    /// Returns the diagnostic message text.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Encodes the diagnostic as a `stream/packet/diagnostic` [`Expr`] map.
    pub fn to_expr(&self) -> Expr {
        Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("packet")),
                Expr::Symbol(Symbol::qualified("stream/packet", "diagnostic")),
            ),
            (
                Expr::Symbol(Symbol::new("kind")),
                Expr::Symbol(self.kind.clone()),
            ),
            (
                Expr::Symbol(Symbol::new("message")),
                Expr::String(self.message.clone()),
            ),
        ])
    }
}

/// An opaque structured payload: a kind-tagged arbitrary [`Expr`] value.
///
/// Used for application-defined traffic the fabric does not interpret, such as
/// model events and rank frontiers (see [`StreamPacket::model_event`] and
/// [`StreamPacket::rank_frontier`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataPacket {
    /// Symbol categorizing the payload (for example `stream/data/model-event`).
    pub kind: Symbol,
    /// The application-defined payload expression.
    pub payload: Expr,
}

impl DataPacket {
    /// Builds a data packet from its kind symbol and payload expression.
    pub fn new(kind: Symbol, payload: Expr) -> Self {
        Self { kind, payload }
    }

    /// Encodes the packet as a `stream/packet/data` [`Expr`] map.
    pub fn to_expr(&self) -> Expr {
        Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("packet")),
                Expr::Symbol(Symbol::qualified("stream/packet", "data")),
            ),
            (
                Expr::Symbol(Symbol::new("kind")),
                Expr::Symbol(self.kind.clone()),
            ),
            (Expr::Symbol(Symbol::new("payload")), self.payload.clone()),
        ])
    }
}

/// Umbrella over every payload kind a stream envelope can carry.
///
/// Each variant maps to a [`StreamMedia`] kind and to a `stream/packet/*`
/// tagged [`Expr`] map. [`TryFrom<Expr>`](StreamPacket#impl-TryFrom<Expr>-for-StreamPacket)
/// reconstructs a packet from that encoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamPacket {
    /// Real-time PCM audio frames.
    Pcm(PcmPacket),
    /// Timed MIDI events.
    Midi(MidiPacket),
    /// In-band diagnostic message.
    Diagnostic(StreamDiagnostic),
    /// Opaque application-defined data.
    Data(DataPacket),
}

impl StreamPacket {
    /// Returns the [`StreamMedia`] kind this payload belongs to.
    pub fn media(&self) -> StreamMedia {
        match self {
            Self::Pcm(_) => StreamMedia::Pcm,
            Self::Midi(_) => StreamMedia::Midi,
            Self::Diagnostic(_) => StreamMedia::Diagnostic,
            Self::Data(_) => StreamMedia::Data,
        }
    }

    /// Builds a [`StreamPacket::Data`] payload from a kind symbol and payload
    /// expression.
    pub fn data(kind: Symbol, payload: Expr) -> Self {
        Self::Data(DataPacket::new(kind, payload))
    }

    /// Builds a `stream/data/model-event` data packet around `payload`.
    pub fn model_event(payload: Expr) -> Self {
        Self::data(Symbol::qualified("stream/data", "model-event"), payload)
    }

    /// Builds a `stream/data/rank-frontier` data packet around `payload`.
    pub fn rank_frontier(payload: Expr) -> Self {
        Self::data(Symbol::qualified("stream/data", "rank-frontier"), payload)
    }

    /// Encodes the packet as its variant's `stream/packet/*` [`Expr`] map.
    pub fn to_expr(&self) -> Expr {
        match self {
            Self::Pcm(packet) => packet.to_expr(),
            Self::Midi(packet) => packet.to_expr(),
            Self::Diagnostic(packet) => packet.to_expr(),
            Self::Data(packet) => packet.to_expr(),
        }
    }

    /// Interns the packet's encoded form into the runtime datum store and
    /// returns a content [`Ref`] to it.
    ///
    /// The kernel owns the datum store and content-addressing; this turns the
    /// packet's [`Expr`] encoding into an interned [`Datum`].
    pub fn intern_ref(&self, cx: &mut Cx) -> Result<Ref> {
        let datum = Datum::try_from(self.to_expr())?;
        cx.datum_store_mut().intern(datum).map(Ref::Content)
    }
}

impl TryFrom<Expr> for StreamPacket {
    type Error = Error;

    fn try_from(expr: Expr) -> Result<Self> {
        let Expr::Map(entries) = &expr else {
            return Err(Error::TypeMismatch {
                expected: "stream packet map",
                found: expr_kind(&expr),
            });
        };
        let packet = packet_symbol(entries)?;
        match packet.as_qualified_str().as_str() {
            "stream/packet/pcm" => PcmPacket::from_entries(entries).map(Self::Pcm),
            "stream/packet/midi" => MidiPacket::from_entries(entries).map(Self::Midi),
            "stream/packet/diagnostic" => {
                StreamDiagnostic::from_entries(entries).map(Self::Diagnostic)
            }
            "stream/packet/data" => DataPacket::from_entries(entries).map(Self::Data),
            other => Err(Error::Eval(format!("unknown stream packet kind {other}"))),
        }
    }
}

impl MidiPacket {
    fn from_entries(entries: &[(Expr, Expr)]) -> Result<Self> {
        let tpq = parse_string_field::<u16>(entries, "tpq")?;
        let events = list_field(entries, "events")?
            .iter()
            .enumerate()
            .map(|(index, expr)| {
                let event = MidiPacketEvent::from_expr(expr)?;
                if event.tpq() != tpq {
                    return Err(Error::Eval(format!(
                        "MIDI packet event {index} TPQ {} does not match packet TPQ {tpq}",
                        event.tpq()
                    )));
                }
                Ok(event)
            })
            .collect::<Result<Vec<_>>>()?;
        Self::new(events)
    }
}

impl MidiPacketEvent {
    fn from_expr(expr: &Expr) -> Result<Self> {
        let Expr::Map(entries) = expr else {
            return Err(Error::TypeMismatch {
                expected: "MIDI packet event map",
                found: expr_kind(expr),
            });
        };
        let ticks = parse_string_field::<i64>(entries, "ticks")?;
        let tpq = parse_string_field::<u16>(entries, "tpq")?;
        let bytes = bytes_field(entries, "bytes")?.to_vec();
        Self::new(ticks, tpq, bytes)
    }
}

impl StreamDiagnostic {
    fn from_entries(entries: &[(Expr, Expr)]) -> Result<Self> {
        Ok(Self::new(
            symbol_field(entries, "kind")?.clone(),
            string_field(entries, "message")?.to_owned(),
        ))
    }
}

impl DataPacket {
    fn from_entries(entries: &[(Expr, Expr)]) -> Result<Self> {
        ensure_data_fields_closed(entries)?;
        Ok(Self::new(
            symbol_field(entries, "kind")?.clone(),
            field(entries, "payload")?.clone(),
        ))
    }
}

fn packet_symbol(entries: &[(Expr, Expr)]) -> Result<&Symbol> {
    entries
        .iter()
        .find_map(|(key, value)| match (key, value) {
            (Expr::Symbol(key), Expr::Symbol(value)) if key.name.as_ref() == "packet" => {
                Some(value)
            }
            _ => None,
        })
        .ok_or_else(|| Error::Eval("stream packet missing packet symbol".to_owned()))
}

fn ensure_data_fields_closed(entries: &[(Expr, Expr)]) -> Result<()> {
    for (key, _) in entries {
        let Expr::Symbol(symbol) = key else {
            return Err(Error::TypeMismatch {
                expected: "symbol data packet field",
                found: expr_kind(key),
            });
        };
        if symbol.namespace.is_none()
            && matches!(symbol.name.as_ref(), "packet" | "kind" | "payload")
        {
            continue;
        }
        return Err(Error::Eval(format!(
            "unknown data packet field {}",
            symbol.as_qualified_str()
        )));
    }
    Ok(())
}

pub(super) fn parse_string_field<T>(entries: &[(Expr, Expr)], name: &str) -> Result<T>
where
    T: FromStr,
    T::Err: Display,
{
    string_field(entries, name)?
        .parse::<T>()
        .map_err(|err| Error::Eval(format!("invalid stream packet {name}: {err}")))
}

pub(super) fn parse_string_expr<T>(expr: &Expr, expected: &'static str) -> Result<T>
where
    T: FromStr,
    T::Err: Display,
{
    match expr {
        Expr::String(value) => value
            .parse::<T>()
            .map_err(|err| Error::Eval(format!("{expected} parse failed: {err}"))),
        other => Err(Error::TypeMismatch {
            expected,
            found: expr_kind(other),
        }),
    }
}

pub(super) fn list_field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> Result<&'a [Expr]> {
    access::entry_required_list(entries, name, "list field")
}

fn bytes_field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> Result<&'a [u8]> {
    match field(entries, name)? {
        Expr::Bytes(bytes) => Ok(bytes),
        other => Err(Error::TypeMismatch {
            expected: "bytes field",
            found: expr_kind(other),
        }),
    }
}

fn midi_event_expr(event: &MidiPacketEvent) -> Expr {
    Expr::Map(vec![
        (
            Expr::Symbol(Symbol::new("ticks")),
            Expr::String(event.ticks.to_string()),
        ),
        (
            Expr::Symbol(Symbol::new("tpq")),
            Expr::String(event.tpq.to_string()),
        ),
        (
            Expr::Symbol(Symbol::new("bytes")),
            Expr::Bytes(event.bytes.clone()),
        ),
    ])
}

use std::ops::RangeBounds;
use std::sync::{Arc, Mutex};

use sim_kernel::{
    Cx, Error, Event, EventKind, EventLedger, Ref, Result, Severity, Symbol, Tick, value_from_ref,
};
use sim_lib_stream_core::{
    StreamCassette, StreamDiagnostic, StreamItem, StreamMetadata, StreamPacket, StreamStats,
    TransportProfile,
};

use crate::stream::{Stream, StreamNode};

/// A fully captured stream: its metadata plus every packet it produced.
///
/// A recording is the materialized, replayable form of a finished stream. It is
/// produced by draining a [`Stream`] to `done` and can be replayed any number
/// of times, seeked into, or serialized to a transport cassette.
///
/// # Examples
///
/// ```
/// use sim_kernel::{Expr, Symbol};
/// use sim_lib_stream_core::{
///     BufferOverflowPolicy, BufferPolicy, StreamDirection, StreamItem, StreamMedia,
///     StreamMetadata, StreamPacket,
/// };
/// use sim_lib_stream_combinators::{record_bang, Stream};
///
/// let metadata = StreamMetadata::new(
///     Symbol::qualified("stream", "doc"),
///     StreamMedia::Data,
///     StreamDirection::Source,
///     Symbol::qualified("clock", "doc"),
///     BufferPolicy::bounded_with_overflow(8, BufferOverflowPolicy::DropNewest).unwrap(),
/// );
/// let item = StreamItem::new(StreamPacket::data(
///     Symbol::qualified("stream/data", "model-event"),
///     Expr::Nil,
/// ));
/// let stream = Stream::pull(metadata, vec![item.clone()]);
///
/// let recording = record_bang(&stream).unwrap();
/// assert_eq!(recording.len(), 1);
/// assert_eq!(recording.replay().take_packets(8).unwrap(), vec![item]);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamRecording {
    metadata: StreamMetadata,
    items: Vec<StreamItem>,
}

impl StreamRecording {
    /// Builds a recording from explicit metadata and captured packets.
    pub fn new(metadata: StreamMetadata, items: Vec<StreamItem>) -> Self {
        Self { metadata, items }
    }

    /// Returns the metadata of the recorded stream.
    pub fn metadata(&self) -> &StreamMetadata {
        &self.metadata
    }

    /// Returns the captured packets in their recorded order.
    pub fn items(&self) -> &[StreamItem] {
        &self.items
    }

    /// Returns the number of captured packets.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Reports whether the recording captured no packets.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns a fresh stream that replays the captured packets.
    pub fn replay(&self) -> Stream {
        replay(self)
    }

    /// Replays the recording from the first packet matching `target`.
    pub fn seek(&self, target: SeekTarget) -> Stream {
        seek(self.replay(), target)
    }

    /// Serializes the recording into a transport cassette for `profile`.
    pub fn cassette(&self, profile: TransportProfile) -> Result<StreamCassette> {
        StreamCassette::from_items(
            self.metadata.clone(),
            self.items.clone(),
            profile,
            StreamStats {
                yielded: self.items.len() as u64,
                ..StreamStats::default()
            },
        )
    }
}

/// Where a [`seek`] should begin replaying within a recorded stream.
///
/// # Examples
///
/// ```
/// use sim_lib_stream_combinators::SeekTarget;
///
/// let by_index = SeekTarget::packet_index(2);
/// assert_eq!(by_index, SeekTarget::PacketIndex(2));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeekTarget {
    /// Start at the packet at this zero-based position in the stream.
    PacketIndex(usize),
    /// Start at the first packet bearing `index` on the named `clock`.
    ClockIndex {
        /// The clock whose tick index is matched.
        clock: Symbol,
        /// The tick index on `clock` to seek to.
        index: Ref,
    },
}

impl SeekTarget {
    /// Builds a [`SeekTarget::PacketIndex`] for the given position.
    pub fn packet_index(index: usize) -> Self {
        Self::PacketIndex(index)
    }

    /// Builds a [`SeekTarget::ClockIndex`] for the given clock and tick index.
    pub fn clock_index(clock: Symbol, index: Ref) -> Self {
        Self::ClockIndex { clock, index }
    }
}

/// Drains `source` to `done` and captures it as a [`StreamRecording`].
///
/// Errors if the stream is exhausted without reaching its terminal `done`.
pub fn record_bang(source: &Stream) -> Result<StreamRecording> {
    let mut items = Vec::new();
    while let Some(item) = source.next_packet()? {
        items.push(item);
    }
    if !source.is_done()? {
        return Err(Error::Eval(
            "cannot record a stream that has not reached done".to_owned(),
        ));
    }
    Ok(StreamRecording::new(source.metadata().clone(), items))
}

/// Returns a fresh stream replaying every packet of `recording`.
pub fn replay(recording: &StreamRecording) -> Stream {
    Stream::pull(recording.metadata.clone(), recording.items.clone())
}

/// Records `source` to completion and serializes it to a cassette for `profile`.
pub fn record_cassette_bang(source: &Stream, profile: TransportProfile) -> Result<StreamCassette> {
    record_bang(source)?.cassette(profile)
}

/// Rebuilds a replayable stream from a serialized transport `cassette`.
pub fn replay_cassette(cassette: &StreamCassette) -> Result<Stream> {
    Ok(Stream::from_value(Arc::new(
        cassette.replay_stream_value()?,
    )))
}

/// Returns a stream that skips ahead in `source` to the first packet at `target`.
///
/// The stream then continues from that packet; if no packet matches, it is
/// empty.
pub fn seek(source: Stream, target: SeekTarget) -> Stream {
    Stream::new(SeekNode {
        source,
        target,
        state: Mutex::new(SeekState::Pending),
    })
}

/// Reconstructs a recording from all of `run`'s events in `ledger`.
///
/// Convenience wrapper over [`record_events`] for an entire run.
pub fn record_ledger_run(
    cx: &mut Cx,
    metadata: StreamMetadata,
    ledger: &EventLedger,
    run: &Ref,
) -> Result<StreamRecording> {
    record_events(cx, metadata, ledger.events_for_run(run))
}

/// Reconstructs a recording from the events of `run` within `seq_range`.
///
/// Like [`record_ledger_run`] but limited to events whose sequence number
/// falls inside `seq_range`.
pub fn record_ledger_slice<R>(
    cx: &mut Cx,
    metadata: StreamMetadata,
    ledger: &EventLedger,
    run: &Ref,
    seq_range: R,
) -> Result<StreamRecording>
where
    R: RangeBounds<u64>,
{
    record_events(
        cx,
        metadata,
        ledger
            .events_for_run(run)
            .iter()
            .filter(|event| seq_range.contains(&event.seq)),
    )
}

/// Reconstructs a recording from an arbitrary sequence of kernel `events`.
///
/// Chunk events are decoded back into stream packets and diagnostic events into
/// diagnostic packets; a `done` event ends capture, a `failed` event errors,
/// and other event kinds are ignored.
pub fn record_events<'a>(
    cx: &mut Cx,
    metadata: StreamMetadata,
    events: impl IntoIterator<Item = &'a Event>,
) -> Result<StreamRecording> {
    let mut items = Vec::new();
    for event in events {
        match &event.kind {
            EventKind::Chunk { payload } => {
                items.push(item_from_payload(cx, payload, event.ticks.clone())?);
            }
            EventKind::Diagnostic(diagnostic) => {
                items.push(StreamItem::new(StreamPacket::Diagnostic(
                    diagnostic_packet(diagnostic),
                )));
            }
            EventKind::Done => break,
            EventKind::Failed(_) => {
                return Err(Error::Eval(
                    "cannot record a failed stream event slice".to_owned(),
                ));
            }
            EventKind::Started { .. }
            | EventKind::Claim { .. }
            | EventKind::Trace(_)
            | EventKind::EffectRequested { .. }
            | EventKind::EffectResolved { .. }
            | EventKind::Capture { .. }
            | EventKind::Card { .. }
            | EventKind::Final(_) => {}
        }
    }
    Ok(StreamRecording::new(metadata, items))
}

fn item_from_payload(cx: &mut Cx, payload: &Ref, ticks: Vec<Tick>) -> Result<StreamItem> {
    let value = value_from_ref(cx, payload)?;
    let packet = StreamPacket::try_from(value.object().as_expr(cx)?)?;
    StreamItem::with_ticks(packet, ticks)
}

fn diagnostic_packet(diagnostic: &sim_kernel::Diagnostic) -> StreamDiagnostic {
    let kind = diagnostic
        .code
        .clone()
        .unwrap_or_else(|| Symbol::qualified("stream/combinator", "Diagnostic"));
    let prefix = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Note => "note",
    };
    StreamDiagnostic::new(kind, format!("{prefix}: {}", diagnostic.message))
}

struct SeekNode {
    source: Stream,
    target: SeekTarget,
    state: Mutex<SeekState>,
}

enum SeekState {
    Pending,
    Ready,
    Drained,
}

impl StreamNode for SeekNode {
    fn metadata(&self) -> &StreamMetadata {
        self.source.metadata()
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("seek stream"))?;
        match *state {
            SeekState::Ready => self.source.next_packet(),
            SeekState::Drained => Ok(None),
            SeekState::Pending => {
                let item = seek_first(&self.source, &self.target)?;
                *state = if item.is_some() {
                    SeekState::Ready
                } else {
                    SeekState::Drained
                };
                Ok(item)
            }
        }
    }

    fn is_done(&self) -> Result<bool> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("seek stream"))?;
        match *state {
            SeekState::Drained => Ok(true),
            SeekState::Pending | SeekState::Ready => self.source.is_done(),
        }
    }
}

fn seek_first(source: &Stream, target: &SeekTarget) -> Result<Option<StreamItem>> {
    match target {
        SeekTarget::PacketIndex(index) => {
            for _ in 0..*index {
                if source.next_packet()?.is_none() {
                    return Ok(None);
                }
            }
            source.next_packet()
        }
        SeekTarget::ClockIndex { clock, index } => {
            while let Some(item) = source.next_packet()? {
                if item
                    .ticks()
                    .iter()
                    .any(|tick| &tick.clock == clock && &tick.index == index)
                {
                    return Ok(Some(item));
                }
            }
            Ok(None)
        }
    }
}

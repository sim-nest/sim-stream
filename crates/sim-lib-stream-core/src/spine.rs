//! The stream spine: the runtime-visible stream value and its base combinators.
//!
//! A [`StreamValue`] is the homogeneous stream object the runtime hands around.
//! It pairs immutable [`StreamMetadata`] with one of two internal spines -- a
//! pull spine over a fixed buffer of pre-built items, or a push spine whose
//! bounded queue is fed by an external producer. Both are driven through one
//! base combinator surface: `next`/`peek`/`take`/`run`/`cancel`/`done?`/
//! `metadata`/`stats`, exposed both as Rust methods on [`StreamValue`] and as
//! free `stream_*_bang` verb helpers paired with `stream_*_symbol` helpers that
//! name the corresponding kernel [`Symbol`].
//!
//! The unit of flow is a [`StreamItem`]: a [`StreamPacket`] plus the clock-
//! domain [`Tick`]s observed at it. The kernel defines the protocol contracts
//! ([`Sequence`], [`Object`], [`Event`], [`Symbol`]); this module supplies the
//! concrete streaming-fabric behavior over them.

mod event_source;
mod queue;

use std::{sync::Arc, time::Duration};

use sim_citizen_derive::non_citizen;
use sim_kernel::{
    CORE_SEQUENCE_CLASS_ID, ClassRef, Cx, Error, Event, Object, ObjectCompat, Ref, Result,
    Sequence, SequenceItem, Symbol, Tick, Value, stream_surface::stream_packet_event,
    validate_ticks,
};

use crate::{StreamMetadata, StreamPacket, publish_metadata_claims};

pub use event_source::StreamEventSource;
use queue::{PullSpine, PushSpine};
pub use queue::{PushResult, StreamStats};

/// One unit of flow through the spine: a packet plus its clock-domain ticks.
///
/// Couples a [`StreamPacket`] payload with the [`Tick`]s observed at it, so a
/// packet carries its clock-domain context as it moves through the stream. An
/// item can be projected into a runtime [`Value`], a [`SequenceItem`], or a
/// sequenced packet [`Event`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamItem {
    packet: StreamPacket,
    ticks: Vec<Tick>,
}

impl StreamItem {
    /// Creates an item carrying `packet` with no ticks.
    pub fn new(packet: StreamPacket) -> Self {
        Self {
            packet,
            ticks: Vec::new(),
        }
    }

    /// Creates an item carrying `packet` with `ticks`, validating the ticks.
    ///
    /// Returns an error if `ticks` is not a valid clock-domain reading.
    pub fn with_ticks(packet: StreamPacket, ticks: Vec<Tick>) -> Result<Self> {
        validate_ticks(&ticks)?;
        Ok(Self { packet, ticks })
    }

    /// Returns the packet payload.
    pub fn packet(&self) -> &StreamPacket {
        &self.packet
    }

    /// Returns the clock-domain ticks observed at this item.
    pub fn ticks(&self) -> &[Tick] {
        &self.ticks
    }

    /// Materializes the packet payload as a runtime [`Value`].
    pub fn packet_value(&self, cx: &mut Cx) -> Result<Value> {
        cx.factory().expr(self.packet.to_expr())
    }

    /// Projects this item into a [`SequenceItem`], preserving its ticks.
    pub fn sequence_item(&self, cx: &mut Cx) -> Result<SequenceItem> {
        SequenceItem::with_ticks(self.packet_value(cx)?, self.ticks.clone())
    }

    /// Builds a sequenced packet [`Event`] for `run` at sequence number `seq`.
    pub fn chunk_event(&self, cx: &mut Cx, run: Ref, seq: u64) -> Result<Event> {
        let payload = self.packet.intern_ref(cx)?;
        stream_packet_event(run, seq, self.ticks.clone(), payload)
    }
}

/// The runtime-visible stream value: metadata plus a live spine.
///
/// A `StreamValue` is the homogeneous stream object the runtime passes around.
/// It is a non-citizen live handle (it is not serialized directly; consumers
/// reconstruct the `stream/Packet` and `stream/Metadata` descriptors and
/// realize them separately). Build a finite, replayable stream with
/// [`StreamValue::pull`] or a producer-fed stream with [`StreamValue::push`],
/// then drive it through the base combinators ([`next_packet`], [`peek_packet`],
/// [`take_packets`], [`run_events`], [`cancel`], [`is_done`]).
///
/// As a kernel [`Object`] it presents as a [`Sequence`], so it interoperates
/// with sequence-consuming operations directly.
///
/// [`next_packet`]: StreamValue::next_packet
/// [`peek_packet`]: StreamValue::peek_packet
/// [`take_packets`]: StreamValue::take_packets
/// [`run_events`]: StreamValue::run_events
/// [`cancel`]: StreamValue::cancel
/// [`is_done`]: StreamValue::is_done
#[non_citizen(
    reason = "live stream spine; reconstruct stream/Packet and stream/Metadata descriptors then realize separately",
    kind = "handle",
    descriptor = "stream/Packet"
)]
pub struct StreamValue {
    metadata: StreamMetadata,
    spine: StreamSpine,
}

enum StreamSpine {
    Pull(PullSpine),
    Push(PushSpine),
}

impl StreamValue {
    /// Builds a pull stream that yields the given pre-built `items` in order.
    ///
    /// A pull stream is finite and self-draining: it serves `items` until
    /// exhausted, then reports done. It rejects pushed packets.
    pub fn pull(metadata: StreamMetadata, items: Vec<StreamItem>) -> Self {
        Self {
            metadata,
            spine: StreamSpine::Pull(PullSpine::new(items)),
        }
    }

    /// Builds a push stream fed by an external producer.
    ///
    /// The stream starts empty; producers call [`StreamValue::push_packet`] to
    /// enqueue items under the buffer policy carried by `metadata`, and
    /// consumers pull them out.
    pub fn push(metadata: StreamMetadata) -> Self {
        Self {
            spine: StreamSpine::Push(PushSpine::new(metadata.buffer().clone())),
            metadata,
        }
    }

    /// Returns the stream's immutable metadata.
    pub fn metadata(&self) -> &StreamMetadata {
        &self.metadata
    }

    /// Publishes this stream's metadata as claims about `subject` into `cx`.
    pub fn publish_claims(&self, cx: &mut Cx, subject: Ref) -> Result<()> {
        publish_metadata_claims(cx, subject, &self.metadata)
    }

    /// Pushes a packet into a push stream, returning the backpressure outcome.
    ///
    /// Returns an error when called on a pull stream, which accepts no input.
    pub fn push_packet(&self, item: StreamItem) -> Result<PushResult> {
        match &self.spine {
            StreamSpine::Pull(_) => Err(Error::Eval(
                "cannot push packets into a pull stream".to_owned(),
            )),
            StreamSpine::Push(spine) => spine.push(item),
        }
    }

    /// Closes the stream to further input, leaving buffered items to drain.
    pub fn close_push(&self) -> Result<()> {
        match &self.spine {
            StreamSpine::Pull(spine) => spine.close(),
            StreamSpine::Push(spine) => spine.close(),
        }
    }

    /// Pulls the next packet, or `None` if the stream is currently empty or
    /// exhausted.
    ///
    /// Does not block; on a push stream an empty-but-open queue yields `None`.
    pub fn next_packet(&self) -> Result<Option<StreamItem>> {
        match &self.spine {
            StreamSpine::Pull(spine) => spine.next(),
            StreamSpine::Push(spine) => spine.next(),
        }
    }

    /// Pulls the next packet, blocking up to `timeout` for one to arrive.
    ///
    /// On a pull stream this is equivalent to [`StreamValue::next_packet`]; on a
    /// push stream it waits for a producer up to `timeout` before yielding
    /// `None`.
    pub fn next_packet_timeout(&self, timeout: Duration) -> Result<Option<StreamItem>> {
        match &self.spine {
            StreamSpine::Pull(spine) => spine.next(),
            StreamSpine::Push(spine) => spine.next_timeout(timeout),
        }
    }

    /// Returns a clone of the next packet without consuming it.
    pub fn peek_packet(&self) -> Result<Option<StreamItem>> {
        match &self.spine {
            StreamSpine::Pull(spine) => spine.peek(),
            StreamSpine::Push(spine) => spine.peek(),
        }
    }

    /// Reports whether the stream is exhausted and will yield no more packets.
    pub fn is_done(&self) -> Result<bool> {
        match &self.spine {
            StreamSpine::Pull(spine) => spine.is_done(),
            StreamSpine::Push(spine) => spine.is_done(),
        }
    }

    /// Pulls up to `limit` packets, stopping early when the stream runs dry.
    pub fn take_packets(&self, limit: usize) -> Result<Vec<StreamItem>> {
        let mut out = Vec::new();
        for _ in 0..limit {
            let Some(item) = self.next_packet()? else {
                break;
            };
            out.push(item);
        }
        Ok(out)
    }

    /// Drains the stream into a vector of sequenced packet events.
    ///
    /// Pulls every currently available packet, emitting one packet [`Event`] per
    /// item numbered from `start_seq`, and appends a terminal `done` event if
    /// the stream is exhausted. Events are attributed to `run`.
    pub fn run_events(&self, cx: &mut Cx, run: Ref, start_seq: u64) -> Result<Vec<Event>> {
        let mut seq = start_seq;
        let mut out = Vec::new();
        while let Some(item) = self.next_packet()? {
            out.push(item.chunk_event(cx, run.clone(), seq)?);
            seq = seq.saturating_add(1);
        }
        if self.is_done()? {
            out.push(Event::done(run, seq)?);
        }
        Ok(out)
    }

    /// Cancels the stream: closes it and discards any buffered packets.
    pub fn cancel(&self) -> Result<()> {
        match &self.spine {
            StreamSpine::Pull(spine) => spine.cancel(),
            StreamSpine::Push(spine) => spine.cancel(),
        }
    }

    /// Returns a snapshot of the stream's lifetime [`StreamStats`].
    pub fn stats(&self) -> Result<StreamStats> {
        match &self.spine {
            StreamSpine::Pull(spine) => spine.stats(),
            StreamSpine::Push(spine) => spine.stats(),
        }
    }

    /// Returns the number of packets currently buffered in the spine.
    pub fn queue_depth(&self) -> Result<usize> {
        match &self.spine {
            StreamSpine::Pull(spine) => spine.depth(),
            StreamSpine::Push(spine) => spine.depth(),
        }
    }

    /// Builds an [`StreamEventSource`] that feeds this stream's packets into the
    /// run ledger as sequenced events numbered from `start_seq`.
    pub fn event_source(self: &Arc<Self>, run: Ref, start_seq: u64) -> Arc<StreamEventSource> {
        Arc::new(StreamEventSource::new(Arc::clone(self), run, start_seq))
    }
}

impl Object for StreamValue {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok(format!("#<stream {}>", self.metadata.id()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for StreamValue {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        cx.factory().class_stub(
            CORE_SEQUENCE_CLASS_ID,
            Symbol::qualified("stream", "Stream"),
        )
    }

    fn as_sequence(&self) -> Option<&dyn Sequence> {
        Some(self)
    }
}

impl Sequence for StreamValue {
    fn next_item(&self, cx: &mut Cx) -> Result<Option<SequenceItem>> {
        self.next_packet()?
            .map(|item| item.sequence_item(cx))
            .transpose()
    }

    fn close(&self, _cx: &mut Cx) -> Result<()> {
        self.cancel()
    }

    fn peek_item(&self, cx: &mut Cx) -> Result<Option<SequenceItem>> {
        self.peek_packet()?
            .map(|item| item.sequence_item(cx))
            .transpose()
    }

    fn is_done(&self, _cx: &mut Cx) -> Result<bool> {
        self.is_done()
    }
}

/// `stream/next!`: pulls and consumes the next packet from `stream`.
pub fn stream_next_bang(stream: &StreamValue) -> Result<Option<StreamItem>> {
    stream.next_packet()
}

/// `stream/peek!`: returns the next packet of `stream` without consuming it.
pub fn stream_peek_bang(stream: &StreamValue) -> Result<Option<StreamItem>> {
    stream.peek_packet()
}

/// `stream/done?`: reports whether `stream` is exhausted.
pub fn stream_done_q(stream: &StreamValue) -> Result<bool> {
    stream.is_done()
}

/// `stream/take`: pulls up to `limit` packets from `stream`.
pub fn stream_take(stream: &StreamValue, limit: usize) -> Result<Vec<StreamItem>> {
    stream.take_packets(limit)
}

/// `stream/run!`: drains `stream` into sequenced events numbered from
/// `start_seq`.
pub fn stream_run_bang(
    stream: &StreamValue,
    cx: &mut Cx,
    run: Ref,
    start_seq: u64,
) -> Result<Vec<Event>> {
    stream.run_events(cx, run, start_seq)
}

/// `stream/cancel!`: cancels `stream`, discarding any buffered packets.
pub fn stream_cancel_bang(stream: &StreamValue) -> Result<()> {
    stream.cancel()
}

/// `stream/stats`: returns a snapshot of `stream`'s lifetime counters.
pub fn stream_stats(stream: &StreamValue) -> Result<StreamStats> {
    stream.stats()
}

/// `stream/metadata`: returns `stream`'s immutable metadata.
pub fn stream_metadata(stream: &StreamValue) -> &StreamMetadata {
    stream.metadata()
}

/// The kernel [`Symbol`] naming the `stream/next!` operation.
pub fn stream_next_symbol() -> Symbol {
    Symbol::qualified("stream", "next!")
}

/// The kernel [`Symbol`] naming the `stream/peek!` operation.
pub fn stream_peek_symbol() -> Symbol {
    Symbol::qualified("stream", "peek!")
}

/// The kernel [`Symbol`] naming the `stream/done?` operation.
pub fn stream_done_symbol() -> Symbol {
    Symbol::qualified("stream", "done?")
}

/// The kernel [`Symbol`] naming the `stream/take` operation.
pub fn stream_take_symbol() -> Symbol {
    Symbol::qualified("stream", "take")
}

/// The kernel [`Symbol`] naming the `stream/run!` operation.
pub fn stream_run_symbol() -> Symbol {
    Symbol::qualified("stream", "run!")
}

/// The kernel [`Symbol`] naming the `stream/cancel!` operation.
pub fn stream_cancel_symbol() -> Symbol {
    Symbol::qualified("stream", "cancel!")
}

/// The kernel [`Symbol`] naming the `stream/stats` operation.
pub fn stream_stats_symbol() -> Symbol {
    Symbol::qualified("stream", "stats")
}

/// The kernel [`Symbol`] naming the `stream/metadata` operation.
pub fn stream_metadata_symbol() -> Symbol {
    Symbol::qualified("stream", "metadata")
}

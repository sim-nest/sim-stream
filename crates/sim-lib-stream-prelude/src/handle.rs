use std::sync::{Arc, Mutex};

use sim_citizen_derive::non_citizen;
use sim_kernel::{
    CORE_FUNCTION_CLASS_ID, ClassId, ClassRef, Cx, Error, Expr, Object, ObjectCompat, Result,
    Symbol,
};
use sim_lib_stream_audio::{PcmBuffer, PcmSpec};
use sim_lib_stream_core::{
    MidiPacket, StreamItem, StreamMetadata, StreamPacket, StreamStats, StreamValue,
};

#[non_citizen(
    reason = "live stream handle; reconstruct stream/Metadata and stream/Packet descriptors then open explicitly",
    kind = "handle",
    descriptor = "stream/Metadata"
)]
/// Live handle to a memory stream source, sink, or pipeline.
///
/// A [`StreamHandle`] is a cheap, cloneable reference (an `Arc` inner) to one
/// of four kinds of memory endpoint: a pull source, a PCM sink, a MIDI sink, or
/// a pipeline that connects a source to an optional sink. Reads
/// ([`StreamHandle::next_packet`]), writes ([`StreamHandle::write_packet`]), and
/// whole-stream runs ([`StreamHandle::run`]) are dispatched on the handle kind
/// and fail closed when invoked on an endpoint that does not support them.
#[derive(Clone)]
pub struct StreamHandle {
    inner: Arc<HandleInner>,
}

struct HandleInner {
    metadata: StreamMetadata,
    kind: HandleKind,
}

enum HandleKind {
    Source {
        stream: Arc<StreamValue>,
    },
    PcmSink {
        spec: PcmSpec,
        state: Mutex<SinkState>,
    },
    MidiSink {
        tpq: u16,
        state: Mutex<SinkState>,
    },
    Pipeline {
        source: StreamHandle,
        sink: Option<StreamHandle>,
    },
}

/// Summary of one [`StreamHandle::run`] over a source or pipeline.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunReport {
    /// Number of packets pulled from the source.
    pub packets: usize,
    /// Number of packets written into the sink (zero when there is no sink).
    pub written: usize,
}

/// Kind of pipeline stage carried by a [`StageHandle`].
#[derive(Clone, Debug)]
pub enum StageKind {
    /// Pass-through stage that forwards every packet unchanged.
    Identity,
}

#[non_citizen(
    reason = "live stream stage handle; reconstruct stream/StageDescriptor then realize explicitly",
    kind = "handle",
    descriptor = "stream/StageDescriptor"
)]
/// Handle to a pipeline stage placed between a source and a sink.
///
/// In STREAM 6 the only stage is the identity stage (see
/// [`StageHandle::identity`]); `stream/pipe` accepts identity stages and rejects
/// any other stage kind.
#[derive(Clone, Debug)]
pub struct StageHandle {
    kind: StageKind,
}

#[derive(Clone, Debug, Default)]
struct SinkState {
    packets: Vec<StreamPacket>,
    stats: StreamStats,
    closed: bool,
}

impl StreamHandle {
    /// Builds a pull-source handle backed by the given stream value.
    pub fn source(metadata: StreamMetadata, stream: Arc<StreamValue>) -> Self {
        Self::new(metadata, HandleKind::Source { stream })
    }

    /// Builds a PCM sink handle that validates writes against `spec`.
    pub fn pcm_sink(metadata: StreamMetadata, spec: PcmSpec) -> Self {
        Self::new(
            metadata,
            HandleKind::PcmSink {
                spec,
                state: Mutex::new(SinkState::default()),
            },
        )
    }

    /// Builds a MIDI sink handle that validates writes against `tpq`.
    pub fn midi_sink(metadata: StreamMetadata, tpq: u16) -> Self {
        Self::new(
            metadata,
            HandleKind::MidiSink {
                tpq,
                state: Mutex::new(SinkState::default()),
            },
        )
    }

    /// Builds a pipeline handle from a source-like handle and optional sink.
    ///
    /// # Errors
    ///
    /// Returns an error when `source` is not source-like, or when `sink` is
    /// present but is not a sink handle.
    pub fn pipeline(source: StreamHandle, sink: Option<StreamHandle>) -> Result<Self> {
        if !source.is_source_like() {
            return Err(Error::Eval(
                "stream/pipe expects a source stream first".to_owned(),
            ));
        }
        if sink.as_ref().is_some_and(|handle| !handle.is_sink()) {
            return Err(Error::Eval(
                "stream/pipe sink position must be a sink handle".to_owned(),
            ));
        }
        Ok(Self::new(
            source.metadata().clone(),
            HandleKind::Pipeline { source, sink },
        ))
    }

    fn new(metadata: StreamMetadata, kind: HandleKind) -> Self {
        Self {
            inner: Arc::new(HandleInner { metadata, kind }),
        }
    }

    /// Returns the stream metadata describing this handle.
    pub fn metadata(&self) -> &StreamMetadata {
        &self.inner.metadata
    }

    /// Returns `true` when this handle is a PCM or MIDI sink.
    pub fn is_sink(&self) -> bool {
        matches!(
            self.inner.kind,
            HandleKind::PcmSink { .. } | HandleKind::MidiSink { .. }
        )
    }

    /// Returns `true` when this handle is a pipeline that drives a sink.
    pub fn is_pipeline_with_sink(&self) -> bool {
        matches!(&self.inner.kind, HandleKind::Pipeline { sink: Some(_), .. })
    }

    /// Pulls the next packet from a source or pipeline source.
    ///
    /// Returns `Ok(None)` once the stream is exhausted.
    ///
    /// # Errors
    ///
    /// Returns an error when called on a sink handle, or when the underlying
    /// source fails to produce the next packet.
    pub fn next_packet(&self) -> Result<Option<StreamItem>> {
        match &self.inner.kind {
            HandleKind::Source { stream } => stream.next_packet(),
            HandleKind::Pipeline { source, .. } => source.next_packet(),
            HandleKind::PcmSink { .. } | HandleKind::MidiSink { .. } => Err(Error::Eval(
                "stream/next! expects a source stream handle".to_owned(),
            )),
        }
    }

    /// Writes one packet into a PCM or MIDI sink.
    ///
    /// # Errors
    ///
    /// Returns an error when called on a non-sink handle, when the packet media
    /// does not match the sink, or when the packet fails sink validation.
    pub fn write_packet(&self, packet: StreamPacket) -> Result<()> {
        match &self.inner.kind {
            HandleKind::PcmSink { spec, state } => {
                let StreamPacket::Pcm(pcm) = &packet else {
                    return Err(Error::Eval(
                        "PCM memory sink expects PCM stream packets".to_owned(),
                    ));
                };
                PcmBuffer::from_packet(*spec, pcm)?;
                write_sink_packet(state, packet)
            }
            HandleKind::MidiSink { tpq, state } => {
                let StreamPacket::Midi(midi) = &packet else {
                    return Err(Error::Eval(
                        "MIDI memory sink expects MIDI stream packets".to_owned(),
                    ));
                };
                ensure_midi_tpq(*tpq, midi)?;
                write_sink_packet(state, packet)
            }
            HandleKind::Source { .. } | HandleKind::Pipeline { .. } => Err(Error::Eval(
                "stream/write! expects a sink stream handle".to_owned(),
            )),
        }
    }

    /// Drains a source or pipeline to completion, returning a [`RunReport`].
    ///
    /// For a pipeline with a sink, every pulled packet is written into the sink
    /// and the sink is closed at the end.
    ///
    /// # Errors
    ///
    /// Returns an error when called on a bare sink handle, or when reading or
    /// writing a packet fails.
    pub fn run(&self) -> Result<RunReport> {
        match &self.inner.kind {
            HandleKind::Source { .. } => {
                let mut report = RunReport::default();
                while self.next_packet()?.is_some() {
                    report.packets += 1;
                }
                Ok(report)
            }
            HandleKind::Pipeline { source, sink } => run_pipeline(source, sink.as_ref()),
            HandleKind::PcmSink { .. } | HandleKind::MidiSink { .. } => Err(Error::Eval(
                "stream/run! expects a source or pipeline handle".to_owned(),
            )),
        }
    }

    /// Cancels the stream, marking sources and sinks closed and cancelled.
    ///
    /// Cancelling a pipeline cancels its source and sink in turn.
    ///
    /// # Errors
    ///
    /// Returns an error when a sink lock is poisoned or a source cancel fails.
    pub fn cancel(&self) -> Result<()> {
        match &self.inner.kind {
            HandleKind::Source { stream } => stream.cancel(),
            HandleKind::Pipeline { source, sink } => {
                source.cancel()?;
                if let Some(sink) = sink {
                    sink.cancel()?;
                }
                Ok(())
            }
            HandleKind::PcmSink { state, .. } | HandleKind::MidiSink { state, .. } => {
                let mut state = state
                    .lock()
                    .map_err(|_| Error::PoisonedLock("stream sink"))?;
                state.closed = true;
                state.stats.closed = true;
                state.stats.cancelled = true;
                Ok(())
            }
        }
    }

    /// Returns a snapshot of the stream's runtime statistics.
    ///
    /// # Errors
    ///
    /// Returns an error when a sink lock is poisoned or a source cannot report
    /// its statistics.
    pub fn stats(&self) -> Result<StreamStats> {
        match &self.inner.kind {
            HandleKind::Source { stream } => stream.stats(),
            HandleKind::Pipeline { source, .. } => source.stats(),
            HandleKind::PcmSink { state, .. } | HandleKind::MidiSink { state, .. } => state
                .lock()
                .map_err(|_| Error::PoisonedLock("stream sink"))
                .map(|state| state.stats.clone()),
        }
    }

    /// Returns `true` when the stream has no more work to do.
    ///
    /// A pipeline is done only when both its source and its sink are done.
    ///
    /// # Errors
    ///
    /// Returns an error when a sink lock is poisoned or a source cannot report
    /// its completion state.
    pub fn done(&self) -> Result<bool> {
        match &self.inner.kind {
            HandleKind::Source { stream } => stream.is_done(),
            HandleKind::Pipeline { source, sink } => {
                let source_done = source.done()?;
                let sink_done = match sink {
                    Some(sink) => sink.done()?,
                    None => true,
                };
                Ok(source_done && sink_done)
            }
            HandleKind::PcmSink { state, .. } | HandleKind::MidiSink { state, .. } => state
                .lock()
                .map_err(|_| Error::PoisonedLock("stream sink"))
                .map(|state| state.closed),
        }
    }

    /// Returns a copy of the packets accumulated by a sink handle.
    ///
    /// # Errors
    ///
    /// Returns an error when called on a non-sink handle or when the sink lock
    /// is poisoned.
    pub fn sink_packets(&self) -> Result<Vec<StreamPacket>> {
        match &self.inner.kind {
            HandleKind::PcmSink { state, .. } | HandleKind::MidiSink { state, .. } => state
                .lock()
                .map_err(|_| Error::PoisonedLock("stream sink"))
                .map(|state| state.packets.clone()),
            _ => Err(Error::Eval(
                "stream/sink-packets expects a sink stream handle".to_owned(),
            )),
        }
    }

    /// Renders this handle as a stream graph expression.
    ///
    /// A pipeline renders as a `stream/pipe` call over its source and optional
    /// sink ids; a bare source or sink renders as its id string.
    pub fn graph_lisp_expr(&self) -> Expr {
        match &self.inner.kind {
            HandleKind::Pipeline { source, sink } => {
                let mut args = vec![source.graph_lisp_atom()];
                if let Some(sink) = sink {
                    args.push(sink.graph_lisp_atom());
                }
                Expr::Call {
                    operator: Box::new(Expr::Symbol(Symbol::qualified("stream", "pipe"))),
                    args,
                }
            }
            HandleKind::Source { .. }
            | HandleKind::PcmSink { .. }
            | HandleKind::MidiSink { .. } => self.graph_lisp_atom(),
        }
    }

    fn graph_lisp_atom(&self) -> Expr {
        Expr::String(self.metadata().id().to_string())
    }

    fn is_source_like(&self) -> bool {
        matches!(
            self.inner.kind,
            HandleKind::Source { .. } | HandleKind::Pipeline { .. }
        )
    }

    fn close_sink(&self) -> Result<()> {
        match &self.inner.kind {
            HandleKind::PcmSink { state, .. } | HandleKind::MidiSink { state, .. } => {
                let mut state = state
                    .lock()
                    .map_err(|_| Error::PoisonedLock("stream sink"))?;
                state.closed = true;
                state.stats.closed = true;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl StageHandle {
    /// Builds the identity pass-through stage handle.
    pub fn identity() -> Self {
        Self {
            kind: StageKind::Identity,
        }
    }

    /// Returns `true` when this stage is the identity pass-through stage.
    pub fn is_identity(&self) -> bool {
        matches!(self.kind, StageKind::Identity)
    }
}

impl Object for StreamHandle {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok(format!("#<stream-handle {}>", self.metadata().id()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for StreamHandle {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        cx.factory()
            .class_stub(ClassId(0), Symbol::qualified("stream", "Handle"))
    }
}

impl Object for StageHandle {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok("#<stream-stage identity>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for StageHandle {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        cx.factory()
            .class_stub(CORE_FUNCTION_CLASS_ID, Symbol::qualified("stream", "Stage"))
    }
}

fn run_pipeline(source: &StreamHandle, sink: Option<&StreamHandle>) -> Result<RunReport> {
    let mut report = RunReport::default();
    while let Some(item) = source.next_packet()? {
        report.packets += 1;
        if let Some(sink) = sink {
            sink.write_packet(item.packet().clone())?;
            report.written += 1;
        }
    }
    if let Some(sink) = sink {
        sink.close_sink()?;
    }
    Ok(report)
}

fn write_sink_packet(state: &Mutex<SinkState>, packet: StreamPacket) -> Result<()> {
    let mut state = state
        .lock()
        .map_err(|_| Error::PoisonedLock("stream sink"))?;
    if state.closed {
        return Err(Error::Eval(
            "cannot write to a closed stream sink".to_owned(),
        ));
    }
    state.stats.pushed += 1;
    state.stats.accepted += 1;
    state.packets.push(packet);
    Ok(())
}

fn ensure_midi_tpq(expected: u16, packet: &MidiPacket) -> Result<()> {
    if packet.tpq() != expected {
        return Err(Error::Eval(format!(
            "MIDI packet TPQ {} does not match sink TPQ {expected}",
            packet.tpq()
        )));
    }
    Ok(())
}

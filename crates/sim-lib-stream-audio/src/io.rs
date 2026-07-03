use std::collections::VecDeque;

use sim_kernel::{Error, Result};

use crate::{PcmBuffer, PcmSpec};

/// Pull source of PCM audio buffers in a fixed [`PcmSpec`].
///
/// Implementors yield [`PcmBuffer`]s until exhausted. Every buffer a source
/// produces is expected to match the source's [`spec`](PcmSource::spec).
pub trait PcmSource {
    /// Returns the audio format every buffer from this source uses.
    fn spec(&self) -> &PcmSpec;

    /// Reads the next buffer, or `Ok(None)` once the source is drained.
    fn read_buffer(&mut self) -> Result<Option<PcmBuffer>>;
}

/// Push sink for PCM audio buffers in a fixed [`PcmSpec`].
///
/// Implementors accept [`PcmBuffer`]s that match their
/// [`spec`](PcmSink::spec) and signal end-of-stream through
/// [`flush`](PcmSink::flush).
pub trait PcmSink {
    /// Returns the audio format this sink accepts.
    fn spec(&self) -> &PcmSpec;

    /// Writes one buffer to the sink.
    ///
    /// Returns an error when the buffer's spec does not match the sink's spec.
    fn write_buffer(&mut self, buffer: PcmBuffer) -> Result<()>;

    /// Flushes any pending output, marking a stream boundary.
    fn flush(&mut self) -> Result<()>;
}

/// In-memory [`PcmSource`] that replays a fixed queue of buffers.
///
/// This is the deterministic test backend for the source side of the PCM
/// fabric: it hands out the buffers it was constructed with, in order, then
/// reports end-of-stream.
///
/// # Examples
///
/// ```
/// use sim_lib_stream_audio::{MemoryPcmSource, PcmBuffer, PcmSource, PcmSpec};
///
/// let spec = PcmSpec::i16(1, 48_000)?;
/// let buffer = PcmBuffer::i16(spec, 2, vec![1, 2])?;
/// let mut source = MemoryPcmSource::new(spec, vec![buffer])?;
/// assert_eq!(source.remaining(), 1);
/// assert!(source.read_buffer()?.is_some());
/// assert!(source.read_buffer()?.is_none());
/// # Ok::<(), sim_kernel::Error>(())
/// ```
#[derive(Clone, Debug)]
pub struct MemoryPcmSource {
    spec: PcmSpec,
    buffers: VecDeque<PcmBuffer>,
}

impl MemoryPcmSource {
    /// Builds a source that will replay `buffers` in order.
    ///
    /// Returns an error if any buffer's spec does not match `spec`.
    pub fn new(spec: PcmSpec, buffers: Vec<PcmBuffer>) -> Result<Self> {
        for buffer in &buffers {
            ensure_spec("source", spec, buffer.spec())?;
        }
        Ok(Self {
            spec,
            buffers: buffers.into(),
        })
    }

    /// Returns the number of buffers not yet read.
    pub fn remaining(&self) -> usize {
        self.buffers.len()
    }
}

impl PcmSource for MemoryPcmSource {
    fn spec(&self) -> &PcmSpec {
        &self.spec
    }

    fn read_buffer(&mut self) -> Result<Option<PcmBuffer>> {
        Ok(self.buffers.pop_front())
    }
}

/// In-memory [`PcmSink`] that records every written buffer and flush.
///
/// This is the deterministic test backend for the sink side of the PCM fabric:
/// written buffers accumulate in order and can be inspected through
/// [`buffers`](MemoryPcmSink::buffers) or taken with
/// [`into_buffers`](MemoryPcmSink::into_buffers), while flushes are counted.
///
/// # Examples
///
/// ```
/// use sim_lib_stream_audio::{MemoryPcmSink, PcmBuffer, PcmSink, PcmSpec};
///
/// let spec = PcmSpec::i16(1, 48_000)?;
/// let mut sink = MemoryPcmSink::new(spec);
/// sink.write_buffer(PcmBuffer::i16(spec, 1, vec![7])?)?;
/// sink.flush()?;
/// assert_eq!(sink.buffers().len(), 1);
/// assert_eq!(sink.flush_count(), 1);
/// # Ok::<(), sim_kernel::Error>(())
/// ```
#[derive(Clone, Debug)]
pub struct MemoryPcmSink {
    spec: PcmSpec,
    buffers: Vec<PcmBuffer>,
    flush_count: usize,
}

impl MemoryPcmSink {
    /// Builds an empty sink that accepts buffers matching `spec`.
    pub fn new(spec: PcmSpec) -> Self {
        Self {
            spec,
            buffers: Vec::new(),
            flush_count: 0,
        }
    }

    /// Returns the buffers written so far, in write order.
    pub fn buffers(&self) -> &[PcmBuffer] {
        &self.buffers
    }

    /// Returns how many times [`flush`](PcmSink::flush) has been called.
    pub fn flush_count(&self) -> usize {
        self.flush_count
    }

    /// Consumes the sink and returns the recorded buffers.
    pub fn into_buffers(self) -> Vec<PcmBuffer> {
        self.buffers
    }
}

impl PcmSink for MemoryPcmSink {
    fn spec(&self) -> &PcmSpec {
        &self.spec
    }

    fn write_buffer(&mut self, buffer: PcmBuffer) -> Result<()> {
        ensure_spec("sink", self.spec, buffer.spec())?;
        self.buffers.push(buffer);
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.flush_count += 1;
        Ok(())
    }
}

/// Tally of how much audio moved through a pump or stream-adapter run.
///
/// Returned by [`pump_pcm`] and the spine adapters, it records the number of
/// buffers transferred and the total frame count across them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PcmPumpSummary {
    buffers: usize,
    frames: usize,
}

impl PcmPumpSummary {
    /// Returns the number of buffers transferred.
    pub fn buffers(self) -> usize {
        self.buffers
    }

    /// Returns the total number of frames across all transferred buffers.
    pub fn frames(self) -> usize {
        self.frames
    }

    pub(crate) fn record(&mut self, buffer: &PcmBuffer) {
        self.buffers += 1;
        self.frames += buffer.frames();
    }
}

/// Drains `source` into `sink`, then flushes the sink.
///
/// Reads every buffer from `source` and writes it to `sink`, returning a
/// [`PcmPumpSummary`] of the transfer. Returns an error when the source and
/// sink specs differ or when any write fails.
pub fn pump_pcm(source: &mut impl PcmSource, sink: &mut impl PcmSink) -> Result<PcmPumpSummary> {
    ensure_spec("sink", *source.spec(), *sink.spec())?;
    let mut summary = PcmPumpSummary::default();
    while let Some(buffer) = source.read_buffer()? {
        summary.record(&buffer);
        sink.write_buffer(buffer)?;
    }
    sink.flush()?;
    Ok(summary)
}

pub(crate) fn ensure_spec(role: &str, expected: PcmSpec, actual: PcmSpec) -> Result<()> {
    if expected != actual {
        return Err(Error::Eval(format!(
            "PCM {role} spec mismatch: expected {:?}, got {:?}",
            expected, actual
        )));
    }
    Ok(())
}

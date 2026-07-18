use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use sim_kernel::{Error, Result, Symbol};
use sim_lib_stream_core::{
    ClockDomain, ClockTickIndex, DomainBridgeDescriptor, PcmPacket, StreamItem, StreamMetadata,
    StreamPacket, tick_clock_index,
};

use crate::{Stream, StreamNode};

/// Resamples a PCM stream from `input_hz` to `output_hz`.
///
/// Each PCM packet is rate-converted by nearest-source-frame interleaving;
/// non-PCM packets pass through unchanged. Errors if either rate is zero.
pub fn resample_pcm(source: Stream, input_hz: u32, output_hz: u32) -> Result<Stream> {
    let descriptor = DomainBridgeDescriptor::resampler(input_hz, output_hz)?;
    let metadata = source.metadata().clone();
    Ok(Stream::new(ResamplePcmNode {
        source,
        metadata,
        input_hz,
        output_hz,
        _descriptor: descriptor,
    }))
}

/// Reorders packets by `clock` tick within a bounded latency window.
///
/// The buffer keeps an online reordering window of `max_late_packets + 1`
/// packets: it pulls only enough of `source` to fill that window (never draining
/// a live source to its end), then emits the lowest-tick packet, breaking ties
/// by arrival order so equal ticks stay stable. A packet whose tick falls below
/// the last emitted tick has arrived more than `max_late_packets` positions
/// behind the highest accepted tick; it is dropped and counted rather than
/// reordered. With `max_late_packets` of `0` the window is a single packet, so
/// any out-of-order packet is dropped.
pub fn jitter_buffer(source: Stream, clock: Symbol, max_late_packets: u32) -> Stream {
    jitter_buffer_with_drops(source, clock, max_late_packets).0
}

/// Builds a jitter buffer alongside a shared counter of late-dropped packets.
///
/// The public [`jitter_buffer`] wraps this and discards the counter; tests read
/// the counter to assert the positive-lateness bound.
pub(crate) fn jitter_buffer_with_drops(
    source: Stream,
    clock: Symbol,
    max_late_packets: u32,
) -> (Stream, Arc<AtomicUsize>) {
    let descriptor = DomainBridgeDescriptor::jitter_buffer(max_late_packets);
    let metadata = source.metadata().clone();
    let late_dropped = Arc::new(AtomicUsize::new(0));
    let stream = Stream::new(JitterBufferNode {
        source,
        metadata,
        clock,
        max_late_packets,
        state: Mutex::new(JitterBufferState::default()),
        late_dropped: Arc::clone(&late_dropped),
        _descriptor: descriptor,
    });
    (stream, late_dropped)
}

/// Records a `frames`-frame latency-compensation delay over the stream.
///
/// The packets pass through untouched; the descriptor carries the declared
/// delay so downstream clock alignment can account for it.
pub fn latency_comp_delay(source: Stream, frames: u64) -> Stream {
    let descriptor = DomainBridgeDescriptor::latency_comp_delay(frames);
    let metadata = source.metadata().clone();
    Stream::new(PassthroughBridgeNode {
        source,
        metadata,
        _descriptor: descriptor,
    })
}

/// Bridges an event stream into the control clock domain as a rate gate.
///
/// The source clock domain is read from its metadata and validated into a gate
/// descriptor; packets pass through unchanged.
pub fn event_rate_gate(source: Stream) -> Result<Stream> {
    let input_domain = ClockDomain::for_stream_clock(source.metadata().clock())?;
    let descriptor = DomainBridgeDescriptor::event_rate_gate(input_domain)?;
    let metadata = source.metadata().clone();
    Ok(Stream::new(PassthroughBridgeNode {
        source,
        metadata,
        _descriptor: descriptor,
    }))
}

struct ResamplePcmNode {
    source: Stream,
    metadata: StreamMetadata,
    input_hz: u32,
    output_hz: u32,
    _descriptor: DomainBridgeDescriptor,
}

impl StreamNode for ResamplePcmNode {
    fn metadata(&self) -> &StreamMetadata {
        &self.metadata
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        let Some(item) = self.source.next_packet()? else {
            return Ok(None);
        };
        let StreamPacket::Pcm(packet) = item.packet() else {
            return Ok(Some(item));
        };
        let packet = resample_packet(packet, self.input_hz, self.output_hz)?;
        StreamItem::with_ticks(StreamPacket::Pcm(packet), item.ticks().to_vec()).map(Some)
    }

    fn is_done(&self) -> Result<bool> {
        self.source.is_done()
    }
}

struct JitterBufferNode {
    source: Stream,
    metadata: StreamMetadata,
    clock: Symbol,
    max_late_packets: u32,
    state: Mutex<JitterBufferState>,
    late_dropped: Arc<AtomicUsize>,
    _descriptor: DomainBridgeDescriptor,
}

/// Online reordering window shared behind the node's mutex.
#[derive(Default)]
struct JitterBufferState {
    /// Buffered packets awaiting emission, each tagged with its arrival ordinal.
    window: Vec<(usize, StreamItem)>,
    /// Monotonic arrival counter; breaks ties in tick order stably.
    next_ordinal: usize,
    /// Highest emitted tick; a lower newly accepted tick is late.
    last_emitted: Option<ClockTickIndex>,
    /// Whether the upstream source has reached its terminal `done`.
    source_done: bool,
}

impl StreamNode for JitterBufferNode {
    fn metadata(&self) -> &StreamMetadata {
        &self.metadata
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("jitter-buffer state"))?;
        self.fill_window(&mut state)?;
        let target = self.max_late_packets as usize + 1;
        if state.window.len() < target && !state.source_done {
            // Live source without enough ordering context yet: emit nothing.
            return Ok(None);
        }
        self.pop_next(&mut state)
    }

    fn is_done(&self) -> Result<bool> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("jitter-buffer state"))?;
        Ok(state.window.is_empty() && (state.source_done || self.source.is_done()?))
    }
}

impl JitterBufferNode {
    /// Pulls upstream packets until the ordering window is full or the source
    /// signals no packet is currently available. Never drains to end of source.
    fn fill_window(&self, state: &mut JitterBufferState) -> Result<()> {
        let target = self.max_late_packets as usize + 1;
        while !state.source_done && state.window.len() < target {
            match self.source.next_packet()? {
                Some(item) => self.accept_or_drop(state, item)?,
                None => {
                    if self.source.is_done()? {
                        state.source_done = true;
                    }
                    break;
                }
            }
        }
        Ok(())
    }

    /// Buffers `item`, or drops it (and counts it) when it is more than
    /// `max_late_packets` behind the highest accepted tick.
    fn accept_or_drop(&self, state: &mut JitterBufferState, item: StreamItem) -> Result<()> {
        let key = tick_key(&item, &self.clock)?;
        let late = match (&key, &state.last_emitted) {
            (Some(key), Some(last)) => key < last,
            _ => false,
        };
        if late {
            self.late_dropped.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        let ordinal = state.next_ordinal;
        state.next_ordinal = state.next_ordinal.saturating_add(1);
        state.window.push((ordinal, item));
        Ok(())
    }

    /// Removes and returns the lowest-tick buffered packet, advancing the
    /// highest-emitted marker. Ties are broken by arrival order.
    fn pop_next(&self, state: &mut JitterBufferState) -> Result<Option<StreamItem>> {
        if state.window.is_empty() {
            return Ok(None);
        }
        let mut best = 0usize;
        for index in 1..state.window.len() {
            if self.precedes(&state.window[index], &state.window[best])? {
                best = index;
            }
        }
        let (_, item) = state.window.remove(best);
        if let Some(key) = tick_key(&item, &self.clock)? {
            state.last_emitted = Some(key);
        }
        Ok(Some(item))
    }

    /// Reports whether `left` should be emitted before `right`: lower tick
    /// first, ties (and keyless packets) by arrival order.
    fn precedes(&self, left: &(usize, StreamItem), right: &(usize, StreamItem)) -> Result<bool> {
        Ok(
            match (
                tick_key(&left.1, &self.clock)?,
                tick_key(&right.1, &self.clock)?,
            ) {
                (Some(left_key), Some(right_key)) => (left_key, left.0) < (right_key, right.0),
                _ => left.0 < right.0,
            },
        )
    }
}

struct PassthroughBridgeNode {
    source: Stream,
    metadata: StreamMetadata,
    _descriptor: DomainBridgeDescriptor,
}

impl StreamNode for PassthroughBridgeNode {
    fn metadata(&self) -> &StreamMetadata {
        &self.metadata
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        self.source.next_packet()
    }

    fn is_done(&self) -> Result<bool> {
        self.source.is_done()
    }
}

fn resample_packet(packet: &PcmPacket, input_hz: u32, output_hz: u32) -> Result<PcmPacket> {
    if input_hz == 0 || output_hz == 0 {
        return Err(Error::Eval("PCM resample rates must be nonzero".to_owned()));
    }
    let output_frames = resampled_frame_count(packet.frames(), input_hz, output_hz);
    match packet.sample_format() {
        sim_lib_stream_core::PcmSampleFormat::I16 => PcmPacket::i16(
            packet.channels(),
            output_frames,
            resample_interleaved(
                packet.samples_i16(),
                packet.channels(),
                output_frames,
                |v| v,
            ),
        ),
        sim_lib_stream_core::PcmSampleFormat::F32 => PcmPacket::f32(
            packet.channels(),
            output_frames,
            resample_interleaved(
                packet.samples_f32(),
                packet.channels(),
                output_frames,
                |v| v,
            ),
        ),
    }
}

fn resampled_frame_count(input_frames: usize, input_hz: u32, output_hz: u32) -> usize {
    let frames = (input_frames as u64)
        .saturating_mul(u64::from(output_hz))
        .saturating_add(u64::from(input_hz / 2))
        / u64::from(input_hz);
    frames.max(1) as usize
}

fn resample_interleaved<T: Copy>(
    samples: &[T],
    channels: usize,
    output_frames: usize,
    copy: impl Fn(T) -> T,
) -> Vec<T> {
    let input_frames = samples.len() / channels;
    let mut out = Vec::with_capacity(output_frames * channels);
    for frame in 0..output_frames {
        let source_frame = frame.saturating_mul(input_frames) / output_frames;
        let source_frame = source_frame.min(input_frames.saturating_sub(1));
        for channel in 0..channels {
            out.push(copy(samples[source_frame * channels + channel]));
        }
    }
    out
}

fn tick_key(item: &StreamItem, clock: &Symbol) -> Result<Option<ClockTickIndex>> {
    item.ticks().iter().try_fold(None, |found, tick| {
        tick_clock_index(tick, clock).map(|parsed| found.or(parsed))
    })
}

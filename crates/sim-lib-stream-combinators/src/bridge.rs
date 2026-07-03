use std::{collections::VecDeque, sync::Mutex};

use sim_kernel::{Error, Ref, Result, Symbol};
use sim_lib_stream_core::{
    ClockDomain, DomainBridgeDescriptor, PcmPacket, StreamItem, StreamMetadata, StreamPacket,
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

/// Reorders packets by `clock` tick, tolerating up to `max_late_packets`.
///
/// The buffer drains the source, sorts packets by their tick index on `clock`
/// (stable on ties), and replays them in order. With `max_late_packets` of `0`
/// any out-of-order packet is dropped rather than reordered.
pub fn jitter_buffer(source: Stream, clock: Symbol, max_late_packets: u32) -> Stream {
    let descriptor = DomainBridgeDescriptor::jitter_buffer(max_late_packets);
    let metadata = source.metadata().clone();
    Stream::new(JitterBufferNode {
        source,
        metadata,
        clock,
        max_late_packets,
        queue: Mutex::new(None),
        _descriptor: descriptor,
    })
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
/// The source clock domain is read from its metadata (defaulting to
/// [`ClockDomain::Control`](sim_lib_stream_core::ClockDomain) when unknown) and
/// validated into a gate descriptor; packets pass through unchanged.
pub fn event_rate_gate(source: Stream) -> Result<Stream> {
    let input_domain =
        ClockDomain::from_symbol(source.metadata().clock()).unwrap_or(ClockDomain::Control);
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
    queue: Mutex<Option<VecDeque<StreamItem>>>,
    _descriptor: DomainBridgeDescriptor,
}

impl StreamNode for JitterBufferNode {
    fn metadata(&self) -> &StreamMetadata {
        &self.metadata
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        let mut queue = self
            .queue
            .lock()
            .map_err(|_| Error::PoisonedLock("jitter-buffer queue"))?;
        if queue.is_none() {
            *queue = Some(VecDeque::from(load_jitter_buffer(
                &self.source,
                &self.clock,
                self.max_late_packets,
            )?));
        }
        Ok(queue.as_mut().and_then(VecDeque::pop_front))
    }

    fn is_done(&self) -> Result<bool> {
        let queue = self
            .queue
            .lock()
            .map_err(|_| Error::PoisonedLock("jitter-buffer queue"))?;
        Ok(queue.as_ref().is_some_and(VecDeque::is_empty) || self.source.is_done()?)
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

fn load_jitter_buffer(
    source: &Stream,
    clock: &Symbol,
    max_late_packets: u32,
) -> Result<Vec<StreamItem>> {
    let mut highest_key = None;
    let mut indexed = Vec::new();
    let mut ordinal = 0usize;
    while let Some(item) = source.next_packet()? {
        let key = tick_key(&item, clock);
        let late = highest_key
            .as_ref()
            .zip(key.as_ref())
            .is_some_and(|(highest, key)| key < highest);
        if late && max_late_packets == 0 {
            continue;
        }
        if key
            .as_ref()
            .zip(highest_key.as_ref())
            .is_some_and(|(key, highest)| key > highest)
            || highest_key.is_none()
        {
            highest_key = key.clone();
        }
        indexed.push((ordinal, item));
        ordinal = ordinal.saturating_add(1);
    }
    indexed.sort_by(|(left_index, left), (right_index, right)| {
        match (tick_key(left, clock), tick_key(right, clock)) {
            (Some(left), Some(right)) => left.cmp(&right).then(left_index.cmp(right_index)),
            _ => left_index.cmp(right_index),
        }
    });
    Ok(indexed.into_iter().map(|(_, item)| item).collect())
}

fn tick_key(item: &StreamItem, clock: &Symbol) -> Option<Ref> {
    item.ticks()
        .iter()
        .find(|tick| &tick.clock == clock)
        .map(|tick| tick.index.clone())
}

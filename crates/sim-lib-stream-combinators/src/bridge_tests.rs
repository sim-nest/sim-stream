use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use sim_kernel::{ContentId, Ref, Result, Symbol, Tick};
use sim_lib_stream_core::{
    BufferPolicy, ClockDomain, PcmPacket, StreamDirection, StreamItem, StreamMedia, StreamMetadata,
    StreamPacket,
};

use crate::bridge::jitter_buffer_with_drops;
use crate::{Stream, StreamNode, event_rate_gate, jitter_buffer, latency_comp_delay, resample_pcm};

#[test]
fn resampler_bridges_sample_rates_with_nearest_frames() {
    let stream = Stream::pull(
        metadata("pcm", StreamMedia::Pcm),
        vec![StreamItem::new(StreamPacket::Pcm(
            PcmPacket::f32(1, 2, vec![0.25, 0.75]).unwrap(),
        ))],
    );

    let out = resample_pcm(stream, 48_000, 96_000)
        .unwrap()
        .take_packets(1)
        .unwrap();

    let StreamPacket::Pcm(packet) = out[0].packet() else {
        panic!("expected PCM packet");
    };
    assert_eq!(packet.frames(), 4);
    assert_eq!(packet.samples_f32(), &[0.25, 0.25, 0.75, 0.75]);
}

#[test]
fn jitter_buffer_reorders_packets_and_can_drop_late_packets() {
    let reordered = Stream::pull(
        metadata("diag", StreamMedia::Diagnostic),
        vec![packet("two", 2), packet("one", 1)],
    );
    let out = jitter_buffer(reordered, clock(), 1)
        .take_packets(4)
        .unwrap();
    assert_eq!(messages(out), vec!["one", "two"]);

    let late = Stream::pull(
        metadata("diag-late", StreamMedia::Diagnostic),
        vec![packet("two", 2), packet("one", 1)],
    );
    let out = jitter_buffer(late, clock(), 0).take_packets(4).unwrap();
    assert_eq!(messages(out), vec!["two"]);
}

#[test]
fn jitter_buffer_pulls_only_the_ordering_window_from_a_live_source() {
    let pulls = Arc::new(AtomicUsize::new(0));
    let source = Stream::new(InfiniteSource {
        metadata: metadata("live", StreamMedia::Diagnostic),
        next_tick: Mutex::new(0),
        pulls: Arc::clone(&pulls),
    });

    let buffer = jitter_buffer(source, clock(), 2);
    let first = buffer.next_packet().unwrap();

    assert!(first.is_some());
    // A window of max_late_packets + 1 = 3 packets is pulled, never the whole
    // (infinite) source.
    assert_eq!(pulls.load(Ordering::SeqCst), 3);
    assert!(!buffer.is_done().unwrap());
}

#[test]
fn jitter_buffer_drops_packets_beyond_the_positive_lateness_bound() {
    let source = Stream::pull(
        metadata("late-bound", StreamMedia::Diagnostic),
        vec![packet("three", 3), packet("four", 4), packet("one", 1)],
    );

    let (buffer, dropped) = jitter_buffer_with_drops(source, clock(), 1);
    let out = buffer.take_packets(8).unwrap();

    // `one` arrives after `three` was emitted -- more than one behind the
    // highest accepted tick -- so it is dropped and counted.
    assert_eq!(messages(out), vec!["three", "four"]);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}

#[test]
fn jitter_buffer_keeps_equal_ticks_in_arrival_order() {
    let source = Stream::pull(
        metadata("ties", StreamMedia::Diagnostic),
        vec![packet("a", 5), packet("b", 5), packet("c", 5)],
    );

    let out = jitter_buffer(source, clock(), 2).take_packets(8).unwrap();

    assert_eq!(messages(out), vec!["a", "b", "c"]);
}

#[test]
fn jitter_buffer_waits_for_ordering_context_on_a_live_source() {
    let source = Stream::new(PartialLiveSource {
        metadata: metadata("partial", StreamMedia::Diagnostic),
        items: Mutex::new(VecDeque::from(vec![packet("only", 1)])),
    });

    // With a window of two the single available packet is not enough context,
    // so the buffer emits nothing yet and is not done (the source is live).
    let buffer = jitter_buffer(source, clock(), 1);
    assert!(buffer.next_packet().unwrap().is_none());
    assert!(!buffer.is_done().unwrap());
}

#[test]
fn deterministic_passthrough_bridges_preserve_packet_order() {
    let stream = Stream::pull(
        metadata_with_clock(
            "diag",
            StreamMedia::Diagnostic,
            ClockDomain::MidiTick.symbol(),
        ),
        vec![packet("one", 1), packet("two", 2)],
    );
    let stream = event_rate_gate(latency_comp_delay(stream, 64)).unwrap();
    let out = stream.take_packets(4).unwrap();

    assert_eq!(messages(out), vec!["one", "two"]);
}

fn metadata(name: &str, media: StreamMedia) -> StreamMetadata {
    metadata_with_clock(name, media, clock())
}

fn metadata_with_clock(name: &str, media: StreamMedia, clock: Symbol) -> StreamMetadata {
    StreamMetadata::new(
        Symbol::qualified("stream/test", name),
        media,
        StreamDirection::Source,
        clock,
        BufferPolicy::bounded(8).unwrap(),
    )
}

fn packet(message: &str, tick: u8) -> StreamItem {
    StreamItem::with_ticks(
        StreamPacket::Diagnostic(sim_lib_stream_core::StreamDiagnostic::new(
            Symbol::qualified("stream/test", "packet"),
            message,
        )),
        vec![Tick::new(
            clock(),
            Ref::Content(ContentId::from_bytes(
                Symbol::qualified("core", "sha256"),
                [tick; 32],
            )),
        )],
    )
    .unwrap()
}

fn messages(items: Vec<StreamItem>) -> Vec<String> {
    items
        .iter()
        .map(|item| match item.packet() {
            StreamPacket::Diagnostic(packet) => packet.message().to_owned(),
            _ => panic!("expected diagnostic packet"),
        })
        .collect()
}

fn clock() -> Symbol {
    Symbol::qualified("clock", "test")
}

/// A never-ending source of strictly increasing ticks that counts every pull.
struct InfiniteSource {
    metadata: StreamMetadata,
    next_tick: Mutex<u8>,
    pulls: Arc<AtomicUsize>,
}

impl StreamNode for InfiniteSource {
    fn metadata(&self) -> &StreamMetadata {
        &self.metadata
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        self.pulls.fetch_add(1, Ordering::SeqCst);
        let mut tick = self.next_tick.lock().expect("tick lock");
        let value = *tick;
        *tick = tick.wrapping_add(1);
        Ok(Some(packet("tick", value)))
    }

    fn is_done(&self) -> Result<bool> {
        Ok(false)
    }
}

/// A live source that yields its queued packets then reports "no packet yet"
/// (returning `None`) without ever reaching `done`.
struct PartialLiveSource {
    metadata: StreamMetadata,
    items: Mutex<VecDeque<StreamItem>>,
}

impl StreamNode for PartialLiveSource {
    fn metadata(&self) -> &StreamMetadata {
        &self.metadata
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        Ok(self.items.lock().expect("items lock").pop_front())
    }

    fn is_done(&self) -> Result<bool> {
        Ok(false)
    }
}

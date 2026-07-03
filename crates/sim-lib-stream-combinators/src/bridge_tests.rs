use sim_kernel::{ContentId, Ref, Symbol, Tick};
use sim_lib_stream_core::{
    BufferPolicy, ClockDomain, PcmPacket, StreamDirection, StreamItem, StreamMedia, StreamMetadata,
    StreamPacket,
};

use crate::{Stream, event_rate_gate, jitter_buffer, latency_comp_delay, resample_pcm};

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

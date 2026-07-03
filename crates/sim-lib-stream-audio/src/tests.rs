use sim_kernel::Symbol;
use sim_lib_stream_core::{
    BufferPolicy, StreamDirection, StreamMedia, StreamMetadata, StreamPacket,
};

use crate::{
    MemoryPcmSink, MemoryPcmSource, PcmBuffer, PcmFormatDescriptor, PcmSpec,
    f32_interleaved_to_planar, f32_planar_to_interleaved, f32_sample_to_i16, f32_samples_to_i16,
    i16_interleaved_to_planar, i16_planar_to_interleaved, i16_sample_to_f32, i16_samples_to_f32,
    pcm_source_to_stream, pump_pcm, stream_to_pcm_sink,
};

#[test]
fn empty_source_pumps_zero_buffers_and_flushes_once() {
    let spec = spec();
    let mut source = MemoryPcmSource::new(spec, Vec::new()).unwrap();
    let mut sink = MemoryPcmSink::new(spec);

    let summary = pump_pcm(&mut source, &mut sink).unwrap();

    assert_eq!(summary.buffers(), 0);
    assert_eq!(summary.frames(), 0);
    assert!(sink.buffers().is_empty());
    assert_eq!(sink.flush_count(), 1);
}

#[test]
fn buffer_order_preserved() {
    let spec = spec();
    let first = buffer(&[1, 2, 3, 4]);
    let second = buffer(&[5, 6]);
    let mut source = MemoryPcmSource::new(spec, vec![first.clone(), second.clone()]).unwrap();
    let mut sink = MemoryPcmSink::new(spec);

    let summary = pump_pcm(&mut source, &mut sink).unwrap();

    assert_eq!(summary.buffers(), 2);
    assert_eq!(summary.frames(), 3);
    assert_eq!(sink.buffers(), &[first, second]);
}

#[test]
fn spec_mismatch_rejected() {
    let spec = spec();
    let other_spec = PcmSpec::i16(1, 48_000).unwrap();
    let mut source = MemoryPcmSource::new(spec, vec![buffer(&[1, 2])]).unwrap();
    let mut sink = MemoryPcmSink::new(other_spec);

    let err = pump_pcm(&mut source, &mut sink).unwrap_err();

    assert!(format!("{err}").contains("spec mismatch"));
    assert!(sink.buffers().is_empty());
    assert_eq!(sink.flush_count(), 0);
}

#[test]
fn source_to_spine_yields_ordered_pcm_packets_then_nil() {
    let spec = spec();
    let first = buffer(&[10, 11]);
    let second = buffer(&[12, 13, 14, 15]);
    let mut source = MemoryPcmSource::new(spec, vec![first.clone(), second.clone()]).unwrap();

    let stream = pcm_source_to_stream(&mut source, metadata()).unwrap();

    assert_eq!(source.remaining(), 0);
    assert_pcm_packet(stream.next_packet().unwrap(), &first);
    assert_pcm_packet(stream.next_packet().unwrap(), &second);
    assert!(stream.next_packet().unwrap().is_none());
    assert!(stream.is_done().unwrap());
}

#[test]
fn stream_to_sink_writes_ordered_buffers_and_flushes_once() {
    let first = buffer(&[21, 22]);
    let second = buffer(&[23, 24]);
    let stream = sim_lib_stream_core::StreamValue::pull(
        metadata(),
        vec![
            sim_lib_stream_core::StreamItem::new(StreamPacket::Pcm(first.to_packet().unwrap())),
            sim_lib_stream_core::StreamItem::new(StreamPacket::Pcm(second.to_packet().unwrap())),
        ],
    );
    let mut sink = MemoryPcmSink::new(spec());

    let summary = stream_to_pcm_sink(&stream, &mut sink).unwrap();

    assert_eq!(summary.buffers(), 2);
    assert_eq!(sink.buffers(), &[first, second]);
    assert_eq!(sink.flush_count(), 1);
}

#[test]
fn f32_buffers_round_trip_through_pcm_packets() {
    let spec = f32_spec();
    let buffer = PcmBuffer::f32(spec, 2, vec![0.0, -0.5, 1.0, -1.0]).unwrap();

    let packet = buffer.to_packet().unwrap();
    let decoded = PcmBuffer::from_packet(spec, &packet).unwrap();

    assert_eq!(decoded, buffer);
    assert_eq!(decoded.samples_f32(), &[0.0, -0.5, 1.0, -1.0]);
}

#[test]
fn citizen_pcm_format_descriptor_round_trips_and_fails_closed() {
    let descriptor = PcmFormatDescriptor::new(PcmSpec::f32(2, 96_000).unwrap());
    let spec = descriptor.spec().unwrap();
    assert_eq!(spec.channels(), 2);
    assert_eq!(spec.sample_rate_hz(), 96_000);

    let mut expr = descriptor.as_expr().clone();
    let sim_kernel::Expr::Map(entries) = &mut expr else {
        panic!("PCM descriptor should encode as a map");
    };
    for (key, value) in entries {
        if key == &sim_kernel::Expr::Symbol(Symbol::qualified("stream-audio", "channels")) {
            *value = sim_kernel::Expr::String("0".to_owned());
        }
    }
    let err = PcmFormatDescriptor::from_expr(expr).unwrap_err();
    assert!(format!("{err}").contains("greater than zero"));
}

#[test]
fn f32_stream_to_sink_writes_ordered_buffers() {
    let spec = f32_spec();
    let first = PcmBuffer::f32(spec, 1, vec![0.0, 0.25]).unwrap();
    let second = PcmBuffer::f32(spec, 1, vec![0.5, -0.5]).unwrap();
    let stream = sim_lib_stream_core::StreamValue::pull(
        metadata(),
        vec![
            sim_lib_stream_core::StreamItem::new(StreamPacket::Pcm(first.to_packet().unwrap())),
            sim_lib_stream_core::StreamItem::new(StreamPacket::Pcm(second.to_packet().unwrap())),
        ],
    );
    let mut sink = MemoryPcmSink::new(spec);

    let summary = stream_to_pcm_sink(&stream, &mut sink).unwrap();

    assert_eq!(summary.buffers(), 2);
    assert_eq!(sink.buffers(), &[first, second]);
    assert_eq!(sink.flush_count(), 1);
}

#[test]
fn sample_conversion_helpers_cover_i16_and_f32_boundaries() {
    assert_eq!(i16_sample_to_f32(i16::MIN), -1.0);
    assert_eq!(i16_sample_to_f32(0), 0.0);
    assert_eq!(i16_sample_to_f32(i16::MAX), 1.0);

    assert_eq!(f32_sample_to_i16(-1.0).unwrap(), i16::MIN);
    assert_eq!(f32_sample_to_i16(0.0).unwrap(), 0);
    assert_eq!(f32_sample_to_i16(1.0).unwrap(), i16::MAX);
    assert_eq!(f32_sample_to_i16(2.0).unwrap(), i16::MAX);
    assert!(f32_sample_to_i16(f32::NAN).is_err());

    assert_eq!(
        i16_samples_to_f32(&[i16::MIN, 0, i16::MAX]),
        vec![-1.0, 0.0, 1.0]
    );
    assert_eq!(
        f32_samples_to_i16(&[-1.0, 0.0, 1.0]).unwrap(),
        vec![i16::MIN, 0, i16::MAX]
    );
}

#[test]
fn planar_and_interleaved_helpers_round_trip() {
    let i16_interleaved = vec![1, 2, 3, 4, 5, 6];
    let i16_planar = i16_interleaved_to_planar(&i16_interleaved, 2).unwrap();
    assert_eq!(i16_planar, vec![vec![1, 3, 5], vec![2, 4, 6]]);
    assert_eq!(
        i16_planar_to_interleaved(&i16_planar).unwrap(),
        i16_interleaved
    );

    let f32_interleaved = vec![0.0, 0.25, 0.5, 0.75];
    let f32_planar = f32_interleaved_to_planar(&f32_interleaved, 2).unwrap();
    assert_eq!(f32_planar, vec![vec![0.0, 0.5], vec![0.25, 0.75]]);
    assert_eq!(
        f32_planar_to_interleaved(&f32_planar).unwrap(),
        f32_interleaved
    );

    assert!(i16_interleaved_to_planar(&[1, 2, 3], 2).is_err());
    assert!(f32_planar_to_interleaved(&[vec![0.0], vec![f32::INFINITY]]).is_err());
}

fn spec() -> PcmSpec {
    PcmSpec::i16(2, 48_000).unwrap()
}

fn f32_spec() -> PcmSpec {
    PcmSpec::f32(2, 48_000).unwrap()
}

fn buffer(samples: &[i16]) -> PcmBuffer {
    PcmBuffer::i16(spec(), samples.len() / 2, samples.to_vec()).unwrap()
}

fn metadata() -> StreamMetadata {
    StreamMetadata::new(
        Symbol::qualified("stream", "pcm-memory"),
        StreamMedia::Pcm,
        StreamDirection::Source,
        Symbol::qualified("clock", "sample"),
        BufferPolicy::bounded(4).unwrap(),
    )
}

fn assert_pcm_packet(item: Option<sim_lib_stream_core::StreamItem>, expected: &PcmBuffer) {
    let item = item.expect("expected a PCM stream item");
    let StreamPacket::Pcm(packet) = item.packet() else {
        panic!("expected a PCM packet");
    };
    assert_eq!(packet.channels(), expected.spec().channels());
    assert_eq!(packet.frames(), expected.frames());
    assert_eq!(packet.samples_i16(), expected.samples_i16());
}

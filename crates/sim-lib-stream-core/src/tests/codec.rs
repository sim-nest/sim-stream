use sim_codec::{Input, Output, decode_with_codec, encode_with_codec};
use sim_codec_binary::BinaryCodecLib;
use sim_codec_binary_base64::BinaryBase64CodecLib;
use sim_codec_json::JsonCodecLib;
use sim_codec_lisp::{LispCodecLib, encode_object_lisp};
use sim_kernel::{
    CapabilitySet, CodecId, Cx, EncodeOptions, EncodePosition, Expr, ReadPolicy, Symbol,
    TrustLevel, WriteCx, read_construct_capability,
};

use crate::{
    BackpressureOutcome, BufferPolicy, ClockDomain, LatencyClass, MidiPacket, MidiPacketEvent,
    PcmPacket, StreamCapability, StreamDiagnostic, StreamDirection, StreamEnvelope, StreamItem,
    StreamMedia, StreamMetadata, StreamPacket, install_stream_core_classes,
    install_stream_core_shapes_lib, stream_backpressure_shape_symbol,
    stream_buffer_policy_shape_symbol, stream_capability_shape_symbol,
    stream_clock_domain_shape_symbol, stream_clock_shape_symbol, stream_data_packet_shape_symbol,
    stream_diagnostic_shape_symbol, stream_envelope_shape_symbol,
    stream_latency_class_shape_symbol, stream_media_shape_symbol, stream_metadata_class_symbol,
    stream_metadata_shape_symbol, stream_packet_shape_symbol, stream_tempo_shape_symbol,
};

#[test]
fn stream_shapes_register_metadata_media_clock_backpressure_buffer_packet_data_and_diagnostic() {
    let mut cx = super::cx();
    install_stream_core_shapes_lib(&mut cx).unwrap();
    install_stream_core_shapes_lib(&mut cx).unwrap();

    let packet_shape = cx
        .registry()
        .shape_by_symbol(&stream_packet_shape_symbol())
        .expect("stream packet shape")
        .clone();
    let doc = packet_shape
        .object()
        .as_shape()
        .expect("shape protocol")
        .describe(&mut cx)
        .unwrap();

    assert_eq!(doc.name, "StreamPacket");
    assert!(
        cx.registry()
            .shape_by_symbol(&stream_metadata_shape_symbol())
            .is_some()
    );
    assert!(
        cx.registry()
            .shape_by_symbol(&stream_envelope_shape_symbol())
            .is_some()
    );
    assert!(
        cx.registry()
            .shape_by_symbol(&stream_media_shape_symbol())
            .is_some()
    );
    assert!(
        cx.registry()
            .shape_by_symbol(&stream_clock_domain_shape_symbol())
            .is_some()
    );
    assert!(
        cx.registry()
            .shape_by_symbol(&stream_latency_class_shape_symbol())
            .is_some()
    );
    assert!(
        cx.registry()
            .shape_by_symbol(&stream_capability_shape_symbol())
            .is_some()
    );
    assert!(
        cx.registry()
            .shape_by_symbol(&stream_backpressure_shape_symbol())
            .is_some()
    );
    assert!(
        cx.registry()
            .shape_by_symbol(&stream_clock_shape_symbol())
            .is_some()
    );
    assert!(
        cx.registry()
            .shape_by_symbol(&stream_tempo_shape_symbol())
            .is_some()
    );
    assert!(
        cx.registry()
            .shape_by_symbol(&stream_buffer_policy_shape_symbol())
            .is_some()
    );
    assert!(
        cx.registry()
            .shape_by_symbol(&stream_data_packet_shape_symbol())
            .is_some()
    );
    assert!(
        cx.registry()
            .shape_by_symbol(&stream_diagnostic_shape_symbol())
            .is_some()
    );
}

#[test]
fn metadata_round_trips_lisp_value_lisp() {
    let mut cx = super::cx();
    let lisp_id = install_lisp_codec(&mut cx);
    install_stream_core_classes(&mut cx).unwrap();
    cx.grant(read_construct_capability());

    let source = concat!(
        "#(stream/Metadata stream/demo stream/media/pcm ",
        "stream/direction/source clock/sample ",
        "(expr:map [capacity \"2\"] [overflow stream/overflow/drop-oldest]))"
    )
    .to_owned();
    let decoded = decode_with_codec(
        &mut cx,
        &Symbol::qualified("codec", "lisp"),
        Input::Text(source),
        read_policy_with_construct(),
    )
    .unwrap();
    let Expr::Call { operator, args } = decoded else {
        panic!("expected stream metadata constructor expression");
    };
    assert_eq!(*operator, Expr::Symbol(stream_metadata_class_symbol()));
    assert_eq!(
        StreamMetadata::from_constructor_args(args.clone()).unwrap(),
        super::metadata()
    );

    let value_args = expr_values(&mut cx, args);
    let value = cx
        .read_construct(&stream_metadata_class_symbol(), value_args)
        .unwrap();
    let mut write = WriteCx {
        cx: &mut cx,
        codec: lisp_id,
        options: EncodeOptions {
            position: EncodePosition::Quote,
            ..Default::default()
        },
    };
    let encoded = encode_object_lisp(&mut write, value).unwrap();
    assert!(encoded.starts_with("#(stream/Metadata "));

    let decoded_again = decode_with_codec(
        &mut cx,
        &Symbol::qualified("codec", "lisp"),
        Input::Text(encoded),
        read_policy_with_construct(),
    )
    .unwrap();
    let Expr::Call { args, .. } = decoded_again else {
        panic!("expected stream metadata constructor expression");
    };
    assert_eq!(
        StreamMetadata::from_constructor_args(args).unwrap(),
        super::metadata()
    );
}

#[test]
fn midi_and_pcm_packets_round_trip_through_json() {
    let mut cx = super::cx();
    install_json_codec(&mut cx);
    let pcm = StreamPacket::Pcm(PcmPacket::i16(2, 2, vec![1, -1, 7, -7]).unwrap());
    let midi = StreamPacket::Midi(
        MidiPacket::new(vec![
            MidiPacketEvent::new(0, 480, vec![0x90, 60, 100]).unwrap(),
            MidiPacketEvent::new(240, 480, vec![0x80, 60, 0]).unwrap(),
        ])
        .unwrap(),
    );

    assert_eq!(
        roundtrip_packet(&mut cx, &Symbol::qualified("codec", "json"), &pcm),
        pcm
    );
    assert_eq!(
        roundtrip_packet(&mut cx, &Symbol::qualified("codec", "json"), &midi),
        midi
    );
}

#[test]
fn binary_base64_preserves_pcm_sample_count() {
    let mut cx = super::cx();
    install_binary_codec(&mut cx);
    install_binary_base64_codec(&mut cx);
    let packet = StreamPacket::Pcm(PcmPacket::i16(1, 4, vec![3, 4, 5, 6]).unwrap());

    assert_eq!(
        roundtrip_packet(&mut cx, &Symbol::qualified("codec", "binary"), &packet),
        packet
    );

    let decoded = roundtrip_packet(
        &mut cx,
        &Symbol::qualified("codec", "binary-base64"),
        &packet,
    );
    let StreamPacket::Pcm(pcm) = decoded else {
        panic!("expected decoded PCM packet");
    };

    assert_eq!(pcm.samples_i16().len(), 4);
}

#[test]
fn data_packet_round_trips_through_lisp() {
    let mut cx = super::cx();
    install_lisp_codec(&mut cx);
    let packet = sample_data_packet();

    assert_eq!(
        roundtrip_packet(&mut cx, &Symbol::qualified("codec", "lisp"), &packet),
        packet
    );
}

#[test]
fn data_packet_round_trips_through_json() {
    let mut cx = super::cx();
    install_json_codec(&mut cx);
    let packet = sample_data_packet();

    assert_eq!(
        roundtrip_packet(&mut cx, &Symbol::qualified("codec", "json"), &packet),
        packet
    );
}

#[test]
fn data_packet_round_trips_through_binary() {
    let mut cx = super::cx();
    install_binary_codec(&mut cx);
    let packet = sample_data_packet();

    assert_eq!(
        roundtrip_packet(&mut cx, &Symbol::qualified("codec", "binary"), &packet),
        packet
    );
}

#[test]
fn data_packet_round_trips_through_binary_base64() {
    let mut cx = super::cx();
    install_binary_codec(&mut cx);
    install_binary_base64_codec(&mut cx);
    let packet = sample_data_packet();

    assert_eq!(
        roundtrip_packet(
            &mut cx,
            &Symbol::qualified("codec", "binary-base64"),
            &packet,
        ),
        packet
    );
}

#[test]
fn stream_envelope_round_trips_through_json() {
    let mut cx = super::cx();
    install_json_codec(&mut cx);
    let envelope = sample_envelope();

    assert_eq!(
        roundtrip_envelope(&mut cx, &Symbol::qualified("codec", "json"), &envelope),
        envelope
    );
}

#[test]
fn stream_envelope_round_trips_through_lisp() {
    let mut cx = super::cx();
    install_lisp_codec(&mut cx);
    let envelope = sample_envelope();

    assert_eq!(
        roundtrip_envelope(&mut cx, &Symbol::qualified("codec", "lisp"), &envelope),
        envelope
    );
}

#[test]
fn unknown_data_packet_fields_fail_closed() {
    let mut expr = sample_data_packet().to_expr();
    let Expr::Map(entries) = &mut expr else {
        panic!("data packet expression should be a map");
    };
    entries.push(key_expr("unexpected", Expr::Bool(true)));

    let err = StreamPacket::try_from(expr).unwrap_err();

    assert!(format!("{err}").contains("unknown data packet field unexpected"));
}

#[test]
fn data_packet_constructors_use_expected_kinds() {
    let expr_packet = StreamPacket::data(
        Symbol::qualified("stream/data", "expr"),
        Expr::String("raw expr".to_owned()),
    );
    let model_packet = StreamPacket::model_event(Expr::String("model".to_owned()));
    let rank_packet = StreamPacket::rank_frontier(Expr::String("rank".to_owned()));

    assert_data_kind(expr_packet, Symbol::qualified("stream/data", "expr"));
    assert_data_kind(
        model_packet,
        Symbol::qualified("stream/data", "model-event"),
    );
    assert_data_kind(
        rank_packet,
        Symbol::qualified("stream/data", "rank-frontier"),
    );
}

#[test]
fn unknown_media_tag_returns_diagnostic_error() {
    let err = StreamMedia::from_symbol(&Symbol::qualified("stream/media", "unknown")).unwrap_err();

    assert_eq!(
        StreamMedia::from_symbol(&Symbol::qualified("stream/media", "data")).unwrap(),
        StreamMedia::Data
    );
    assert!(format!("{err}").contains("unknown stream media stream/media/unknown"));
}

#[test]
fn unknown_envelope_field_fails_closed() {
    let mut expr = sample_envelope().to_expr();
    let Expr::Map(entries) = &mut expr else {
        panic!("stream envelope expression should be a map");
    };
    entries.push(key_expr("unexpected", Expr::Bool(true)));

    let err = StreamEnvelope::try_from(expr).unwrap_err();

    assert!(format!("{err}").contains("unknown stream envelope field unexpected"));
}

#[test]
fn canonical_clock_and_latency_symbols_decode() {
    assert_eq!(
        ClockDomain::from_symbol(&Symbol::qualified("stream/clock-domain", "midi-tick")).unwrap(),
        ClockDomain::MidiTick
    );
    assert_eq!(
        ClockDomain::from_symbol(&Symbol::qualified("clock", "midi")).unwrap(),
        ClockDomain::MidiTick
    );
    assert_eq!(
        LatencyClass::from_symbol(&Symbol::qualified("stream/latency", "remote-collaboration"))
            .unwrap(),
        LatencyClass::RemoteCollaboration
    );
    assert_eq!(
        StreamCapability::from_symbol(&Symbol::qualified("stream/capability", "resumable"))
            .unwrap(),
        StreamCapability::Resumable
    );
    assert_eq!(
        BackpressureOutcome::from_symbol(&Symbol::qualified("stream/backpressure", "timed-out"))
            .unwrap(),
        BackpressureOutcome::TimedOut
    );
}

fn install_lisp_codec(cx: &mut Cx) -> CodecId {
    let id = cx.registry_mut().fresh_codec_id();
    let lib = LispCodecLib::new(id).unwrap();
    cx.load_lib(&lib).unwrap();
    id
}

fn install_json_codec(cx: &mut Cx) {
    let lib = JsonCodecLib::new(cx.registry_mut().fresh_codec_id());
    cx.load_lib(&lib).unwrap();
}

fn install_binary_codec(cx: &mut Cx) {
    let lib = BinaryCodecLib::new(cx.registry_mut().fresh_codec_id());
    cx.load_lib(&lib).unwrap();
}

fn install_binary_base64_codec(cx: &mut Cx) {
    let lib = BinaryBase64CodecLib::new(cx.registry_mut().fresh_codec_id());
    cx.load_lib(&lib).unwrap();
}

fn roundtrip_packet(cx: &mut Cx, codec: &Symbol, packet: &StreamPacket) -> StreamPacket {
    let output = encode_with_codec(cx, codec, &packet.to_expr(), EncodeOptions::default()).unwrap();
    let decoded =
        decode_with_codec(cx, codec, input_from_output(output), ReadPolicy::default()).unwrap();
    StreamPacket::try_from(decoded).unwrap()
}

fn roundtrip_envelope(cx: &mut Cx, codec: &Symbol, envelope: &StreamEnvelope) -> StreamEnvelope {
    let output =
        encode_with_codec(cx, codec, &envelope.to_expr(), EncodeOptions::default()).unwrap();
    let decoded =
        decode_with_codec(cx, codec, input_from_output(output), ReadPolicy::default()).unwrap();
    StreamEnvelope::try_from(decoded).unwrap()
}

fn sample_envelope() -> StreamEnvelope {
    let metadata = StreamMetadata::new(
        Symbol::qualified("stream", "diagnostic-demo"),
        StreamMedia::Diagnostic,
        StreamDirection::Source,
        Symbol::qualified("clock", "sample"),
        BufferPolicy::bounded(4).unwrap(),
    );
    let item = StreamItem::new(StreamPacket::Diagnostic(StreamDiagnostic::new(
        Symbol::qualified("stream/diagnostic", "codec"),
        "codec envelope",
    )));
    StreamEnvelope::from_item(&metadata, 42, &item).unwrap()
}

fn sample_data_packet() -> StreamPacket {
    StreamPacket::model_event(Expr::Map(vec![
        key_bool("model-event", true),
        key_expr("event", Expr::Symbol(Symbol::new("delta"))),
        key_expr("runner", Expr::Symbol(Symbol::new("core-data"))),
        key_expr("model", Expr::String("runner/fake".to_owned())),
        key_expr("span-id", Expr::String("span-data".to_owned())),
        key_expr(
            "parts",
            Expr::List(vec![
                Expr::String("hello".to_owned()),
                Expr::Map(vec![key_expr("index", Expr::String("1".to_owned()))]),
            ]),
        ),
    ]))
}

fn assert_data_kind(packet: StreamPacket, expected: Symbol) {
    let StreamPacket::Data(packet) = packet else {
        panic!("expected data packet");
    };
    assert_eq!(packet.kind, expected);
}

fn key_bool(name: &str, value: bool) -> (Expr, Expr) {
    key_expr(name, Expr::Bool(value))
}

fn key_expr(name: &str, value: Expr) -> (Expr, Expr) {
    (Expr::Symbol(Symbol::new(name)), value)
}

fn input_from_output(output: Output) -> Input {
    match output {
        Output::Text(text) => Input::Text(text),
        Output::Bytes(bytes) => Input::Bytes(bytes),
    }
}

fn expr_values(cx: &mut Cx, exprs: Vec<Expr>) -> Vec<sim_kernel::Value> {
    exprs
        .into_iter()
        .map(|expr| cx.factory().expr(expr).unwrap())
        .collect()
}

fn read_policy_with_construct() -> ReadPolicy {
    ReadPolicy {
        trust: TrustLevel::TrustedSource,
        capabilities: CapabilitySet::new().grant(read_construct_capability()),
    }
}

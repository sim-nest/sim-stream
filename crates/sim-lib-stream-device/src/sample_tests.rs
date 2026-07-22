use std::sync::Arc;

use sim_kernel::{Expr, ObjectEncoding, Symbol, read_construct_capability, testing::bare_cx as cx};

use crate::{
    DeviceCaps, DeviceSample, DeviceSampleValue, ModeledDeviceCapsSource, ModeledSource,
    device_caps_sample_kind_symbol, device_sample_class_symbol, device_sample_record_symbol,
    install_device_stream_base, roundtrip_ok, sample_packet, seq_is_monotone,
};

#[test]
fn demo_sample_round_trips_and_fails_closed() {
    let sample = DeviceCaps::demo(7);
    assert!(roundtrip_ok(&sample));
    assert_eq!(DeviceCaps::from_expr(&sample.to_expr()).unwrap(), sample);

    let missing_seq = Expr::Map(vec![
        (
            Expr::Symbol(Symbol::new("kind")),
            Expr::Symbol(device_sample_record_symbol()),
        ),
        (
            Expr::Symbol(Symbol::new("sample")),
            Expr::Symbol(device_caps_sample_kind_symbol()),
        ),
    ]);
    let err = DeviceCaps::from_expr(&missing_seq).unwrap_err();
    assert!(err.to_string().contains("missing field seq"));

    let unknown = Expr::Map(vec![
        (
            Expr::Symbol(Symbol::new("kind")),
            Expr::Symbol(device_sample_record_symbol()),
        ),
        (
            Expr::Symbol(Symbol::new("sample")),
            Expr::Symbol(Symbol::qualified("stream/device-sample", "unknown")),
        ),
        (Expr::Symbol(Symbol::new("seq")), sim_value::build::uint(1)),
    ]);
    assert!(DeviceSampleValue::new(unknown).is_err());
}

#[test]
fn modeled_source_is_deterministic_and_seq_monotone() {
    let source = ModeledDeviceCapsSource::demo().with_seq_base(10);
    assert_eq!(source.at(4), source.at(4));
    assert_ne!(source.at(4).seq(), source.at(5).seq());
    assert_eq!(source.at(4).seq(), 14);
    assert!(seq_is_monotone(&source, 0, 16));
}

#[test]
fn sample_wraps_as_stream_data_packet() {
    let sample = DeviceCaps::demo(3);
    let packet = sample_packet(&sample);
    let sim_lib_stream_core::StreamPacket::Data(data) = packet else {
        panic!("device sample should wrap as data packet");
    };
    assert_eq!(data.kind, device_caps_sample_kind_symbol());
    assert_eq!(data.payload, sample.to_expr());
}

#[test]
fn device_sample_read_construct_round_trips() {
    let mut cx = cx();
    install_device_stream_base(&mut cx).unwrap();
    cx.grant(read_construct_capability());

    let sample = DeviceCaps::demo(11);
    let value = cx
        .factory()
        .opaque(Arc::new(DeviceSampleValue::new(sample.to_expr()).unwrap()))
        .unwrap();
    let ObjectEncoding::Constructor { class, args } = value
        .object()
        .as_object_encoder()
        .unwrap()
        .object_encoding(&mut cx)
        .unwrap()
    else {
        panic!("device sample should encode as constructor");
    };
    assert_eq!(class, device_sample_class_symbol());

    let args = args
        .iter()
        .map(|expr| cx.factory().expr(expr.clone()))
        .collect::<sim_kernel::Result<Vec<_>>>()
        .unwrap();
    let decoded = cx.read_construct(&class, args).unwrap();
    let decoded = decoded
        .object()
        .downcast_ref::<DeviceSampleValue>()
        .unwrap();
    assert_eq!(decoded.device_caps().unwrap(), sample);
}

#[test]
fn install_device_stream_base_registers_class_once() {
    let mut cx = cx();
    install_device_stream_base(&mut cx).unwrap();
    install_device_stream_base(&mut cx).unwrap();
    assert!(
        cx.registry()
            .class_by_symbol(&device_sample_class_symbol())
            .is_some()
    );
}

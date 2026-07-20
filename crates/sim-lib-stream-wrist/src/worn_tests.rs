use std::sync::Arc;

use sim_kernel::{Expr, ObjectEncoding, Symbol, read_construct_capability, testing::bare_cx as cx};
use sim_lib_stream_device::{
    DeviceSample, ModeledSource, roundtrip_ok, sample_packet, seq_is_monotone,
};

use crate::{
    MicAudioFrame, ModeledBatterySource, ModeledConnectionSource, ModeledHeartRateSource,
    ModeledLocationSource, ModeledMotionSource, WornEvent, WornEventValue, WornSensor,
    install_wrist_stream_lib, worn_event_class_symbol, worn_event_sample_kind_symbol,
};

#[test]
fn all_worn_sensor_symbols_round_trip_without_voice_intent() {
    assert_eq!(WornSensor::all().len(), 20);
    for sensor in WornSensor::all() {
        assert_eq!(WornSensor::from_symbol(&sensor.symbol()).unwrap(), *sensor);
        assert_ne!(sensor.name(), "voice-intent");
    }
    assert!(
        WornSensor::from_symbol(&Symbol::qualified("stream/worn-sensor", "voice-intent")).is_err()
    );
}

#[test]
fn worn_event_round_trips_and_fails_closed() {
    let sample = WornEvent::heart_rate(7, 74).unwrap();
    assert!(roundtrip_ok(&sample));
    assert_eq!(WornEvent::from_expr(&sample.to_expr()).unwrap(), sample);

    let missing_seq = sim_value::build::map(vec![
        (
            "kind",
            Expr::Symbol(sim_lib_stream_device::device_sample_record_symbol()),
        ),
        ("sample", Expr::Symbol(worn_event_sample_kind_symbol())),
    ]);
    let err = WornEvent::from_expr(&missing_seq).unwrap_err();
    assert!(err.to_string().contains("missing field seq"));

    let unknown_sensor = sim_value::build::map(vec![
        (
            "kind",
            Expr::Symbol(sim_lib_stream_device::device_sample_record_symbol()),
        ),
        ("sample", Expr::Symbol(worn_event_sample_kind_symbol())),
        ("seq", sim_value::build::uint(1)),
        (
            "sensor",
            Expr::Symbol(Symbol::qualified("stream/worn-sensor", "unknown")),
        ),
        ("confidence", sim_value::build::uint(9_000)),
        ("payload", Expr::Nil),
    ]);
    assert!(WornEvent::from_expr(&unknown_sensor).is_err());

    let Expr::Map(mut entries) = sample.to_expr() else {
        panic!("worn event encodes as a map");
    };
    entries.push((Expr::Symbol(Symbol::new("extra")), Expr::Bool(true)));
    assert!(WornEvent::from_expr(&Expr::Map(entries)).is_err());
}

#[test]
fn mic_audio_requires_raw_framed_payload() {
    let err = WornEvent::new(
        1,
        WornSensor::MicAudio,
        9_000,
        Expr::String("start running".to_owned()),
    )
    .unwrap_err();
    assert!(err.to_string().contains("non-map"));

    let frame = MicAudioFrame::new(4, 16_000, 1, vec![1, 2, 3, 4]).unwrap();
    let event = WornEvent::mic_audio(2, 9_100, frame.clone()).unwrap();
    assert_eq!(WornEvent::from_expr(&event.to_expr()).unwrap(), event);
    assert_eq!(MicAudioFrame::from_expr(event.payload()).unwrap(), frame);
}

#[test]
fn modeled_heart_rate_is_deterministic_and_seq_monotone() {
    let source = ModeledHeartRateSource;
    assert_eq!(source.at(4), source.at(4));
    assert_ne!(source.at(4).payload(), source.at(5).payload());
    assert_eq!(source.at(4).seq(), 4);
    assert!(seq_is_monotone(&source, 0, 16));
}

#[test]
fn modeled_sources_are_index_driven() {
    let motion = ModeledMotionSource;
    let location = ModeledLocationSource;
    let battery = ModeledBatterySource;
    let connection = ModeledConnectionSource;

    assert_eq!(motion.at(12), motion.at(12));
    assert_eq!(location.at(12), location.at(12));
    assert_eq!(battery.at(12), battery.at(12));
    assert_eq!(connection.at(12), connection.at(12));

    assert_eq!(motion.at(12).seq(), 12);
    assert_eq!(location.at(12).seq(), 12);
    assert_eq!(battery.at(12).seq(), 12);
    assert_eq!(connection.at(12).seq(), 12);
}

#[test]
fn worn_event_wraps_as_stream_data_packet() {
    let sample = WornEvent::battery(3, 87, false).unwrap();
    let packet = sample_packet(&sample);
    let sim_lib_stream_core::StreamPacket::Data(data) = packet else {
        panic!("worn event should wrap as data packet");
    };
    assert_eq!(data.kind, worn_event_sample_kind_symbol());
    assert_eq!(data.payload, sample.to_expr());
}

#[test]
fn worn_event_read_construct_round_trips() {
    let mut cx = cx();
    install_wrist_stream_lib(&mut cx).unwrap();
    cx.grant(read_construct_capability());

    let sample = WornEvent::connection(11, true, -52).unwrap();
    let value = cx
        .factory()
        .opaque(Arc::new(WornEventValue::new(sample.to_expr()).unwrap()))
        .unwrap();
    let ObjectEncoding::Constructor { class, args } = value
        .object()
        .as_object_encoder()
        .unwrap()
        .object_encoding(&mut cx)
        .unwrap()
    else {
        panic!("worn event should encode as constructor");
    };
    assert_eq!(class, worn_event_class_symbol());

    let args = args
        .iter()
        .map(|expr| cx.factory().expr(expr.clone()))
        .collect::<sim_kernel::Result<Vec<_>>>()
        .unwrap();
    let decoded = cx.read_construct(&class, args).unwrap();
    let decoded = decoded.object().downcast_ref::<WornEventValue>().unwrap();
    assert_eq!(decoded.worn_event().unwrap(), sample);
}

#[test]
fn install_wrist_stream_lib_registers_base_and_wrist_once() {
    let mut cx = cx();
    install_wrist_stream_lib(&mut cx).unwrap();
    install_wrist_stream_lib(&mut cx).unwrap();
    assert!(
        cx.registry()
            .class_by_symbol(&sim_lib_stream_device::device_sample_class_symbol())
            .is_some()
    );
    assert!(
        cx.registry()
            .class_by_symbol(&worn_event_class_symbol())
            .is_some()
    );
}

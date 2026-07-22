use std::sync::Arc;

use sim_kernel::{Expr, ObjectEncoding, Symbol, read_construct_capability, testing::bare_cx as cx};
use sim_lib_stream_device::{
    DeviceSample, ModeledSource, roundtrip_ok, sample_packet, seq_is_monotone,
};

use crate::{
    ModeledHaloCameraSource, ModeledHaloMicSource, ModeledHaloMotionSource, ModeledHaloTapSource,
    ModeledVitureHandSource, ModeledViturePoseSource, ModeledVitureStereoCameraSource,
    XrCameraFrameRef, XrHandSample, XrMicChunkRef, XrPoseSample, XrSampleValue, XrTapSample,
    XrTrackingStatus, install_xr_stream_lib, xr_camera_frame_sample_kind_symbol,
    xr_mic_chunk_sample_kind_symbol, xr_sample_class_symbol,
};

#[test]
fn xr_sample_records_round_trip_and_fail_closed() {
    let pose = ModeledViturePoseSource.at(2);
    let camera = ModeledVitureStereoCameraSource.at(2);
    let hand = ModeledVitureHandSource.at(2);
    let tap = ModeledHaloTapSource.at(2);
    let mic = ModeledHaloMicSource.at(2);

    assert!(roundtrip_ok(&pose));
    assert!(roundtrip_ok(&camera));
    assert!(roundtrip_ok(&hand));
    assert!(roundtrip_ok(&tap));
    assert!(roundtrip_ok(&mic));
    assert_eq!(XrPoseSample::from_expr(&pose.to_expr()).unwrap(), pose);
    assert_eq!(
        XrCameraFrameRef::from_expr(&camera.to_expr()).unwrap(),
        camera
    );
    assert_eq!(XrHandSample::from_expr(&hand.to_expr()).unwrap(), hand);
    assert_eq!(XrTapSample::from_expr(&tap.to_expr()).unwrap(), tap);
    assert_eq!(XrMicChunkRef::from_expr(&mic.to_expr()).unwrap(), mic);

    let missing_seq = sim_value::build::map(vec![
        (
            "kind",
            Expr::Symbol(sim_lib_stream_device::device_sample_record_symbol()),
        ),
        ("sample", Expr::Symbol(xr_camera_frame_sample_kind_symbol())),
    ]);
    let err = XrCameraFrameRef::from_expr(&missing_seq).unwrap_err();
    assert!(err.to_string().contains("missing field seq"));

    let Expr::Map(mut entries) = mic.to_expr() else {
        panic!("mic chunk encodes as a map");
    };
    entries.push((
        Expr::Symbol(Symbol::new("transcript")),
        Expr::String("show messages".to_owned()),
    ));
    assert!(XrMicChunkRef::from_expr(&Expr::Map(entries)).is_err());

    assert!(
        XrTrackingStatus::from_symbol(&Symbol::qualified("stream/xr-tracking", "unknown")).is_err()
    );
}

#[test]
fn modeled_viture_pose_and_halo_inputs_deterministic() {
    let viture_pose = ModeledViturePoseSource;
    let viture_camera = ModeledVitureStereoCameraSource;
    let viture_hand = ModeledVitureHandSource;
    let halo_motion = ModeledHaloMotionSource;
    let halo_tap = ModeledHaloTapSource;
    let halo_camera = ModeledHaloCameraSource;
    let halo_mic = ModeledHaloMicSource;

    assert_eq!(viture_pose.at(4), viture_pose.at(4));
    assert_ne!(viture_pose.at(4), viture_pose.at(5));
    assert_eq!(viture_pose.at(4).dof(), 6);
    assert!(viture_pose.at(4).position_m().is_some());
    assert_eq!(halo_motion.at(4).dof(), 3);
    assert!(halo_motion.at(4).position_m().is_none());

    assert_eq!(viture_camera.at(4), viture_camera.at(4));
    assert!(viture_camera.at(4).stereo());
    assert_eq!(halo_camera.at(4).width_px(), 640);
    assert!(!halo_camera.at(4).stereo());
    assert_eq!(viture_hand.at(4), viture_hand.at(4));
    assert_eq!(halo_tap.at(4).tap_index(), 4);
    assert_eq!(halo_mic.at(4).ms(), 40);
    assert_eq!(
        halo_mic.at(4).store_key(),
        &Symbol::qualified("stream/xr-mic-chunk", "halo-canned-000004")
    );

    assert!(seq_is_monotone(&viture_pose, 0, 16));
    assert!(seq_is_monotone(&viture_camera, 0, 16));
    assert!(seq_is_monotone(&viture_hand, 0, 16));
    assert!(seq_is_monotone(&halo_motion, 0, 16));
    assert!(seq_is_monotone(&halo_tap, 0, 16));
    assert!(seq_is_monotone(&halo_camera, 0, 16));
    assert!(seq_is_monotone(&halo_mic, 0, 16));
}

#[test]
fn xr_sample_wraps_as_stream_data_packet() {
    let sample = ModeledHaloMicSource.at(3);
    let packet = sample_packet(&sample);
    let sim_lib_stream_core::StreamPacket::Data(data) = packet else {
        panic!("XR sample should wrap as data packet");
    };
    assert_eq!(data.kind, xr_mic_chunk_sample_kind_symbol());
    assert_eq!(data.payload, sample.to_expr());
}

#[test]
fn xr_sample_read_construct_round_trips() {
    let mut cx = cx();
    install_xr_stream_lib(&mut cx).unwrap();
    cx.grant(read_construct_capability());

    let sample = ModeledHaloCameraSource.at(11);
    let value = cx
        .factory()
        .opaque(Arc::new(XrSampleValue::new(sample.to_expr()).unwrap()))
        .unwrap();
    let ObjectEncoding::Constructor { class, args } = value
        .object()
        .as_object_encoder()
        .unwrap()
        .object_encoding(&mut cx)
        .unwrap()
    else {
        panic!("XR sample should encode as constructor");
    };
    assert_eq!(class, xr_sample_class_symbol());

    let args = args
        .iter()
        .map(|expr| cx.factory().expr(expr.clone()))
        .collect::<sim_kernel::Result<Vec<_>>>()
        .unwrap();
    let decoded = cx.read_construct(&class, args).unwrap();
    let decoded = decoded.object().downcast_ref::<XrSampleValue>().unwrap();
    assert_eq!(decoded.camera_frame().unwrap(), sample);
}

#[test]
fn install_xr_stream_lib_registers_base_and_xr_once() {
    let mut cx = cx();
    install_xr_stream_lib(&mut cx).unwrap();
    install_xr_stream_lib(&mut cx).unwrap();
    assert!(
        cx.registry()
            .class_by_symbol(&sim_lib_stream_device::device_sample_class_symbol())
            .is_some()
    );
    assert!(
        cx.registry()
            .class_by_symbol(&xr_sample_class_symbol())
            .is_some()
    );
}

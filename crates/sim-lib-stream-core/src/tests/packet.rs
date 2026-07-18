use crate::{
    BackpressureOutcome, BufferPolicy, MidiPacket, MidiPacketEvent, PcmPacket, PcmSampleFormat,
    StreamPacket,
};

#[test]
fn capacity_zero_rejected() {
    assert!(BufferPolicy::bounded(0).is_err());
}

#[test]
fn pcm_buffer_length_mismatch_rejected() {
    let err = PcmPacket::i16(2, 2, vec![1, 2, 3]).unwrap_err();
    assert!(format!("{err}").contains("does not match"));
}

#[test]
fn pcm_zero_frame_packets_are_valid_empty_packets() {
    let i16_packet = PcmPacket::i16(2, 0, Vec::new()).unwrap();
    assert_eq!(i16_packet.channels(), 2);
    assert_eq!(i16_packet.frames(), 0);
    assert!(i16_packet.samples_i16().is_empty());

    let f32_packet = PcmPacket::f32(2, 0, Vec::new()).unwrap();
    assert_eq!(f32_packet.channels(), 2);
    assert_eq!(f32_packet.frames(), 0);
    assert!(f32_packet.samples_f32().is_empty());

    let packet = StreamPacket::Pcm(f32_packet);
    let decoded = StreamPacket::try_from(packet.to_expr()).unwrap();
    assert_eq!(decoded, packet);
}

#[test]
fn pcm_nonzero_frame_count_rejects_empty_payload() {
    let err = PcmPacket::i16(2, 1, Vec::new()).unwrap_err();
    assert!(format!("{err}").contains("does not match"));
}

#[test]
fn pcm_f32_packet_round_trips_through_expr() {
    let packet = StreamPacket::Pcm(PcmPacket::f32(2, 2, vec![0.0, -0.5, 1.0, -1.0]).unwrap());

    let decoded = StreamPacket::try_from(packet.to_expr()).unwrap();

    assert_eq!(decoded, packet);
    let StreamPacket::Pcm(pcm) = decoded else {
        panic!("expected decoded PCM packet");
    };
    assert_eq!(pcm.sample_format(), PcmSampleFormat::F32);
    assert_eq!(pcm.samples_f32(), &[0.0, -0.5, 1.0, -1.0]);
}

#[test]
fn pcm_f32_packet_rejects_non_finite_samples() {
    let err = PcmPacket::f32(1, 1, vec![f32::NAN]).unwrap_err();
    assert!(format!("{err}").contains("must be finite"));
}

#[test]
fn midi_packet_rejects_mixed_tpq() {
    let first = MidiPacketEvent::new(0, 480, vec![0x90, 60, 100]).unwrap();
    let second = MidiPacketEvent::new(1, 960, vec![0x80, 60, 0]).unwrap();

    let err = MidiPacket::new(vec![first, second]).unwrap_err();

    assert!(format!("{err}").contains("shared TPQ"));
}

#[test]
fn midi_packet_rejects_empty_event_list() {
    let err = MidiPacket::new(Vec::new()).unwrap_err();
    assert!(format!("{err}").contains("at least one event"));
}

#[test]
fn backpressure_outcomes_use_canonical_symbols() {
    let outcomes = [
        BackpressureOutcome::Accepted,
        BackpressureOutcome::DroppedNewest,
        BackpressureOutcome::DroppedOldest,
        BackpressureOutcome::Blocked,
        BackpressureOutcome::TimedOut,
        BackpressureOutcome::Rejected,
        BackpressureOutcome::Closed,
    ];

    for outcome in outcomes {
        assert_eq!(
            BackpressureOutcome::from_symbol(&outcome.symbol()).unwrap(),
            outcome
        );
    }
}

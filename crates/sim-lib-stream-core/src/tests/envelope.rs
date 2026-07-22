use super::*;

#[test]
fn stream_item_converts_to_versioned_envelope() {
    let sample_tick = Tick::new(
        Symbol::qualified("clock", "sample"),
        Ref::Symbol(Symbol::qualified("frame", "zero")),
    );
    let transport_tick = Tick::new(
        Symbol::qualified("clock", "transport"),
        Ref::Symbol(Symbol::qualified("bar", "one")),
    );
    let item = StreamItem::with_ticks(
        diagnostic_packet("wrapped"),
        vec![sample_tick.clone(), transport_tick.clone()],
    )
    .unwrap();
    let metadata = diagnostic_metadata();

    let envelope = StreamEnvelope::from_item(&metadata, 7, &item).unwrap();

    assert_eq!(envelope.version(), crate::STREAM_ENVELOPE_VERSION);
    assert_eq!(envelope.stream_id(), metadata.id());
    assert_eq!(
        envelope.packet_id().namespace.as_deref(),
        Some("stream/packet-id")
    );
    assert_eq!(envelope.media(), StreamMedia::Diagnostic);
    assert_eq!(envelope.direction(), StreamDirection::Source);
    assert_eq!(envelope.sequence(), 7);
    assert_eq!(envelope.ticks(), &[sample_tick, transport_tick]);
    assert_eq!(envelope.clock_domain(), ClockDomain::Sample);
    assert_eq!(
        envelope.clock_domains(),
        &[ClockDomain::Sample, ClockDomain::Transport]
    );
    assert_eq!(envelope.profile().latency_class(), LatencyClass::BlockLocal);
    assert!(
        envelope
            .profile()
            .capabilities()
            .contains(&StreamCapability::Replayable)
    );
    assert_eq!(envelope.packet(), item.packet());
}

#[test]
fn stream_item_preserves_non_stream_ticks_without_clock_domain_summary() {
    let rank_tick = Tick::new(
        Symbol::qualified("rank/order", "position"),
        Ref::Symbol(Symbol::qualified("rank/ordinal", "zero")),
    );
    let item =
        StreamItem::with_ticks(diagnostic_packet("ranked"), vec![rank_tick.clone()]).unwrap();
    let metadata = diagnostic_metadata();

    let envelope = StreamEnvelope::from_item(&metadata, 9, &item).unwrap();

    assert_eq!(envelope.ticks(), &[rank_tick]);
    assert_eq!(envelope.clock_domain(), ClockDomain::Sample);
    assert_eq!(envelope.clock_domains(), &[ClockDomain::Sample]);
}

#[test]
fn stream_item_can_select_remote_fabric_profile() {
    let item = StreamItem::new(diagnostic_packet("remote"));
    let metadata = diagnostic_metadata();

    let envelope = StreamEnvelope::from_item_with_profile(
        &metadata,
        3,
        &item,
        TransportProfile::remote_stream_fabric(),
    )
    .unwrap();

    assert_eq!(
        envelope.profile().name(),
        &Symbol::qualified("stream/profile", "remote-stream-fabric")
    );
    assert_eq!(
        envelope.profile().latency_class(),
        LatencyClass::RemoteCollaboration
    );
    assert!(envelope.profile().has_capability(StreamCapability::Remote));
    assert!(
        envelope
            .profile()
            .has_capability(StreamCapability::Resumable)
    );
}

#[test]
fn stream_cassette_records_envelopes_timing_diagnostics_and_final_stats() {
    let stream = crate::StreamValue::pull(
        diagnostic_metadata(),
        vec![item("one"), ticked_item("two", 2)],
    );

    let cassette =
        StreamCassette::from_stream_value(&stream, TransportProfile::memory_local()).unwrap();
    let decoded = StreamCassette::from_expr(&cassette.to_expr()).unwrap();

    assert_eq!(decoded, cassette);
    assert_eq!(
        table_value(&cassette.to_expr(), "cassette"),
        Some(&Expr::Symbol(stream_cassette_format_symbol()))
    );
    assert_eq!(cassette.envelopes().len(), 2);
    assert_eq!(cassette.timing().packet_count, 2);
    assert_eq!(cassette.timing().first_sequence, Some(0));
    assert_eq!(cassette.timing().last_sequence, Some(1));
    assert_eq!(
        cassette.diagnostics(),
        &[Symbol::qualified("stream/test", "packet")]
    );
    assert_eq!(cassette.final_stats().yielded, 2);
    assert_eq!(
        cassette
            .replay_stream_value()
            .unwrap()
            .take_packets(4)
            .unwrap(),
        vec![item("one"), ticked_item("two", 2)]
    );
}

#[test]
fn golden_stream_fixture_rules_require_replayable_finite_redacted_streams() {
    let metadata = StreamMetadata::new(
        Symbol::new("device/CoreAudio Built-in Output"),
        StreamMedia::Data,
        StreamDirection::Source,
        ClockDomain::ServerFrame.symbol(),
        BufferPolicy::bounded(4).unwrap(),
    );
    let private = StreamItem::with_ticks(
        StreamPacket::data(
            Symbol::qualified("stream/private", "payload"),
            Expr::Map(vec![
                (Expr::Symbol(Symbol::new("private")), Expr::Bool(true)),
                (
                    Expr::Symbol(Symbol::new("device")),
                    Expr::String("hw:USB Keyboard".to_owned()),
                ),
            ]),
        ),
        vec![Tick::new(
            Symbol::qualified("clock", "sample"),
            Ref::Symbol(Symbol::new("device/CoreAudio Frame")),
        )],
    )
    .unwrap();
    let cassette = StreamCassette::from_items(
        metadata,
        vec![private],
        TransportProfile::memory_local(),
        Default::default(),
    )
    .unwrap();

    assert!(
        cassette
            .validate_golden_fixture("fixtures/streams/golden/private.simcassette")
            .is_err()
    );

    let redacted = cassette.redacted().unwrap();
    let report = redacted
        .validate_golden_fixture("fixtures/streams/golden/private.simcassette")
        .unwrap();
    assert_eq!(report.format, stream_cassette_format_symbol());
    assert_eq!(report.packet_count, 1);
    assert_eq!(
        redacted.metadata().clock(),
        &ClockDomain::ServerFrame.symbol()
    );
    assert_eq!(
        redacted.items().unwrap()[0].ticks()[0].index,
        Ref::Symbol(Symbol::qualified("stream/redacted", "device"))
    );
    assert_eq!(stream_cassette_golden_root(), "fixtures/streams/golden");
    assert!(
        redacted
            .validate_golden_fixture("tmp/private.simcassette")
            .is_err()
    );
    assert!(
        redacted
            .validate_golden_fixture("fixtures/streams/goldenish/private.simcassette")
            .is_err()
    );
    assert!(
        redacted
            .validate_golden_fixture("fixtures/streams/golden/private.simcassette.bak")
            .is_err()
    );
}

#[test]
fn stream_metadata_clock_rejects_unknown_and_accepts_aliases() {
    let metadata = StreamMetadata::new(
        Symbol::qualified("stream", "external-clock"),
        StreamMedia::Diagnostic,
        StreamDirection::Source,
        Symbol::qualified("clock", "external"),
        BufferPolicy::bounded(2).unwrap(),
    );
    let item = StreamItem::new(diagnostic_packet("external"));

    let err = StreamEnvelope::from_item(&metadata, 1, &item).unwrap_err();
    assert!(format!("{err}").contains("unknown stream clock domain clock/external"));

    for clock in [
        Symbol::new("sample"),
        Symbol::qualified("clock", "sample"),
        ClockDomain::Sample.symbol(),
        Symbol::new("midi"),
        Symbol::qualified("clock", "midi-tick"),
        ClockDomain::MidiTick.symbol(),
    ] {
        let metadata = StreamMetadata::new(
            Symbol::qualified("stream", "canonical-clock"),
            StreamMedia::Diagnostic,
            StreamDirection::Source,
            clock,
            BufferPolicy::bounded(2).unwrap(),
        );
        assert!(StreamEnvelope::from_item(&metadata, 1, &item).is_ok());
    }
}

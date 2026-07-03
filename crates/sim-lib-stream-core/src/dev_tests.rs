use sim_kernel::{Expr, Ref, Symbol, Tick};

use crate::{
    ClockDomain, DevCassette, DevEvent, LatencyClass, StreamFaultKind, StreamFaultPlan,
    StreamFaultSpec, StreamMedia, StreamPacket, dev_dropped_chunks_diagnostic, dev_event_media,
    stream_cassette_golden_root,
};

#[test]
fn dev_event_media_names_ide_event_descriptors() {
    let descriptor = dev_event_media("edit").unwrap();

    assert_eq!(descriptor.symbol(), &Symbol::qualified("ide/event", "edit"));
    assert_eq!(descriptor.stream_media(), StreamMedia::Data);
    assert!(dev_event_media("../bad").is_err());
}

#[test]
fn refusal_event_records_as_refusal_envelope() {
    let refusal = DevEvent::refusal(
        Symbol::qualified("atelier/node", "guard"),
        Expr::String("edit outside lease".to_owned()),
    )
    .unwrap();
    let cassette =
        DevCassette::from_events(Symbol::qualified("atelier/dev", "refusal"), vec![refusal])
            .unwrap();

    let envelope = &cassette.cassette().envelopes()[0];
    let StreamPacket::Data(packet) = envelope.packet() else {
        panic!("refusal event should be a data packet");
    };
    assert_eq!(packet.kind, Symbol::qualified("ide/event", "refusal"));
    assert_eq!(
        envelope.profile().latency_class(),
        LatencyClass::Interactive
    );
}

#[test]
fn dev_cassette_records_replays_hashes_redacts_and_faults() {
    let edit = DevEvent::edit(
        Symbol::qualified("atelier/node", "editor"),
        Expr::String("/workspace/example-repo/src/lib.rs".to_owned()),
    )
    .unwrap()
    .with_ticks(vec![Tick::new(
        ClockDomain::ServerFrame.symbol(),
        Ref::Symbol(Symbol::qualified("dev/tick", "edit")),
    )])
    .unwrap();
    let validate = DevEvent::validate(
        Symbol::qualified("atelier/node", "validator"),
        Expr::String("cargo test".to_owned()),
    )
    .unwrap();

    let cassette = DevCassette::from_events(
        Symbol::qualified("atelier/dev", "session"),
        vec![edit, validate],
    )
    .unwrap();

    assert_eq!(cassette.cassette().envelopes().len(), 2);
    assert_eq!(
        cassette.cassette().envelopes()[0].media(),
        StreamMedia::Data
    );
    assert_eq!(
        cassette.cassette().envelopes()[0].profile().latency_class(),
        LatencyClass::Interactive
    );
    assert_eq!(
        cassette.cassette().envelopes()[1].profile().latency_class(),
        LatencyClass::OfflineRender
    );
    assert_eq!(
        cassette.content_hash(),
        cassette.replay_content_hash().unwrap()
    );
    assert!(
        cassette
            .validate_golden_fixture("fixtures/streams/golden/dev-session.simcassette")
            .is_err()
    );

    let redacted = cassette.redacted().unwrap();
    assert!(!format!("{:?}", redacted.cassette().to_expr()).contains("/workspace"));
    redacted
        .validate_golden_fixture("fixtures/streams/golden/dev-session.simcassette")
        .unwrap();
    assert_eq!(stream_cassette_golden_root(), "fixtures/streams/golden");

    let report = redacted
        .replay_with_fault(&StreamFaultPlan::new(vec![StreamFaultSpec::new(
            StreamFaultKind::Drop,
            1,
        )]))
        .unwrap();
    assert!(report.diagnostics.contains(&StreamFaultKind::Drop.symbol()));
    assert!(
        report
            .diagnostics
            .contains(&dev_dropped_chunks_diagnostic())
    );
}

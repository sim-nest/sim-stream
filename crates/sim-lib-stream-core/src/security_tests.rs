use sim_kernel::{Expr, Symbol};

use crate::{
    BufferPolicy, StreamCassette, StreamDirection, StreamItem, StreamMedia, StreamMetadata,
    StreamPacket, StreamRedactionFinding, StreamRemoteLimits, StreamSecurityCapability,
    StreamSecurityPolicy, StreamStats, TransportProfile, stream_host_device_capability,
    stream_open_capability, stream_redaction_finding_symbols, stream_remote_network_capability,
    stream_security_capability_names,
};

#[test]
fn stream_security_capability_names_are_stable() {
    let names = stream_security_capability_names()
        .into_iter()
        .map(|capability| capability.as_str().to_owned())
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            "stream.open",
            "stream.read",
            "stream.push",
            "stream.cancel",
            "stream.stats",
            "stream.remote.preview",
            "stream.remote.render",
            "stream.lan.midi",
            "stream.host.device",
            "stream.remote.network",
        ]
    );
    assert_eq!(stream_open_capability().as_str(), "stream.open");
    assert_eq!(
        stream_host_device_capability().as_str(),
        "stream.host.device"
    );
    assert_eq!(
        stream_remote_network_capability().as_str(),
        "stream.remote.network"
    );
    assert_eq!(
        StreamSecurityCapability::RemotePreview.symbol(),
        Symbol::qualified("stream/security-capability", "stream.remote.preview")
    );
}

#[test]
fn remote_limits_cover_size_duration_rate_and_inflight() {
    let limits = StreamRemoteLimits::default();

    assert_eq!(limits.max_frame_payload_bytes, 1024 * 1024);
    assert_eq!(limits.max_stream_frames, 1024);
    assert_eq!(limits.max_inflight_frames, 64);
    assert_eq!(limits.max_duration_ms, 60_000);
    assert_eq!(limits.max_rate_hz, 120);
    assert_eq!(limits.max_binary_payload_bytes, 256 * 1024);
    assert_eq!(limits.effective_frame_limit(), 1024);
    limits
        .validate_profile(&TransportProfile::remote_stream_fabric())
        .unwrap();
    assert!(
        limits
            .validate_profile(&TransportProfile::realtime_local_audio())
            .is_err()
    );

    let short = StreamRemoteLimits {
        max_stream_frames: 100,
        max_duration_ms: 1,
        max_rate_hz: 1,
        ..limits
    };
    assert_eq!(short.effective_frame_limit(), 1);
}

#[test]
fn public_stream_payload_policy_fails_closed_on_sensitive_content() {
    let policy = StreamSecurityPolicy::default();
    let cases = [
        (
            Expr::String("private-path=session.simcassette".to_owned()),
            StreamRedactionFinding::PrivatePath,
        ),
        (
            Expr::String("C:\\temp\\session.simcassette".to_owned()),
            StreamRedactionFinding::AbsolutePath,
        ),
        (
            Expr::String("https://sim.example/stream".to_owned()),
            StreamRedactionFinding::HostName,
        ),
        (
            Expr::String("token=abc123".to_owned()),
            StreamRedactionFinding::Credential,
        ),
        (
            Expr::String("dx7 patch-bank payload".to_owned()),
            StreamRedactionFinding::PatchBankPayload,
        ),
        (
            Expr::Bytes(vec![0; policy.remote_limits.max_binary_payload_bytes + 1]),
            StreamRedactionFinding::LargeBinaryData,
        ),
    ];

    assert_eq!(stream_redaction_finding_symbols().len(), 6);
    for (expr, finding) in cases {
        assert_eq!(policy.finding_for_expr(&expr), Some(finding));
        assert!(policy.validate_public_expr(&expr).is_err());
    }
    policy
        .validate_public_expr(&Expr::String("stream-visible".to_owned()))
        .unwrap();
}

#[test]
fn cassette_redaction_covers_security_policy_findings() {
    let policy = StreamSecurityPolicy::default();
    let payload = Expr::Map(vec![
        field("credential", Expr::String("token=abc123".to_owned())),
        field("path", Expr::String("private-path=session.mid".to_owned())),
        field("bank", Expr::String("dx7 patch-bank payload".to_owned())),
        field(
            "blob",
            Expr::Bytes(vec![0; policy.remote_limits.max_binary_payload_bytes + 1]),
        ),
    ]);
    let item = StreamItem::new(StreamPacket::data(
        Symbol::qualified("stream/data", "expr"),
        payload,
    ));
    let cassette = StreamCassette::from_items(
        security_metadata(),
        vec![item],
        TransportProfile::remote_stream_fabric(),
        StreamStats::default(),
    )
    .unwrap();

    assert!(
        cassette
            .validate_golden_fixture("fixtures/streams/golden/security.simcassette")
            .is_err()
    );
    let redacted = cassette.redacted().unwrap();
    let report = redacted
        .validate_golden_fixture("fixtures/streams/golden/security.simcassette")
        .unwrap();
    assert_eq!(report.packet_count, 1);
    let items = redacted.items().unwrap();
    assert!(matches!(
        items[0].packet(),
        StreamPacket::Data(data)
            if data.kind == Symbol::qualified("stream/data", "redacted")
    ));
}

#[test]
fn diagnostic_cassette_redaction_covers_security_policy_findings() {
    let item = StreamItem::new(StreamPacket::Diagnostic(crate::StreamDiagnostic::new(
        Symbol::qualified("stream/diagnostic", "credential"),
        "token=abc123".to_owned(),
    )));
    let cassette = StreamCassette::from_items(
        diagnostic_security_metadata(),
        vec![item],
        TransportProfile::remote_stream_fabric(),
        StreamStats::default(),
    )
    .unwrap();

    assert!(
        cassette
            .validate_golden_fixture("fixtures/streams/golden/security-diagnostic.simcassette")
            .is_err()
    );
    let redacted = cassette.redacted().unwrap();
    redacted
        .validate_golden_fixture("fixtures/streams/golden/security-diagnostic.simcassette")
        .unwrap();
    let items = redacted.items().unwrap();
    assert!(matches!(
        items[0].packet(),
        StreamPacket::Diagnostic(diagnostic)
            if diagnostic.message() == "[redacted stream metadata]"
    ));
}

fn security_metadata() -> StreamMetadata {
    StreamMetadata::new(
        Symbol::qualified("stream", "security"),
        StreamMedia::Data,
        StreamDirection::Source,
        Symbol::qualified("clock", "server-frame"),
        BufferPolicy::bounded(2).unwrap(),
    )
}

fn diagnostic_security_metadata() -> StreamMetadata {
    StreamMetadata::new(
        Symbol::qualified("stream", "security-diagnostic"),
        StreamMedia::Diagnostic,
        StreamDirection::Source,
        Symbol::qualified("clock", "server-frame"),
        BufferPolicy::bounded(2).unwrap(),
    )
}

fn field(name: &str, value: Expr) -> (Expr, Expr) {
    (Expr::Symbol(Symbol::new(name)), value)
}

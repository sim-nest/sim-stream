use sim_kernel::Symbol;

use crate::{ClockDomain, LatencyClass, RateContract, StreamCapability, TransportProfile};

#[test]
fn rate_contracts_reuse_stream_clock_and_latency_vocabularies() {
    let audio = RateContract::sample_exact(Some(48_000));
    assert_eq!(audio.clock_domain(), ClockDomain::Sample);
    assert_eq!(audio.latency_class(), LatencyClass::SampleExact);
    assert_eq!(audio.nominal_rate_hz(), Some(48_000));

    let same_domain_without_rate = RateContract::sample_exact(None);
    assert!(audio.is_compatible_with(same_domain_without_rate));

    let control = RateContract::control();
    assert_eq!(control.clock_domain(), ClockDomain::Control);
    assert_eq!(control.latency_class(), LatencyClass::Interactive);
    assert!(audio.ensure_compatible(control).is_err());
}

#[test]
fn stream_capabilities_map_to_latency_classes() {
    assert_eq!(
        StreamCapability::Exact.latency_class(),
        LatencyClass::SampleExact
    );
    assert_eq!(
        StreamCapability::Deterministic.latency_class(),
        LatencyClass::OfflineRender
    );
    assert_eq!(
        StreamCapability::Realtime.latency_class(),
        LatencyClass::SampleExact
    );
    assert_eq!(
        StreamCapability::Bounded.latency_class(),
        LatencyClass::BlockLocal
    );
    assert_eq!(
        StreamCapability::Remote.latency_class(),
        LatencyClass::RemoteCollaboration
    );
    assert_eq!(
        StreamCapability::Replayable.latency_class(),
        LatencyClass::OfflineRender
    );
    assert_eq!(
        StreamCapability::Preview.latency_class(),
        LatencyClass::BufferedPreview
    );
    assert_eq!(
        StreamCapability::Persistent.latency_class(),
        LatencyClass::RemoteCollaboration
    );
    assert_eq!(
        StreamCapability::Resumable.latency_class(),
        LatencyClass::RemoteCollaboration
    );
    assert_eq!(
        StreamCapability::Lossy.latency_class(),
        LatencyClass::BufferedPreview
    );
}

#[test]
fn named_transport_profiles_cover_realtime_audio_and_preview_chunks() {
    let realtime = TransportProfile::realtime_local_audio();
    assert_eq!(
        realtime.name(),
        &Symbol::qualified("stream/profile", "realtime-local-audio")
    );
    assert_eq!(realtime.latency_class(), LatencyClass::SampleExact);
    assert!(realtime.has_capability(StreamCapability::Exact));
    assert!(realtime.has_capability(StreamCapability::Realtime));
    assert!(realtime.has_capability(StreamCapability::Bounded));
    assert!(!realtime.has_capability(StreamCapability::Remote));

    let preview = TransportProfile::buffered_pcm_preview();
    assert_eq!(
        preview.name(),
        &Symbol::qualified("stream/profile", "buffered-pcm-preview")
    );
    assert_eq!(preview.latency_class(), LatencyClass::BufferedPreview);
    assert!(preview.has_capability(StreamCapability::Bounded));
    assert!(preview.has_capability(StreamCapability::Preview));
    assert!(preview.has_capability(StreamCapability::Lossy));

    let remote = TransportProfile::remote_stream_fabric();
    assert_eq!(
        remote.name(),
        &Symbol::qualified("stream/profile", "remote-stream-fabric")
    );
    assert_eq!(remote.latency_class(), LatencyClass::RemoteCollaboration);
    assert!(remote.has_capability(StreamCapability::Remote));
    assert!(remote.has_capability(StreamCapability::Bounded));
    assert!(remote.has_capability(StreamCapability::Replayable));
    assert!(remote.has_capability(StreamCapability::Resumable));
    assert!(!remote.has_capability(StreamCapability::Realtime));

    let lan_midi = TransportProfile::lan_midi_control();
    assert_eq!(
        lan_midi.name(),
        &Symbol::qualified("stream/profile", "lan-midi-control")
    );
    assert_eq!(lan_midi.latency_class(), LatencyClass::Interactive);
    assert!(lan_midi.has_capability(StreamCapability::Remote));
    assert!(lan_midi.has_capability(StreamCapability::Bounded));
    assert!(lan_midi.has_capability(StreamCapability::Replayable));
    assert!(!lan_midi.has_capability(StreamCapability::Realtime));

    let lan_preview = TransportProfile::lan_buffered_audio_preview();
    assert_eq!(
        lan_preview.name(),
        &Symbol::qualified("stream/profile", "lan-buffered-audio-preview")
    );
    assert_eq!(lan_preview.latency_class(), LatencyClass::BufferedPreview);
    assert!(lan_preview.has_capability(StreamCapability::Remote));
    assert!(lan_preview.has_capability(StreamCapability::Bounded));
    assert!(lan_preview.has_capability(StreamCapability::Preview));
    assert!(lan_preview.has_capability(StreamCapability::Lossy));

    let lan_render = TransportProfile::lan_render_return();
    assert_eq!(
        lan_render.name(),
        &Symbol::qualified("stream/profile", "lan-render-return")
    );
    assert_eq!(lan_render.latency_class(), LatencyClass::OfflineRender);
    assert!(lan_render.has_capability(StreamCapability::Remote));
    assert!(lan_render.has_capability(StreamCapability::Bounded));
    assert!(lan_render.has_capability(StreamCapability::Deterministic));
    assert!(lan_render.has_capability(StreamCapability::Replayable));
    assert!(lan_render.has_capability(StreamCapability::Resumable));
    assert!(!lan_render.has_capability(StreamCapability::Realtime));
}

#[test]
fn unsupported_stream_capability_combinations_fail_closed() {
    let exact_lossy = TransportProfile::new(
        Symbol::qualified("stream/profile", "bad"),
        LatencyClass::BufferedPreview,
        vec![StreamCapability::Exact, StreamCapability::Lossy],
    )
    .unwrap_err();
    assert!(format!("{exact_lossy}").contains("exact and lossy"));

    let remote_sample_exact = TransportProfile::new(
        Symbol::qualified("stream/profile", "bad"),
        LatencyClass::SampleExact,
        vec![StreamCapability::Remote],
    )
    .unwrap_err();
    assert!(format!("{remote_sample_exact}").contains("sample-exact"));

    let realtime_remote = TransportProfile::new(
        Symbol::qualified("stream/profile", "bad"),
        LatencyClass::RemoteCollaboration,
        vec![StreamCapability::Realtime],
    )
    .unwrap_err();
    assert!(format!("{realtime_remote}").contains("realtime"));
}

use crate::{
    BridgeLatency, ClockDomain, DomainBridgeDescriptor, DomainBridgeKind, LatencyClass,
    StreamDirection, StreamMedia,
};

#[test]
fn domain_bridges_declare_rate_change_latency_and_diagnostics() {
    let resampler = DomainBridgeDescriptor::resampler(48_000, 96_000).unwrap();

    assert_eq!(resampler.name(), "resampler");
    assert_eq!(resampler.input_rate().nominal_rate_hz(), Some(48_000));
    assert_eq!(resampler.output_rate().nominal_rate_hz(), Some(96_000));
    assert_eq!(resampler.latency(), BridgeLatency::frames(32));
    assert_eq!(
        resampler.diagnostics(),
        &[DomainBridgeKind::Resampler.diagnostic_symbol()]
    );
}

#[test]
fn bridge_edges_use_the_shared_node_port_contract() {
    let gate = DomainBridgeDescriptor::event_rate_gate(ClockDomain::MidiTick).unwrap();
    let input = gate.input_edge(StreamMedia::Midi);
    let output = gate.output_edge(StreamMedia::Midi);

    assert_eq!(input.port().name.as_ref(), "in");
    assert_eq!(input.metadata().direction(), StreamDirection::Sink);
    assert_eq!(input.rate_contract().clock_domain(), ClockDomain::MidiTick);
    assert_eq!(output.port().name.as_ref(), "out");
    assert_eq!(output.metadata().direction(), StreamDirection::Source);
    assert_eq!(output.rate_contract().clock_domain(), ClockDomain::Block);
}

#[test]
fn deterministic_bridge_latency_adds_exactly() {
    let delay = DomainBridgeDescriptor::latency_comp_delay(128);
    let resampler = DomainBridgeDescriptor::resampler(48_000, 48_000).unwrap();
    let total = delay.latency().plus(resampler.latency());

    assert_eq!(total.frame_count(), 160);
    assert_eq!(total.packet_count(), 0);
    assert_eq!(delay.input_rate().latency_class(), LatencyClass::BlockLocal);
}

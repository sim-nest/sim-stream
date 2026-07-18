use sim_kernel::Symbol;
use sim_lib_stream_core::{BridgeLatency, ClockDomain, DomainBridgeKind, LatencyClass};

use super::{Graph, Node, PortRef, sym, test_cx};
use crate::{
    Edge, PlacementNodeProfile, PlacementRefusalReason, SiteMap, SiteProfile, place, place_graph,
};

#[test]
fn all_local_map_places_without_bridges() {
    let mut cx = test_cx();
    let graph = fx_graph();
    let report = place(&mut cx, &graph, &all_audio_site_map()).expect("place graph");

    assert!(report.is_accepted());
    assert_eq!(report.placed.len(), 3);
    assert!(
        report
            .placed
            .iter()
            .all(|node| node.site.as_symbol() == &Symbol::new("audio"))
    );
    assert!(report.bridges.is_empty());
    assert_eq!(report.latency.len(), 1);
    assert_eq!(report.latency[0].node.as_symbol(), &Symbol::new("out"));
    assert_eq!(report.latency[0].latency, BridgeLatency::frames(16));
}

#[test]
fn offloaded_fx_map_inserts_latency_comp_bridges() {
    let mut cx = test_cx();
    let compiled = crate::compile_graph(&mut cx, &fx_graph()).expect("compiled graph");
    let report = place_graph(&compiled, &offloaded_fx_site_map());

    assert!(report.is_accepted());
    assert_eq!(report.bridges.len(), 2);
    assert!(
        report
            .bridges
            .iter()
            .all(|bridge| bridge.kind() == DomainBridgeKind::LatencyCompDelay)
    );
    assert_eq!(report.bridges[0].from.as_symbol(), &Symbol::new("in"));
    assert_eq!(report.bridges[0].to.as_symbol(), &Symbol::new("fx"));
    assert_eq!(report.bridges[1].from.as_symbol(), &Symbol::new("fx"));
    assert_eq!(report.bridges[1].to.as_symbol(), &Symbol::new("out"));
    assert_eq!(report.latency[0].latency, BridgeLatency::frames(64));
}

#[test]
fn illegal_realtime_remote_map_is_refused_as_data() {
    let mut cx = test_cx();
    let graph = fx_graph();
    let report = place(&mut cx, &graph, &illegal_realtime_site_map()).expect("place graph");

    assert!(!report.is_accepted());
    assert!(report.refusals.iter().any(|refusal| {
        refusal.node.as_symbol() == &Symbol::new("fx")
            && refusal.site.as_symbol() == &Symbol::new("worker")
            && refusal.reason == PlacementRefusalReason::RealtimePinViolation
    }));
}

#[test]
fn site_latency_class_mismatch_is_refused_as_data() {
    let mut cx = test_cx();
    let graph = fx_graph();
    let report = place(&mut cx, &graph, &unsupported_latency_site_map()).expect("place graph");

    assert!(!report.is_accepted());
    assert!(report.refusals.iter().any(|refusal| {
        refusal.node.as_symbol() == &Symbol::new("fx")
            && refusal.reason == PlacementRefusalReason::UnsupportedLatencyClass
    }));
}

#[test]
fn incomparable_clock_domain_edge_is_refused_as_data() {
    let mut cx = test_cx();
    let graph = fx_graph();
    let report = place(&mut cx, &graph, &incomparable_clock_site_map()).expect("place graph");

    assert!(!report.is_accepted());
    assert!(report.bridges.is_empty());
    assert!(report.refusals.iter().any(|refusal| {
        refusal.node.as_symbol() == &Symbol::new("fx")
            && refusal.reason
                == PlacementRefusalReason::IncomparableClockDomain {
                    from: ClockDomain::Block,
                    to: ClockDomain::Job,
                }
    }));
}

fn fx_graph() -> Graph {
    let mut graph = Graph::minimal("placement-fx");
    let mut fx = Node::named("fx", "call");
    fx.target = Some(sym("gain"));
    graph.nodes = vec![Node::named("in", "in"), fx, Node::named("out", "out")];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("fx")),
        Edge::new(1, PortRef::output("fx"), PortRef::input("out")),
    ];
    graph
}

fn all_audio_site_map() -> SiteMap {
    SiteMap::new(SiteProfile::audio_clock("audio"))
        .with_node_profile("in", block_profile())
        .with_node_profile(
            "fx",
            block_profile().with_latency(BridgeLatency::frames(16)),
        )
        .with_node_profile("out", block_profile())
}

fn offloaded_fx_site_map() -> SiteMap {
    SiteMap::new(SiteProfile::audio_clock("audio"))
        .with_site(SiteProfile::local_worker("worker"))
        .assign_node("fx", "worker")
        .with_node_profile("in", block_profile())
        .with_node_profile(
            "fx",
            block_profile().with_latency(BridgeLatency::frames(64)),
        )
        .with_node_profile("out", block_profile())
}

fn illegal_realtime_site_map() -> SiteMap {
    SiteMap::new(SiteProfile::audio_clock("audio"))
        .with_site(SiteProfile::local_worker("worker"))
        .assign_node("fx", "worker")
        .with_node_profile("in", block_profile())
        .with_node_profile("fx", PlacementNodeProfile::sample_exact(Some(48_000), true))
        .with_node_profile("out", block_profile())
}

fn unsupported_latency_site_map() -> SiteMap {
    SiteMap::new(SiteProfile::audio_clock("audio"))
        .with_site(SiteProfile::local_worker("worker"))
        .assign_node("fx", "worker")
        .with_node_profile("in", block_profile())
        .with_node_profile(
            "fx",
            PlacementNodeProfile::new(
                sim_lib_stream_core::RateContract::new(
                    sim_lib_stream_core::ClockDomain::Job,
                    LatencyClass::RemoteCollaboration,
                    None,
                ),
                false,
            ),
        )
        .with_node_profile("out", block_profile())
}

fn incomparable_clock_site_map() -> SiteMap {
    SiteMap::new(SiteProfile::audio_clock("audio"))
        .with_node_profile("in", block_profile())
        .with_node_profile(
            "fx",
            PlacementNodeProfile::new(
                sim_lib_stream_core::RateContract::new(
                    ClockDomain::Job,
                    LatencyClass::BlockLocal,
                    None,
                ),
                false,
            ),
        )
        .with_node_profile("out", block_profile())
}

fn block_profile() -> PlacementNodeProfile {
    PlacementNodeProfile::block_local()
}

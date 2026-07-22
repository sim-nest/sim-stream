//! Site placement: assigning topology nodes to sites and bridging clock domains.

#[path = "place/model.rs"]
mod model;

use sim_kernel::{Cx, Result};
use sim_lib_stream_core::{ClockDomain, DomainBridgeDescriptor, RateContract};

use crate::{CompiledGraph, Graph, compile_graph, place_latency};
pub use model::{
    DomainBridge, PlacedNode, PlacementNodeProfile, PlacementRefusal, PlacementRefusalReason,
    PlacementReport, PortLatency, SiteId, SiteMap, SiteProfile,
};

/// Compiles `topology` and places the resulting graph against `sites`.
///
/// This is the place stage of the topology pipeline (see the crate-level
/// documentation): it runs [`compile_graph`] then [`place_graph`].
///
/// # Examples
///
/// ```rust
/// use std::sync::Arc;
///
/// use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};
/// use sim_lib_topology::{SiteMap, SiteProfile, parse_package, place};
///
/// let package = parse_package(
///     "graph:\ntopology flow\nnode in verb=in\nnode out verb=out\nwire in -> out\n",
/// )
/// .unwrap();
///
/// let sites = SiteMap::new(SiteProfile::audio_clock("studio"));
/// let mut cx = Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));
/// let report = place(&mut cx, &package.graph, &sites).unwrap();
///
/// assert!(report.is_accepted());
/// assert_eq!(report.placed.len(), 2);
/// ```
pub fn place(cx: &mut Cx, topology: &Graph, sites: &SiteMap) -> Result<PlacementReport> {
    let compiled = compile_graph(cx, topology)?;
    Ok(place_graph(&compiled, sites))
}

/// Places an already-compiled graph against `sites`, returning the report.
pub fn place_graph(graph: &CompiledGraph, sites: &SiteMap) -> PlacementReport {
    let placed = placed_nodes(graph, sites);
    let refusals = placement_refusals(graph, sites);
    let bridges = domain_bridges(graph, sites);
    let latency = place_latency::latency_budget(graph, sites, &bridges);
    PlacementReport {
        placed,
        bridges,
        latency,
        refusals,
    }
}

fn placed_nodes(graph: &CompiledGraph, sites: &SiteMap) -> Vec<PlacedNode> {
    graph
        .nodes
        .iter()
        .map(|node| {
            let profile = sites.profile_for(&node.id);
            PlacedNode {
                node: node.id.clone(),
                site: sites.site_for(&node.id).clone(),
                clock_domain: profile.rate_contract().clock_domain(),
                latency_class: profile.rate_contract().latency_class(),
                realtime_pin: profile.realtime_pin(),
            }
        })
        .collect()
}

fn placement_refusals(graph: &CompiledGraph, sites: &SiteMap) -> Vec<PlacementRefusal> {
    let mut refusals = Vec::new();
    for node in &graph.nodes {
        let site_id = sites.site_for(&node.id).clone();
        let profile = sites.profile_for(&node.id);
        let Some(site) = sites.site_profile(&site_id) else {
            refusals.push(PlacementRefusal {
                node: node.id.clone(),
                site: site_id,
                reason: PlacementRefusalReason::UnknownSite,
            });
            continue;
        };
        if requires_audio_clock(&profile) && !site.is_audio_clock() {
            refusals.push(PlacementRefusal {
                node: node.id.clone(),
                site: site_id.clone(),
                reason: PlacementRefusalReason::RealtimePinViolation,
            });
        }
        if !site.supports_latency_class(profile.rate_contract().latency_class()) {
            refusals.push(PlacementRefusal {
                node: node.id.clone(),
                site: site_id.clone(),
                reason: PlacementRefusalReason::UnsupportedLatencyClass,
            });
        }
        if !site.supports_clock_domain(profile.rate_contract().clock_domain()) {
            refusals.push(PlacementRefusal {
                node: node.id.clone(),
                site: site_id.clone(),
                reason: PlacementRefusalReason::UnsupportedClockDomain {
                    domain: profile.rate_contract().clock_domain(),
                },
            });
        }
        if node.has_stream_ports && !site.supports_stream_ports() {
            refusals.push(PlacementRefusal {
                node: node.id.clone(),
                site: site_id,
                reason: PlacementRefusalReason::UnsupportedStreamPorts,
            });
        }
    }
    refusals.extend(edge_clock_refusals(graph, sites));
    refusals
}

fn edge_clock_refusals(graph: &CompiledGraph, sites: &SiteMap) -> Vec<PlacementRefusal> {
    graph
        .edges
        .iter()
        .filter_map(|edge| {
            let from_node = &graph.nodes[edge.from_node].id;
            let to_node = &graph.nodes[edge.to_node].id;
            let from_profile = sites.profile_for(from_node);
            let to_profile = sites.profile_for(to_node);
            match bridge_plan(
                from_profile.rate_contract(),
                to_profile.rate_contract(),
                sites.site_for(from_node) != sites.site_for(to_node),
            ) {
                BridgePlan::Refusal(reason) => Some(PlacementRefusal {
                    node: to_node.clone(),
                    site: sites.site_for(to_node).clone(),
                    reason,
                }),
                BridgePlan::Bridge(_) | BridgePlan::None => None,
            }
        })
        .collect()
}

fn domain_bridges(graph: &CompiledGraph, sites: &SiteMap) -> Vec<DomainBridge> {
    graph
        .edges
        .iter()
        .filter_map(|edge| {
            let from_node = &graph.nodes[edge.from_node].id;
            let to_node = &graph.nodes[edge.to_node].id;
            let from_site = sites.site_for(from_node);
            let to_site = sites.site_for(to_node);
            let from_profile = sites.profile_for(from_node);
            let to_profile = sites.profile_for(to_node);
            match bridge_plan(
                from_profile.rate_contract(),
                to_profile.rate_contract(),
                from_site != to_site,
            ) {
                BridgePlan::Bridge(descriptor) => Some(DomainBridge {
                    edge: edge.id,
                    from: from_node.clone(),
                    to: to_node.clone(),
                    from_site: from_site.clone(),
                    to_site: to_site.clone(),
                    descriptor,
                }),
                BridgePlan::Refusal(_) | BridgePlan::None => None,
            }
        })
        .collect()
}

enum BridgePlan {
    Bridge(DomainBridgeDescriptor),
    Refusal(PlacementRefusalReason),
    None,
}

fn bridge_plan(from: RateContract, to: RateContract, crosses_site: bool) -> BridgePlan {
    if !crosses_site && from.is_compatible_with(to) {
        return BridgePlan::None;
    }
    match (from.clock_domain(), to.clock_domain()) {
        (ClockDomain::Sample, ClockDomain::Sample)
            if from.nominal_rate_hz() != to.nominal_rate_hz() =>
        {
            BridgePlan::Bridge(
                DomainBridgeDescriptor::resampler(
                    from.nominal_rate_hz().unwrap_or(1),
                    to.nominal_rate_hz().unwrap_or(1),
                )
                .expect("planner supplies nonzero fallback resampler rates"),
            )
        }
        (ClockDomain::Control | ClockDomain::MidiTick, ClockDomain::Block) => BridgePlan::Bridge(
            DomainBridgeDescriptor::event_rate_gate(from.clock_domain())
                .expect("planner only requests event-rate gates for supported event domains"),
        ),
        (ClockDomain::Wall, _) | (_, ClockDomain::Wall) => {
            BridgePlan::Bridge(DomainBridgeDescriptor::jitter_buffer(1))
        }
        (from, to) if from != to => {
            BridgePlan::Refusal(PlacementRefusalReason::IncomparableClockDomain { from, to })
        }
        _ if crosses_site || !from.is_compatible_with(to) => {
            BridgePlan::Bridge(DomainBridgeDescriptor::latency_comp_delay(0))
        }
        _ => BridgePlan::None,
    }
}

fn requires_audio_clock(profile: &PlacementNodeProfile) -> bool {
    profile.realtime_pin() || profile.rate_contract().clock_domain() == ClockDomain::Sample
}

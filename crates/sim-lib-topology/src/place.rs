//! Site placement: assigning topology nodes to sites and bridging clock domains.

use std::collections::BTreeMap;

use sim_kernel::{Cx, Result, Symbol};
use sim_lib_stream_core::{
    BridgeLatency, ClockDomain, DomainBridgeDescriptor, DomainBridgeKind, LatencyClass,
    RateContract,
};

use crate::{CompiledGraph, EdgeId, Graph, NodeId, compile_graph};

/// Identifier for a placement site: a named host that nodes can be assigned to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SiteId(Symbol);

impl SiteId {
    /// Builds a site id from a name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(Symbol::new(name.into()))
    }

    /// Returns the underlying symbol.
    pub fn as_symbol(&self) -> &Symbol {
        &self.0
    }
}

impl From<&str> for SiteId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for SiteId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Capabilities a site offers: which latency classes it serves and whether it
/// can host the audio (sample) clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteProfile {
    id: SiteId,
    latency_classes: Vec<LatencyClass>,
    audio_clock: bool,
}

impl SiteProfile {
    /// Builds a site profile with an explicit latency-class set and clock flag.
    pub fn new(
        id: impl Into<SiteId>,
        latency_classes: Vec<LatencyClass>,
        audio_clock: bool,
    ) -> Self {
        Self {
            id: id.into(),
            latency_classes,
            audio_clock,
        }
    }

    /// Preset for an audio-clock site that serves sample-exact through render
    /// latency classes.
    pub fn audio_clock(id: impl Into<SiteId>) -> Self {
        Self::new(
            id,
            vec![
                LatencyClass::SampleExact,
                LatencyClass::BlockLocal,
                LatencyClass::Interactive,
                LatencyClass::OfflineRender,
            ],
            true,
        )
    }

    /// Preset for a local worker site: block-local through render, no audio clock.
    pub fn local_worker(id: impl Into<SiteId>) -> Self {
        Self::new(
            id,
            vec![
                LatencyClass::BlockLocal,
                LatencyClass::Interactive,
                LatencyClass::BufferedPreview,
                LatencyClass::OfflineRender,
            ],
            false,
        )
    }

    /// Preset for a buffered remote site: preview and collaboration latency
    /// classes, no audio clock.
    pub fn buffered_remote(id: impl Into<SiteId>) -> Self {
        Self::new(
            id,
            vec![
                LatencyClass::BufferedPreview,
                LatencyClass::CollabBarDelay,
                LatencyClass::RemoteCollaboration,
                LatencyClass::OfflineRender,
            ],
            false,
        )
    }

    /// Returns the site id.
    pub fn id(&self) -> &SiteId {
        &self.id
    }

    /// Reports whether this site serves the given latency class.
    pub fn supports_latency_class(&self, latency_class: LatencyClass) -> bool {
        self.latency_classes.contains(&latency_class)
    }

    /// Reports whether this site can host the audio (sample) clock.
    pub fn is_audio_clock(&self) -> bool {
        self.audio_clock
    }
}

/// Per-node placement requirements: rate contract, real-time pin, and the
/// node's own latency contribution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacementNodeProfile {
    rate_contract: RateContract,
    realtime_pin: bool,
    latency: BridgeLatency,
}

impl PlacementNodeProfile {
    /// Builds a node profile from a rate contract and real-time pin flag.
    pub fn new(rate_contract: RateContract, realtime_pin: bool) -> Self {
        Self {
            rate_contract,
            realtime_pin,
            latency: BridgeLatency::zero(),
        }
    }

    /// Preset for a sample-exact node at an optional nominal rate.
    pub fn sample_exact(nominal_rate_hz: Option<u32>, realtime_pin: bool) -> Self {
        Self::new(RateContract::sample_exact(nominal_rate_hz), realtime_pin)
    }

    /// Preset for a block-local node.
    pub fn block_local() -> Self {
        Self::new(RateContract::block_local(), false)
    }

    /// Preset for a control-rate node.
    pub fn control() -> Self {
        Self::new(RateContract::control(), false)
    }

    /// Sets the node's own latency contribution, returning the updated profile.
    pub fn with_latency(mut self, latency: BridgeLatency) -> Self {
        self.latency = latency;
        self
    }

    /// Returns the node's rate contract.
    pub fn rate_contract(&self) -> RateContract {
        self.rate_contract
    }

    /// Reports whether the node is pinned to a real-time clock.
    pub fn realtime_pin(&self) -> bool {
        self.realtime_pin
    }

    /// Returns the node's own latency contribution.
    pub fn latency(&self) -> BridgeLatency {
        self.latency
    }
}

impl Default for PlacementNodeProfile {
    fn default() -> Self {
        Self::block_local()
    }
}

/// Placement input: the known sites, per-node site assignments, and per-node
/// profiles that [`place`] resolves against a graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteMap {
    default_site: SiteId,
    sites: BTreeMap<SiteId, SiteProfile>,
    assignments: BTreeMap<NodeId, SiteId>,
    node_profiles: BTreeMap<NodeId, PlacementNodeProfile>,
}

impl SiteMap {
    /// Builds a site map whose default (fallback) site is the given profile.
    pub fn new(default_site: SiteProfile) -> Self {
        let default_site_id = default_site.id().clone();
        let mut sites = BTreeMap::new();
        sites.insert(default_site_id.clone(), default_site);
        Self {
            default_site: default_site_id,
            sites,
            assignments: BTreeMap::new(),
            node_profiles: BTreeMap::new(),
        }
    }

    /// Registers another site, returning the updated map.
    pub fn with_site(mut self, site: SiteProfile) -> Self {
        self.sites.insert(site.id().clone(), site);
        self
    }

    /// Assigns a node to a site, returning the updated map.
    pub fn assign_node(mut self, node: impl Into<NodeId>, site: impl Into<SiteId>) -> Self {
        self.assignments.insert(node.into(), site.into());
        self
    }

    /// Sets a node's placement profile, returning the updated map.
    pub fn with_node_profile(
        mut self,
        node: impl Into<NodeId>,
        profile: PlacementNodeProfile,
    ) -> Self {
        self.node_profiles.insert(node.into(), profile);
        self
    }

    /// Returns the site a node is assigned to, falling back to the default site.
    pub fn site_for(&self, node: &NodeId) -> &SiteId {
        self.assignments.get(node).unwrap_or(&self.default_site)
    }

    /// Returns a node's placement profile, defaulting to block-local.
    pub fn profile_for(&self, node: &NodeId) -> PlacementNodeProfile {
        self.node_profiles.get(node).cloned().unwrap_or_default()
    }

    /// Looks up a registered site profile by id.
    pub fn site_profile(&self, site: &SiteId) -> Option<&SiteProfile> {
        self.sites.get(site)
    }
}

/// Outcome of placing a graph: where nodes landed, the clock-domain bridges
/// inserted across edges, the resulting latency budget, and any refusals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacementReport {
    /// The site and clock assignment computed for each node.
    pub placed: Vec<PlacedNode>,
    /// Bridges inserted on edges that cross a clock domain or site boundary.
    pub bridges: Vec<DomainBridge>,
    /// Accumulated latency reaching each output node.
    pub latency: Vec<PortLatency>,
    /// Placements that could not be satisfied.
    pub refusals: Vec<PlacementRefusal>,
}

impl PlacementReport {
    /// Reports whether placement succeeded with no refusals.
    pub fn is_accepted(&self) -> bool {
        self.refusals.is_empty()
    }
}

/// A node's resolved placement: its site, clock domain, and latency class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedNode {
    /// The placed node.
    pub node: NodeId,
    /// The site the node was assigned to.
    pub site: SiteId,
    /// The node's resolved clock domain.
    pub clock_domain: ClockDomain,
    /// The node's resolved latency class.
    pub latency_class: LatencyClass,
    /// Whether the node is pinned to a real-time clock.
    pub realtime_pin: bool,
}

/// A clock-domain bridge inserted on an edge that crosses domains or sites.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainBridge {
    /// The bridged edge.
    pub edge: EdgeId,
    /// The source node.
    pub from: NodeId,
    /// The destination node.
    pub to: NodeId,
    /// The source node's site.
    pub from_site: SiteId,
    /// The destination node's site.
    pub to_site: SiteId,
    /// The bridge descriptor (resampler, gate, jitter buffer, ...).
    pub descriptor: DomainBridgeDescriptor,
}

/// Accumulated latency reaching one output node under a placement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortLatency {
    /// The output node.
    pub node: NodeId,
    /// The node's site.
    pub site: SiteId,
    /// The accumulated latency along the worst path reaching the node.
    pub latency: BridgeLatency,
    /// The node's latency class.
    pub latency_class: LatencyClass,
}

/// A placement that could not be satisfied at the assigned site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacementRefusal {
    /// The refused node.
    pub node: NodeId,
    /// The site the node was assigned to.
    pub site: SiteId,
    /// Why the placement was refused.
    pub reason: PlacementRefusalReason,
}

/// Why a node could not be placed at its assigned site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlacementRefusalReason {
    /// The assigned site is not registered in the site map.
    UnknownSite,
    /// A real-time-pinned node was assigned to a site without an audio clock.
    RealtimePinViolation,
    /// The site does not serve the node's latency class.
    UnsupportedLatencyClass,
    /// The edge crosses clock domains that have no semantic bridge.
    IncomparableClockDomain {
        /// The source node's clock domain.
        from: ClockDomain,
        /// The destination node's clock domain.
        to: ClockDomain,
    },
}

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
    let latency = latency_budget(graph, sites, &bridges);
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
                site: site_id,
                reason: PlacementRefusalReason::UnsupportedLatencyClass,
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

fn latency_budget(
    graph: &CompiledGraph,
    sites: &SiteMap,
    bridges: &[DomainBridge],
) -> Vec<PortLatency> {
    let bridge_latency = bridges
        .iter()
        .map(|bridge| (bridge.edge, bridge.descriptor.latency()))
        .collect::<BTreeMap<_, _>>();
    let mut budgets = vec![BridgeLatency::zero(); graph.nodes.len()];

    for node_index in 0..graph.nodes.len() {
        let node = &graph.nodes[node_index];
        budgets[node_index] = budgets[node_index].plus(sites.profile_for(&node.id).latency());
        for edge_index in &graph.outgoing_edges[node_index] {
            let edge = &graph.edges[*edge_index];
            let candidate = budgets[node_index].plus(
                *bridge_latency
                    .get(&edge.id)
                    .unwrap_or(&BridgeLatency::zero()),
            );
            let target = &mut budgets[edge.to_node];
            *target = max_latency(*target, candidate);
        }
    }

    graph
        .output_nodes
        .iter()
        .map(|node_index| {
            let node = &graph.nodes[*node_index];
            let profile = sites.profile_for(&node.id);
            PortLatency {
                node: node.id.clone(),
                site: sites.site_for(&node.id).clone(),
                latency: budgets[*node_index],
                latency_class: profile.rate_contract().latency_class(),
            }
        })
        .collect()
}

fn requires_audio_clock(profile: &PlacementNodeProfile) -> bool {
    profile.realtime_pin() || profile.rate_contract().clock_domain() == ClockDomain::Sample
}

fn max_latency(left: BridgeLatency, right: BridgeLatency) -> BridgeLatency {
    BridgeLatency::frames_and_packets(
        left.frame_count().max(right.frame_count()),
        left.packet_count().max(right.packet_count()),
    )
}

impl DomainBridge {
    /// Returns the bridge kind from its descriptor.
    pub fn kind(&self) -> DomainBridgeKind {
        self.descriptor.kind()
    }
}

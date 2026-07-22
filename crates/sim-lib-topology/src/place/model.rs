use std::collections::BTreeMap;

use sim_kernel::{Error, Export, Result, Symbol};
use sim_lib_stream_core::{
    BridgeLatency, ClockDomain, DomainBridgeDescriptor, DomainBridgeKind, LatencyClass,
    RateContract,
};

use crate::{EdgeId, NodeId};

/// Identifier for a placement site: a named host that nodes can be assigned to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SiteId(Symbol);

impl SiteId {
    /// Builds a site id from a name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(Symbol::new(name.into()))
    }

    /// Builds a site id from a runtime export symbol.
    pub fn from_symbol(symbol: Symbol) -> Self {
        Self(symbol)
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
    export: Export,
    latency_classes: Vec<LatencyClass>,
    clock_domains: Vec<ClockDomain>,
    audio_clock: bool,
    stream_ports: bool,
}

impl SiteProfile {
    /// Builds a site profile with an explicit latency-class set and clock flag.
    pub fn new(
        id: impl Into<SiteId>,
        latency_classes: Vec<LatencyClass>,
        audio_clock: bool,
    ) -> Self {
        let id = id.into();
        let export = Export::Site {
            symbol: id.as_symbol().clone(),
            runtime_id: None,
        };
        Self {
            id,
            export,
            clock_domains: default_clock_domains(audio_clock),
            latency_classes,
            audio_clock,
            stream_ports: true,
        }
    }

    /// Builds a placement profile from a kernel runtime site export and
    /// topology site claims.
    pub fn from_site_export(
        export: Export,
        latency_classes: Vec<LatencyClass>,
        audio_clock: bool,
    ) -> Result<Self> {
        let Export::Site { symbol, .. } = &export else {
            return Err(Error::Eval(
                "topology placement site requires a kernel site export".to_owned(),
            ));
        };
        Ok(Self {
            id: SiteId::from_symbol(symbol.clone()),
            export,
            clock_domains: default_clock_domains(audio_clock),
            latency_classes,
            audio_clock,
            stream_ports: true,
        })
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

    /// Returns the kernel site export this placement profile describes.
    pub fn site_export(&self) -> &Export {
        &self.export
    }

    /// Reports whether this site serves the given latency class.
    pub fn supports_latency_class(&self, latency_class: LatencyClass) -> bool {
        self.latency_classes.contains(&latency_class)
    }

    /// Sets the clock domains represented by this runtime site contract.
    pub fn with_clock_domains(mut self, clock_domains: Vec<ClockDomain>) -> Self {
        self.clock_domains = clock_domains;
        self
    }

    /// Reports whether this site represents the given clock domain.
    pub fn supports_clock_domain(&self, clock_domain: ClockDomain) -> bool {
        self.clock_domains.contains(&clock_domain)
    }

    /// Sets whether this site can host stream-mode topology ports.
    pub fn with_stream_ports(mut self, stream_ports: bool) -> Self {
        self.stream_ports = stream_ports;
        self
    }

    /// Reports whether this site can host stream-mode topology ports.
    pub fn supports_stream_ports(&self) -> bool {
        self.stream_ports
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
/// profiles that `place` resolves against a graph.
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

impl DomainBridge {
    /// Returns the bridge kind from its descriptor.
    pub fn kind(&self) -> DomainBridgeKind {
        self.descriptor.kind()
    }
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
    /// The site does not represent the node's clock domain.
    UnsupportedClockDomain {
        /// The requested clock domain.
        domain: ClockDomain,
    },
    /// The site does not claim support for stream-mode topology ports.
    UnsupportedStreamPorts,
    /// The edge crosses clock domains that have no semantic bridge.
    IncomparableClockDomain {
        /// The source node's clock domain.
        from: ClockDomain,
        /// The destination node's clock domain.
        to: ClockDomain,
    },
}

fn default_clock_domains(audio_clock: bool) -> Vec<ClockDomain> {
    let mut domains = vec![
        ClockDomain::Block,
        ClockDomain::Control,
        ClockDomain::MidiTick,
        ClockDomain::Wall,
        ClockDomain::Job,
    ];
    if audio_clock {
        domains.insert(0, ClockDomain::Sample);
    }
    domains
}

//! Stream placement -- where stream fragments live and how they are wired.
//!
//! This module describes the placement surface of the streaming fabric: a
//! [`StreamEndpoint`] is a site that produces, consumes, or routes a stream; a
//! [`StreamEdge`] is a typed, rate-contracted port carrying envelopes between
//! sites; and a [`PlacedFragment`] is a graph node bound to its input and
//! output edges. The kernel supplies the protocol types (clock domains, rate
//! contracts, [`Symbol`], [`Expr`]); this module supplies the concrete
//! placement and routing behavior.

use sim_kernel::{Error, Expr, Result, Symbol};

use crate::{
    ClockDomain, LatencyClass, RateContract, StreamDirection, StreamEnvelope, StreamItem,
    StreamMedia, StreamMetadata, StreamPacket, TransportProfile,
};

/// Role a [`StreamEndpoint`] plays at a placement site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamEndpointKind {
    /// Site that originates stream packets.
    Producer,
    /// Site that terminates a stream by consuming packets.
    Consumer,
    /// Site that forwards a stream between two other sites.
    Bridge,
    /// Site that observes a stream without altering its flow.
    Inspector,
    /// Site that evaluates over the stream as a distributed eval target.
    EvalSite,
}

impl StreamEndpointKind {
    /// Returns the stable wire label for this kind.
    pub fn wire_label(self) -> &'static str {
        match self {
            Self::Producer => "producer",
            Self::Consumer => "consumer",
            Self::Bridge => "bridge",
            Self::Inspector => "inspector",
            Self::EvalSite => "eval-site",
        }
    }

    /// Returns this kind as a qualified `stream/endpoint-kind` symbol.
    pub fn symbol(self) -> Symbol {
        Symbol::qualified("stream/endpoint-kind", self.wire_label())
    }
}

/// A typed, rate-contracted port carrying envelopes between sites.
///
/// An edge names its port, the [`RateContract`] (and thus clock domain) it must
/// honor, the stream [`StreamMetadata`] flowing across it, and any envelopes
/// already buffered on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamEdge {
    port: Symbol,
    rate_contract: RateContract,
    metadata: StreamMetadata,
    envelopes: Vec<StreamEnvelope>,
}

impl StreamEdge {
    /// Creates an empty edge for `port` under the given rate contract and
    /// metadata.
    pub fn new(port: Symbol, rate_contract: RateContract, metadata: StreamMetadata) -> Self {
        Self {
            port,
            rate_contract,
            metadata,
            envelopes: Vec::new(),
        }
    }

    /// Returns the edge with its buffered envelopes replaced.
    pub fn with_envelopes(mut self, envelopes: Vec<StreamEnvelope>) -> Self {
        self.envelopes = envelopes;
        self
    }

    /// Returns the port symbol naming this edge.
    pub fn port(&self) -> &Symbol {
        &self.port
    }

    /// Returns the rate contract this edge must honor.
    pub fn rate_contract(&self) -> RateContract {
        self.rate_contract
    }

    /// Returns the metadata of the stream flowing across this edge.
    pub fn metadata(&self) -> &StreamMetadata {
        &self.metadata
    }

    /// Returns the envelopes currently buffered on this edge.
    pub fn envelopes(&self) -> &[StreamEnvelope] {
        &self.envelopes
    }

    /// Builds a `site-result` data envelope for this edge.
    ///
    /// Wraps `payload` in a `stream/data` packet sequenced at `sequence` under
    /// a memory-local transport profile, using this edge's metadata.
    pub fn result_envelope(&self, sequence: u64, payload: Expr) -> Result<StreamEnvelope> {
        let item = StreamItem::new(StreamPacket::data(
            Symbol::qualified("stream/data", "site-result"),
            payload,
        ));
        StreamEnvelope::from_item_with_profile(
            &self.metadata,
            sequence,
            &item,
            TransportProfile::memory_local(),
        )
    }
}

/// A graph node placed at a site with its wired input and output edges.
///
/// Identifies the fragment, holds the [`Expr`] node it evaluates, and tracks
/// the [`StreamEdge`]s feeding into and out of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedFragment {
    id: Symbol,
    node: Expr,
    input_edges: Vec<StreamEdge>,
    output_edges: Vec<StreamEdge>,
}

impl PlacedFragment {
    /// Creates a fragment for `id` evaluating `node`, with no edges yet.
    pub fn new(id: Symbol, node: Expr) -> Self {
        Self {
            id,
            node,
            input_edges: Vec::new(),
            output_edges: Vec::new(),
        }
    }

    /// Returns the fragment with `edge` appended to its input edges.
    pub fn with_input_edge(mut self, edge: StreamEdge) -> Self {
        self.input_edges.push(edge);
        self
    }

    /// Returns the fragment with `edge` appended to its output edges.
    pub fn with_output_edge(mut self, edge: StreamEdge) -> Self {
        self.output_edges.push(edge);
        self
    }

    /// Returns the fragment identifier.
    pub fn id(&self) -> &Symbol {
        &self.id
    }

    /// Returns the [`Expr`] node this fragment evaluates.
    pub fn node(&self) -> &Expr {
        &self.node
    }

    /// Returns the fragment's input edges.
    pub fn input_edges(&self) -> &[StreamEdge] {
        &self.input_edges
    }

    /// Returns the fragment's output edges.
    pub fn output_edges(&self) -> &[StreamEdge] {
        &self.output_edges
    }

    /// Collects the envelopes buffered across every output edge.
    pub fn output_envelopes(&self) -> Vec<StreamEnvelope> {
        self.output_edges
            .iter()
            .flat_map(|edge| edge.envelopes().iter().cloned())
            .collect()
    }
}

/// A site that produces, consumes, or routes a stream.
///
/// An endpoint declares its identity, [kind](StreamEndpointKind), clock domain,
/// and latency class, and accepts input edges whose rate contracts must agree
/// with its clock domain. Implementors supply the concrete placement behavior;
/// the default methods enforce the clock-domain contract and surface a
/// fragment's output envelopes.
pub trait StreamEndpoint: Send + Sync {
    /// Returns the endpoint's stable identifier.
    fn endpoint_id(&self) -> Symbol;
    /// Returns the role this endpoint plays.
    fn endpoint_kind(&self) -> StreamEndpointKind;
    /// Returns the clock domain this endpoint runs in.
    fn clock_domain(&self) -> ClockDomain;
    /// Returns the latency class this endpoint targets.
    fn latency_class(&self) -> LatencyClass;

    /// Validates that each input edge's clock domain matches this endpoint.
    ///
    /// Returns an error naming the first edge whose rate-contract clock domain
    /// differs from [`clock_domain`](StreamEndpoint::clock_domain).
    fn accept_input_edges(&self, edges: &[StreamEdge]) -> Result<()> {
        for edge in edges {
            if edge.rate_contract().clock_domain() != self.clock_domain() {
                return Err(Error::Eval(format!(
                    "stream edge {} clock domain {} does not match endpoint {}",
                    edge.port(),
                    edge.rate_contract().clock_domain().wire_label(),
                    self.clock_domain().wire_label()
                )));
            }
        }
        Ok(())
    }

    /// Returns the envelopes this endpoint emits for `fragment`.
    ///
    /// Defaults to the fragment's own output envelopes; endpoints that
    /// transform the stream override this.
    fn output_envelopes(&self, fragment: &PlacedFragment) -> Result<Vec<StreamEnvelope>> {
        Ok(fragment.output_envelopes())
    }
}

/// Builds a [`StreamEdge`] for `port` from media, direction, and rate contract.
///
/// Synthesizes a `stream/edge` metadata record with a bounded single-slot
/// buffer and the clock domain drawn from `rate_contract`.
pub fn stream_edge(
    port: impl Into<String>,
    media: StreamMedia,
    direction: StreamDirection,
    rate_contract: RateContract,
) -> StreamEdge {
    let port = Symbol::new(port.into());
    let metadata = StreamMetadata::new(
        Symbol::qualified("stream/edge", port.name.to_string()),
        media,
        direction,
        rate_contract.clock_domain().symbol(),
        crate::BufferPolicy::bounded(1).expect("stream edge helper uses a nonzero buffer"),
    );
    StreamEdge::new(port, rate_contract, metadata)
}

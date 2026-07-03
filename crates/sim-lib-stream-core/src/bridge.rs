//! Domain-bridge descriptors for streams that cross clock domains.
//!
//! A domain bridge sits between two stream segments whose rate contracts
//! disagree -- a different clock domain, sample rate, or latency class -- and
//! describes how the fabric reconciles them and what latency the crossing
//! costs. [`DomainBridgeKind`] names the reconciliation strategy,
//! [`BridgeLatency`] measures the cost in frames and packets, and
//! [`DomainBridgeDescriptor`] binds a strategy to its input/output
//! [`RateContract`]s, latency, and diagnostic symbols, and can project the
//! input/output [`StreamEdge`]s the bridge presents to the graph.
//!
//! These are descriptive contract values: they record what a bridge promises,
//! while the concrete resampling/buffering behavior lives in higher sim-stream
//! crates.

use sim_kernel::{Error, Result, Symbol};

use crate::{
    BufferPolicy, ClockDomain, LatencyClass, RateContract, StreamDirection, StreamEdge,
    StreamMedia, StreamMetadata,
};

/// Reconciliation strategy a domain bridge uses to cross between rate contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainBridgeKind {
    /// Converts between two exact sample rates by resampling.
    Resampler,
    /// Absorbs arrival jitter by holding late packets in a buffer.
    JitterBuffer,
    /// Inserts a fixed frame delay to compensate for downstream latency.
    LatencyCompDelay,
    /// Gates an event-rate (control or MIDI-tick) input into a block-local rate.
    EventRateGate,
}

impl DomainBridgeKind {
    /// Returns the stable wire label for this bridge kind.
    pub fn name(self) -> &'static str {
        match self {
            Self::Resampler => "resampler",
            Self::JitterBuffer => "jitter-buffer",
            Self::LatencyCompDelay => "latency-comp-delay",
            Self::EventRateGate => "event-rate-gate",
        }
    }

    /// Returns the `stream/bridge/<name>` symbol identifying this bridge kind.
    pub fn symbol(self) -> Symbol {
        Symbol::qualified("stream/bridge", self.name())
    }

    /// Returns the `stream/bridge-diagnostic/<name>` symbol used to tag this
    /// bridge kind's diagnostics.
    pub fn diagnostic_symbol(self) -> Symbol {
        Symbol::qualified("stream/bridge-diagnostic", self.name())
    }
}

/// Latency a bridge incurs, measured in frames and packets.
///
/// The two axes are independent: frame latency reflects fixed sample/block
/// delay, while packet latency reflects how many in-flight packets the bridge
/// may hold (for example a jitter buffer's late-packet window).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BridgeLatency {
    frames: u64,
    packets: u32,
}

impl BridgeLatency {
    /// Returns a latency of zero frames and zero packets.
    pub fn zero() -> Self {
        Self {
            frames: 0,
            packets: 0,
        }
    }

    /// Returns a latency of `frames` frames and zero packets.
    pub fn frames(frames: u64) -> Self {
        Self { frames, packets: 0 }
    }

    /// Returns a latency of `packets` packets and zero frames.
    pub fn packets(packets: u32) -> Self {
        Self { frames: 0, packets }
    }

    /// Returns a latency with both a frame and a packet component.
    pub fn frames_and_packets(frames: u64, packets: u32) -> Self {
        Self { frames, packets }
    }

    /// Returns the frame component of this latency.
    pub fn frame_count(self) -> u64 {
        self.frames
    }

    /// Returns the packet component of this latency.
    pub fn packet_count(self) -> u32 {
        self.packets
    }

    /// Returns the component-wise saturating sum of two latencies.
    pub fn plus(self, other: Self) -> Self {
        Self {
            frames: self.frames.saturating_add(other.frames),
            packets: self.packets.saturating_add(other.packets),
        }
    }
}

/// Full description of one domain bridge: its kind, the input and output rate
/// contracts it joins, the latency it costs, and its diagnostic symbols.
///
/// The constructor helpers ([`resampler`](Self::resampler),
/// [`jitter_buffer`](Self::jitter_buffer),
/// [`latency_comp_delay`](Self::latency_comp_delay),
/// [`event_rate_gate`](Self::event_rate_gate)) build the canonical descriptor
/// for each [`DomainBridgeKind`] with its standard rates and latency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainBridgeDescriptor {
    kind: DomainBridgeKind,
    input_rate: RateContract,
    output_rate: RateContract,
    latency: BridgeLatency,
    diagnostics: Vec<Symbol>,
}

impl DomainBridgeDescriptor {
    /// Builds a descriptor from explicit kind, input/output rates, latency, and
    /// diagnostics.
    pub fn new(
        kind: DomainBridgeKind,
        input_rate: RateContract,
        output_rate: RateContract,
        latency: BridgeLatency,
        diagnostics: Vec<Symbol>,
    ) -> Self {
        Self {
            kind,
            input_rate,
            output_rate,
            latency,
            diagnostics,
        }
    }

    /// Builds a resampler bridge between two exact sample rates.
    ///
    /// Returns an error if either rate is zero.
    pub fn resampler(input_hz: u32, output_hz: u32) -> Result<Self> {
        if input_hz == 0 || output_hz == 0 {
            return Err(Error::Eval(
                "resampler rates must be greater than zero".to_owned(),
            ));
        }
        Ok(Self::new(
            DomainBridgeKind::Resampler,
            RateContract::sample_exact(Some(input_hz)),
            RateContract::sample_exact(Some(output_hz)),
            BridgeLatency::frames(32),
            vec![DomainBridgeKind::Resampler.diagnostic_symbol()],
        ))
    }

    /// Builds a jitter-buffer bridge that tolerates up to `max_late_packets`
    /// late packets of wall-clock buffered-preview input.
    pub fn jitter_buffer(max_late_packets: u32) -> Self {
        Self::new(
            DomainBridgeKind::JitterBuffer,
            RateContract::new(ClockDomain::Wall, LatencyClass::BufferedPreview, None),
            RateContract::new(ClockDomain::Wall, LatencyClass::BufferedPreview, None),
            BridgeLatency::packets(max_late_packets),
            vec![DomainBridgeKind::JitterBuffer.diagnostic_symbol()],
        )
    }

    /// Builds a latency-compensation bridge that delays a block-local stream by
    /// a fixed number of frames.
    pub fn latency_comp_delay(frames: u64) -> Self {
        Self::new(
            DomainBridgeKind::LatencyCompDelay,
            RateContract::block_local(),
            RateContract::block_local(),
            BridgeLatency::frames(frames),
            vec![DomainBridgeKind::LatencyCompDelay.diagnostic_symbol()],
        )
    }

    /// Builds an event-rate-gate bridge that gates a control or MIDI-tick input
    /// domain into a block-local output.
    ///
    /// Returns an error if `input_domain` is neither
    /// [`ClockDomain::Control`] nor [`ClockDomain::MidiTick`].
    pub fn event_rate_gate(input_domain: ClockDomain) -> Result<Self> {
        let input_rate = match input_domain {
            ClockDomain::Control => RateContract::control(),
            ClockDomain::MidiTick => RateContract::midi_tick(),
            other => {
                return Err(Error::Eval(format!(
                    "event-rate-gate cannot accept {} input",
                    other.wire_label()
                )));
            }
        };
        Ok(Self::new(
            DomainBridgeKind::EventRateGate,
            input_rate,
            RateContract::block_local(),
            BridgeLatency::zero(),
            vec![DomainBridgeKind::EventRateGate.diagnostic_symbol()],
        ))
    }

    /// Returns the bridge kind.
    pub fn kind(&self) -> DomainBridgeKind {
        self.kind
    }

    /// Returns the bridge kind's wire label.
    pub fn name(&self) -> &'static str {
        self.kind.name()
    }

    /// Returns the rate contract this bridge accepts on its input.
    pub fn input_rate(&self) -> RateContract {
        self.input_rate
    }

    /// Returns the rate contract this bridge emits on its output.
    pub fn output_rate(&self) -> RateContract {
        self.output_rate
    }

    /// Returns the latency this bridge incurs.
    pub fn latency(&self) -> BridgeLatency {
        self.latency
    }

    /// Returns the diagnostic symbols this bridge may raise.
    pub fn diagnostics(&self) -> &[Symbol] {
        &self.diagnostics
    }

    /// Builds the input [`StreamEdge`] this bridge presents as a sink for the
    /// given media.
    pub fn input_edge(&self, media: StreamMedia) -> StreamEdge {
        StreamEdge::new(
            Symbol::new("in"),
            self.input_rate,
            bridge_metadata(
                self.kind,
                "in",
                media,
                StreamDirection::Sink,
                self.input_rate,
            ),
        )
    }

    /// Builds the output [`StreamEdge`] this bridge presents as a source for the
    /// given media.
    pub fn output_edge(&self, media: StreamMedia) -> StreamEdge {
        StreamEdge::new(
            Symbol::new("out"),
            self.output_rate,
            bridge_metadata(
                self.kind,
                "out",
                media,
                StreamDirection::Source,
                self.output_rate,
            ),
        )
    }
}

fn bridge_metadata(
    kind: DomainBridgeKind,
    port: &str,
    media: StreamMedia,
    direction: StreamDirection,
    rate: RateContract,
) -> StreamMetadata {
    StreamMetadata::new(
        Symbol::qualified("stream/bridge-edge", format!("{}-{port}", kind.name())),
        media,
        direction,
        rate.clock_domain().symbol(),
        BufferPolicy::bounded(1).expect("bridge metadata uses a nonzero buffer"),
    )
}

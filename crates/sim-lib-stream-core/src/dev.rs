//! Development-event media and cassettes for the SIM Atelier.

use sim_kernel::{Error, Expr, Result, Symbol, Tick};

use crate::{
    BufferPolicy, ClockDomain, LatencyClass, StreamCapability, StreamCassette, StreamDirection,
    StreamEnvelope, StreamFaultKind, StreamFaultPlan, StreamItem, StreamMedia, StreamMetadata,
    StreamPacket, StreamStats, TransportProfile,
};

/// Descriptor for a stream media family carried by [`StreamEnvelope`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaDescriptor {
    symbol: Symbol,
    stream_media: StreamMedia,
}

impl MediaDescriptor {
    /// Builds a descriptor from a stable symbolic media name.
    pub fn named(name: impl AsRef<str>) -> Result<Self> {
        let name = name.as_ref();
        if let Some(kind) = name.strip_prefix("ide/event/") {
            return dev_event_media(kind);
        }
        let symbol = match name {
            "stream/media/pcm" => StreamMedia::Pcm.symbol(),
            "stream/media/midi" => StreamMedia::Midi.symbol(),
            "stream/media/diagnostic" => StreamMedia::Diagnostic.symbol(),
            "stream/media/data" => StreamMedia::Data.symbol(),
            other => {
                return Err(Error::Eval(format!(
                    "unsupported stream media descriptor {other}"
                )));
            }
        };
        let stream_media = StreamMedia::from_symbol(&symbol)?;
        Ok(Self {
            symbol,
            stream_media,
        })
    }

    /// Returns the descriptor symbol.
    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }

    /// Returns the envelope media carrying this descriptor.
    pub fn stream_media(&self) -> StreamMedia {
        self.stream_media
    }
}

/// Returns the descriptor for an `ide/event/<kind>` development event.
pub fn dev_event_media(kind: &str) -> Result<MediaDescriptor> {
    validate_dev_event_kind(kind)?;
    Ok(MediaDescriptor {
        symbol: Symbol::qualified("ide/event", kind),
        stream_media: StreamMedia::Data,
    })
}

/// Creates a stream metadata record for development events.
pub fn dev_event_metadata(stream_id: Symbol) -> Result<StreamMetadata> {
    Ok(StreamMetadata::new(
        stream_id,
        StreamMedia::Data,
        StreamDirection::Source,
        ClockDomain::ServerFrame.symbol(),
        BufferPolicy::bounded(128)?,
    ))
}

/// One development event before it is wrapped in a [`StreamEnvelope`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevEvent {
    kind: String,
    atelier_node: Symbol,
    latency_class: LatencyClass,
    payload: Expr,
    ticks: Vec<Tick>,
}

impl DevEvent {
    /// Builds a development event with an explicit latency class.
    pub fn new(
        kind: impl Into<String>,
        atelier_node: Symbol,
        latency_class: LatencyClass,
        payload: Expr,
    ) -> Result<Self> {
        let kind = kind.into();
        validate_dev_event_kind(&kind)?;
        Ok(Self {
            kind,
            atelier_node,
            latency_class,
            payload,
            ticks: Vec::new(),
        })
    }

    /// Builds an interactive edit event.
    pub fn edit(atelier_node: Symbol, payload: Expr) -> Result<Self> {
        Self::new("edit", atelier_node, LatencyClass::Interactive, payload)
    }

    /// Builds an offline validation event.
    pub fn validate(atelier_node: Symbol, payload: Expr) -> Result<Self> {
        Self::new(
            "validate",
            atelier_node,
            LatencyClass::OfflineRender,
            payload,
        )
    }

    /// Builds a refusal event for a denied development action.
    pub fn refusal(atelier_node: Symbol, payload: Expr) -> Result<Self> {
        Self::new("refusal", atelier_node, LatencyClass::Interactive, payload)
    }

    /// Attaches ticks to this event.
    pub fn with_ticks(mut self, ticks: Vec<Tick>) -> Result<Self> {
        sim_kernel::validate_ticks(&ticks)?;
        self.ticks = ticks;
        Ok(self)
    }

    /// Returns the development event kind.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Returns the originating Atelier node id.
    pub fn atelier_node(&self) -> &Symbol {
        &self.atelier_node
    }

    /// Returns the event latency class.
    pub fn latency_class(&self) -> LatencyClass {
        self.latency_class
    }

    /// Converts this event to a stream item using `ide/event/<kind>` data.
    pub fn stream_item(&self) -> Result<StreamItem> {
        StreamItem::with_ticks(
            StreamPacket::data(
                dev_event_media(&self.kind)?.symbol().clone(),
                self.payload_expr(),
            ),
            self.ticks.clone(),
        )
    }

    fn transport_profile(&self) -> Result<TransportProfile> {
        TransportProfile::new(
            Symbol::qualified(
                "stream/profile",
                format!("dev-{}", self.latency_class.wire_label()),
            ),
            self.latency_class,
            vec![
                StreamCapability::Deterministic,
                StreamCapability::Bounded,
                StreamCapability::Replayable,
            ],
        )
    }

    fn payload_expr(&self) -> Expr {
        Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("event-kind")),
                Expr::Symbol(Symbol::qualified("ide/event", self.kind.clone())),
            ),
            (
                Expr::Symbol(Symbol::new("atelier-node")),
                Expr::Symbol(self.atelier_node.clone()),
            ),
            (
                Expr::Symbol(Symbol::new("latency-class")),
                Expr::Symbol(self.latency_class.symbol()),
            ),
            (Expr::Symbol(Symbol::new("payload")), self.payload.clone()),
        ])
    }
}

/// A development cassette backed by the standard [`StreamCassette`] format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevCassette {
    cassette: StreamCassette,
    content_hash: String,
}

impl DevCassette {
    /// Records a development session into a stream cassette.
    pub fn from_events(stream_id: Symbol, events: Vec<DevEvent>) -> Result<Self> {
        let metadata = dev_event_metadata(stream_id)?;
        let envelopes = events
            .iter()
            .enumerate()
            .map(|(sequence, event)| {
                StreamEnvelope::from_item_with_profile(
                    &metadata,
                    sequence as u64,
                    &event.stream_item()?,
                    event.transport_profile()?,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let final_stats = StreamStats {
            yielded: envelopes.len() as u64,
            closed: true,
            ..StreamStats::default()
        };
        Self::from_stream_cassette(StreamCassette::from_envelopes(
            metadata,
            envelopes,
            final_stats,
        )?)
    }

    /// Wraps an existing stream cassette as a development cassette.
    pub fn from_stream_cassette(cassette: StreamCassette) -> Result<Self> {
        let content_hash = cassette_content_hash(&cassette);
        Ok(Self {
            cassette,
            content_hash,
        })
    }

    /// Returns the underlying stream cassette.
    pub fn cassette(&self) -> &StreamCassette {
        &self.cassette
    }

    /// Returns the deterministic cassette content hash.
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    /// Returns a copy with host paths, host names, and private payloads redacted.
    pub fn redacted(&self) -> Result<Self> {
        Self::from_stream_cassette(self.cassette.redacted()?)
    }

    /// Delegates golden fixture validation to the underlying stream cassette.
    pub fn validate_golden_fixture(&self, path: &str) -> Result<crate::StreamGoldenFixtureReport> {
        self.cassette.validate_golden_fixture(path)
    }

    /// Replays the cassette and recomputes the content hash.
    pub fn replay_content_hash(&self) -> Result<String> {
        let items = self.cassette.items()?;
        let metadata = self.cassette.metadata().clone();
        let envelopes = items
            .iter()
            .enumerate()
            .zip(self.cassette.envelopes())
            .map(|((sequence, item), original)| {
                StreamEnvelope::from_item_with_profile(
                    &metadata,
                    sequence as u64,
                    item,
                    original.profile().clone(),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let replay = StreamCassette::from_envelopes(
            metadata,
            envelopes,
            self.cassette.final_stats().clone(),
        )?;
        Ok(cassette_content_hash(&replay))
    }

    /// Replays the cassette with a stream fault plan applied.
    pub fn replay_with_fault(&self, plan: &StreamFaultPlan) -> Result<DevFaultReport> {
        let result = plan.apply(&self.cassette.items()?);
        let mut diagnostics = result.diagnostics;
        if diagnostics.contains(&StreamFaultKind::Drop.symbol()) {
            push_unique(&mut diagnostics, dev_dropped_chunks_diagnostic());
        }
        Ok(DevFaultReport {
            items: result.items,
            diagnostics,
        })
    }
}

/// Result of replaying a development cassette with an injected fault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevFaultReport {
    /// Items left after the fault plan is applied.
    pub items: Vec<StreamItem>,
    /// Diagnostics emitted by the fault replay.
    pub diagnostics: Vec<Symbol>,
}

/// Diagnostic emitted when a cassette replay drops development chunks.
pub fn dev_dropped_chunks_diagnostic() -> Symbol {
    Symbol::qualified("dev/diagnostic", "dropped-chunks")
}

fn validate_dev_event_kind(kind: &str) -> Result<()> {
    let valid = !kind.is_empty()
        && kind
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-'));
    if valid {
        Ok(())
    } else {
        Err(Error::Eval(format!("invalid dev event kind {kind:?}")))
    }
}

fn cassette_content_hash(cassette: &StreamCassette) -> String {
    let key = cassette.to_expr().canonical_key();
    let mut hash = 0xcbf29ce484222325u64;
    hash_bytes(&mut hash, format!("{key:?}").as_bytes());
    format!("fnv1a64:{hash:016x}")
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(0x100000001b3);
    }
}

fn push_unique(symbols: &mut Vec<Symbol>, symbol: Symbol) {
    if !symbols.contains(&symbol) {
        symbols.push(symbol);
    }
}

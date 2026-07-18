//! Golden-fixture record and replay for streams.
//!
//! A [`StreamCassette`] captures a deterministic trace of a stream -- its
//! metadata, the ordered envelopes it produced, derived timing, accumulated
//! diagnostics, and the final stats -- so that the same trace can be replayed
//! as a fresh [`StreamValue`] or persisted as a golden fixture for tests. The
//! kernel supplies the protocol types ([`Expr`], [`Symbol`], [`Error`]) for
//! cassette serialization; this module supplies the concrete streaming-fabric
//! behavior that records, redacts, validates, and round-trips those traces.

use sim_kernel::{Error, Expr, Result, Symbol};
use sim_value::access;

#[path = "cassette/redaction.rs"]
mod redaction;
#[path = "cassette/stats.rs"]
mod stats;

use crate::buffer::{expr_kind, field, string_field, symbol_field};
use crate::{
    StreamCapability, StreamEnvelope, StreamItem, StreamMetadata, StreamPacket, StreamStats,
    StreamValue, TransportProfile,
};

use redaction::{
    envelope_has_host_device, is_host_device_symbol, metadata_has_host_device,
    packet_has_private_payload, redact_envelope, redact_metadata, redact_symbol,
};
use stats::{stream_stats_expr, stream_stats_from_expr};

/// Repository-relative root directory under which golden stream fixtures live.
pub const STREAM_CASSETTE_FIXTURE_ROOT: &str = "fixtures/streams/golden";
/// File extension (without the leading dot) for a persisted golden fixture.
pub const STREAM_CASSETTE_EXTENSION: &str = "simcassette";

/// Derived timing summary for a recorded cassette.
///
/// Captures the clock the stream ran on, how many packets were recorded, the
/// sequence range of the first and last envelopes, and whether the trace is
/// finite (a golden fixture must be finite).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamCassetteTiming {
    /// Clock-domain symbol the recorded stream advanced against.
    pub clock: Symbol,
    /// Number of envelopes captured in the cassette.
    pub packet_count: usize,
    /// Sequence number of the first envelope, or `None` when empty.
    pub first_sequence: Option<u64>,
    /// Sequence number of the last envelope, or `None` when empty.
    pub last_sequence: Option<u64>,
    /// Whether the recorded trace terminated; golden fixtures must be finite.
    pub finite: bool,
}

/// A recorded, replayable trace of a single stream.
///
/// Holds the stream metadata, the ordered envelopes, derived [timing](StreamCassetteTiming),
/// the deduplicated diagnostic symbols observed, and the final [`StreamStats`].
/// A cassette can be rebuilt into a live [`StreamValue`] via
/// [`replay_stream_value`](StreamCassette::replay_stream_value), serialized to
/// an [`Expr`] map, and validated as a golden fixture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamCassette {
    metadata: StreamMetadata,
    envelopes: Vec<StreamEnvelope>,
    timing: StreamCassetteTiming,
    diagnostics: Vec<Symbol>,
    final_stats: StreamStats,
}

/// Outcome of validating a cassette against the golden-fixture rules.
///
/// Returned by [`StreamCassette::validate_golden_fixture`] once a cassette
/// passes every fixture invariant; records where the fixture lives, its format
/// symbol, the packet count, and the final stats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamGoldenFixtureReport {
    /// Validated fixture path under [`STREAM_CASSETTE_FIXTURE_ROOT`].
    pub path: String,
    /// Cassette format symbol the fixture was written with.
    pub format: Symbol,
    /// Number of envelopes the fixture contains.
    pub packet_count: usize,
    /// Final accumulated stats captured at the end of the trace.
    pub final_stats: StreamStats,
}

impl StreamCassette {
    /// Records a cassette by draining every packet from a live stream.
    ///
    /// Pulls currently available packets until the stream reports terminal
    /// `done`, snapshots its final stats, and builds the cassette from the
    /// metadata, drained items, and the given [`TransportProfile`]. An
    /// empty-but-open live stream is rejected because a cassette is a finite
    /// replay artifact.
    pub fn from_stream_value(stream: &StreamValue, profile: TransportProfile) -> Result<Self> {
        let mut items = Vec::new();
        while let Some(item) = stream.next_packet()? {
            items.push(item);
        }
        if !stream.is_done()? {
            return Err(Error::Eval(
                "cannot record a cassette from a stream that has not reached done".to_owned(),
            ));
        }
        let final_stats = stream.stats()?;
        Self::from_items(stream.metadata().clone(), items, profile, final_stats)
    }

    /// Records a cassette from already-drained stream items.
    ///
    /// Wraps each item in a sequenced [`StreamEnvelope`] under the given
    /// [`TransportProfile`], then delegates to
    /// [`from_envelopes`](StreamCassette::from_envelopes).
    pub fn from_items(
        metadata: StreamMetadata,
        items: Vec<StreamItem>,
        profile: TransportProfile,
        final_stats: StreamStats,
    ) -> Result<Self> {
        let envelopes = items
            .iter()
            .enumerate()
            .map(|(sequence, item)| {
                StreamEnvelope::from_item_with_profile(
                    &metadata,
                    sequence as u64,
                    item,
                    profile.clone(),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        Self::from_envelopes(metadata, envelopes, final_stats)
    }

    /// Builds a cassette directly from sequenced envelopes.
    ///
    /// Derives the [timing](StreamCassetteTiming) and the deduplicated
    /// diagnostic set from the envelopes, pairing them with the supplied
    /// metadata and final stats.
    pub fn from_envelopes(
        metadata: StreamMetadata,
        envelopes: Vec<StreamEnvelope>,
        final_stats: StreamStats,
    ) -> Result<Self> {
        let timing = timing_from_envelopes(&metadata, &envelopes);
        let diagnostics = diagnostics_from_envelopes(&envelopes);
        Ok(Self {
            metadata,
            envelopes,
            timing,
            diagnostics,
            final_stats,
        })
    }

    /// Returns the metadata of the recorded stream.
    pub fn metadata(&self) -> &StreamMetadata {
        &self.metadata
    }

    /// Returns the recorded envelopes in sequence order.
    pub fn envelopes(&self) -> &[StreamEnvelope] {
        &self.envelopes
    }

    /// Returns the derived timing summary for the trace.
    pub fn timing(&self) -> &StreamCassetteTiming {
        &self.timing
    }

    /// Returns the deduplicated diagnostic symbols observed during recording.
    pub fn diagnostics(&self) -> &[Symbol] {
        &self.diagnostics
    }

    /// Returns the final accumulated stats captured at end of trace.
    pub fn final_stats(&self) -> &StreamStats {
        &self.final_stats
    }

    /// Reconstructs the stream items from the recorded envelopes.
    ///
    /// Each item pairs an envelope's packet with its captured ticks, ready to
    /// feed a replay stream.
    pub fn items(&self) -> Result<Vec<StreamItem>> {
        self.envelopes
            .iter()
            .map(|envelope| {
                StreamItem::with_ticks(envelope.packet().clone(), envelope.ticks().to_vec())
            })
            .collect()
    }

    /// Rebuilds a live, pull-based [`StreamValue`] from the recorded trace.
    pub fn replay_stream_value(&self) -> Result<StreamValue> {
        Ok(StreamValue::pull(self.metadata.clone(), self.items()?))
    }

    /// Serializes the cassette to an [`Expr`] map keyed by field symbol.
    ///
    /// The map carries the format symbol, metadata table, timing, envelope
    /// list, diagnostics, and final stats, suitable for persistence and
    /// round-tripping through [`from_expr`](StreamCassette::from_expr).
    pub fn to_expr(&self) -> Expr {
        Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("cassette")),
                Expr::Symbol(stream_cassette_format_symbol()),
            ),
            (
                Expr::Symbol(Symbol::new("metadata")),
                self.metadata.table_expr(),
            ),
            (Expr::Symbol(Symbol::new("timing")), self.timing.to_expr()),
            (
                Expr::Symbol(Symbol::new("envelopes")),
                Expr::List(self.envelopes.iter().map(StreamEnvelope::to_expr).collect()),
            ),
            (
                Expr::Symbol(Symbol::new("diagnostics")),
                Expr::List(self.diagnostics.iter().cloned().map(Expr::Symbol).collect()),
            ),
            (
                Expr::Symbol(Symbol::new("final-stats")),
                stream_stats_expr(&self.final_stats),
            ),
        ])
    }

    /// Deserializes a cassette from an [`Expr`] map produced by
    /// [`to_expr`](StreamCassette::to_expr).
    ///
    /// Validates the field set and format symbol, then reconstructs metadata,
    /// envelopes, timing, diagnostics, and final stats. Fails closed on an
    /// unknown format, missing or unexpected fields, or type mismatches.
    pub fn from_expr(expr: &Expr) -> Result<Self> {
        let Expr::Map(entries) = expr else {
            return Err(Error::TypeMismatch {
                expected: "stream cassette map",
                found: expr_kind(expr),
            });
        };
        ensure_fields(
            entries,
            &[
                "cassette",
                "metadata",
                "timing",
                "envelopes",
                "diagnostics",
                "final-stats",
            ],
        )?;
        let format = symbol_field(entries, "cassette")?;
        if *format != stream_cassette_format_symbol() {
            return Err(Error::Eval(format!(
                "unknown stream cassette format {}",
                format.as_qualified_str()
            )));
        }
        let metadata = StreamMetadata::from_table_expr(field(entries, "metadata")?)?;
        let envelopes = list_field(entries, "envelopes")?
            .iter()
            .map(|expr| StreamEnvelope::try_from(expr.clone()))
            .collect::<Result<Vec<_>>>()?;
        let metadata = restore_metadata_id(metadata, &envelopes);
        let timing = StreamCassetteTiming::from_expr(field(entries, "timing")?)?;
        let diagnostics = symbol_list(entries, "diagnostics")?;
        let final_stats = stream_stats_from_expr(field(entries, "final-stats")?)?;
        Ok(Self {
            metadata,
            envelopes,
            timing,
            diagnostics,
            final_stats,
        })
    }

    /// Returns a copy with host-device names and private payloads redacted.
    ///
    /// Redacts the metadata, every envelope, and the diagnostic symbols so the
    /// result is safe to persist as a golden fixture.
    pub fn redacted(&self) -> Result<Self> {
        let metadata = redact_metadata(&self.metadata);
        let envelopes = self
            .envelopes
            .iter()
            .map(redact_envelope)
            .collect::<Result<Vec<_>>>()?;
        let mut redacted = Self::from_envelopes(metadata, envelopes, self.final_stats.clone())?;
        redacted.diagnostics = self.diagnostics.iter().map(redact_symbol).collect();
        Ok(redacted)
    }

    /// Validates the cassette as a golden fixture at `path`.
    ///
    /// Checks the path lives under [`STREAM_CASSETTE_FIXTURE_ROOT`] with the
    /// cassette extension, that the trace is finite, that envelope sequences
    /// match their packet index, that each transport profile is replayable or
    /// previewable but never realtime, and that no unredacted payload or
    /// host-device name remains. Returns a [`StreamGoldenFixtureReport`] on
    /// success, or an error describing the first failed invariant.
    pub fn validate_golden_fixture(&self, path: &str) -> Result<StreamGoldenFixtureReport> {
        validate_fixture_path(path)?;
        if !self.timing.finite {
            return Err(Error::Eval(
                "golden stream fixture must be finite".to_owned(),
            ));
        }
        for (index, envelope) in self.envelopes.iter().enumerate() {
            if envelope.sequence() != index as u64 {
                return Err(Error::Eval(format!(
                    "golden stream fixture sequence {} is not packet index {index}",
                    envelope.sequence()
                )));
            }
            if !envelope
                .profile()
                .has_capability(StreamCapability::Replayable)
                && !envelope.profile().has_capability(StreamCapability::Preview)
            {
                return Err(Error::Eval(format!(
                    "golden stream fixture profile {} is not replayable or previewable",
                    envelope.profile().name()
                )));
            }
            if envelope
                .profile()
                .has_capability(StreamCapability::Realtime)
            {
                return Err(Error::Eval(
                    "golden stream fixture cannot require realtime transport".to_owned(),
                ));
            }
            if packet_has_private_payload(envelope.packet()) || envelope_has_host_device(envelope) {
                return Err(Error::Eval(
                    "golden stream fixture contains unredacted payload".to_owned(),
                ));
            }
        }
        if metadata_has_host_device(&self.metadata)
            || is_host_device_symbol(&self.timing.clock)
            || self.diagnostics.iter().any(is_host_device_symbol)
        {
            return Err(Error::Eval(
                "golden stream fixture contains an unredacted host device name".to_owned(),
            ));
        }
        Ok(StreamGoldenFixtureReport {
            path: path.to_owned(),
            format: stream_cassette_format_symbol(),
            packet_count: self.envelopes.len(),
            final_stats: self.final_stats.clone(),
        })
    }
}

impl StreamCassetteTiming {
    /// Serializes the timing summary to an [`Expr`] map keyed by field symbol.
    pub fn to_expr(&self) -> Expr {
        Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("clock")),
                Expr::Symbol(self.clock.clone()),
            ),
            (
                Expr::Symbol(Symbol::new("packet-count")),
                Expr::String(self.packet_count.to_string()),
            ),
            (
                Expr::Symbol(Symbol::new("first-sequence")),
                optional_u64_expr(self.first_sequence),
            ),
            (
                Expr::Symbol(Symbol::new("last-sequence")),
                optional_u64_expr(self.last_sequence),
            ),
            (Expr::Symbol(Symbol::new("finite")), Expr::Bool(self.finite)),
        ])
    }

    /// Deserializes a timing summary from an [`Expr`] map produced by
    /// [`to_expr`](StreamCassetteTiming::to_expr).
    ///
    /// Validates the field set and fails closed on missing or unexpected
    /// fields or type mismatches.
    pub fn from_expr(expr: &Expr) -> Result<Self> {
        let Expr::Map(entries) = expr else {
            return Err(Error::TypeMismatch {
                expected: "stream cassette timing map",
                found: expr_kind(expr),
            });
        };
        ensure_fields(
            entries,
            &[
                "clock",
                "packet-count",
                "first-sequence",
                "last-sequence",
                "finite",
            ],
        )?;
        Ok(Self {
            clock: symbol_field(entries, "clock")?.clone(),
            packet_count: parse_usize(entries, "packet-count")?,
            first_sequence: optional_u64(field(entries, "first-sequence")?)?,
            last_sequence: optional_u64(field(entries, "last-sequence")?)?,
            finite: bool_field(entries, "finite")?,
        })
    }
}

/// Returns the format symbol stamped into every serialized cassette.
pub fn stream_cassette_format_symbol() -> Symbol {
    Symbol::qualified("stream/cassette", "v1")
}

/// Returns the repository-relative root for golden stream fixtures.
pub fn stream_cassette_golden_root() -> &'static str {
    STREAM_CASSETTE_FIXTURE_ROOT
}

/// Returns the file extension (without leading dot) for golden fixtures.
pub fn stream_cassette_golden_extension() -> &'static str {
    STREAM_CASSETTE_EXTENSION
}

fn timing_from_envelopes(
    metadata: &StreamMetadata,
    envelopes: &[StreamEnvelope],
) -> StreamCassetteTiming {
    StreamCassetteTiming {
        clock: metadata.clock().clone(),
        packet_count: envelopes.len(),
        first_sequence: envelopes.first().map(StreamEnvelope::sequence),
        last_sequence: envelopes.last().map(StreamEnvelope::sequence),
        finite: true,
    }
}

fn restore_metadata_id(metadata: StreamMetadata, envelopes: &[StreamEnvelope]) -> StreamMetadata {
    let Some(first) = envelopes.first() else {
        return metadata;
    };
    if metadata.id().as_qualified_str() != first.stream_id().as_qualified_str() {
        return metadata;
    }
    StreamMetadata::new(
        first.stream_id().clone(),
        metadata.media(),
        metadata.direction(),
        metadata.clock().clone(),
        metadata.buffer().clone(),
    )
}

fn diagnostics_from_envelopes(envelopes: &[StreamEnvelope]) -> Vec<Symbol> {
    let mut diagnostics = Vec::new();
    for envelope in envelopes {
        for diagnostic in envelope.diagnostics() {
            push_unique(&mut diagnostics, diagnostic.clone());
        }
        if let StreamPacket::Diagnostic(packet) = envelope.packet() {
            push_unique(&mut diagnostics, packet.kind().clone());
        }
    }
    diagnostics
}

fn push_unique(symbols: &mut Vec<Symbol>, symbol: Symbol) {
    if !symbols.contains(&symbol) {
        symbols.push(symbol);
    }
}

fn validate_fixture_path(path: &str) -> Result<()> {
    let Some(relative) = path.strip_prefix(STREAM_CASSETTE_FIXTURE_ROOT) else {
        return Err(Error::Eval(format!(
            "golden stream fixture path must live under {STREAM_CASSETTE_FIXTURE_ROOT}"
        )));
    };
    if !relative.starts_with('/') || relative == "/" {
        return Err(Error::Eval(format!(
            "golden stream fixture path must live under {STREAM_CASSETTE_FIXTURE_ROOT}"
        )));
    }
    let expected_extension = format!(".{STREAM_CASSETTE_EXTENSION}");
    if !path.ends_with(&expected_extension) {
        return Err(Error::Eval(format!(
            "golden stream fixture path must end in .{STREAM_CASSETTE_EXTENSION}"
        )));
    }
    Ok(())
}

fn ensure_fields(entries: &[(Expr, Expr)], allowed: &[&str]) -> Result<()> {
    for (key, _) in entries {
        let Expr::Symbol(symbol) = key else {
            return Err(Error::TypeMismatch {
                expected: "symbol stream cassette field",
                found: expr_kind(key),
            });
        };
        if symbol.namespace.is_none() && allowed.contains(&symbol.name.as_ref()) {
            continue;
        }
        return Err(Error::Eval(format!(
            "unknown stream cassette field {}",
            symbol.as_qualified_str()
        )));
    }
    Ok(())
}

fn list_field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> Result<&'a [Expr]> {
    access::entry_required_list(entries, name, "list field")
}

fn symbol_list(entries: &[(Expr, Expr)], name: &str) -> Result<Vec<Symbol>> {
    list_field(entries, name)?
        .iter()
        .map(|expr| match expr {
            Expr::Symbol(symbol) => Ok(symbol.clone()),
            other => Err(Error::TypeMismatch {
                expected: "symbol list item",
                found: expr_kind(other),
            }),
        })
        .collect()
}

fn parse_usize(entries: &[(Expr, Expr)], name: &str) -> Result<usize> {
    string_field(entries, name)?
        .parse::<usize>()
        .map_err(|err| Error::Eval(format!("invalid stream cassette {name}: {err}")))
}

fn optional_u64(expr: &Expr) -> Result<Option<u64>> {
    match expr {
        Expr::Nil => Ok(None),
        Expr::String(value) => value
            .parse::<u64>()
            .map(Some)
            .map_err(|err| Error::Eval(format!("invalid stream cassette sequence: {err}"))),
        other => Err(Error::TypeMismatch {
            expected: "optional u64 string",
            found: expr_kind(other),
        }),
    }
}

fn optional_u64_expr(value: Option<u64>) -> Expr {
    value
        .map(|value| Expr::String(value.to_string()))
        .unwrap_or(Expr::Nil)
}

fn bool_field(entries: &[(Expr, Expr)], name: &str) -> Result<bool> {
    access::entry_required_bool(entries, name, "bool field")
}

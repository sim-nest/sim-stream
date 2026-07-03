//! Stream inspector and fault-injection surface for stream-core.
//!
//! This module supplies the concrete observability behavior layered over the
//! streaming fabric: a point-in-time [`StreamInspectorSnapshot`] of a live
//! stream's health, the [`StreamInspectorStatus`] lifecycle classification
//! derived from runtime [`StreamStats`], and a fault model
//! ([`StreamFaultKind`], [`StreamFaultSpec`], [`StreamFaultPlan`],
//! [`StreamFaultResult`]) that tooling uses to inject or simulate degraded
//! delivery. Snapshots and plans render to the kernel [`Expr`] graph so the
//! same data round-trips through any codec surface, and the symbol helpers
//! expose the stable [`Symbol`] vocabulary tooling matches against.

use sim_kernel::{Error, Expr, Result, Symbol};

use crate::{
    BufferPolicy, StreamItem, StreamMedia, StreamMetadata, StreamStats, StreamValue,
    TransportProfile,
};

/// Lifecycle classification of an observed stream.
///
/// Reported by [`StreamInspectorSnapshot`] to describe whether a stream is
/// still flowing, has finished, or has entered a degraded or terminal
/// condition. Each variant carries a stable wire label and qualified symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamInspectorStatus {
    /// Stream is open and actively delivering items.
    Live,
    /// Stream closed normally after delivering its items.
    Ended,
    /// Stream was cancelled by a consumer or producer.
    Cancelled,
    /// Stream's bounded buffer dropped, rejected, or overflowed items.
    BufferOverflow,
    /// Stream transport is currently disconnected.
    Disconnected,
    /// Stream transport is attempting to re-establish a connection.
    Reconnecting,
    /// Stream's transport profile was refused as unsupported.
    RefusedProfile,
    /// Stream has been forced into a fault condition for inspection.
    Faulted,
}

impl StreamInspectorStatus {
    /// Returns the stable lowercase wire label for this status.
    pub fn wire_label(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Ended => "ended",
            Self::Cancelled => "cancelled",
            Self::BufferOverflow => "buffer-overflow",
            Self::Disconnected => "disconnected",
            Self::Reconnecting => "reconnecting",
            Self::RefusedProfile => "refused-profile",
            Self::Faulted => "faulted",
        }
    }

    /// Returns this status as a `stream/inspector-status/<label>` symbol.
    pub fn symbol(self) -> Symbol {
        Symbol::qualified("stream/inspector-status", self.wire_label())
    }

    /// Classifies a stream's status from its runtime stats.
    ///
    /// Precedence is cancellation, then buffer loss (any dropped, overflowed,
    /// or rejected items), then end-of-stream (`done` or closed), otherwise
    /// [`StreamInspectorStatus::Live`].
    pub fn from_stats(stats: &StreamStats, done: bool) -> Self {
        if stats.cancelled {
            Self::Cancelled
        } else if stats.dropped_newest > 0
            || stats.dropped_oldest > 0
            || stats.overflow_errors > 0
            || stats.rejected > 0
        {
            Self::BufferOverflow
        } else if done || stats.closed {
            Self::Ended
        } else {
            Self::Live
        }
    }
}

/// Point-in-time observation of a single stream's identity and health.
///
/// Captures the stream's metadata-derived identity (id, media, clock, buffer
/// policy), its routing and transport profile, its current
/// [`StreamInspectorStatus`], and live queue/loss counters. Render with
/// [`StreamInspectorSnapshot::to_expr`] to hand the snapshot to a codec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamInspectorSnapshot {
    /// Stable identifier of the observed stream.
    pub stream_id: Symbol,
    /// Route the stream is being observed on.
    pub route: Symbol,
    /// Media kind carried by the stream.
    pub media: StreamMedia,
    /// Name of the transport profile in effect.
    pub profile: Symbol,
    /// Clock domain the stream is timed against.
    pub clock: Symbol,
    /// Current lifecycle status of the stream.
    pub status: StreamInspectorStatus,
    /// Bounded buffer policy governing the stream's queue.
    pub buffer: BufferPolicy,
    /// Number of items currently queued in the buffer.
    pub queue_depth: usize,
    /// Total items dropped (newest plus oldest) since the stream opened.
    pub dropped_count: u64,
    /// Sequence number of the most recent observed item, if any.
    pub last_sequence: Option<u64>,
    /// Recent diagnostic symbols collected for the stream.
    pub recent_diagnostics: Vec<Symbol>,
}

impl StreamInspectorSnapshot {
    /// Builds a snapshot from stream metadata, stats, and observed counters.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        metadata: &StreamMetadata,
        route: Symbol,
        profile: Symbol,
        status: StreamInspectorStatus,
        queue_depth: usize,
        stats: &StreamStats,
        last_sequence: Option<u64>,
        recent_diagnostics: Vec<Symbol>,
    ) -> Self {
        Self {
            stream_id: metadata.id().clone(),
            route,
            media: metadata.media(),
            profile,
            clock: metadata.clock().clone(),
            status,
            buffer: metadata.buffer().clone(),
            queue_depth,
            dropped_count: stats.dropped_newest.saturating_add(stats.dropped_oldest),
            last_sequence,
            recent_diagnostics,
        }
    }

    /// Builds a snapshot by querying a live [`StreamValue`].
    ///
    /// Reads the stream's stats, queue depth, and completion flag to derive the
    /// last observed sequence and [`StreamInspectorStatus`]. Returns an error if
    /// any of those queries fail.
    pub fn from_stream_value(
        stream: &StreamValue,
        route: Symbol,
        profile: &TransportProfile,
        recent_diagnostics: Vec<Symbol>,
    ) -> Result<Self> {
        let stats = stream.stats()?;
        let queue_depth = stream.queue_depth()?;
        let observed = stats
            .accepted
            .max(stats.yielded.saturating_add(queue_depth as u64));
        let last_sequence = observed.checked_sub(1);
        let status = StreamInspectorStatus::from_stats(&stats, stream.is_done()?);
        Ok(Self::new(
            stream.metadata(),
            route,
            profile.name().clone(),
            status,
            queue_depth,
            &stats,
            last_sequence,
            recent_diagnostics,
        ))
    }

    /// Renders the snapshot as a tagged [`Expr`] map for codec round-tripping.
    pub fn to_expr(&self) -> Expr {
        Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("inspector")),
                Expr::Symbol(stream_inspector_model_symbol()),
            ),
            (
                Expr::Symbol(Symbol::new("id")),
                Expr::Symbol(self.stream_id.clone()),
            ),
            (
                Expr::Symbol(Symbol::new("route")),
                Expr::Symbol(self.route.clone()),
            ),
            (
                Expr::Symbol(Symbol::new("media")),
                Expr::Symbol(self.media.symbol()),
            ),
            (
                Expr::Symbol(Symbol::new("profile")),
                Expr::Symbol(self.profile.clone()),
            ),
            (
                Expr::Symbol(Symbol::new("clock")),
                Expr::Symbol(self.clock.clone()),
            ),
            (
                Expr::Symbol(Symbol::new("status")),
                Expr::Symbol(self.status.symbol()),
            ),
            (Expr::Symbol(Symbol::new("buffer")), self.buffer.to_expr()),
            (
                Expr::Symbol(Symbol::new("queue-depth")),
                Expr::String(self.queue_depth.to_string()),
            ),
            (
                Expr::Symbol(Symbol::new("dropped-count")),
                Expr::String(self.dropped_count.to_string()),
            ),
            (
                Expr::Symbol(Symbol::new("last-sequence")),
                optional_u64_expr(self.last_sequence),
            ),
            (
                Expr::Symbol(Symbol::new("recent-diagnostics")),
                Expr::List(
                    self.recent_diagnostics
                        .iter()
                        .cloned()
                        .map(Expr::Symbol)
                        .collect(),
                ),
            ),
        ])
    }
}

/// Kind of fault a [`StreamFaultPlan`] can inject into a stream.
///
/// Each variant names a class of degraded delivery. Item-level kinds
/// ([`StreamFaultKind::Drop`], [`StreamFaultKind::Reorder`],
/// [`StreamFaultKind::Duplicate`], [`StreamFaultKind::Delay`]) rewrite the item
/// sequence when applied; transport-level kinds only record a diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamFaultKind {
    /// Discards leading items from the stream.
    Drop,
    /// Swaps the order of the first two items.
    Reorder,
    /// Re-emits the leading item one or more extra times.
    Duplicate,
    /// Rotates leading items to the back to simulate late arrival.
    Delay,
    /// Models a consumer or producer cancellation.
    Cancel,
    /// Models a delivery timeout.
    Timeout,
    /// Models a transport disconnect.
    Disconnect,
    /// Models a transport reconnect.
    Reconnect,
    /// Models a refused, unsupported transport profile.
    UnsupportedProfile,
}

impl StreamFaultKind {
    /// Returns the stable lowercase wire label for this fault kind.
    pub fn wire_label(self) -> &'static str {
        match self {
            Self::Drop => "drop",
            Self::Reorder => "reorder",
            Self::Duplicate => "duplicate",
            Self::Delay => "delay",
            Self::Cancel => "cancel",
            Self::Timeout => "timeout",
            Self::Disconnect => "disconnect",
            Self::Reconnect => "reconnect",
            Self::UnsupportedProfile => "unsupported-profile",
        }
    }

    /// Returns this fault kind as a `stream/fault/<label>` symbol.
    pub fn symbol(self) -> Symbol {
        Symbol::qualified("stream/fault", self.wire_label())
    }

    /// Parses a fault kind from its bare or fully qualified symbol.
    ///
    /// Accepts both the short label (`drop`) and the qualified form
    /// (`stream/fault/drop`). Returns an error for any unknown fault.
    pub fn from_symbol(symbol: &Symbol) -> Result<Self> {
        match symbol.as_qualified_str().as_str() {
            "drop" | "stream/fault/drop" => Ok(Self::Drop),
            "reorder" | "stream/fault/reorder" => Ok(Self::Reorder),
            "duplicate" | "stream/fault/duplicate" => Ok(Self::Duplicate),
            "delay" | "stream/fault/delay" => Ok(Self::Delay),
            "cancel" | "stream/fault/cancel" => Ok(Self::Cancel),
            "timeout" | "stream/fault/timeout" => Ok(Self::Timeout),
            "disconnect" | "stream/fault/disconnect" => Ok(Self::Disconnect),
            "reconnect" | "stream/fault/reconnect" => Ok(Self::Reconnect),
            "unsupported-profile" | "stream/fault/unsupported-profile" => {
                Ok(Self::UnsupportedProfile)
            }
            other => Err(Error::Eval(format!("unknown stream fault {other}"))),
        }
    }
}

/// A single fault to apply, paired with a repetition count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamFaultSpec {
    /// Kind of fault to inject.
    pub kind: StreamFaultKind,
    /// Number of items the fault affects (at least 1).
    pub count: usize,
}

impl StreamFaultSpec {
    /// Builds a fault spec, clamping `count` to a minimum of 1.
    pub fn new(kind: StreamFaultKind, count: usize) -> Self {
        Self {
            kind,
            count: count.max(1),
        }
    }
}

/// An ordered list of faults to apply to a stream's item sequence.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamFaultPlan {
    faults: Vec<StreamFaultSpec>,
}

/// Outcome of applying a [`StreamFaultPlan`] to a sequence of items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamFaultResult {
    /// Items after the plan's faults have been applied.
    pub items: Vec<StreamItem>,
    /// Diagnostic symbols recording each fault that was applied, in order.
    pub diagnostics: Vec<Symbol>,
}

impl StreamFaultPlan {
    /// Builds a plan from an ordered list of fault specs.
    pub fn new(faults: Vec<StreamFaultSpec>) -> Self {
        Self { faults }
    }

    /// Returns the plan's fault specs in application order.
    pub fn faults(&self) -> &[StreamFaultSpec] {
        &self.faults
    }

    /// Applies every fault in order to a copy of `items`.
    ///
    /// Item-level faults rewrite the sequence; transport-level faults are
    /// recorded as diagnostics without altering items. Returns the rewritten
    /// items together with one diagnostic symbol per applied fault.
    pub fn apply(&self, items: &[StreamItem]) -> StreamFaultResult {
        let mut items = items.to_vec();
        let mut diagnostics = Vec::new();
        for fault in &self.faults {
            diagnostics.push(fault.kind.symbol());
            match fault.kind {
                StreamFaultKind::Drop => {
                    let remove = fault.count.min(items.len());
                    items.drain(0..remove);
                }
                StreamFaultKind::Reorder => {
                    if items.len() > 1 {
                        items.swap(0, 1);
                    }
                }
                StreamFaultKind::Duplicate => {
                    if let Some(item) = items.first().cloned() {
                        for _ in 0..fault.count {
                            items.insert(0, item.clone());
                        }
                    }
                }
                StreamFaultKind::Delay => {
                    if !items.is_empty() {
                        let rotate = fault.count.min(items.len());
                        items.rotate_left(rotate);
                    }
                }
                StreamFaultKind::Cancel
                | StreamFaultKind::Timeout
                | StreamFaultKind::Disconnect
                | StreamFaultKind::Reconnect
                | StreamFaultKind::UnsupportedProfile => {}
            }
        }
        StreamFaultResult { items, diagnostics }
    }

    /// Renders the plan as an [`Expr`] list of fault/count maps.
    pub fn to_expr(&self) -> Expr {
        Expr::List(
            self.faults
                .iter()
                .map(|fault| {
                    Expr::Map(vec![
                        (
                            Expr::Symbol(Symbol::new("fault")),
                            Expr::Symbol(fault.kind.symbol()),
                        ),
                        (
                            Expr::Symbol(Symbol::new("count")),
                            Expr::String(fault.count.to_string()),
                        ),
                    ])
                })
                .collect(),
        )
    }
}

/// Returns the versioned model tag stamped into inspector snapshots.
pub fn stream_inspector_model_symbol() -> Symbol {
    Symbol::qualified("stream/inspector", "v1")
}

/// Returns the route symbol denoting a locally observed stream.
pub fn stream_inspector_route_local_symbol() -> Symbol {
    Symbol::qualified("stream/route", "local")
}

/// Returns every [`StreamInspectorStatus`] symbol as a fixed-size array.
pub fn stream_inspector_status_symbols() -> [Symbol; 8] {
    [
        StreamInspectorStatus::Live.symbol(),
        StreamInspectorStatus::Ended.symbol(),
        StreamInspectorStatus::Cancelled.symbol(),
        StreamInspectorStatus::BufferOverflow.symbol(),
        StreamInspectorStatus::Disconnected.symbol(),
        StreamInspectorStatus::Reconnecting.symbol(),
        StreamInspectorStatus::RefusedProfile.symbol(),
        StreamInspectorStatus::Faulted.symbol(),
    ]
}

/// Returns every [`StreamFaultKind`] symbol as a fixed-size array.
pub fn stream_fault_symbols() -> [Symbol; 9] {
    [
        StreamFaultKind::Drop.symbol(),
        StreamFaultKind::Reorder.symbol(),
        StreamFaultKind::Duplicate.symbol(),
        StreamFaultKind::Delay.symbol(),
        StreamFaultKind::Cancel.symbol(),
        StreamFaultKind::Timeout.symbol(),
        StreamFaultKind::Disconnect.symbol(),
        StreamFaultKind::Reconnect.symbol(),
        StreamFaultKind::UnsupportedProfile.symbol(),
    ]
}

/// Checks that a fault kind is in the supported set, erroring otherwise.
pub fn ensure_fault_supported(kind: StreamFaultKind) -> Result<()> {
    if stream_fault_symbols().contains(&kind.symbol()) {
        Ok(())
    } else {
        Err(Error::Eval("unsupported stream fault".to_owned()))
    }
}

fn optional_u64_expr(value: Option<u64>) -> Expr {
    value
        .map(|value| Expr::String(value.to_string()))
        .unwrap_or(Expr::Nil)
}

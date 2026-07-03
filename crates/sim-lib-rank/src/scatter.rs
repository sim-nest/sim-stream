//! Sharded scatter over a rank ordinal range.
//!
//! Splits a contiguous ordinal range into shards, walks each shard to collect
//! ordinals, and exposes the merged result as both an event source and a data
//! stream for distributed enumeration of a ranked space.

use std::sync::{Arc, Mutex};

use sim_kernel::{Cx, Error as KernelError, Event, EventKind, EventSource, Ref, Symbol, Tick};

use crate::{
    Nat, RankCodec, RankError, RankLimits, RankOrdinalRange, RankResult, coordinate_for_nat,
    intern_ordinal,
    order_score::{key_nat, rank_coordinate_packet, rank_data_metadata},
};
use sim_lib_stream_core::{StreamItem, StreamPacket, StreamValue};

/// Specification for a sharded scatter over a rank ordinal range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankScatterSpec {
    /// Identifier of this scatter run.
    pub id: Symbol,
    /// Symbol of the ranked space being scattered.
    pub space: Symbol,
    /// Closed-open ordinal range to enumerate.
    pub range: RankOrdinalRange,
    /// Number of shards the range is divided into.
    pub shards: usize,
    /// Maximum number of ordinals collected before stopping.
    pub max_events: usize,
    /// Maximum number of buffered ordinals before failing closed.
    pub buffer_limit: usize,
}

impl RankScatterSpec {
    /// Builds a scatter spec over `start..end` with `shards` shards and no
    /// answer or buffer limits.
    pub fn new(id: Symbol, space: Symbol, start: Nat, end: Nat, shards: usize) -> Self {
        Self {
            id,
            space,
            range: RankOrdinalRange::closed_open(start, end),
            shards,
            max_events: usize::MAX,
            buffer_limit: usize::MAX,
        }
    }

    /// Returns the spec with explicit answer and buffer limits applied.
    pub fn with_limits(mut self, max_events: usize, buffer_limit: usize) -> Self {
        self.max_events = max_events;
        self.buffer_limit = buffer_limit;
        self
    }
}

/// One shard of a scatter: a contiguous sub-range of the ordinal space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankScatterShard {
    /// Zero-based shard index.
    pub index: usize,
    /// Inclusive start ordinal of the shard.
    pub start: Nat,
    /// Exclusive end ordinal of the shard.
    pub end: Nat,
}

/// A single scattered ordinal tagged with its owning shard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankScatterItem {
    /// Index of the shard that produced this ordinal.
    pub shard: usize,
    /// The collected ordinal.
    pub ordinal: Nat,
}

/// Merged result of a scatter run: its shard layout and collected ordinals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankScatterResult {
    id: Symbol,
    space: Symbol,
    shards: Vec<RankScatterShard>,
    items: Vec<RankScatterItem>,
}

impl RankScatterResult {
    /// Returns the identifier of the scatter run.
    pub fn id(&self) -> &Symbol {
        &self.id
    }

    /// Returns the symbol of the ranked space that was scattered.
    pub fn space(&self) -> &Symbol {
        &self.space
    }

    /// Returns the shard layout of the run.
    pub fn shards(&self) -> &[RankScatterShard] {
        &self.shards
    }

    /// Returns the collected scatter items in merge order.
    pub fn items(&self) -> &[RankScatterItem] {
        &self.items
    }

    /// Iterates the collected ordinals in merge order, dropping shard tags.
    pub fn merged_ordinals(&self) -> impl Iterator<Item = &Nat> {
        self.items.iter().map(|item| &item.ordinal)
    }

    /// Builds an event source that replays the collected items as coordinate
    /// chunk events for the given run reference.
    pub fn event_source(&self, run: Ref) -> Arc<dyn EventSource> {
        Arc::new(RankScatterEventSource::new(
            run,
            self.space.clone(),
            self.items.clone(),
        ))
    }

    /// Builds a pull-based data stream of coordinate packets for the collected
    /// items.
    pub fn data_stream(&self) -> StreamValue {
        rank_scatter_stream(self)
    }
}

/// Builds a pull-based [`StreamValue`] of coordinate packets, one per collected
/// scatter item.
pub fn rank_scatter_stream(result: &RankScatterResult) -> StreamValue {
    let items = result
        .items
        .iter()
        .enumerate()
        .map(|(position, item)| {
            let packet = rank_scatter_item_packet(result, position, item);
            StreamItem::new(packet)
        })
        .collect::<Vec<_>>();
    StreamValue::pull(rank_data_metadata(result.id.clone(), items.len()), items)
}

fn rank_scatter_item_packet(
    result: &RankScatterResult,
    position: usize,
    item: &RankScatterItem,
) -> StreamPacket {
    let extra = vec![key_nat("shard", &Nat::from(item.shard))];
    rank_coordinate_packet(
        "scatter",
        &result.id,
        "position",
        position,
        &result.space,
        &item.ordinal,
        extra,
    )
}

/// Runs a sharded scatter over the codec's ordinal range.
///
/// Divides the range into shards, walks each shard in order while validating
/// ordinals against `codec`, and stops at the spec's answer limit; fails closed
/// when the buffer limit is exceeded.
pub fn rank_scatter(
    spec: RankScatterSpec,
    codec: &dyn RankCodec,
    limits: &mut RankLimits,
) -> RankResult<RankScatterResult> {
    if spec.shards == 0 {
        return Err(invalid_node("rank scatter requires at least one shard"));
    }
    if spec.max_events == 0 || spec.buffer_limit == 0 {
        return Err(invalid_node(
            "rank scatter answer and buffer limits must be positive",
        ));
    }

    let end = scatter_end(&spec.range, codec)?;
    if spec.range.start > end {
        return Err(invalid_node("rank scatter range start exceeds end"));
    }
    let shards = rank_scatter_shards(spec.range.start.clone(), end, spec.shards)?;
    let mut items = Vec::new();

    'shards: for shard in &shards {
        let mut ordinal = shard.start.clone();
        while ordinal < shard.end {
            limits.consume(1, "rank.scatter")?;
            limits.check_nat(&ordinal, "rank.scatter")?;
            codec.unrank_node(&ordinal)?;
            if items.len() >= spec.buffer_limit {
                return Err(RankError::LimitExceeded {
                    limit: "rank.scatter.buffer",
                    needed: 1,
                    remaining: 0,
                });
            }
            items.push(RankScatterItem {
                shard: shard.index,
                ordinal: ordinal.clone(),
            });
            if items.len() >= spec.max_events {
                break 'shards;
            }
            ordinal = ordinal.checked_add(&Nat::one());
        }
    }

    Ok(RankScatterResult {
        id: spec.id,
        space: spec.space,
        shards,
        items,
    })
}

/// Divides `start..end` into `shard_count` contiguous shards of near-equal
/// width, distributing the remainder to the earliest shards.
pub fn rank_scatter_shards(
    start: Nat,
    end: Nat,
    shard_count: usize,
) -> RankResult<Vec<RankScatterShard>> {
    if shard_count == 0 {
        return Err(invalid_node("rank scatter requires at least one shard"));
    }
    if start > end {
        return Err(invalid_node("rank scatter shard start exceeds end"));
    }

    let span = end.checked_sub(&start)?;
    let (base_width, remainder) = span.div_mod(&Nat::from(shard_count))?;
    let mut shards = Vec::with_capacity(shard_count);
    let mut cursor = start;
    for index in 0..shard_count {
        let mut width = base_width.clone();
        if Nat::from(index) < remainder {
            width = width.checked_add(&Nat::one());
        }
        let next = cursor.checked_add(&width);
        shards.push(RankScatterShard {
            index,
            start: cursor,
            end: next.clone(),
        });
        cursor = next;
    }
    Ok(shards)
}

fn scatter_end(range: &RankOrdinalRange, codec: &dyn RankCodec) -> RankResult<Nat> {
    let end = range
        .end
        .clone()
        .or_else(|| codec.count())
        .ok_or_else(|| invalid_node("open rank scatter range requires a finite codec count"))?;
    if let Some(count) = codec.count()
        && end > count
    {
        return Err(RankError::OrdinalOutOfRange {
            ordinal: end.to_string(),
            count: count.to_string(),
        });
    }
    Ok(end)
}

fn invalid_node(message: &str) -> RankError {
    RankError::InvalidNode {
        message: message.to_owned(),
    }
}

struct RankScatterEventSource {
    run: Ref,
    space: Symbol,
    items: Vec<RankScatterItem>,
    state: Mutex<RankScatterEventState>,
}

struct RankScatterEventState {
    index: usize,
    seq: u64,
    done_sent: bool,
}

impl RankScatterEventSource {
    fn new(run: Ref, space: Symbol, items: Vec<RankScatterItem>) -> Self {
        Self {
            run,
            space,
            items,
            state: Mutex::new(RankScatterEventState {
                index: 0,
                seq: 0,
                done_sent: false,
            }),
        }
    }
}

impl EventSource for RankScatterEventSource {
    fn next(&self, cx: &mut Cx) -> sim_kernel::Result<Option<Event>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| KernelError::PoisonedLock("rank scatter event source"))?;
        let seq = state.seq;
        state.seq = state.seq.saturating_add(1);

        if let Some(item) = self.items.get(state.index) {
            let ticks = vec![
                Tick::new(
                    Symbol::qualified("rank/scatter", "shard"),
                    Ref::Content(intern_ordinal(cx, &Nat::from(item.shard))?),
                ),
                Tick::new(
                    Symbol::qualified("rank/scatter", "ordinal"),
                    Ref::Content(intern_ordinal(cx, &item.ordinal)?),
                ),
            ];
            let event = Event::new(
                self.run.clone(),
                seq,
                ticks,
                EventKind::Chunk {
                    payload: coordinate_for_nat(cx, self.space.clone(), &item.ordinal)?,
                },
            )?;
            state.index += 1;
            return Ok(Some(event));
        }

        if state.done_sent {
            Ok(None)
        } else {
            state.done_sent = true;
            Ok(Some(Event::done(self.run.clone(), seq)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use sim_kernel::{EventKind, Expr};
    use sim_lib_stream_core::StreamPacket;

    use super::*;
    use crate::order_score::rank_coordinate_data_kind;
    use crate::{RankBuilder, RankPrimitiveCodec};

    use sim_kernel::testing::bare_cx as cx;

    fn bool_triple_codec() -> RankPrimitiveCodec {
        RankPrimitiveCodec::new(
            RankBuilder::product(Symbol::qualified("rank-test", "scatter-bools"))
                .field(Symbol::new("a"), RankBuilder::bool())
                .field(Symbol::new("b"), RankBuilder::bool())
                .field(Symbol::new("c"), RankBuilder::bool())
                .build()
                .unwrap(),
        )
    }

    #[test]
    fn finite_scatter_range_has_no_gaps_or_duplicates() {
        let codec = bool_triple_codec();
        let result = rank_scatter(
            RankScatterSpec::new(
                Symbol::qualified("rank-scatter", "test"),
                Symbol::qualified("rank-test", "scatter-bools"),
                Nat::zero(),
                Nat::from(8_u64),
                3,
            ),
            &codec,
            &mut RankLimits::default(),
        )
        .unwrap();
        let ordinals = result.merged_ordinals().cloned().collect::<Vec<_>>();
        let unique = ordinals.iter().cloned().collect::<BTreeSet<_>>();

        assert_eq!(ordinals.len(), 8);
        assert_eq!(unique.len(), 8);
        assert_eq!(ordinals, (0_u64..8).map(Nat::from).collect::<Vec<_>>());
        assert_eq!(result.shards()[0].start, Nat::zero());
        assert_eq!(result.shards().last().unwrap().end, Nat::from(8_u64));
    }

    #[test]
    fn results_merge_deterministically_and_emit_coordinate_events() {
        let codec = bool_triple_codec();
        let spec = RankScatterSpec::new(
            Symbol::qualified("rank-scatter", "stable"),
            Symbol::qualified("rank-test", "scatter-bools"),
            Nat::zero(),
            Nat::from(8_u64),
            5,
        );
        let left = rank_scatter(spec.clone(), &codec, &mut RankLimits::default()).unwrap();
        let right = rank_scatter(spec, &codec, &mut RankLimits::default()).unwrap();
        assert_eq!(left, right);

        let mut cx = cx();
        let source = left.event_source(Ref::Symbol(Symbol::qualified("rank-test", "run")));
        let event = source.next(&mut cx).unwrap().unwrap();
        assert!(matches!(
            event.kind,
            EventKind::Chunk {
                payload: Ref::Coord(_)
            }
        ));
        assert_eq!(event.ticks.len(), 2);
    }

    #[test]
    fn scatter_buffer_limit_fails_closed() {
        let codec = bool_triple_codec();
        let err = rank_scatter(
            RankScatterSpec::new(
                Symbol::qualified("rank-scatter", "limited"),
                Symbol::qualified("rank-test", "scatter-bools"),
                Nat::zero(),
                Nat::from(8_u64),
                2,
            )
            .with_limits(8, 3),
            &codec,
            &mut RankLimits::default(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            RankError::LimitExceeded {
                limit: "rank.scatter.buffer",
                ..
            }
        ));
    }

    #[test]
    fn scatter_stream_is_deterministic_and_bounded() {
        let codec = bool_triple_codec();
        let spec = RankScatterSpec::new(
            Symbol::qualified("rank-scatter", "stream"),
            Symbol::qualified("rank-test", "scatter-bools"),
            Nat::zero(),
            Nat::from(8_u64),
            3,
        )
        .with_limits(4, 4);
        let left = rank_scatter(spec.clone(), &codec, &mut RankLimits::default()).unwrap();
        let right = rank_scatter(spec, &codec, &mut RankLimits::default()).unwrap();

        let left_items = left.data_stream().take_packets(16).unwrap();
        let right_items = right.data_stream().take_packets(16).unwrap();

        assert_eq!(left_items, right_items);
        assert_eq!(left_items.len(), 4);
        assert_eq!(
            left_items.iter().map(stream_ordinal).collect::<Vec<_>>(),
            (0_u64..4).map(Nat::from).collect::<Vec<_>>()
        );
        assert!(left_items.iter().all(|item| matches!(
            item.packet(),
            StreamPacket::Data(packet) if packet.kind == rank_coordinate_data_kind()
        )));
    }

    fn stream_ordinal(item: &sim_lib_stream_core::StreamItem) -> Nat {
        let StreamPacket::Data(packet) = item.packet() else {
            panic!("expected data packet");
        };
        let Expr::Map(entries) = &packet.payload else {
            panic!("expected map payload");
        };
        let Some(Expr::Number(ordinal)) = entries.iter().find_map(|(key, value)| match key {
            Expr::Symbol(symbol)
                if symbol.namespace.is_none() && symbol.name.as_ref() == "ordinal" =>
            {
                Some(value)
            }
            _ => None,
        }) else {
            panic!("missing ordinal field");
        };
        Nat::from_number_literal(ordinal).unwrap()
    }
}

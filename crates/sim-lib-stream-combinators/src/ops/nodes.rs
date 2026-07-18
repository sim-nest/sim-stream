use std::sync::{Arc, Mutex};

use sim_kernel::{Diagnostic, Error, Expr, Result};
use sim_lib_stream_core::{
    ClockTickIndex, StreamDirection, StreamItem, StreamMedia, StreamMetadata, StreamPacket,
};

use crate::stream::{Stream, StreamNode};

use super::{
    ClockConvertFn, DataMapFn, DiagnosticTapFn, MapFn, MergeKeyFn, PredicateFn, ShapePredicateFn,
    TapFn, stream_window_data_kind,
};

mod fan;

pub(super) use fan::fan_readers;

pub(super) fn map_with(source: Stream, f: MapFn) -> Stream {
    Stream::new(MapNode { source, f })
}

pub(super) fn map_data_expr_with(source: Stream, f: DataMapFn) -> Stream {
    Stream::new(DataMapNode { source, f })
}

pub(super) fn filter_with(source: Stream, pred: PredicateFn) -> Stream {
    Stream::new(FilterNode { source, pred })
}

pub(super) fn filter_data_shape_with(source: Stream, matches: ShapePredicateFn) -> Stream {
    Stream::new(DataShapeFilterNode { source, matches })
}

pub(super) fn tap_with(source: Stream, f: TapFn) -> Stream {
    Stream::new(TapNode { source, f })
}

pub(super) fn tap_diagnostics_with(source: Stream, f: DiagnosticTapFn) -> Stream {
    Stream::new(DiagnosticTapNode { source, f })
}

pub(super) fn take_with_limit(source: Stream, limit: usize) -> Stream {
    Stream::new(TakeNode {
        source,
        remaining: Mutex::new(limit),
    })
}

pub(super) fn window_by_count(source: Stream, count: usize) -> Stream {
    let metadata = window_metadata(source.metadata());
    Stream::new(WindowNode {
        source,
        count,
        metadata,
        state: Mutex::new(WindowState::default()),
    })
}

pub(super) fn merge_with_key(left: Stream, right: Stream, key: MergeKeyFn) -> Stream {
    Stream::new(MergeNode {
        left,
        right,
        key,
        state: Mutex::new(MergeState {
            left: None,
            right: None,
        }),
    })
}

pub(super) fn clock_convert_stream(
    source: Stream,
    convert: ClockConvertFn,
    diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
) -> Stream {
    Stream::new(ClockConvertNode {
        source,
        convert,
        diagnostics,
    })
}

struct MapNode {
    source: Stream,
    f: MapFn,
}

impl StreamNode for MapNode {
    fn metadata(&self) -> &StreamMetadata {
        self.source.metadata()
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        self.source
            .next_packet()?
            .map(|item| (self.f)(item))
            .transpose()
    }

    fn is_done(&self) -> Result<bool> {
        self.source.is_done()
    }
}

struct DataMapNode {
    source: Stream,
    f: DataMapFn,
}

impl StreamNode for DataMapNode {
    fn metadata(&self) -> &StreamMetadata {
        self.source.metadata()
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        let Some(item) = self.source.next_packet()? else {
            return Ok(None);
        };
        map_data_item(item, &self.f).map(Some)
    }

    fn is_done(&self) -> Result<bool> {
        self.source.is_done()
    }
}

struct FilterNode {
    source: Stream,
    pred: PredicateFn,
}

impl StreamNode for FilterNode {
    fn metadata(&self) -> &StreamMetadata {
        self.source.metadata()
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        while let Some(item) = self.source.next_packet()? {
            if (self.pred)(&item)? {
                return Ok(Some(item));
            }
        }
        Ok(None)
    }

    fn is_done(&self) -> Result<bool> {
        self.source.is_done()
    }
}

struct DataShapeFilterNode {
    source: Stream,
    matches: ShapePredicateFn,
}

impl StreamNode for DataShapeFilterNode {
    fn metadata(&self) -> &StreamMetadata {
        self.source.metadata()
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        while let Some(item) = self.source.next_packet()? {
            let StreamPacket::Data(packet) = item.packet() else {
                continue;
            };
            if (self.matches)(&packet.payload)? {
                return Ok(Some(item));
            }
        }
        Ok(None)
    }

    fn is_done(&self) -> Result<bool> {
        self.source.is_done()
    }
}

struct TapNode {
    source: Stream,
    f: TapFn,
}

impl StreamNode for TapNode {
    fn metadata(&self) -> &StreamMetadata {
        self.source.metadata()
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        let Some(item) = self.source.next_packet()? else {
            return Ok(None);
        };
        (self.f)(&item)?;
        Ok(Some(item))
    }

    fn is_done(&self) -> Result<bool> {
        self.source.is_done()
    }
}

struct DiagnosticTapNode {
    source: Stream,
    f: DiagnosticTapFn,
}

impl StreamNode for DiagnosticTapNode {
    fn metadata(&self) -> &StreamMetadata {
        self.source.metadata()
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        let Some(item) = self.source.next_packet()? else {
            return Ok(None);
        };
        if let StreamPacket::Diagnostic(diagnostic) = item.packet() {
            (self.f)(diagnostic)?;
        }
        Ok(Some(item))
    }

    fn is_done(&self) -> Result<bool> {
        self.source.is_done()
    }
}

struct TakeNode {
    source: Stream,
    remaining: Mutex<usize>,
}

impl StreamNode for TakeNode {
    fn metadata(&self) -> &StreamMetadata {
        self.source.metadata()
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        let mut remaining = self
            .remaining
            .lock()
            .map_err(|_| Error::PoisonedLock("take stream"))?;
        if *remaining == 0 {
            return Ok(None);
        }
        let item = self.source.next_packet()?;
        if item.is_some() {
            *remaining -= 1;
        }
        Ok(item)
    }

    fn is_done(&self) -> Result<bool> {
        let remaining = self
            .remaining
            .lock()
            .map_err(|_| Error::PoisonedLock("take stream"))?;
        Ok(*remaining == 0 || self.source.is_done()?)
    }
}

struct WindowNode {
    source: Stream,
    count: usize,
    metadata: StreamMetadata,
    state: Mutex<WindowState>,
}

#[derive(Default)]
struct WindowState {
    items: Vec<StreamItem>,
    source_done: bool,
}

impl StreamNode for WindowNode {
    fn metadata(&self) -> &StreamMetadata {
        &self.metadata
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        if self.count == 0 {
            return Err(Error::Eval(
                "stream/window count must be greater than zero".to_owned(),
            ));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("window stream"))?;
        while state.items.len() < self.count && !state.source_done {
            match self.source.next_packet()? {
                Some(item) => state.items.push(item),
                None => {
                    if self.source.is_done()? {
                        state.source_done = true;
                    } else {
                        return Ok(None);
                    }
                }
            }
        }
        if state.items.is_empty() {
            return Ok(None);
        }
        if state.items.len() < self.count && !state.source_done {
            return Ok(None);
        }
        let items = std::mem::take(&mut state.items);
        window_item(items).map(Some)
    }

    fn is_done(&self) -> Result<bool> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("window stream"))?;
        Ok(state.items.is_empty() && (state.source_done || self.source.is_done()?))
    }
}

struct MergeNode {
    left: Stream,
    right: Stream,
    key: MergeKeyFn,
    state: Mutex<MergeState>,
}

struct MergeState {
    left: Option<StreamItem>,
    right: Option<StreamItem>,
}

impl StreamNode for MergeNode {
    fn metadata(&self) -> &StreamMetadata {
        self.left.metadata()
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("merge stream"))?;
        if state.left.is_none() {
            state.left = self.left.next_packet()?;
        }
        if state.right.is_none() {
            state.right = self.right.next_packet()?;
        }
        Ok(match (&state.left, &state.right) {
            (None, None) => None,
            (Some(_), None) => state.left.take(),
            (None, Some(_)) => state.right.take(),
            (Some(left), Some(right)) => {
                if merge_key(left, &self.key)? <= merge_key(right, &self.key)? {
                    state.left.take()
                } else {
                    state.right.take()
                }
            }
        })
    }

    fn is_done(&self) -> Result<bool> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("merge stream"))?;
        Ok(state.left.is_none()
            && state.right.is_none()
            && self.left.is_done()?
            && self.right.is_done()?)
    }
}

fn map_data_item(item: StreamItem, f: &DataMapFn) -> Result<StreamItem> {
    let ticks = item.ticks().to_vec();
    let packet = match item.packet().clone() {
        StreamPacket::Data(mut packet) => {
            packet.payload = (f)(packet.payload)?;
            StreamPacket::Data(packet)
        }
        other => other,
    };
    StreamItem::with_ticks(packet, ticks)
}

fn window_item(items: Vec<StreamItem>) -> Result<StreamItem> {
    let ticks = items
        .last()
        .map(|item| item.ticks().to_vec())
        .unwrap_or_default();
    let payload = Expr::List(
        items
            .iter()
            .map(|item| item.packet().to_expr())
            .collect::<Vec<_>>(),
    );
    StreamItem::with_ticks(
        StreamPacket::data(stream_window_data_kind(), payload),
        ticks,
    )
}

fn window_metadata(source: &StreamMetadata) -> StreamMetadata {
    StreamMetadata::new(
        source.id().clone(),
        StreamMedia::Data,
        StreamDirection::Source,
        source.clock().clone(),
        source.buffer().clone(),
    )
}

fn merge_key(item: &StreamItem, key: &MergeKeyFn) -> Result<Option<ClockTickIndex>> {
    key(item)
}

struct ClockConvertNode {
    source: Stream,
    convert: ClockConvertFn,
    diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
}

impl StreamNode for ClockConvertNode {
    fn metadata(&self) -> &StreamMetadata {
        self.source.metadata()
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        let Some(item) = self.source.next_packet()? else {
            return Ok(None);
        };
        let (ticks, mut diagnostics) = (self.convert)(&item)?;
        if !diagnostics.is_empty() {
            self.diagnostics
                .lock()
                .map_err(|_| Error::PoisonedLock("clock-convert diagnostics"))?
                .append(&mut diagnostics);
        }
        StreamItem::with_ticks(item.packet().clone(), ticks).map(Some)
    }

    fn is_done(&self) -> Result<bool> {
        self.source.is_done()
    }
}

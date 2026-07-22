use std::sync::Mutex;

use sim_kernel::{Args, Cx, Error, Expr, Result, Symbol, Value};
use sim_lib_stream_combinators::stream_window_data_kind;
use sim_lib_stream_core::{
    StreamDirection, StreamItem, StreamMedia, StreamMetadata, StreamPacket, StreamStats,
};

use crate::handle::StreamHandle;

pub(crate) struct TransformSource {
    metadata: StreamMetadata,
    kind: TransformKind,
    state: Mutex<TransformState>,
}

enum TransformKind {
    FilterKind { source: StreamHandle, kind: Symbol },
    FilterShape { source: StreamHandle, shape: Value },
    MapExpr { source: StreamHandle, mapper: Value },
    Window { source: StreamHandle, count: usize },
}

#[derive(Default)]
struct TransformState {
    stats: StreamStats,
    window: Vec<StreamItem>,
    source_done: bool,
}

impl TransformSource {
    pub(crate) fn filter_kind(source: StreamHandle, kind: Symbol) -> Self {
        Self::new(
            source.metadata().clone(),
            TransformKind::FilterKind { source, kind },
        )
    }

    pub(crate) fn filter_shape(source: StreamHandle, shape: Value) -> Self {
        Self::new(
            source.metadata().clone(),
            TransformKind::FilterShape { source, shape },
        )
    }

    pub(crate) fn map_expr(source: StreamHandle, mapper: Value) -> Self {
        Self::new(
            source.metadata().clone(),
            TransformKind::MapExpr { source, mapper },
        )
    }

    pub(crate) fn window(source: StreamHandle, count: usize) -> Self {
        Self::new(
            window_metadata(source.metadata()),
            TransformKind::Window { source, count },
        )
    }

    pub(crate) fn metadata(&self) -> &StreamMetadata {
        &self.metadata
    }

    pub(crate) fn next_packet(&self, cx: &mut Cx) -> Result<Option<StreamItem>> {
        if self.cancelled()? {
            return Ok(None);
        }
        match &self.kind {
            TransformKind::FilterKind { source, kind } => self.next_filter_kind(cx, source, kind),
            TransformKind::FilterShape { source, shape } => {
                self.next_filter_shape(cx, source, shape)
            }
            TransformKind::MapExpr { source, mapper } => self.next_map_expr(cx, source, mapper),
            TransformKind::Window { source, count } => self.next_window(cx, source, *count),
        }
    }

    pub(crate) fn cancel(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream transform"))?;
        state.stats.closed = true;
        state.stats.cancelled = true;
        state.window.clear();
        state.source_done = true;
        Ok(())
    }

    pub(crate) fn stats(&self) -> Result<StreamStats> {
        self.state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream transform"))
            .map(|state| state.stats.clone())
    }

    pub(crate) fn done(&self) -> Result<bool> {
        let source = self.source();
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream transform"))?;
        if state.stats.cancelled || state.stats.closed {
            return Ok(true);
        }
        if source.done()? {
            state.source_done = true;
        }
        let done = match &self.kind {
            TransformKind::Window { .. } => state.source_done && state.window.is_empty(),
            _ => state.source_done,
        };
        if done {
            state.stats.closed = true;
        }
        Ok(done)
    }

    fn new(metadata: StreamMetadata, kind: TransformKind) -> Self {
        Self {
            metadata,
            kind,
            state: Mutex::new(TransformState::default()),
        }
    }

    fn source(&self) -> &StreamHandle {
        match &self.kind {
            TransformKind::FilterKind { source, .. }
            | TransformKind::FilterShape { source, .. }
            | TransformKind::MapExpr { source, .. }
            | TransformKind::Window { source, .. } => source,
        }
    }

    fn next_filter_kind(
        &self,
        cx: &mut Cx,
        source: &StreamHandle,
        kind: &Symbol,
    ) -> Result<Option<StreamItem>> {
        loop {
            match source.next_packet_with_cx(cx)? {
                Some(item) if data_kind(&item) == Some(kind) => return self.record_yield(item),
                Some(_) => {}
                None => return self.handle_source_gap(source),
            }
        }
    }

    fn next_filter_shape(
        &self,
        cx: &mut Cx,
        source: &StreamHandle,
        shape: &Value,
    ) -> Result<Option<StreamItem>> {
        loop {
            let Some(item) = source.next_packet_with_cx(cx)? else {
                return self.handle_source_gap(source);
            };
            let StreamPacket::Data(packet) = item.packet() else {
                continue;
            };
            let shape_ref = shape.object().as_shape().ok_or(Error::TypeMismatch {
                expected: "shape",
                found: "non-shape",
            })?;
            if shape_ref.check_expr(cx, &packet.payload)?.accepted {
                return self.record_yield(item);
            }
        }
    }

    fn next_map_expr(
        &self,
        cx: &mut Cx,
        source: &StreamHandle,
        mapper: &Value,
    ) -> Result<Option<StreamItem>> {
        let Some(item) = source.next_packet_with_cx(cx)? else {
            return self.handle_source_gap(source);
        };
        self.record_yield(map_data_payload(cx, item, mapper)?)
    }

    fn next_window(
        &self,
        cx: &mut Cx,
        source: &StreamHandle,
        count: usize,
    ) -> Result<Option<StreamItem>> {
        if count == 0 {
            return Err(Error::Eval(
                "stream/window count must be greater than zero".to_owned(),
            ));
        }
        loop {
            if let Some(item) = self.take_ready_window(count)? {
                return self.record_yield(item);
            }
            if self.window_source_done()? {
                self.mark_closed()?;
                return Ok(None);
            }
            match source.next_packet_with_cx(cx)? {
                Some(item) => self.push_window(item)?,
                None => {
                    if source.done()? {
                        self.mark_source_done()?;
                    } else {
                        return Ok(None);
                    }
                }
            }
        }
    }

    fn handle_source_gap(&self, source: &StreamHandle) -> Result<Option<StreamItem>> {
        if source.done()? {
            self.mark_source_done()?;
            self.mark_closed()?;
        }
        Ok(None)
    }

    fn cancelled(&self) -> Result<bool> {
        self.state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream transform"))
            .map(|state| state.stats.cancelled)
    }

    fn record_yield(&self, item: StreamItem) -> Result<Option<StreamItem>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream transform"))?;
        state.stats.yielded += 1;
        Ok(Some(item))
    }

    fn mark_source_done(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream transform"))?;
        state.source_done = true;
        Ok(())
    }

    fn mark_closed(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream transform"))?;
        state.stats.closed = true;
        Ok(())
    }

    fn push_window(&self, item: StreamItem) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream transform"))?;
        state.window.push(item);
        Ok(())
    }

    fn window_source_done(&self) -> Result<bool> {
        self.state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream transform"))
            .map(|state| state.source_done && state.window.is_empty())
    }

    fn take_ready_window(&self, count: usize) -> Result<Option<StreamItem>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream transform"))?;
        if state.window.is_empty() {
            return Ok(None);
        }
        if state.window.len() < count && !state.source_done {
            return Ok(None);
        }
        let take_count = if state.window.len() >= count {
            count
        } else {
            state.window.len()
        };
        let items = state.window.drain(..take_count).collect::<Vec<_>>();
        window_item(items).map(Some)
    }
}

fn data_kind(item: &StreamItem) -> Option<&Symbol> {
    match item.packet() {
        StreamPacket::Data(packet) => Some(&packet.kind),
        _ => None,
    }
}

fn map_data_payload(cx: &mut Cx, item: StreamItem, mapper: &Value) -> Result<StreamItem> {
    let ticks = item.ticks().to_vec();
    let packet = match item.packet().clone() {
        StreamPacket::Data(mut packet) => {
            let payload = cx.factory().expr(packet.payload)?;
            let mapped = cx.call_value(mapper.clone(), Args::new(vec![payload]))?;
            packet.payload = mapped.object().as_expr(cx)?;
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

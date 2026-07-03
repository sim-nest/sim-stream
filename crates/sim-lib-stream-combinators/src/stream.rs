use std::sync::Arc;

use sim_kernel::{Cx, Event, Expr, Ref, Result, Symbol};
use sim_lib_stream_core::{StreamDiagnostic, StreamItem, StreamMetadata, StreamValue};

/// Lazy source of stream packets backing a [`Stream`].
///
/// A `StreamNode` is the pull-based engine behind a combinator: it exposes the
/// stream metadata and yields one [`StreamItem`] at a time, advancing only when
/// asked. Combinators wrap one or more upstream streams in their own node so
/// that work is deferred until packets are actually pulled. Implementations are
/// `Send + Sync` so a built graph can be shared across threads.
pub trait StreamNode: Send + Sync {
    /// Returns the metadata describing this node's output stream.
    fn metadata(&self) -> &StreamMetadata;
    /// Pulls the next packet, or `Ok(None)` when no packet is available yet.
    fn next_packet(&self) -> Result<Option<StreamItem>>;
    /// Reports whether the node has reached its terminal `done` state.
    fn is_done(&self) -> Result<bool>;
}

/// Cloneable handle to a lazy combinator stream.
///
/// A `Stream` is a thin shared pointer over a [`StreamNode`]: cloning it shares
/// the same underlying source rather than copying packets. It is the value that
/// every combinator in this crate consumes and produces, forming pull-based
/// graphs over the homogeneous `sim-stream` packet spine.
///
/// # Examples
///
/// ```
/// use sim_kernel::{Expr, Symbol};
/// use sim_lib_stream_core::{
///     BufferOverflowPolicy, BufferPolicy, StreamDirection, StreamItem, StreamMedia,
///     StreamMetadata, StreamPacket,
/// };
/// use sim_lib_stream_combinators::Stream;
///
/// let metadata = StreamMetadata::new(
///     Symbol::qualified("stream", "doc"),
///     StreamMedia::Data,
///     StreamDirection::Source,
///     Symbol::qualified("clock", "doc"),
///     BufferPolicy::bounded_with_overflow(8, BufferOverflowPolicy::DropNewest).unwrap(),
/// );
/// let item = StreamItem::new(StreamPacket::data(
///     Symbol::qualified("stream/data", "model-event"),
///     Expr::Nil,
/// ));
/// let stream = Stream::pull(metadata, vec![item.clone()]);
/// assert_eq!(stream.take_packets(8).unwrap(), vec![item]);
/// assert!(stream.is_done().unwrap());
/// ```
#[derive(Clone)]
pub struct Stream {
    inner: Arc<dyn StreamNode>,
}

impl Stream {
    /// Wraps a [`StreamNode`] implementation in a shareable `Stream` handle.
    pub fn new(inner: impl StreamNode + 'static) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Builds a stream that replays the packets held by a stream-core value.
    pub fn from_value(value: Arc<StreamValue>) -> Self {
        Self::new(ValueStream { value })
    }

    /// Builds an in-memory pull stream from explicit metadata and packets.
    pub fn pull(metadata: StreamMetadata, items: Vec<StreamItem>) -> Self {
        Self::from_value(Arc::new(StreamValue::pull(metadata, items)))
    }

    /// Returns the metadata describing this stream's media, clock, and buffer.
    pub fn metadata(&self) -> &StreamMetadata {
        self.inner.metadata()
    }

    /// Pulls the next packet, or `Ok(None)` when none is currently available.
    pub fn next_packet(&self) -> Result<Option<StreamItem>> {
        self.inner.next_packet()
    }

    /// Reports whether the stream has reached its terminal `done` state.
    pub fn is_done(&self) -> Result<bool> {
        self.inner.is_done()
    }

    /// Pulls up to `limit` packets, stopping early when the source is drained.
    pub fn take_packets(&self, limit: usize) -> Result<Vec<StreamItem>> {
        let mut out = Vec::new();
        for _ in 0..limit {
            let Some(item) = self.next_packet()? else {
                break;
            };
            out.push(item);
        }
        Ok(out)
    }

    /// Drains the stream into kernel events, one chunk event per packet.
    ///
    /// Sequence numbers start at `start_seq` and increase per packet; a final
    /// `done` event for `run` is appended once the source reports done.
    pub fn run_events(&self, cx: &mut Cx, run: Ref, start_seq: u64) -> Result<Vec<Event>> {
        let mut seq = start_seq;
        let mut out = Vec::new();
        while let Some(item) = self.next_packet()? {
            out.push(item.chunk_event(cx, run.clone(), seq)?);
            seq = seq.saturating_add(1);
        }
        if self.is_done()? {
            out.push(Event::done(run, seq)?);
        }
        Ok(out)
    }

    /// Returns a stream that rewrites each data packet's payload expression.
    ///
    /// Method form of the free [`map_data_expr`](crate::map_data_expr)
    /// combinator; non-data packets pass through unchanged.
    pub fn map_data_expr<F>(self, f: F) -> Self
    where
        F: Fn(Expr) -> Result<Expr> + Send + Sync + 'static,
    {
        crate::ops::map_data_expr(self, f)
    }

    /// Returns a stream keeping only data packets of the given `kind`.
    ///
    /// Method form of the free [`filter_data_kind`](crate::filter_data_kind)
    /// combinator.
    pub fn filter_data_kind(self, kind: Symbol) -> Self {
        crate::ops::filter_data_kind(self, kind)
    }

    /// Returns a stream keeping data packets whose payload matches `matches`.
    ///
    /// Method form of the free [`filter_data_shape`](crate::filter_data_shape)
    /// combinator.
    pub fn filter_data_shape<F>(self, matches: F) -> Self
    where
        F: Fn(&Expr) -> Result<bool> + Send + Sync + 'static,
    {
        crate::ops::filter_data_shape(self, matches)
    }

    /// Returns a stream that batches packets into windows of `count` packets.
    ///
    /// Method form of the free [`window_by_count`](crate::window_by_count)
    /// combinator.
    pub fn window_by_count(self, count: usize) -> Self {
        crate::ops::window_by_count(self, count)
    }

    /// Returns a stream that observes each diagnostic packet without altering it.
    ///
    /// Method form of the free [`tap_diagnostics`](crate::tap_diagnostics)
    /// combinator.
    pub fn tap_diagnostics<F>(self, f: F) -> Self
    where
        F: Fn(&StreamDiagnostic) -> Result<()> + Send + Sync + 'static,
    {
        crate::ops::tap_diagnostics(self, f)
    }
}

struct ValueStream {
    value: Arc<StreamValue>,
}

impl StreamNode for ValueStream {
    fn metadata(&self) -> &StreamMetadata {
        self.value.metadata()
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        self.value.next_packet()
    }

    fn is_done(&self) -> Result<bool> {
        self.value.is_done()
    }
}

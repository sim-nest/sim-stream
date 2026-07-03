mod nodes;

use std::sync::{Arc, Mutex};

use sim_kernel::{Cx, Diagnostic, Error, Event, Expr, Ref, Result, Symbol, Tick};
use sim_lib_stream_core::{StreamDiagnostic, StreamItem, StreamPacket};

use crate::stream::Stream;

/// A reusable, composable transformation from one [`Stream`] to another.
///
/// Stages are the building blocks fed to [`pipe`]: each `*_stage` constructor
/// captures its configuration into a boxed closure so the same transformation
/// can be applied to multiple sources.
pub type StreamStage = Box<dyn Fn(Stream) -> Stream + Send + Sync>;

type MapFn = Arc<dyn Fn(StreamItem) -> Result<StreamItem> + Send + Sync>;
type DataMapFn = Arc<dyn Fn(Expr) -> Result<Expr> + Send + Sync>;
type PredicateFn = Arc<dyn Fn(&StreamItem) -> Result<bool> + Send + Sync>;
type ShapePredicateFn = Arc<dyn Fn(&Expr) -> Result<bool> + Send + Sync>;
type TapFn = Arc<dyn Fn(&StreamItem) -> Result<()> + Send + Sync>;
type DiagnosticTapFn = Arc<dyn Fn(&StreamDiagnostic) -> Result<()> + Send + Sync>;
type MergeKeyFn = Arc<dyn Fn(&StreamItem) -> Option<Ref> + Send + Sync>;
type ClockConvertFn =
    Arc<dyn Fn(&StreamItem) -> Result<(Vec<Tick>, Vec<Diagnostic>)> + Send + Sync>;

/// Applies a sequence of [`StreamStage`] transforms left-to-right to `source`.
///
/// # Examples
///
/// ```
/// use sim_kernel::{Expr, Symbol};
/// use sim_lib_stream_core::{
///     BufferOverflowPolicy, BufferPolicy, StreamDirection, StreamItem, StreamMedia,
///     StreamMetadata, StreamPacket,
/// };
/// use sim_lib_stream_combinators::{identity, pipe, take_stage, Stream};
///
/// let metadata = StreamMetadata::new(
///     Symbol::qualified("stream", "doc"),
///     StreamMedia::Data,
///     StreamDirection::Source,
///     Symbol::qualified("clock", "doc"),
///     BufferPolicy::bounded_with_overflow(8, BufferOverflowPolicy::DropNewest).unwrap(),
/// );
/// let item = || StreamItem::new(StreamPacket::data(
///     Symbol::qualified("stream/data", "model-event"),
///     Expr::Nil,
/// ));
/// let stream = Stream::pull(metadata, vec![item(), item(), item()]);
///
/// let out = pipe(stream, vec![identity(), take_stage(2)]);
/// assert_eq!(out.take_packets(8).unwrap().len(), 2);
/// ```
pub fn pipe(source: Stream, stages: Vec<StreamStage>) -> Stream {
    stages
        .into_iter()
        .fold(source, |stream, stage| stage(stream))
}

/// Returns a stage that forwards its source stream unchanged.
pub fn identity() -> StreamStage {
    Box::new(|stream| stream)
}

/// Returns a stream that applies `f` to every packet of `source`.
pub fn map<F>(source: Stream, f: F) -> Stream
where
    F: Fn(StreamItem) -> Result<StreamItem> + Send + Sync + 'static,
{
    nodes::map_with(source, Arc::new(f))
}

/// Returns a reusable stage form of [`map`].
pub fn map_stage<F>(f: F) -> StreamStage
where
    F: Fn(StreamItem) -> Result<StreamItem> + Send + Sync + 'static,
{
    let f: MapFn = Arc::new(f);
    Box::new(move |source| nodes::map_with(source, Arc::clone(&f)))
}

/// Returns a stream that rewrites each data packet's payload expression with `f`.
///
/// Non-data packets pass through untouched.
pub fn map_data_expr<F>(source: Stream, f: F) -> Stream
where
    F: Fn(Expr) -> Result<Expr> + Send + Sync + 'static,
{
    nodes::map_data_expr_with(source, Arc::new(f))
}

/// Returns a reusable stage form of [`map_data_expr`].
pub fn map_data_expr_stage<F>(f: F) -> StreamStage
where
    F: Fn(Expr) -> Result<Expr> + Send + Sync + 'static,
{
    let f: DataMapFn = Arc::new(f);
    Box::new(move |source| nodes::map_data_expr_with(source, Arc::clone(&f)))
}

/// Returns a stream keeping only packets for which `pred` holds.
pub fn filter<F>(source: Stream, pred: F) -> Stream
where
    F: Fn(&StreamItem) -> Result<bool> + Send + Sync + 'static,
{
    nodes::filter_with(source, Arc::new(pred))
}

/// Returns a reusable stage form of [`filter`].
pub fn filter_stage<F>(pred: F) -> StreamStage
where
    F: Fn(&StreamItem) -> Result<bool> + Send + Sync + 'static,
{
    let pred: PredicateFn = Arc::new(pred);
    Box::new(move |source| nodes::filter_with(source, Arc::clone(&pred)))
}

/// Returns a stream keeping only data packets whose kind equals `kind`.
pub fn filter_data_kind(source: Stream, kind: Symbol) -> Stream {
    nodes::filter_with(
        source,
        Arc::new(move |item| match item.packet() {
            StreamPacket::Data(packet) => Ok(packet.kind == kind),
            _ => Ok(false),
        }),
    )
}

/// Returns a reusable stage form of [`filter_data_kind`].
pub fn filter_data_kind_stage(kind: Symbol) -> StreamStage {
    Box::new(move |source| filter_data_kind(source, kind.clone()))
}

/// Returns a stream keeping only data packets whose payload matches `matches`.
///
/// Non-data packets are dropped.
pub fn filter_data_shape<F>(source: Stream, matches: F) -> Stream
where
    F: Fn(&Expr) -> Result<bool> + Send + Sync + 'static,
{
    nodes::filter_data_shape_with(source, Arc::new(matches))
}

/// Returns a reusable stage form of [`filter_data_shape`].
pub fn filter_data_shape_stage<F>(matches: F) -> StreamStage
where
    F: Fn(&Expr) -> Result<bool> + Send + Sync + 'static,
{
    let matches: ShapePredicateFn = Arc::new(matches);
    Box::new(move |source| nodes::filter_data_shape_with(source, Arc::clone(&matches)))
}

/// Returns a stream that runs `f` on each packet as a side effect, unchanged.
pub fn tap<F>(source: Stream, f: F) -> Stream
where
    F: Fn(&StreamItem) -> Result<()> + Send + Sync + 'static,
{
    nodes::tap_with(source, Arc::new(f))
}

/// Returns a reusable stage form of [`tap`].
pub fn tap_stage<F>(f: F) -> StreamStage
where
    F: Fn(&StreamItem) -> Result<()> + Send + Sync + 'static,
{
    let f: TapFn = Arc::new(f);
    Box::new(move |source| nodes::tap_with(source, Arc::clone(&f)))
}

/// Returns a stream that runs `f` on each diagnostic packet, leaving it intact.
///
/// Non-diagnostic packets pass through without invoking `f`.
pub fn tap_diagnostics<F>(source: Stream, f: F) -> Stream
where
    F: Fn(&StreamDiagnostic) -> Result<()> + Send + Sync + 'static,
{
    nodes::tap_diagnostics_with(source, Arc::new(f))
}

/// Returns a reusable stage form of [`tap_diagnostics`].
pub fn tap_diagnostics_stage<F>(f: F) -> StreamStage
where
    F: Fn(&StreamDiagnostic) -> Result<()> + Send + Sync + 'static,
{
    let f: DiagnosticTapFn = Arc::new(f);
    Box::new(move |source| nodes::tap_diagnostics_with(source, Arc::clone(&f)))
}

/// Returns a stream that yields at most the first `limit` packets of `source`.
pub fn take(source: Stream, limit: usize) -> Stream {
    nodes::take_with_limit(source, limit)
}

/// Returns a reusable stage form of [`take`].
pub fn take_stage(limit: usize) -> StreamStage {
    Box::new(move |source| take(source, limit))
}

/// Returns a stream that batches packets into windows of `count` packets each.
///
/// Each window is emitted as a data packet of [`stream_window_data_kind`]
/// whose payload lists the windowed packets; a trailing partial window is kept.
pub fn window_by_count(source: Stream, count: usize) -> Stream {
    nodes::window_by_count(source, count)
}

/// Returns a reusable stage form of [`window_by_count`].
pub fn window_by_count_stage(count: usize) -> StreamStage {
    Box::new(move |source| window_by_count(source, count))
}

/// Returns the canonical data-packet kind emitted by [`window_by_count`].
pub fn stream_window_data_kind() -> Symbol {
    Symbol::qualified("stream/data", "window")
}

/// Returns a stream interleaving `left` and `right` in pull arrival order.
///
/// With no clock key, available packets are emitted left-before-right.
pub fn merge(left: Stream, right: Stream) -> Stream {
    nodes::merge_with_key(left, right, Arc::new(|_| None))
}

/// Returns a stream merging `left` and `right` ordered by their `clock` tick.
///
/// At each step the packet with the lower tick index on `clock` is emitted
/// first; packets without that clock tick sort as having no key.
pub fn merge_by_clock(left: Stream, right: Stream, clock: Symbol) -> Stream {
    nodes::merge_with_key(
        left,
        right,
        Arc::new(move |item| {
            item.ticks()
                .iter()
                .find(|tick| tick.clock == clock)
                .map(|tick| tick.index.clone())
        }),
    )
}

/// Two independent readers that each see every packet of a fanned-out source.
pub struct Fanout {
    /// First reader over the shared source.
    pub left: Stream,
    /// Second reader over the shared source.
    pub right: Stream,
}

/// Splits `source` into a `Fanout` of two readers that each see all packets.
///
/// Packets pulled by one reader are buffered so the other reader still observes
/// the full sequence regardless of read order.
pub fn fan(source: Stream) -> Fanout {
    let (left, right) = nodes::fan_readers(source);
    Fanout { left, right }
}

/// A clock-converted stream paired with the diagnostics its conversion emits.
///
/// The conversion closure may report lossy or approximate clock mappings; those
/// diagnostics accumulate as packets are pulled and can be read back via
/// [`ClockConvertedStream::diagnostics`].
pub struct ClockConvertedStream {
    stream: Stream,
    diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
}

impl ClockConvertedStream {
    /// Borrows the underlying converted stream.
    pub fn stream(&self) -> &Stream {
        &self.stream
    }

    /// Consumes this wrapper and returns the underlying converted stream.
    pub fn into_stream(self) -> Stream {
        self.stream
    }

    /// Pulls the next converted packet from the underlying stream.
    pub fn next_packet(&self) -> Result<Option<StreamItem>> {
        self.stream.next_packet()
    }

    /// Returns the diagnostics accumulated by the conversion so far.
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>> {
        self.diagnostics
            .lock()
            .map_err(|_| Error::PoisonedLock("clock-convert diagnostics"))
            .map(|diagnostics| diagnostics.clone())
    }
}

/// Rewrites each packet's ticks via `convert`, collecting its diagnostics.
///
/// For every packet, `convert` returns the replacement ticks and any
/// diagnostics describing the conversion; the diagnostics are gathered into the
/// returned [`ClockConvertedStream`].
pub fn clock_convert<F>(source: Stream, convert: F) -> ClockConvertedStream
where
    F: Fn(&StreamItem) -> Result<(Vec<Tick>, Vec<Diagnostic>)> + Send + Sync + 'static,
{
    let diagnostics = Arc::new(Mutex::new(Vec::new()));
    ClockConvertedStream {
        stream: nodes::clock_convert_stream(source, Arc::new(convert), Arc::clone(&diagnostics)),
        diagnostics,
    }
}

/// Drains `stream` into kernel events for `run`, starting at `start_seq`.
///
/// Free-function form of [`Stream::run_events`](crate::Stream::run_events): one
/// chunk event per packet followed by a `done` event when the source completes.
pub fn run_bang(stream: &Stream, cx: &mut Cx, run: Ref, start_seq: u64) -> Result<Vec<Event>> {
    stream.run_events(cx, run, start_seq)
}

use std::sync::Arc;

use sim_kernel::{Expr, Symbol};
use sim_shape::{
    AnyShape, ExactExprShape, ExprKind, ExprKindShape, OrShape, RepeatShape, Shape,
    TableExtraPolicy, TableFieldSpec, TableShape,
};

use crate::buffer::{BackpressureOutcome, BufferOverflowPolicy};
use crate::envelope::{
    ClockDomain, LatencyClass, STREAM_ENVELOPE_VERSION, StreamCapability,
    stream_envelope_tag_symbol,
};
use crate::metadata::{StreamDirection, StreamMedia};

pub(super) fn metadata_shape() -> Arc<dyn Shape> {
    table(vec![
        required_exact_symbol("kind", sim_kernel::stream_surface::stream_kind()),
        required("id", string_shape()),
        required("media", media_shape()),
        required("direction", direction_shape()),
        required("clock", symbol_shape()),
        required("buffer", buffer_policy_shape()),
    ])
}

pub(super) fn envelope_shape() -> Arc<dyn Shape> {
    table(vec![
        required_exact_symbol("envelope", stream_envelope_tag_symbol()),
        required_exact_string("version", STREAM_ENVELOPE_VERSION.to_string()),
        required("stream-id", symbol_shape()),
        required("packet-id", symbol_shape()),
        required("media", media_shape()),
        required("direction", direction_shape()),
        required("sequence", string_shape()),
        required("ticks", list_of(clock_shape())),
        required("clock-domain", clock_domain_shape()),
        required("clock-domains", list_of(clock_domain_shape())),
        required("profile", transport_profile_shape()),
        required("diagnostics", list_of(symbol_shape())),
        required("packet", packet_shape()),
    ])
}

pub(super) fn media_shape() -> Arc<dyn Shape> {
    exact_symbols([
        StreamMedia::Pcm.symbol(),
        StreamMedia::Midi.symbol(),
        StreamMedia::Diagnostic.symbol(),
        StreamMedia::Data.symbol(),
    ])
}

fn direction_shape() -> Arc<dyn Shape> {
    exact_symbols([
        StreamDirection::Source.symbol(),
        StreamDirection::Sink.symbol(),
        StreamDirection::Duplex.symbol(),
    ])
}

pub(super) fn clock_domain_shape() -> Arc<dyn Shape> {
    exact_symbols([
        ClockDomain::Sample.symbol(),
        ClockDomain::Block.symbol(),
        ClockDomain::Control.symbol(),
        ClockDomain::MidiTick.symbol(),
        ClockDomain::Wall.symbol(),
        ClockDomain::Transport.symbol(),
        ClockDomain::ServerFrame.symbol(),
        ClockDomain::BrowserFrame.symbol(),
        ClockDomain::TraceStep.symbol(),
        ClockDomain::Job.symbol(),
    ])
}

pub(super) fn latency_class_shape() -> Arc<dyn Shape> {
    exact_symbols([
        LatencyClass::OfflineRender.symbol(),
        LatencyClass::BlockLocal.symbol(),
        LatencyClass::Interactive.symbol(),
        LatencyClass::SampleExact.symbol(),
        LatencyClass::BufferedPreview.symbol(),
        LatencyClass::CollabBarDelay.symbol(),
        LatencyClass::RemoteCollaboration.symbol(),
    ])
}

pub(super) fn capability_shape() -> Arc<dyn Shape> {
    exact_symbols([
        StreamCapability::Exact.symbol(),
        StreamCapability::Deterministic.symbol(),
        StreamCapability::Realtime.symbol(),
        StreamCapability::Bounded.symbol(),
        StreamCapability::Remote.symbol(),
        StreamCapability::Replayable.symbol(),
        StreamCapability::Preview.symbol(),
        StreamCapability::Persistent.symbol(),
        StreamCapability::Resumable.symbol(),
        StreamCapability::Lossy.symbol(),
    ])
}

pub(super) fn backpressure_shape() -> Arc<dyn Shape> {
    exact_symbols([
        BackpressureOutcome::Accepted.symbol(),
        BackpressureOutcome::DroppedNewest.symbol(),
        BackpressureOutcome::DroppedOldest.symbol(),
        BackpressureOutcome::Blocked.symbol(),
        BackpressureOutcome::TimedOut.symbol(),
        BackpressureOutcome::Rejected.symbol(),
        BackpressureOutcome::Closed.symbol(),
    ])
}

pub(super) fn clock_shape() -> Arc<dyn Shape> {
    table(vec![
        required("clock", symbol_shape()),
        required("index", any_shape()),
    ])
}

pub(super) fn tempo_shape() -> Arc<dyn Shape> {
    table(vec![required("segments", list_shape())])
}

pub(super) fn buffer_policy_shape() -> Arc<dyn Shape> {
    table(vec![
        required("capacity", string_shape()),
        required("overflow", buffer_overflow_shape()),
    ])
}

fn buffer_overflow_shape() -> Arc<dyn Shape> {
    exact_symbols([
        BufferOverflowPolicy::DropNewest.symbol(),
        BufferOverflowPolicy::DropOldest.symbol(),
        BufferOverflowPolicy::Error.symbol(),
    ])
}

pub(super) fn packet_shape() -> Arc<dyn Shape> {
    table(vec![required("packet", packet_tag_shape())])
}

pub(super) fn data_packet_shape() -> Arc<dyn Shape> {
    table(vec![
        required_exact_symbol("packet", Symbol::qualified("stream/packet", "data")),
        required("kind", symbol_shape()),
        required("payload", any_shape()),
    ])
}

pub(super) fn diagnostic_shape() -> Arc<dyn Shape> {
    table(vec![
        required_exact_symbol("packet", Symbol::qualified("stream/packet", "diagnostic")),
        required("kind", symbol_shape()),
        required("message", string_shape()),
    ])
}

fn packet_tag_shape() -> Arc<dyn Shape> {
    exact_symbols([
        Symbol::qualified("stream/packet", "pcm"),
        Symbol::qualified("stream/packet", "midi"),
        Symbol::qualified("stream/packet", "diagnostic"),
        Symbol::qualified("stream/packet", "data"),
    ])
}

fn transport_profile_shape() -> Arc<dyn Shape> {
    table(vec![
        required("name", symbol_shape()),
        required("latency-class", latency_class_shape()),
        required("capabilities", list_of(capability_shape())),
    ])
}

fn table(fields: Vec<TableFieldSpec>) -> Arc<dyn Shape> {
    Arc::new(TableShape::new(fields, TableExtraPolicy::Allow))
}

fn required(key: &str, shape: Arc<dyn Shape>) -> TableFieldSpec {
    TableFieldSpec {
        key: Symbol::new(key),
        shape,
        required: true,
    }
}

fn required_exact_symbol(key: &str, symbol: Symbol) -> TableFieldSpec {
    required(key, exact_symbol(symbol))
}

fn required_exact_string(key: &str, value: String) -> TableFieldSpec {
    required(key, Arc::new(ExactExprShape::new(Expr::String(value))))
}

fn exact_symbols(symbols: impl IntoIterator<Item = Symbol>) -> Arc<dyn Shape> {
    let choices = symbols.into_iter().map(exact_symbol).collect();
    Arc::new(OrShape::new(choices))
}

fn exact_symbol(symbol: Symbol) -> Arc<dyn Shape> {
    Arc::new(ExactExprShape::new(Expr::Symbol(symbol)))
}

fn any_shape() -> Arc<dyn Shape> {
    Arc::new(AnyShape)
}

fn symbol_shape() -> Arc<dyn Shape> {
    Arc::new(ExprKindShape::new(ExprKind::Symbol))
}

fn string_shape() -> Arc<dyn Shape> {
    Arc::new(ExprKindShape::new(ExprKind::String))
}

fn list_shape() -> Arc<dyn Shape> {
    Arc::new(ExprKindShape::new(ExprKind::List))
}

fn list_of(item: Arc<dyn Shape>) -> Arc<dyn Shape> {
    Arc::new(RepeatShape::new(item))
}

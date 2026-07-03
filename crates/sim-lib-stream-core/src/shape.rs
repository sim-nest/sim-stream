//! Shape protocol integration for the stream-core types.
//!
//! The kernel defines the [`Shape`] protocol -- its one shared engine for
//! parsing, checking, binding, and dispatch. This module supplies the concrete
//! behavior of registering stream-core's types as first-class shape targets
//! without redefining that contract: [`StreamCoreShapesLib`] is a loadable
//! [`Lib`] that exports one named shape per stream type, and
//! [`install_stream_core_shapes_lib`] loads it idempotently into a [`Cx`].
//!
//! Each registered shape is a `DocumentedShape` that delegates all matching
//! to the kernel's total [`AnyShape`] and contributes only descriptive
//! [`ShapeDoc`] metadata; the `stream_*_shape_symbol` accessors expose the
//! stable [`Symbol`] under which each type's shape is registered.

use std::sync::Arc;

use sim_kernel::{
    AbiVersion, Cx, Export, Lib, LibManifest, LibTarget, Linker, Result, Symbol, Version,
};
use sim_shape::{AnyShape, Shape, ShapeDoc, shape_value};

const STREAM_CORE_SHAPES_LIB_ID: &str = "stream-core-shapes";

/// Loadable library that registers the stream-core types as kernel shapes.
///
/// Implements [`Lib`]: its manifest exports one [`Export::Shape`] per stream
/// type, and loading binds each export's [`Symbol`] to a documented shape value
/// in the [`Linker`]. Prefer [`install_stream_core_shapes_lib`] for idempotent
/// installation.
pub struct StreamCoreShapesLib;

impl Lib for StreamCoreShapesLib {
    fn manifest(&self) -> LibManifest {
        LibManifest {
            id: Symbol::new(STREAM_CORE_SHAPES_LIB_ID),
            version: Version(env!("CARGO_PKG_VERSION").to_owned()),
            abi: AbiVersion { major: 0, minor: 1 },
            target: LibTarget::HostRegistered,
            requires: Vec::new(),
            capabilities: Vec::new(),
            exports: shape_specs()
                .into_iter()
                .map(|(symbol, _, _)| Export::Shape {
                    symbol,
                    shape_id: None,
                })
                .collect(),
        }
    }

    fn load(&self, _cx: &mut sim_kernel::LoadCx, linker: &mut Linker<'_>) -> Result<()> {
        for (symbol, name, details) in shape_specs() {
            linker.shape_value(
                symbol.clone(),
                shape_value(symbol, Arc::new(DocumentedShape::new(name, details))),
            )?;
        }
        Ok(())
    }
}

/// Installs [`StreamCoreShapesLib`] into `cx`, skipping if already loaded.
///
/// Idempotent: returns `Ok(())` immediately when the library is already
/// registered, otherwise loads it.
pub fn install_stream_core_shapes_lib(cx: &mut Cx) -> Result<()> {
    if cx
        .registry()
        .lib(&Symbol::new(STREAM_CORE_SHAPES_LIB_ID))
        .is_some()
    {
        return Ok(());
    }
    cx.load_lib(&StreamCoreShapesLib).map(|_| ())
}

fn shape_specs() -> Vec<(Symbol, &'static str, Vec<&'static str>)> {
    vec![
        (
            stream_metadata_shape_symbol(),
            "StreamMetadata",
            vec![
                "stream metadata read-construct surface",
                "fields: id, media, direction, clock, buffer",
            ],
        ),
        (
            stream_envelope_shape_symbol(),
            "StreamEnvelope",
            vec![
                "versioned stream packet envelope",
                "fields: stream id, packet id, media, direction, sequence, ticks, primary clock domain, clock domains, profile, diagnostics, packet",
            ],
        ),
        (
            stream_media_shape_symbol(),
            "StreamMedia",
            vec![
                "stream media symbol used by metadata",
                "known media include pcm, midi, diagnostic, and data",
            ],
        ),
        (
            stream_clock_domain_shape_symbol(),
            "ClockDomain",
            vec![
                "shared timing vocabulary for envelopes, stream descriptors, and placement",
                "known domains include sample, block, control, midi-tick, wall, transport, server-frame, browser-frame, trace-step, and job",
            ],
        ),
        (
            stream_latency_class_shape_symbol(),
            "LatencyClass",
            vec![
                "shared latency vocabulary for streams and placement",
                "known classes include offline-render, block-local, interactive, sample-exact, buffered-preview, collab-bardelay, and remote-collaboration",
            ],
        ),
        (
            stream_capability_shape_symbol(),
            "StreamCapability",
            vec![
                "stream transport capability flags",
                "known flags include exact, deterministic, realtime, bounded, remote, replayable, preview, persistent, resumable, and lossy",
            ],
        ),
        (
            stream_backpressure_shape_symbol(),
            "BackpressureOutcome",
            vec![
                "shared stream queue outcome vocabulary",
                "known outcomes include accepted, dropped-newest, dropped-oldest, blocked, timed-out, rejected, and closed",
            ],
        ),
        (
            stream_clock_shape_symbol(),
            "StreamClock",
            vec![
                "clock chart descriptor shared by frame and MIDI indexes",
                "kernel stream events still carry KERNEL 6 Tick values",
            ],
        ),
        (
            stream_tempo_shape_symbol(),
            "StreamTempo",
            vec![
                "tempo map descriptor for MIDI clock conversion",
                "segments require a tick-zero anchor and increasing ticks",
            ],
        ),
        (
            stream_buffer_policy_shape_symbol(),
            "StreamBufferPolicy",
            vec![
                "bounded stream buffer policy",
                "capacity plus overflow behavior map",
            ],
        ),
        (
            stream_packet_shape_symbol(),
            "StreamPacket",
            vec![
                "tagged packet map for PCM, MIDI, diagnostics, and data",
                "codec round trips preserve packet tags and payload fields",
            ],
        ),
        (
            stream_data_packet_shape_symbol(),
            "DataPacket",
            vec![
                "generic runtime data packet",
                "fields: packet stream/packet/data, kind symbol, payload expr",
            ],
        ),
        (
            stream_diagnostic_shape_symbol(),
            "StreamDiagnostic",
            vec![
                "diagnostic packet payload",
                "kind symbol plus message string",
            ],
        ),
    ]
}

/// Returns the registration symbol for the `StreamMetadata` shape.
pub fn stream_metadata_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "Metadata")
}

/// Returns the registration symbol for the `StreamEnvelope` shape.
pub fn stream_envelope_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "Envelope")
}

/// Returns the registration symbol for the `StreamMedia` shape.
pub fn stream_media_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "Media")
}

/// Returns the registration symbol for the `ClockDomain` shape.
pub fn stream_clock_domain_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "ClockDomain")
}

/// Returns the registration symbol for the `LatencyClass` shape.
pub fn stream_latency_class_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "LatencyClass")
}

/// Returns the registration symbol for the `StreamCapability` shape.
pub fn stream_capability_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "Capability")
}

/// Returns the registration symbol for the `BackpressureOutcome` shape.
pub fn stream_backpressure_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "BackpressureOutcome")
}

/// Returns the registration symbol for the `StreamClock` shape.
pub fn stream_clock_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "Clock")
}

/// Returns the registration symbol for the `StreamTempo` shape.
pub fn stream_tempo_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "Tempo")
}

/// Returns the registration symbol for the `StreamBufferPolicy` shape.
pub fn stream_buffer_policy_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "BufferPolicy")
}

/// Returns the registration symbol for the `StreamPacket` shape.
pub fn stream_packet_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "Packet")
}

/// Returns the registration symbol for the `DataPacket` shape.
pub fn stream_data_packet_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "DataPacket")
}

/// Returns the registration symbol for the `StreamDiagnostic` shape.
pub fn stream_diagnostic_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "Diagnostic")
}

struct DocumentedShape {
    name: &'static str,
    details: Vec<&'static str>,
}

impl DocumentedShape {
    fn new(name: &'static str, details: Vec<&'static str>) -> Self {
        Self { name, details }
    }
}

impl Shape for DocumentedShape {
    fn is_total(&self) -> bool {
        AnyShape.is_total()
    }

    fn check_value(
        &self,
        cx: &mut sim_kernel::Cx,
        value: sim_kernel::Value,
    ) -> Result<sim_shape::ShapeMatch> {
        AnyShape.check_value(cx, value)
    }

    fn check_expr(
        &self,
        cx: &mut sim_kernel::Cx,
        expr: &sim_kernel::Expr,
    ) -> Result<sim_shape::ShapeMatch> {
        AnyShape.check_expr(cx, expr)
    }

    fn describe(&self, _cx: &mut sim_kernel::Cx) -> Result<ShapeDoc> {
        let mut doc = ShapeDoc::new(self.name);
        for detail in &self.details {
            doc = doc.with_detail(*detail);
        }
        Ok(doc)
    }
}

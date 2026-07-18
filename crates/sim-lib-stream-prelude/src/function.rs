use std::sync::Arc;

mod helpers;

use sim_kernel::{
    AbiVersion, Args, Callable, ClassRef, Cx, Error, Export, Expr, Lib, LibManifest, LibTarget,
    Linker, Object, ObjectCompat, RawArgs, Result, Symbol, Value, Version,
};
use sim_lib_stream_combinators::{filter_data_kind, window_by_count};
use sim_lib_stream_core::{
    StreamPacket, install_stream_core_classes, install_stream_core_shapes_lib,
    stream_cancel_symbol, stream_metadata_symbol, stream_next_symbol, stream_run_symbol,
    stream_stats_symbol,
};

use crate::{
    cap::{
        stream_cancel_capability, stream_open_capability, stream_push_capability,
        stream_read_capability, stream_stats_capability, stream_transform_capability,
    },
    card::{stats_value, stream_card},
    handle::{StageHandle, StreamHandle},
    live::StreamRuntime,
    live_control::{
        cancel_older_than_fn, cell_fn, cell_set_fn, cell_value_fn, describe_fn,
        explain_diagnostic_fn, graph_lisp_fn, list_fn, reroute_fn,
    },
    spec::{memory_specs_value, open_spec_from_expr},
};

use helpers::{
    collect_stream_to_handle, data_expr, ensure_done, eval_value, handle_arg, handle_stream,
    handle_value_from_items, map_data_payload, run_report_value, symbol_arg, usize_arg,
};

const STREAM_PRELUDE_LIB_ID: &str = "stream-prelude";

/// Host-registered library that installs the STREAM 6 prelude functions.
///
/// [`StreamPreludeLib`] reports a [`LibManifest`] exporting the memory-spec
/// catalog value plus every capability-gated stream function, and on load it
/// links those functions and the `stream/memory-specs` value into the runtime.
/// Prefer [`install_stream_prelude_lib`] over loading this type directly; it
/// also installs the underlying stream-core classes and shapes.
pub struct StreamPreludeLib;

impl Lib for StreamPreludeLib {
    fn manifest(&self) -> LibManifest {
        LibManifest {
            id: manifest_name(),
            version: Version(env!("CARGO_PKG_VERSION").to_owned()),
            abi: AbiVersion { major: 0, minor: 1 },
            target: LibTarget::HostRegistered,
            requires: Vec::new(),
            capabilities: Vec::new(),
            exports: stream_prelude_exports(),
        }
    }

    fn load(&self, cx: &mut sim_kernel::LoadCx, linker: &mut Linker<'_>) -> Result<()> {
        let runtime = Arc::new(StreamRuntime::default());
        for (symbol, implementation) in function_table() {
            linker.function_value(
                symbol.clone(),
                cx.factory().opaque(Arc::new(StreamFunction {
                    symbol,
                    implementation,
                    runtime: Arc::clone(&runtime),
                }))?,
            )?;
        }
        linker.value(stream_memory_specs_symbol(), memory_specs_value(cx)?)?;
        Ok(())
    }
}

/// Installs the stream prelude and its prerequisites into `cx`.
///
/// This installs the stream-core classes and shapes, then loads
/// [`StreamPreludeLib`] exactly once. After it returns, the `stream/*`
/// functions and the `stream/memory-specs` catalog are available to evaluated
/// code.
pub fn install_stream_prelude_lib(cx: &mut Cx) -> Result<()> {
    install_stream_core_classes(cx)?;
    install_stream_core_shapes_lib(cx)?;
    sim_lib_core::install_once(cx, &StreamPreludeLib).map(|_| ())
}

/// Returns the export records advertised by the prelude library.
///
/// The list contains the `stream/memory-specs` value export followed by one
/// function export per entry in the prelude function table.
pub fn stream_prelude_exports() -> Vec<Export> {
    let mut exports = vec![Export::Value {
        symbol: stream_memory_specs_symbol(),
    }];
    exports.extend(
        function_table()
            .into_iter()
            .map(|(symbol, _)| Export::Function {
                symbol,
                function_id: None,
            }),
    );
    exports
}

/// Returns the manifest id symbol of the prelude library (`stream-prelude`).
pub fn manifest_name() -> Symbol {
    Symbol::new(STREAM_PRELUDE_LIB_ID)
}

/// Returns the `stream/open` function symbol.
///
/// `stream/open` builds a memory stream handle from a memory-spec table.
///
/// # Examples
///
/// ```
/// use sim_lib_stream_prelude::stream_open_symbol;
///
/// assert_eq!(stream_open_symbol().as_qualified_str(), "stream/open");
/// ```
pub fn stream_open_symbol() -> Symbol {
    Symbol::qualified("stream", "open")
}

/// Returns the `stream/write!` function symbol.
///
/// `stream/write!` pushes a single packet into a sink handle.
pub fn stream_write_symbol() -> Symbol {
    Symbol::qualified("stream", "write!")
}

/// Returns the `stream/pipe` function symbol.
///
/// `stream/pipe` connects a source handle to optional stages and at most one
/// sink, producing a pipeline handle.
pub fn stream_pipe_symbol() -> Symbol {
    Symbol::qualified("stream", "pipe")
}

/// Returns the `stream/card` function symbol.
///
/// `stream/card` renders a browseable Card for a stream handle.
pub fn stream_card_symbol() -> Symbol {
    Symbol::qualified("stream", "card")
}

/// Returns the `stream/sink-packets` function symbol.
///
/// `stream/sink-packets` reads back the packets accumulated by a sink handle.
pub fn stream_sink_packets_symbol() -> Symbol {
    Symbol::qualified("stream", "sink-packets")
}

/// Returns the `stream/memory-specs` value symbol.
///
/// `stream/memory-specs` names the catalog value describing every supported
/// memory source/sink spec and its required fields.
pub fn stream_memory_specs_symbol() -> Symbol {
    Symbol::qualified("stream", "memory-specs")
}

fn stream_identity_symbol() -> Symbol {
    Symbol::qualified("stream", "identity")
}

fn stream_list_symbol() -> Symbol {
    Symbol::qualified("stream", "list")
}

fn stream_describe_symbol() -> Symbol {
    Symbol::qualified("stream", "describe")
}

fn stream_graph_lisp_symbol() -> Symbol {
    Symbol::qualified("stream", "graph-lisp")
}

fn stream_explain_diagnostic_symbol() -> Symbol {
    Symbol::qualified("stream", "explain-diagnostic")
}

fn stream_cell_symbol() -> Symbol {
    Symbol::qualified("stream", "cell")
}

fn stream_cell_value_symbol() -> Symbol {
    Symbol::qualified("stream", "cell-value")
}

fn stream_cell_set_symbol() -> Symbol {
    Symbol::qualified("stream", "cell-set!")
}

fn stream_reroute_symbol() -> Symbol {
    Symbol::qualified("stream", "reroute!")
}

fn stream_cancel_older_than_symbol() -> Symbol {
    Symbol::qualified("stream", "cancel-older-than!")
}

fn stream_filter_kind_symbol() -> Symbol {
    Symbol::qualified("stream", "filter-kind")
}

fn stream_filter_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "filter-shape")
}

fn stream_map_expr_symbol() -> Symbol {
    Symbol::qualified("stream", "map-expr")
}

fn stream_window_symbol() -> Symbol {
    Symbol::qualified("stream", "window")
}

fn function_table() -> Vec<(Symbol, StreamFn)> {
    vec![
        (stream_open_symbol(), open_fn),
        (stream_next_symbol(), next_fn),
        (stream_write_symbol(), write_fn),
        (stream_pipe_symbol(), pipe_fn),
        (stream_identity_symbol(), identity_fn),
        (stream_run_symbol(), run_fn),
        (stream_cancel_symbol(), cancel_fn),
        (stream_stats_symbol(), stats_fn),
        (stream_metadata_symbol(), metadata_fn),
        (stream_card_symbol(), card_fn),
        (stream_sink_packets_symbol(), sink_packets_fn),
        (stream_list_symbol(), list_fn),
        (stream_describe_symbol(), describe_fn),
        (stream_graph_lisp_symbol(), graph_lisp_fn),
        (stream_explain_diagnostic_symbol(), explain_diagnostic_fn),
        (stream_cell_symbol(), cell_fn),
        (stream_cell_value_symbol(), cell_value_fn),
        (stream_cell_set_symbol(), cell_set_fn),
        (stream_reroute_symbol(), reroute_fn),
        (stream_cancel_older_than_symbol(), cancel_older_than_fn),
        (stream_filter_kind_symbol(), filter_kind_fn),
        (stream_filter_shape_symbol(), filter_shape_fn),
        (stream_map_expr_symbol(), map_expr_fn),
        (stream_window_symbol(), window_fn),
    ]
}

type StreamFn = fn(&StreamRuntime, &mut Cx, &[Expr]) -> Result<Value>;

struct StreamFunction {
    symbol: Symbol,
    implementation: StreamFn,
    runtime: Arc<StreamRuntime>,
}

impl Object for StreamFunction {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok(format!("#<function {}>", self.symbol))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for StreamFunction {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        cx.factory().class_stub(
            sim_kernel::CORE_FUNCTION_CLASS_ID,
            Symbol::qualified("core", "Function"),
        )
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }
}

impl Callable for StreamFunction {
    fn call(&self, cx: &mut Cx, args: Args) -> Result<Value> {
        let exprs = args
            .into_vec()
            .into_iter()
            .map(|value| value.object().as_expr(cx))
            .collect::<Result<Vec<_>>>()?;
        (self.implementation)(&self.runtime, cx, &exprs)
    }

    fn call_exprs(&self, cx: &mut Cx, args: RawArgs) -> Result<Value> {
        (self.implementation)(&self.runtime, cx, args.exprs())
    }
}

fn open_fn(runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_open_capability())?;
    let [spec] = args else {
        return Err(Error::Eval(
            "stream/open expects one memory spec".to_owned(),
        ));
    };
    let spec = open_spec_from_expr(data_expr(cx, spec)?)?;
    let handle = spec.into_handle(cx)?;
    runtime.register_stream(handle.clone())?;
    cx.factory().opaque(Arc::new(handle))
}

fn next_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_read_capability())?;
    let [stream] = args else {
        return Err(Error::Eval(
            "stream/next! expects one stream handle".to_owned(),
        ));
    };
    match handle_arg(cx, stream)?.next_packet()? {
        Some(item) => cx.factory().expr(item.packet().to_expr()),
        None => cx.factory().nil(),
    }
}

fn write_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_push_capability())?;
    let [sink, packet] = args else {
        return Err(Error::Eval(
            "stream/write! expects a sink handle and packet".to_owned(),
        ));
    };
    let sink = handle_arg(cx, sink)?;
    let packet = StreamPacket::try_from(data_expr(cx, packet)?)?;
    sink.write_packet(packet)?;
    cx.factory().bool(true)
}

fn pipe_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    let [source, rest @ ..] = args else {
        return Err(Error::Eval(
            "stream/pipe expects a source handle".to_owned(),
        ));
    };
    let source = handle_arg(cx, source)?;
    let mut sink = None;
    for expr in rest {
        let value = eval_value(cx, expr)?;
        if let Some(stage) = value.object().downcast_ref::<StageHandle>() {
            if !stage.is_identity() {
                return Err(Error::Eval("unsupported stream stage".to_owned()));
            }
            continue;
        }
        let handle = value
            .object()
            .downcast_ref::<StreamHandle>()
            .cloned()
            .ok_or(Error::TypeMismatch {
                expected: "stream stage or sink handle",
                found: "non-stream-pipeline-argument",
            })?;
        if sink.replace(handle).is_some() {
            return Err(Error::Eval(
                "stream/pipe accepts at most one sink handle in STR6.8".to_owned(),
            ));
        }
    }
    cx.factory()
        .opaque(Arc::new(StreamHandle::pipeline(source, sink)?))
}

fn identity_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    if !args.is_empty() {
        return Err(Error::Eval(
            "stream/identity expects no arguments".to_owned(),
        ));
    }
    cx.factory().opaque(Arc::new(StageHandle::identity()))
}

fn run_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    let [stream] = args else {
        return Err(Error::Eval(
            "stream/run! expects one stream handle".to_owned(),
        ));
    };
    let stream = handle_arg(cx, stream)?;
    cx.require(&stream_read_capability())?;
    if stream.is_pipeline_with_sink() {
        cx.require(&stream_push_capability())?;
    }
    run_report_value(cx, stream.run()?)
}

fn cancel_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_cancel_capability())?;
    let [stream] = args else {
        return Err(Error::Eval(
            "stream/cancel! expects one stream handle".to_owned(),
        ));
    };
    handle_arg(cx, stream)?.cancel()?;
    cx.factory().nil()
}

fn stats_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_stats_capability())?;
    let [stream] = args else {
        return Err(Error::Eval(
            "stream/stats expects one stream handle".to_owned(),
        ));
    };
    let handle = handle_arg(cx, stream)?;
    let stats = handle.stats()?;
    stats_value(cx, &stats)
}

fn metadata_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_read_capability())?;
    let [stream] = args else {
        return Err(Error::Eval(
            "stream/metadata expects one stream handle".to_owned(),
        ));
    };
    let handle = handle_arg(cx, stream)?;
    cx.factory().expr(handle.metadata().table_expr())
}

fn card_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_read_capability())?;
    cx.require(&stream_stats_capability())?;
    let [stream] = args else {
        return Err(Error::Eval(
            "stream/card expects one stream handle".to_owned(),
        ));
    };
    let handle = handle_arg(cx, stream)?;
    stream_card(cx, &handle)
}

fn sink_packets_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_read_capability())?;
    let [sink] = args else {
        return Err(Error::Eval(
            "stream/sink-packets expects one sink handle".to_owned(),
        ));
    };
    let packets = handle_arg(cx, sink)?.sink_packets()?;
    cx.factory().list(
        packets
            .into_iter()
            .map(|packet| cx.factory().expr(packet.to_expr()))
            .collect::<Result<Vec<_>>>()?,
    )
}

fn filter_kind_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_read_capability())?;
    let [stream, kind] = args else {
        return Err(Error::Eval(
            "stream/filter-kind expects a stream handle and data kind".to_owned(),
        ));
    };
    let source = handle_arg(cx, stream)?;
    let kind = symbol_arg(cx, kind)?;
    let transformed = filter_data_kind(handle_stream(source), kind);
    collect_stream_to_handle(cx, transformed)
}

fn filter_shape_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_read_capability())?;
    cx.require(&stream_transform_capability())?;
    let [stream, shape] = args else {
        return Err(Error::Eval(
            "stream/filter-shape expects a stream handle and shape".to_owned(),
        ));
    };
    let source = handle_arg(cx, stream)?;
    let shape = eval_value(cx, shape)?;
    if shape.object().as_shape().is_none() {
        return Err(Error::TypeMismatch {
            expected: "shape",
            found: "non-shape",
        });
    }
    let metadata = source.metadata().clone();
    let mut items = Vec::new();
    while let Some(item) = source.next_packet()? {
        let StreamPacket::Data(packet) = item.packet() else {
            continue;
        };
        let shape_ref = shape.object().as_shape().ok_or(Error::TypeMismatch {
            expected: "shape",
            found: "non-shape",
        })?;
        if shape_ref.check_expr(cx, &packet.payload)?.accepted {
            items.push(item);
        }
    }
    ensure_done(&source, "stream/filter-shape")?;
    handle_value_from_items(cx, metadata, items)
}

fn map_expr_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_read_capability())?;
    cx.require(&stream_transform_capability())?;
    let [stream, mapper] = args else {
        return Err(Error::Eval(
            "stream/map-expr expects a stream handle and callable".to_owned(),
        ));
    };
    let source = handle_arg(cx, stream)?;
    let mapper = eval_value(cx, mapper)?;
    if mapper.object().as_callable().is_none() {
        return Err(Error::TypeMismatch {
            expected: "callable",
            found: "non-callable",
        });
    }
    let metadata = source.metadata().clone();
    let mut items = Vec::new();
    while let Some(item) = source.next_packet()? {
        items.push(map_data_payload(cx, item, mapper.clone())?);
    }
    ensure_done(&source, "stream/map-expr")?;
    handle_value_from_items(cx, metadata, items)
}

fn window_fn(_runtime: &StreamRuntime, cx: &mut Cx, args: &[Expr]) -> Result<Value> {
    cx.require(&stream_read_capability())?;
    let [stream, count] = args else {
        return Err(Error::Eval(
            "stream/window expects a stream handle and count".to_owned(),
        ));
    };
    let count = usize_arg(cx, count)?;
    if count == 0 {
        return Err(Error::Eval(
            "stream/window count must be greater than zero".to_owned(),
        ));
    }
    let source = handle_arg(cx, stream)?;
    let transformed = window_by_count(handle_stream(source), count);
    collect_stream_to_handle(cx, transformed)
}

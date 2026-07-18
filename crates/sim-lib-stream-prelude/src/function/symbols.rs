use sim_kernel::Symbol;

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

pub(super) fn stream_identity_symbol() -> Symbol {
    Symbol::qualified("stream", "identity")
}

pub(super) fn stream_list_symbol() -> Symbol {
    Symbol::qualified("stream", "list")
}

pub(super) fn stream_describe_symbol() -> Symbol {
    Symbol::qualified("stream", "describe")
}

pub(super) fn stream_graph_lisp_symbol() -> Symbol {
    Symbol::qualified("stream", "graph-lisp")
}

pub(super) fn stream_explain_diagnostic_symbol() -> Symbol {
    Symbol::qualified("stream", "explain-diagnostic")
}

pub(super) fn stream_cell_symbol() -> Symbol {
    Symbol::qualified("stream", "cell")
}

pub(super) fn stream_cell_value_symbol() -> Symbol {
    Symbol::qualified("stream", "cell-value")
}

pub(super) fn stream_cell_set_symbol() -> Symbol {
    Symbol::qualified("stream", "cell-set!")
}

pub(super) fn stream_reroute_symbol() -> Symbol {
    Symbol::qualified("stream", "reroute!")
}

pub(super) fn stream_advance_catalog_time_symbol() -> Symbol {
    Symbol::qualified("stream", "advance-catalog-time!")
}

pub(super) fn stream_cancel_older_than_symbol() -> Symbol {
    Symbol::qualified("stream", "cancel-older-than!")
}

pub(super) fn stream_filter_kind_symbol() -> Symbol {
    Symbol::qualified("stream", "filter-kind")
}

pub(super) fn stream_filter_shape_symbol() -> Symbol {
    Symbol::qualified("stream", "filter-shape")
}

pub(super) fn stream_map_expr_symbol() -> Symbol {
    Symbol::qualified("stream", "map-expr")
}

pub(super) fn stream_window_symbol() -> Symbol {
    Symbol::qualified("stream", "window")
}

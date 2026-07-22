use sim_kernel::CapabilityName;
pub use sim_lib_stream_core::{
    stream_cancel_capability, stream_open_capability, stream_push_capability,
    stream_read_capability, stream_stats_capability,
};

/// Returns the canonical `stream.push` capability name gating sink writes.
///
/// Functions that push packets into a sink (for example `stream/write!` and a
/// pipeline that runs into a sink) require this capability.
pub fn stream_write_capability() -> CapabilityName {
    stream_push_capability()
}

/// Returns the `stream.control` capability name gating live control cells.
///
/// The live control surface (for example `stream/cell-set!`) requires this
/// capability before mutating a versioned control cell.
pub fn stream_control_capability() -> CapabilityName {
    CapabilityName::new("stream.control")
}

/// Returns the `stream.transform` capability name gating shape-aware stages.
///
/// Combinator stages that evaluate caller-supplied shapes or callables (for
/// example `stream/filter-shape` and `stream/map-expr`) require this
/// capability in addition to `stream.read`.
pub fn stream_transform_capability() -> CapabilityName {
    CapabilityName::new("stream.transform")
}

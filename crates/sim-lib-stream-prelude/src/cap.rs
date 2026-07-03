use sim_kernel::CapabilityName;

/// Returns the `stream.open` capability name gating source/sink construction.
///
/// The prelude's `stream/open` function requires this capability before it
/// builds a memory stream handle.
pub fn stream_open_capability() -> CapabilityName {
    CapabilityName::new("stream.open")
}

/// Returns the `stream.read` capability name gating packet reads.
///
/// Functions that pull packets (for example `stream/next!`, `stream/run!`, and
/// the combinator stages) require this capability.
pub fn stream_read_capability() -> CapabilityName {
    CapabilityName::new("stream.read")
}

/// Returns the `stream.write` capability name gating sink writes.
///
/// Functions that push packets into a sink (for example `stream/write!` and a
/// pipeline that runs into a sink) require this capability.
pub fn stream_write_capability() -> CapabilityName {
    CapabilityName::new("stream.write")
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

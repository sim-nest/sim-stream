#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Core stream envelopes, metadata, packets, and buffer values.
//!
//! This crate is intentionally small: it defines the in-memory value surface
//! that later stream clock, lazy-spine, media, and file crates build on. Packet
//! observation uses kernel event helpers and refs; no server frame kind or
//! media hook is hardwired into the kernel.

pub mod bridge;
pub mod buffer;
pub mod cassette;
mod citizen;
pub mod dev;
pub mod envelope;
pub mod inspector;
pub mod metadata;
pub mod packet;
pub mod read_construct;
pub mod security;
pub mod shape;
pub mod site;
pub mod spine;

/// Cookbook recipes for this crate, embedded at build time.
///
/// Holds the crate's `recipes/` directory as an in-binary
/// [`sim_cookbook::EmbeddedDir`] so the runtime can surface stream-core
/// examples without touching the filesystem.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

pub use bridge::{BridgeLatency, DomainBridgeDescriptor, DomainBridgeKind};
pub use buffer::{BackpressureOutcome, BufferOverflowPolicy, BufferPolicy};
pub use cassette::{
    STREAM_CASSETTE_EXTENSION, STREAM_CASSETTE_FIXTURE_ROOT, StreamCassette, StreamCassetteTiming,
    StreamGoldenFixtureReport, stream_cassette_format_symbol, stream_cassette_golden_extension,
    stream_cassette_golden_root,
};
pub use citizen::{StreamPacketDescriptor, stream_packet_class_symbol};
pub use dev::{
    DevCassette, DevEvent, DevFaultReport, MediaDescriptor, dev_dropped_chunks_diagnostic,
    dev_event_media, dev_event_metadata,
};
pub use envelope::{
    ClockDomain, LatencyClass, STREAM_ENVELOPE_VERSION, StreamCapability, StreamEnvelope,
    TransportProfile, stream_envelope_tag_symbol,
};
pub use inspector::{
    StreamFaultKind, StreamFaultPlan, StreamFaultResult, StreamFaultSpec, StreamInspectorSnapshot,
    StreamInspectorStatus, stream_fault_symbols, stream_inspector_model_symbol,
    stream_inspector_route_local_symbol, stream_inspector_status_symbols,
};
pub use metadata::{
    RateContract, StreamDirection, StreamMedia, StreamMetadata, publish_metadata_claims,
    stream_buffer_predicate, stream_direction_predicate, stream_id_predicate,
    stream_media_predicate,
};
pub use packet::{
    DataPacket, MidiPacket, MidiPacketEvent, PcmPacket, PcmSampleFormat, StreamDiagnostic,
    StreamPacket,
};
pub use read_construct::{
    StreamMetadataValue, install_stream_core_classes, stream_metadata_class_symbol,
};
pub use security::{
    StreamRedactionFinding, StreamRemoteLimits, StreamSecurityCapability, StreamSecurityPolicy,
    stream_cancel_capability, stream_host_device_capability, stream_lan_midi_capability,
    stream_open_capability, stream_push_capability, stream_read_capability,
    stream_redaction_finding_symbols, stream_remote_network_capability,
    stream_remote_preview_capability, stream_remote_render_capability,
    stream_security_capabilities, stream_security_capability_names, stream_stats_capability,
};
pub use shape::{
    StreamCoreShapesLib, install_stream_core_shapes_lib, stream_backpressure_shape_symbol,
    stream_buffer_policy_shape_symbol, stream_capability_shape_symbol,
    stream_clock_domain_shape_symbol, stream_clock_shape_symbol, stream_data_packet_shape_symbol,
    stream_diagnostic_shape_symbol, stream_envelope_shape_symbol,
    stream_latency_class_shape_symbol, stream_media_shape_symbol, stream_metadata_shape_symbol,
    stream_packet_shape_symbol, stream_tempo_shape_symbol,
};
pub use site::{PlacedFragment, StreamEdge, StreamEndpoint, StreamEndpointKind, stream_edge};
pub use spine::{
    PushResult, StreamEventSource, StreamItem, StreamStats, StreamValue, stream_cancel_bang,
    stream_cancel_symbol, stream_done_q, stream_done_symbol, stream_metadata,
    stream_metadata_symbol, stream_next_bang, stream_next_symbol, stream_peek_bang,
    stream_peek_symbol, stream_run_bang, stream_run_symbol, stream_stats, stream_stats_symbol,
    stream_take, stream_take_symbol,
};

#[cfg(test)]
mod bridge_tests;

#[cfg(test)]
mod dev_tests;

#[cfg(test)]
mod inspector_tests;

#[cfg(test)]
mod security_tests;

#[cfg(test)]
mod shape_tests;

#[cfg(test)]
mod site_tests;

#[cfg(test)]
mod tests;

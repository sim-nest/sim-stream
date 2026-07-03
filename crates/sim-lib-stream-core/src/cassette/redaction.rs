use sim_kernel::{ContentId, Coordinate, Expr, Ref, Result, Symbol, Tick};

use crate::{
    ClockDomain, StreamDiagnostic, StreamEnvelope, StreamMetadata, StreamPacket,
    StreamSecurityPolicy,
};

pub(super) fn redact_metadata(metadata: &StreamMetadata) -> StreamMetadata {
    StreamMetadata::new(
        redact_symbol(metadata.id()),
        metadata.media(),
        metadata.direction(),
        redact_clock_symbol(metadata.clock()),
        metadata.buffer().clone(),
    )
}

pub(super) fn redact_envelope(envelope: &StreamEnvelope) -> Result<StreamEnvelope> {
    StreamEnvelope::new_with_clock_domains(
        redact_symbol(envelope.stream_id()),
        redact_symbol(envelope.packet_id()),
        envelope.media(),
        envelope.direction(),
        envelope.sequence(),
        envelope.ticks().iter().map(redact_tick).collect(),
        envelope.clock_domain(),
        envelope.clock_domains().to_vec(),
        envelope.profile().clone(),
        envelope.diagnostics().iter().map(redact_symbol).collect(),
        redact_packet(envelope.packet()),
    )
}

pub(super) fn metadata_has_host_device(metadata: &StreamMetadata) -> bool {
    is_host_device_symbol(metadata.id()) || is_host_device_symbol(metadata.clock())
}

pub(super) fn envelope_has_host_device(envelope: &StreamEnvelope) -> bool {
    is_host_device_symbol(envelope.stream_id())
        || is_host_device_symbol(envelope.packet_id())
        || envelope.diagnostics().iter().any(is_host_device_symbol)
        || envelope.ticks().iter().any(tick_has_host_device)
        || packet_has_host_device(envelope.packet())
}

pub(super) fn redact_symbol(symbol: &Symbol) -> Symbol {
    if is_host_device_symbol(symbol) {
        Symbol::qualified("stream/redacted", "device")
    } else if StreamSecurityPolicy::default()
        .finding_for_text(&symbol.as_qualified_str())
        .is_some()
    {
        Symbol::qualified("stream/redacted", "security")
    } else {
        symbol.clone()
    }
}

pub(super) fn packet_has_private_payload(packet: &StreamPacket) -> bool {
    match packet {
        StreamPacket::Data(data) => {
            is_private_text(&data.kind.as_qualified_str())
                || expr_has_private_marker(&data.payload)
                || StreamSecurityPolicy::default()
                    .finding_for_expr(&data.payload)
                    .is_some()
        }
        StreamPacket::Diagnostic(diagnostic) => {
            is_private_text(&diagnostic.kind().as_qualified_str())
                || StreamSecurityPolicy::default()
                    .finding_for_text(diagnostic.message())
                    .is_some()
        }
        _ => false,
    }
}

pub(super) fn packet_has_host_device(packet: &StreamPacket) -> bool {
    match packet {
        StreamPacket::Data(data) => {
            is_host_device_symbol(&data.kind) || expr_has_host_device(&data.payload)
        }
        StreamPacket::Diagnostic(diagnostic) => {
            is_host_device_symbol(diagnostic.kind()) || is_host_device_text(diagnostic.message())
        }
        _ => false,
    }
}

pub(super) fn is_host_device_symbol(symbol: &Symbol) -> bool {
    is_host_device_text(&symbol.as_qualified_str())
}

fn redact_clock_symbol(symbol: &Symbol) -> Symbol {
    if is_host_device_symbol(symbol) {
        ClockDomain::ServerFrame.symbol()
    } else {
        symbol.clone()
    }
}

fn redact_packet(packet: &StreamPacket) -> StreamPacket {
    match packet {
        StreamPacket::Data(_) if packet_has_private_payload(packet) => StreamPacket::data(
            Symbol::qualified("stream/data", "redacted"),
            Expr::String("[redacted stream payload]".to_owned()),
        ),
        StreamPacket::Data(data) => {
            StreamPacket::data(redact_symbol(&data.kind), redact_expr(&data.payload))
        }
        StreamPacket::Diagnostic(diagnostic) => StreamPacket::Diagnostic(StreamDiagnostic::new(
            redact_symbol(diagnostic.kind()),
            redact_host_string(diagnostic.message()),
        )),
        other => other.clone(),
    }
}

fn redact_tick(tick: &Tick) -> Tick {
    Tick::new(redact_clock_symbol(&tick.clock), redact_ref(&tick.index))
}

fn redact_ref(reference: &Ref) -> Ref {
    match reference {
        Ref::Symbol(symbol) => Ref::Symbol(redact_symbol(symbol)),
        Ref::Content(content) => Ref::Content(redact_content_id(content)),
        Ref::Handle(handle) => Ref::Handle(*handle),
        Ref::Coord(coordinate) => Ref::Coord(Coordinate {
            space: redact_symbol(&coordinate.space),
            ordinal: redact_content_id(&coordinate.ordinal),
        }),
    }
}

fn redact_content_id(content: &ContentId) -> ContentId {
    ContentId::from_bytes(redact_symbol(&content.algorithm), content.bytes)
}

fn redact_expr(expr: &Expr) -> Expr {
    match expr {
        Expr::Symbol(symbol) => Expr::Symbol(redact_symbol(symbol)),
        Expr::Local(symbol) => Expr::Local(redact_symbol(symbol)),
        Expr::String(value) => Expr::String(redact_host_string(value)),
        Expr::List(items) => Expr::List(items.iter().map(redact_expr).collect()),
        Expr::Vector(items) => Expr::Vector(items.iter().map(redact_expr).collect()),
        Expr::Set(items) => Expr::Set(items.iter().map(redact_expr).collect()),
        Expr::Map(entries) => Expr::Map(
            entries
                .iter()
                .map(|(key, value)| (redact_expr(key), redact_expr(value)))
                .collect(),
        ),
        Expr::Call { operator, args } => Expr::Call {
            operator: Box::new(redact_expr(operator)),
            args: args.iter().map(redact_expr).collect(),
        },
        Expr::Infix {
            operator,
            left,
            right,
        } => Expr::Infix {
            operator: redact_symbol(operator),
            left: Box::new(redact_expr(left)),
            right: Box::new(redact_expr(right)),
        },
        Expr::Prefix { operator, arg } => Expr::Prefix {
            operator: redact_symbol(operator),
            arg: Box::new(redact_expr(arg)),
        },
        Expr::Postfix { operator, arg } => Expr::Postfix {
            operator: redact_symbol(operator),
            arg: Box::new(redact_expr(arg)),
        },
        Expr::Block(items) => Expr::Block(items.iter().map(redact_expr).collect()),
        Expr::Quote { mode, expr } => Expr::Quote {
            mode: *mode,
            expr: Box::new(redact_expr(expr)),
        },
        Expr::Annotated { expr, annotations } => Expr::Annotated {
            expr: Box::new(redact_expr(expr)),
            annotations: annotations
                .iter()
                .map(|(key, value)| (redact_symbol(key), redact_expr(value)))
                .collect(),
        },
        Expr::Extension { tag, payload } => Expr::Extension {
            tag: redact_symbol(tag),
            payload: Box::new(redact_expr(payload)),
        },
        other => other.clone(),
    }
}

fn redact_host_string(value: &str) -> String {
    if StreamSecurityPolicy::default()
        .finding_for_text(value)
        .is_some()
        || is_host_device_text(value)
    {
        "[redacted stream metadata]".to_owned()
    } else {
        value.to_owned()
    }
}

fn expr_has_private_marker(expr: &Expr) -> bool {
    match expr {
        Expr::Map(entries) => entries.iter().any(|(key, value)| {
            matches!(key, Expr::Symbol(symbol) if symbol.name.as_ref() == "private")
                && matches!(value, Expr::Bool(true))
        }),
        _ => false,
    }
}

fn expr_has_host_device(expr: &Expr) -> bool {
    match expr {
        Expr::Symbol(symbol) | Expr::Local(symbol) => is_host_device_symbol(symbol),
        Expr::String(value) => is_host_device_text(value),
        Expr::List(items) | Expr::Vector(items) | Expr::Set(items) | Expr::Block(items) => {
            items.iter().any(expr_has_host_device)
        }
        Expr::Map(entries) => entries
            .iter()
            .any(|(key, value)| expr_has_host_device(key) || expr_has_host_device(value)),
        Expr::Call { operator, args } => {
            expr_has_host_device(operator) || args.iter().any(expr_has_host_device)
        }
        Expr::Infix { left, right, .. } => {
            expr_has_host_device(left) || expr_has_host_device(right)
        }
        Expr::Prefix { arg, .. } | Expr::Postfix { arg, .. } => expr_has_host_device(arg),
        Expr::Quote { expr, .. } => expr_has_host_device(expr),
        Expr::Annotated { expr, annotations } => {
            expr_has_host_device(expr)
                || annotations
                    .iter()
                    .any(|(key, value)| is_host_device_symbol(key) || expr_has_host_device(value))
        }
        Expr::Extension { tag, payload } => {
            is_host_device_symbol(tag) || expr_has_host_device(payload)
        }
        _ => false,
    }
}

fn tick_has_host_device(tick: &Tick) -> bool {
    is_host_device_symbol(&tick.clock) || ref_has_host_device(&tick.index)
}

fn ref_has_host_device(reference: &Ref) -> bool {
    match reference {
        Ref::Symbol(symbol) => is_host_device_symbol(symbol),
        Ref::Content(content) => is_host_device_symbol(&content.algorithm),
        Ref::Handle(_) => false,
        Ref::Coord(coordinate) => {
            is_host_device_symbol(&coordinate.space)
                || is_host_device_symbol(&coordinate.ordinal.algorithm)
        }
    }
}

fn is_private_text(value: &str) -> bool {
    value.contains("private")
        || StreamSecurityPolicy::default()
            .finding_for_text(value)
            .is_some()
}

fn is_host_device_text(value: &str) -> bool {
    value.contains("/dev/")
        || value.contains("hw:")
        || value.contains("CoreAudio")
        || value.contains("ALSA")
        || value.contains("host-device")
        || value.starts_with("device/")
        || value.starts_with("host/device")
}

use std::sync::Arc;

use sim_kernel::{
    CapabilityName, Cx, DatumStore, Expr, Result, Symbol, Value,
    card::{Card, ref_value},
};
use sim_lib_stream_core::{
    StreamDiagnostic, StreamMetadata, StreamPacket, StreamStats, stream_cancel_capability,
    stream_push_capability, stream_read_capability, stream_stats_capability,
};

use crate::{
    handle::StreamHandle,
    live::{GraphHandle, LiveCell},
};

pub fn stream_card(cx: &mut Cx, handle: &StreamHandle) -> Result<Value> {
    let subject = handle.metadata().subject_ref();
    let stats = handle.stats()?;
    let ops = vec![
        cx.factory().symbol(Symbol::qualified("stream", "next!"))?,
        cx.factory().symbol(Symbol::qualified("stream", "run!"))?,
        cx.factory()
            .symbol(Symbol::qualified("stream", "cancel!"))?,
        cx.factory()
            .symbol(Symbol::qualified("stream", "describe"))?,
    ];
    let requires = vec![
        capability_symbol(cx, stream_read_capability())?,
        capability_symbol(cx, stream_push_capability())?,
        capability_symbol(cx, stream_cancel_capability())?,
        capability_symbol(cx, stream_stats_capability())?,
    ];
    let mut entries = base_card_entries(
        cx,
        &subject,
        Symbol::qualified("stream", "handle"),
        "browseable stream handle with memory runtime state",
        ops,
        requires,
    )?;
    entries.extend(vec![
        (
            field("metadata"),
            cx.factory().expr(metadata_expr(handle.metadata()))?,
        ),
        (field("stats"), stats_value(cx, &stats)?),
        (field("done"), cx.factory().bool(handle.done()?)?),
        (field("cancelled"), cx.factory().bool(stats.cancelled)?),
    ]);
    cx.factory().opaque(Arc::new(Card::new(subject, entries)))
}

pub fn stream_card_with_age(cx: &mut Cx, handle: &StreamHandle, age_seconds: u64) -> Result<Value> {
    let value = stream_card(cx, handle)?;
    let mut entries = value.object().as_table(cx)?.object().as_expr(cx)?;
    let Expr::Map(ref mut fields) = entries else {
        return Ok(value);
    };
    fields.push((
        field_expr("age-seconds"),
        Expr::String(age_seconds.to_string()),
    ));
    let subject = handle.metadata().subject_ref();
    cx.factory().opaque(Arc::new(Card::new(
        subject,
        fields
            .iter()
            .map(|(key, value)| {
                let Expr::Symbol(key) = key else {
                    unreachable!("stream card keys are symbols")
                };
                Ok((key.clone(), cx.factory().expr(value.clone())?))
            })
            .collect::<Result<Vec<_>>>()?,
    )))
}

pub fn packet_card(cx: &mut Cx, packet: &StreamPacket) -> Result<Value> {
    let subject = packet.intern_ref(cx)?;
    let packet_kind = packet_kind(packet);
    let mut entries = base_card_entries(
        cx,
        &subject,
        Symbol::qualified("stream", "packet"),
        "stream packet data with media-specific fields",
        Vec::new(),
        Vec::new(),
    )?;
    entries.extend(vec![
        (field("packet-kind"), cx.factory().symbol(packet_kind)?),
        (field("packet"), cx.factory().expr(packet.to_expr())?),
    ]);
    match packet {
        StreamPacket::Pcm(packet) => entries.extend(vec![
            (
                field("channels"),
                cx.factory().string(packet.channels().to_string())?,
            ),
            (
                field("frames"),
                cx.factory().string(packet.frames().to_string())?,
            ),
        ]),
        StreamPacket::Midi(packet) => entries.extend(vec![
            (field("tpq"), cx.factory().string(packet.tpq().to_string())?),
            (
                field("events"),
                cx.factory().string(packet.events().len().to_string())?,
            ),
        ]),
        StreamPacket::Diagnostic(diagnostic) => entries.extend(diagnostic_fields(cx, diagnostic)?),
        StreamPacket::Data(packet) => entries.extend(vec![
            (
                field("data-kind"),
                cx.factory().symbol(packet.kind.clone())?,
            ),
            (
                field("payload-shape"),
                cx.factory()
                    .symbol(payload_shape_summary(&packet.payload))?,
            ),
            (field("payload"), cx.factory().expr(packet.payload.clone())?),
        ]),
    }
    cx.factory().opaque(Arc::new(Card::new(subject, entries)))
}

pub fn diagnostic_card(
    cx: &mut Cx,
    diagnostic: &StreamDiagnostic,
    stream_id: Option<&Symbol>,
) -> Result<Value> {
    let packet = StreamPacket::Diagnostic(diagnostic.clone());
    let subject = packet.intern_ref(cx)?;
    let mut entries = base_card_entries(
        cx,
        &subject,
        Symbol::qualified("stream", "diagnostic"),
        "stream diagnostic packet with agent-readable explanation",
        Vec::new(),
        Vec::new(),
    )?;
    entries.extend(diagnostic_fields(cx, diagnostic)?);
    if let Some(stream_id) = stream_id {
        entries.push((
            field("stream-id"),
            cx.factory().string(stream_id.to_string())?,
        ));
    }
    entries.push((
        field("explanation"),
        cx.factory()
            .string(diagnostic_explanation(diagnostic, stream_id))?,
    ));
    cx.factory().opaque(Arc::new(Card::new(subject, entries)))
}

pub fn clock_card(cx: &mut Cx, clock: Symbol) -> Result<Value> {
    let subject = sim_kernel::Ref::Symbol(clock.clone());
    let mut entries = base_card_entries(
        cx,
        &subject,
        Symbol::qualified("stream", "clock"),
        "stream clock symbol used by packet ticks",
        Vec::new(),
        Vec::new(),
    )?;
    entries.push((field("clock"), cx.factory().symbol(clock)?));
    cx.factory().opaque(Arc::new(Card::new(subject, entries)))
}

pub fn cell_card(cx: &mut Cx, cell: &LiveCell) -> Result<Value> {
    let subject = sim_kernel::Ref::Symbol(cell.id().clone());
    let mut entries = base_card_entries(
        cx,
        &subject,
        Symbol::qualified("stream", "cell"),
        "versioned live stream control cell",
        vec![
            cx.factory()
                .symbol(Symbol::qualified("stream", "cell-value"))?,
            cx.factory()
                .symbol(Symbol::qualified("stream", "cell-set!"))?,
        ],
        vec![
            cx.factory()
                .symbol(Symbol::qualified("capability", "stream.control"))?,
        ],
    )?;
    entries.extend(vec![
        (field("id"), cx.factory().string(cell.id().to_string())?),
        (
            field("value"),
            cx.factory().string(cell.value()?.to_string())?,
        ),
        (
            field("version"),
            cx.factory().string(cell.version()?.to_string())?,
        ),
    ]);
    cx.factory().opaque(Arc::new(Card::new(subject, entries)))
}

pub fn graph_card(cx: &mut Cx, graph: &GraphHandle) -> Result<Value> {
    let subject = sim_kernel::Ref::Symbol(graph.id().clone());
    let mut entries = base_card_entries(
        cx,
        &subject,
        Symbol::qualified("stream", "graph"),
        "live stream graph or route descriptor",
        vec![
            cx.factory()
                .symbol(Symbol::qualified("stream", "graph-lisp"))?,
        ],
        Vec::new(),
    )?;
    entries.extend(vec![
        (field("id"), cx.factory().string(graph.id().to_string())?),
        (
            field("source"),
            cx.factory().string(graph.source().to_string())?,
        ),
        (
            field("target"),
            match graph.target() {
                Some(target) => cx.factory().string(target.to_string())?,
                None => cx.factory().nil()?,
            },
        ),
        (field("lisp"), cx.factory().expr(graph.lisp_expr())?),
    ]);
    cx.factory().opaque(Arc::new(Card::new(subject, entries)))
}

pub fn graph_expr_card(cx: &mut Cx, graph: Expr) -> Result<Value> {
    let datum = sim_kernel::Datum::try_from(graph.clone())?;
    let subject = sim_kernel::Ref::Content(cx.datum_store_mut().intern(datum)?);
    let mut entries = base_card_entries(
        cx,
        &subject,
        Symbol::qualified("stream", "graph"),
        "stream pipeline graph expression",
        vec![
            cx.factory()
                .symbol(Symbol::qualified("stream", "graph-lisp"))?,
        ],
        Vec::new(),
    )?;
    entries.push((field("lisp"), cx.factory().expr(graph)?));
    cx.factory().opaque(Arc::new(Card::new(subject, entries)))
}

pub fn diagnostic_explanation(diagnostic: &StreamDiagnostic, stream_id: Option<&Symbol>) -> String {
    match stream_id {
        Some(stream_id) => format!(
            "stream {stream_id} reported {}: {}",
            diagnostic.kind(),
            diagnostic.message()
        ),
        None => format!(
            "stream diagnostic {}: {}",
            diagnostic.kind(),
            diagnostic.message()
        ),
    }
}

fn base_card_entries(
    cx: &mut Cx,
    subject: &sim_kernel::Ref,
    kind: Symbol,
    help: &str,
    ops: Vec<Value>,
    requires: Vec<Value>,
) -> Result<Vec<(Symbol, Value)>> {
    Ok(vec![
        (field("subject"), ref_value(cx, subject)?),
        (field("kind"), cx.factory().symbol(kind)?),
        (field("help"), cx.factory().string(help.to_owned())?),
        (
            field("args"),
            cx.factory().symbol(Symbol::qualified("core", "Any"))?,
        ),
        (
            field("result"),
            cx.factory().symbol(Symbol::qualified("core", "Any"))?,
        ),
        (field("tests"), cx.factory().list(Vec::new())?),
        (field("ops"), cx.factory().list(ops)?),
        (field("requires"), cx.factory().list(requires)?),
        (field("see-also"), cx.factory().list(Vec::new())?),
        (field("shape-known"), cx.factory().bool(true)?),
        (field("facets"), cx.factory().list(Vec::new())?),
        (field("coverage"), empty_coverage(cx)?),
        (field("provenance"), cx.factory().list(Vec::new())?),
        (
            field("freshness"),
            cx.factory().symbol(Symbol::qualified("browse", "live"))?,
        ),
    ])
}

fn capability_symbol(cx: &mut Cx, capability: CapabilityName) -> Result<Value> {
    cx.factory()
        .symbol(Symbol::qualified("capability", capability.as_str()))
}

fn empty_coverage(cx: &mut Cx) -> Result<Value> {
    cx.factory().table(vec![
        (field("tests"), cx.factory().string("0".to_owned())?),
        (field("examples"), cx.factory().string("0".to_owned())?),
        (field("runnable"), cx.factory().bool(false)?),
        (field("passed"), cx.factory().nil()?),
        (field("failed"), cx.factory().nil()?),
        (field("skipped"), cx.factory().nil()?),
        (field("last-run"), cx.factory().nil()?),
        (field("stale"), cx.factory().bool(false)?),
    ])
}

fn packet_kind(packet: &StreamPacket) -> Symbol {
    match packet {
        StreamPacket::Pcm(_) => Symbol::qualified("stream/packet", "pcm"),
        StreamPacket::Midi(_) => Symbol::qualified("stream/packet", "midi"),
        StreamPacket::Diagnostic(_) => Symbol::qualified("stream/packet", "diagnostic"),
        StreamPacket::Data(_) => Symbol::qualified("stream/packet", "data"),
    }
}

fn diagnostic_fields(cx: &mut Cx, diagnostic: &StreamDiagnostic) -> Result<Vec<(Symbol, Value)>> {
    Ok(vec![
        (
            field("diagnostic-kind"),
            cx.factory().symbol(diagnostic.kind().clone())?,
        ),
        (
            field("message"),
            cx.factory().string(diagnostic.message().to_owned())?,
        ),
    ])
}

fn payload_shape_summary(payload: &Expr) -> Symbol {
    match payload {
        Expr::Nil => Symbol::qualified("core", "Nil"),
        Expr::Bool(_) => Symbol::qualified("core", "Bool"),
        Expr::Number(_) => Symbol::qualified("core", "Number"),
        Expr::Symbol(_) => Symbol::qualified("core", "Symbol"),
        Expr::Local(_) => Symbol::qualified("core", "Local"),
        Expr::String(_) => Symbol::qualified("core", "String"),
        Expr::Bytes(_) => Symbol::qualified("core", "Bytes"),
        Expr::List(_) => Symbol::qualified("core", "List"),
        Expr::Vector(_) => Symbol::qualified("core", "Vector"),
        Expr::Map(_) => Symbol::qualified("core", "Map"),
        Expr::Set(_) => Symbol::qualified("core", "Set"),
        Expr::Call { .. } => Symbol::qualified("core", "Call"),
        Expr::Infix { .. } => Symbol::qualified("core", "Infix"),
        Expr::Prefix { .. } => Symbol::qualified("core", "Prefix"),
        Expr::Postfix { .. } => Symbol::qualified("core", "Postfix"),
        Expr::Block(_) => Symbol::qualified("core", "Block"),
        Expr::Quote { .. } => Symbol::qualified("core", "Quote"),
        Expr::Annotated { .. } => Symbol::qualified("core", "Annotated"),
        Expr::Extension { .. } => Symbol::qualified("core", "Extension"),
    }
}

pub fn stats_value(cx: &mut Cx, stats: &StreamStats) -> Result<Value> {
    cx.factory().table(vec![
        (
            field("pushed"),
            cx.factory().string(stats.pushed.to_string())?,
        ),
        (
            field("accepted"),
            cx.factory().string(stats.accepted.to_string())?,
        ),
        (
            field("yielded"),
            cx.factory().string(stats.yielded.to_string())?,
        ),
        (
            field("dropped-newest"),
            cx.factory().string(stats.dropped_newest.to_string())?,
        ),
        (
            field("dropped-oldest"),
            cx.factory().string(stats.dropped_oldest.to_string())?,
        ),
        (
            field("overflow-errors"),
            cx.factory().string(stats.overflow_errors.to_string())?,
        ),
        (
            field("timeouts"),
            cx.factory().string(stats.timeouts.to_string())?,
        ),
        (
            field("timed-out"),
            cx.factory().string(stats.timed_out.to_string())?,
        ),
        (
            field("blocked"),
            cx.factory().string(stats.blocked.to_string())?,
        ),
        (
            field("rejected"),
            cx.factory().string(stats.rejected.to_string())?,
        ),
        (field("closed"), cx.factory().bool(stats.closed)?),
        (field("cancelled"), cx.factory().bool(stats.cancelled)?),
    ])
}

fn metadata_expr(metadata: &StreamMetadata) -> Expr {
    metadata.table_expr()
}

fn field(name: &str) -> Symbol {
    Symbol::new(name)
}

fn field_expr(name: &str) -> Expr {
    Expr::Symbol(field(name))
}

use std::sync::Arc;

use sim_kernel::{Cx, Error, Expr, NumberLiteral, Result, Symbol};
use sim_lib_stream_audio::{PcmBuffer, PcmSpec};
use sim_lib_stream_core::{
    BufferPolicy, MidiPacket, MidiPacketEvent, StreamDirection, StreamItem, StreamMedia,
    StreamMetadata, StreamPacket, StreamValue, publish_metadata_claims,
};
use sim_value::access;
use sim_value::kind::expr_kind;

use crate::handle::StreamHandle;

pub enum OpenSpec {
    MidiSource {
        metadata: StreamMetadata,
        packets: Vec<MidiPacket>,
    },
    MidiSink {
        metadata: StreamMetadata,
        tpq: u16,
    },
    PcmSource {
        metadata: StreamMetadata,
        buffers: Vec<PcmBuffer>,
    },
    PcmSink {
        metadata: StreamMetadata,
        spec: PcmSpec,
    },
}

pub fn open_spec_from_expr(expr: Expr) -> Result<OpenSpec> {
    let entries = access::map_entries(&expr, "stream memory spec")?;
    let kind = symbol_field(entries, "kind")?;
    match kind.as_qualified_str().as_str() {
        "stream/memory-midi-source" => midi_source_spec(entries),
        "stream/memory-midi-sink" => midi_sink_spec(entries),
        "stream/memory-pcm-source" => pcm_source_spec(entries),
        "stream/memory-pcm-sink" => pcm_sink_spec(entries),
        other => Err(Error::Eval(format!("unknown stream memory spec {other}"))),
    }
}

impl OpenSpec {
    pub fn into_handle(self, cx: &mut Cx) -> Result<StreamHandle> {
        match self {
            Self::MidiSource { metadata, packets } => {
                let items = packets
                    .into_iter()
                    .map(|packet| StreamItem::new(StreamPacket::Midi(packet)))
                    .collect();
                source_handle(cx, metadata, items)
            }
            Self::MidiSink { metadata, tpq } => {
                publish_metadata_claims(cx, metadata.subject_ref(), &metadata)?;
                Ok(StreamHandle::midi_sink(metadata, tpq))
            }
            Self::PcmSource { metadata, buffers } => {
                let items = buffers
                    .into_iter()
                    .map(|buffer| buffer.to_packet().map(StreamPacket::Pcm))
                    .map(|packet| packet.map(StreamItem::new))
                    .collect::<Result<Vec<_>>>()?;
                source_handle(cx, metadata, items)
            }
            Self::PcmSink { metadata, spec } => {
                publish_metadata_claims(cx, metadata.subject_ref(), &metadata)?;
                Ok(StreamHandle::pcm_sink(metadata, spec))
            }
        }
    }
}

pub fn memory_specs_value(cx: &mut impl SpecBuildCx) -> Result<sim_kernel::Value> {
    let specs = vec![
        spec_descriptor(
            cx,
            "stream/memory-midi-source",
            StreamMedia::Midi,
            StreamDirection::Source,
            &["kind", "id", "tpq", "events", "batch-events"],
        )?,
        spec_descriptor(
            cx,
            "stream/memory-midi-sink",
            StreamMedia::Midi,
            StreamDirection::Sink,
            &["kind", "id", "tpq"],
        )?,
        spec_descriptor(
            cx,
            "stream/memory-pcm-source",
            StreamMedia::Pcm,
            StreamDirection::Source,
            &["kind", "id", "channels", "sample-rate-hz", "buffers"],
        )?,
        spec_descriptor(
            cx,
            "stream/memory-pcm-sink",
            StreamMedia::Pcm,
            StreamDirection::Sink,
            &["kind", "id", "channels", "sample-rate-hz"],
        )?,
    ];
    cx.list(specs)
}

pub trait SpecBuildCx {
    fn symbol(&mut self, symbol: Symbol) -> Result<sim_kernel::Value>;
    fn string(&mut self, value: String) -> Result<sim_kernel::Value>;
    fn bool(&mut self, value: bool) -> Result<sim_kernel::Value>;
    fn list(&mut self, values: Vec<sim_kernel::Value>) -> Result<sim_kernel::Value>;
    fn table(&mut self, entries: Vec<(Symbol, sim_kernel::Value)>) -> Result<sim_kernel::Value>;
}

impl SpecBuildCx for sim_kernel::LoadCx {
    fn symbol(&mut self, symbol: Symbol) -> Result<sim_kernel::Value> {
        self.factory().symbol(symbol)
    }

    fn string(&mut self, value: String) -> Result<sim_kernel::Value> {
        self.factory().string(value)
    }

    fn bool(&mut self, value: bool) -> Result<sim_kernel::Value> {
        self.factory().bool(value)
    }

    fn list(&mut self, values: Vec<sim_kernel::Value>) -> Result<sim_kernel::Value> {
        self.factory().list(values)
    }

    fn table(&mut self, entries: Vec<(Symbol, sim_kernel::Value)>) -> Result<sim_kernel::Value> {
        self.factory().table(entries)
    }
}

impl SpecBuildCx for Cx {
    fn symbol(&mut self, symbol: Symbol) -> Result<sim_kernel::Value> {
        self.factory().symbol(symbol)
    }

    fn string(&mut self, value: String) -> Result<sim_kernel::Value> {
        self.factory().string(value)
    }

    fn bool(&mut self, value: bool) -> Result<sim_kernel::Value> {
        self.factory().bool(value)
    }

    fn list(&mut self, values: Vec<sim_kernel::Value>) -> Result<sim_kernel::Value> {
        self.factory().list(values)
    }

    fn table(&mut self, entries: Vec<(Symbol, sim_kernel::Value)>) -> Result<sim_kernel::Value> {
        self.factory().table(entries)
    }
}

fn source_handle(
    cx: &mut Cx,
    metadata: StreamMetadata,
    items: Vec<StreamItem>,
) -> Result<StreamHandle> {
    let stream = Arc::new(StreamValue::pull(metadata.clone(), items));
    stream.publish_claims(cx, metadata.subject_ref())?;
    Ok(StreamHandle::source(metadata, stream))
}

fn midi_source_spec(entries: &[(Expr, Expr)]) -> Result<OpenSpec> {
    let metadata = metadata(
        entries,
        StreamMedia::Midi,
        StreamDirection::Source,
        "clock/midi-tick",
    )?;
    let tpq = u16_field(entries, "tpq")?;
    let batch_events = optional_usize_field(entries, "batch-events")?.unwrap_or(64);
    if batch_events == 0 {
        return Err(Error::Eval(
            "MIDI memory source batch-events must be greater than zero".to_owned(),
        ));
    }
    let events = optional_seq_field(entries, "events")?
        .unwrap_or(&[])
        .iter()
        .map(|event| midi_event(event, tpq))
        .collect::<Result<Vec<_>>>()?;
    Ok(OpenSpec::MidiSource {
        metadata,
        packets: midi_packets(events, batch_events)?,
    })
}

fn midi_sink_spec(entries: &[(Expr, Expr)]) -> Result<OpenSpec> {
    Ok(OpenSpec::MidiSink {
        metadata: metadata(
            entries,
            StreamMedia::Midi,
            StreamDirection::Sink,
            "clock/midi-tick",
        )?,
        tpq: u16_field(entries, "tpq")?,
    })
}

fn pcm_source_spec(entries: &[(Expr, Expr)]) -> Result<OpenSpec> {
    let spec = pcm_spec(entries)?;
    let buffers = optional_seq_field(entries, "buffers")?
        .unwrap_or(&[])
        .iter()
        .map(|buffer| pcm_buffer(buffer, spec))
        .collect::<Result<Vec<_>>>()?;
    Ok(OpenSpec::PcmSource {
        metadata: metadata(
            entries,
            StreamMedia::Pcm,
            StreamDirection::Source,
            "clock/sample",
        )?,
        buffers,
    })
}

fn pcm_sink_spec(entries: &[(Expr, Expr)]) -> Result<OpenSpec> {
    Ok(OpenSpec::PcmSink {
        metadata: metadata(
            entries,
            StreamMedia::Pcm,
            StreamDirection::Sink,
            "clock/sample",
        )?,
        spec: pcm_spec(entries)?,
    })
}

fn metadata(
    entries: &[(Expr, Expr)],
    media: StreamMedia,
    direction: StreamDirection,
    default_clock: &str,
) -> Result<StreamMetadata> {
    let id = symbol_or_string_field(entries, "id")?;
    let clock = optional_symbol_or_string_field(entries, "clock")?
        .unwrap_or_else(|| Symbol::new(default_clock));
    let buffer = match access::entry_field(entries, "buffer") {
        Some(expr) => BufferPolicy::from_expr(expr)?,
        None => BufferPolicy::bounded(16)?,
    };
    Ok(StreamMetadata::new(id, media, direction, clock, buffer))
}

fn midi_event(expr: &Expr, default_tpq: u16) -> Result<MidiPacketEvent> {
    let entries = access::map_entries(expr, "MIDI memory event")?;
    let ticks = i64_field(entries, "ticks")?;
    let tpq = optional_u16_field(entries, "tpq")?.unwrap_or(default_tpq);
    let bytes = bytes_field(entries, "bytes")?;
    MidiPacketEvent::new(ticks, tpq, bytes)
}

fn midi_packets(events: Vec<MidiPacketEvent>, batch_events: usize) -> Result<Vec<MidiPacket>> {
    let mut packets = Vec::new();
    for chunk in events.chunks(batch_events) {
        packets.push(MidiPacket::new(chunk.to_vec())?);
    }
    Ok(packets)
}

fn pcm_spec(entries: &[(Expr, Expr)]) -> Result<PcmSpec> {
    PcmSpec::i16(
        usize_field(entries, "channels")?,
        u32_field(entries, "sample-rate-hz")?,
    )
}

fn pcm_buffer(expr: &Expr, spec: PcmSpec) -> Result<PcmBuffer> {
    let entries = access::map_entries(expr, "PCM memory buffer")?;
    let frames = usize_field(entries, "frames")?;
    let samples = seq_field(entries, "samples")?
        .iter()
        .map(i16_expr)
        .collect::<Result<Vec<_>>>()?;
    PcmBuffer::i16(spec, frames, samples)
}

fn spec_descriptor(
    cx: &mut impl SpecBuildCx,
    kind: &str,
    media: StreamMedia,
    direction: StreamDirection,
    fields: &[&str],
) -> Result<sim_kernel::Value> {
    let required = fields
        .iter()
        .map(|field| cx.string((*field).to_owned()))
        .collect::<Result<Vec<_>>>()?;
    let kind = cx.symbol(slash_symbol(kind))?;
    let media = cx.symbol(media.symbol())?;
    let direction = cx.symbol(direction.symbol())?;
    let memory = cx.bool(true)?;
    let required = cx.list(required)?;
    cx.table(vec![
        (Symbol::new("kind"), kind),
        (Symbol::new("media"), media),
        (Symbol::new("direction"), direction),
        (Symbol::new("memory"), memory),
        (Symbol::new("required-fields"), required),
    ])
}

fn slash_symbol(text: &str) -> Symbol {
    match text.rsplit_once('/') {
        Some((namespace, name)) => Symbol::qualified(namespace, name),
        None => Symbol::new(text),
    }
}

fn required_spec_field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> Result<&'a Expr> {
    access::entry_field(entries, name)
        .ok_or_else(|| Error::Eval(format!("stream spec missing {name}")))
}

fn symbol_field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> Result<&'a Symbol> {
    access::entry_required_sym(entries, name, "symbol field")
}

fn symbol_or_string_field(entries: &[(Expr, Expr)], name: &str) -> Result<Symbol> {
    symbol_or_string(required_spec_field(entries, name)?)
}

fn optional_symbol_or_string_field(entries: &[(Expr, Expr)], name: &str) -> Result<Option<Symbol>> {
    access::entry_field(entries, name)
        .map(symbol_or_string)
        .transpose()
}

fn symbol_or_string(expr: &Expr) -> Result<Symbol> {
    match expr {
        Expr::Symbol(symbol) => Ok(symbol.clone()),
        Expr::String(text) => Ok(Symbol::new(text.clone())),
        other => Err(Error::TypeMismatch {
            expected: "symbol or string",
            found: expr_kind(other),
        }),
    }
}

fn seq_field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> Result<&'a [Expr]> {
    sequence(required_spec_field(entries, name)?)
}

fn optional_seq_field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> Result<Option<&'a [Expr]>> {
    access::entry_field(entries, name).map(sequence).transpose()
}

fn sequence(expr: &Expr) -> Result<&[Expr]> {
    match expr {
        Expr::List(values) | Expr::Vector(values) => Ok(values),
        other => Err(Error::TypeMismatch {
            expected: "list or vector",
            found: expr_kind(other),
        }),
    }
}

fn bytes_field(entries: &[(Expr, Expr)], name: &str) -> Result<Vec<u8>> {
    match required_spec_field(entries, name)? {
        Expr::Bytes(bytes) => Ok(bytes.clone()),
        expr => sequence(expr)?
            .iter()
            .map(u8_expr)
            .collect::<Result<Vec<_>>>(),
    }
}

fn usize_field(entries: &[(Expr, Expr)], name: &str) -> Result<usize> {
    usize_expr(required_spec_field(entries, name)?)
}

fn optional_usize_field(entries: &[(Expr, Expr)], name: &str) -> Result<Option<usize>> {
    access::entry_field(entries, name)
        .map(usize_expr)
        .transpose()
}

fn i64_field(entries: &[(Expr, Expr)], name: &str) -> Result<i64> {
    parse_number(required_spec_field(entries, name)?, name)
}

fn u32_field(entries: &[(Expr, Expr)], name: &str) -> Result<u32> {
    parse_number(required_spec_field(entries, name)?, name)
}

fn u16_field(entries: &[(Expr, Expr)], name: &str) -> Result<u16> {
    parse_number(required_spec_field(entries, name)?, name)
}

fn optional_u16_field(entries: &[(Expr, Expr)], name: &str) -> Result<Option<u16>> {
    access::entry_field(entries, name)
        .map(|expr| parse_number(expr, name))
        .transpose()
}

fn usize_expr(expr: &Expr) -> Result<usize> {
    parse_number(expr, "usize")
}

fn u8_expr(expr: &Expr) -> Result<u8> {
    parse_number(expr, "byte")
}

fn i16_expr(expr: &Expr) -> Result<i16> {
    parse_number(expr, "PCM sample")
}

fn parse_number<T>(expr: &Expr, label: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let text = match expr {
        Expr::Number(NumberLiteral { canonical, .. }) | Expr::String(canonical) => canonical,
        other => {
            return Err(Error::TypeMismatch {
                expected: "number or numeric string",
                found: expr_kind(other),
            });
        }
    };
    text.parse::<T>()
        .map_err(|err| Error::Eval(format!("invalid {label}: {err}")))
}

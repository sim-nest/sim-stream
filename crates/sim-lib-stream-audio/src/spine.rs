use sim_kernel::{Error, Result};
use sim_lib_stream_core::{
    StreamDirection, StreamItem, StreamMedia, StreamMetadata, StreamPacket, StreamValue,
};

use crate::{PcmBuffer, PcmPumpSummary, PcmSink, PcmSource};

/// Drains a [`PcmSource`] into a pull-mode `sim-lib-stream-core` `StreamValue`.
///
/// Each source buffer becomes one PCM stream packet carried on `metadata`.
/// Returns an error when `metadata` is not PCM media or is marked sink-only, or
/// when a buffer fails to encode.
pub fn pcm_source_to_stream(
    source: &mut impl PcmSource,
    metadata: StreamMetadata,
) -> Result<StreamValue> {
    ensure_source_metadata(&metadata)?;
    let mut items = Vec::new();
    while let Some(buffer) = source.read_buffer()? {
        items.push(StreamItem::new(StreamPacket::Pcm(buffer.to_packet()?)));
    }
    Ok(StreamValue::pull(metadata, items))
}

/// Drains a `sim-lib-stream-core` `StreamValue` into a [`PcmSink`].
///
/// Each PCM stream packet is decoded into a [`PcmBuffer`] matching the sink's
/// spec, written, and counted in the returned [`PcmPumpSummary`]; the sink is
/// flushed at the end. Returns an error on a non-PCM packet or a spec mismatch.
pub fn stream_to_pcm_sink(stream: &StreamValue, sink: &mut impl PcmSink) -> Result<PcmPumpSummary> {
    let mut summary = PcmPumpSummary::default();
    while let Some(item) = stream.next_packet()? {
        let StreamPacket::Pcm(packet) = item.packet() else {
            return Err(Error::Eval(
                "PCM sink adapter received a non-PCM stream packet".to_owned(),
            ));
        };
        let buffer = PcmBuffer::from_packet(*sink.spec(), packet)?;
        summary.record(&buffer);
        sink.write_buffer(buffer)?;
    }
    sink.flush()?;
    Ok(summary)
}

fn ensure_source_metadata(metadata: &StreamMetadata) -> Result<()> {
    if metadata.media() != StreamMedia::Pcm {
        return Err(Error::Eval(
            "PCM source stream metadata must use PCM media".to_owned(),
        ));
    }
    if metadata.direction() == StreamDirection::Sink {
        return Err(Error::Eval(
            "PCM source stream metadata must not be sink-only".to_owned(),
        ));
    }
    Ok(())
}

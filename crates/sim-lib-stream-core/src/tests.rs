use std::sync::Arc;
use std::time::Duration;

use sim_kernel::{
    ClaimPattern, ContentId, Cx, DatumStore, EventKind, EventSource, Expr, ObjectEncoding, Ref,
    Symbol, Tick, card::card_for_value, read_construct_capability, seq_next_value,
    stream_surface::stream_packet_event,
};

use crate::{
    BackpressureOutcome, BufferOverflowPolicy, BufferPolicy, ClockDomain, LatencyClass, PcmPacket,
    StreamCapability, StreamCassette, StreamDirection, StreamEnvelope, StreamEventSource,
    StreamItem, StreamMedia, StreamMetadata, StreamMetadataValue, StreamPacket,
    StreamPacketDescriptor, TransportProfile, install_stream_core_classes, publish_metadata_claims,
    spine::{PushResult, stream_next_bang, stream_run_bang},
    stream_cassette_format_symbol, stream_cassette_golden_root, stream_direction_predicate,
    stream_media_predicate, stream_metadata_class_symbol, stream_packet_class_symbol,
};

mod codec;
mod envelope;
mod live_completion;
mod packet;
mod profile;

use sim_kernel::testing::bare_cx as cx;

#[test]
fn citizen_packet_descriptor_round_trips_and_fails_closed() {
    let packet = StreamPacket::Pcm(PcmPacket::i16(2, 2, vec![1, 2, 3, 4]).unwrap());
    let descriptor = StreamPacketDescriptor::new(packet.clone());
    assert_eq!(descriptor.packet().unwrap(), packet);

    let mut cx = cx();
    cx.load_lib(&sim_citizen::CitizenLib::all()).unwrap();
    cx.grant(read_construct_capability());
    let value = cx.factory().opaque(Arc::new(descriptor)).unwrap();
    let ObjectEncoding::Constructor { class, args } = value
        .object()
        .as_object_encoder()
        .unwrap()
        .object_encoding(&mut cx)
        .unwrap()
    else {
        panic!("packet descriptor should encode as constructor");
    };
    assert_eq!(class, stream_packet_class_symbol());
    let args = args
        .iter()
        .map(|expr| sim_citizen::value_from_expr(&mut cx, expr))
        .collect::<sim_kernel::Result<Vec<_>>>()
        .unwrap();

    let decoded = cx.read_construct(&class, args).unwrap();
    let decoded = decoded
        .object()
        .downcast_ref::<StreamPacketDescriptor>()
        .unwrap();
    assert_eq!(decoded.packet().unwrap(), packet);

    let err = StreamPacketDescriptor::from_expr(Expr::Map(vec![(
        Expr::Symbol(Symbol::new("packet")),
        Expr::Symbol(Symbol::qualified("stream/packet", "host-device")),
    )]))
    .unwrap_err();
    assert!(format!("{err}").contains("unknown stream packet kind"));
}

#[test]
fn metadata_read_construct_round_trips() {
    let mut cx = cx();
    install_stream_core_classes(&mut cx).unwrap();
    cx.grant(read_construct_capability());
    let metadata = metadata();
    let args = metadata
        .to_constructor_args()
        .into_iter()
        .map(|expr| cx.factory().expr(expr))
        .collect::<sim_kernel::Result<Vec<_>>>()
        .unwrap();

    let value = cx
        .read_construct(&stream_metadata_class_symbol(), args)
        .unwrap();
    let decoded = value
        .object()
        .downcast_ref::<StreamMetadataValue>()
        .unwrap()
        .metadata();

    assert_eq!(decoded, &metadata);
    assert_eq!(
        StreamMetadata::from_constructor_args(metadata.to_constructor_args()).unwrap(),
        metadata
    );
}

#[test]
fn card_includes_stream_metadata_fields_and_claims() {
    let mut cx = cx();
    let metadata = metadata();
    let subject = metadata.subject_ref();
    publish_metadata_claims(&mut cx, subject.clone(), &metadata).unwrap();

    assert_has_claim(
        &mut cx,
        subject.clone(),
        stream_media_predicate(),
        Ref::Symbol(StreamMedia::Pcm.symbol()),
    );
    assert_has_claim(
        &mut cx,
        subject,
        stream_direction_predicate(),
        Ref::Symbol(StreamDirection::Source.symbol()),
    );

    let value = cx
        .factory()
        .opaque(Arc::new(StreamMetadataValue::new(metadata)))
        .unwrap();
    let card = card_for_value(&mut cx, value)
        .unwrap()
        .object()
        .as_expr(&mut cx)
        .unwrap();

    assert_eq!(
        table_value(&card, "id"),
        Some(&Expr::String("stream/demo".to_owned()))
    );
    assert_eq!(
        table_value(&card, "media"),
        Some(&Expr::Symbol(StreamMedia::Pcm.symbol()))
    );
    assert_eq!(
        table_value(&card, "direction"),
        Some(&Expr::Symbol(StreamDirection::Source.symbol()))
    );
    assert_eq!(
        table_value(&card, "clock"),
        Some(&Expr::Symbol(Symbol::qualified("clock", "sample")))
    );
    assert!(matches!(table_value(&card, "buffer"), Some(Expr::Map(_))));
}

#[test]
fn packet_ref_interning_yields_content_ref_for_chunk_events() {
    let mut cx = cx();
    let packet = StreamPacket::Pcm(PcmPacket::i16(1, 2, vec![7, 8]).unwrap());
    let payload = packet.intern_ref(&mut cx).unwrap();
    let Ref::Content(id) = &payload else {
        panic!("packet should intern as content ref");
    };
    assert!(cx.datum_store().contains(id));

    let tick = Tick::new(
        Symbol::qualified("clock", "sample"),
        Ref::Content(ContentId::from_bytes(
            Symbol::qualified("core", "sha256"),
            [4; 32],
        )),
    );
    let event = stream_packet_event(
        Ref::Symbol(Symbol::qualified("run", "stream")),
        0,
        vec![tick],
        payload.clone(),
    )
    .unwrap();

    assert!(matches!(event.kind, EventKind::Chunk { payload: actual } if actual == payload));
}

#[test]
fn stream_envelope_rejects_packet_media_that_conflicts_with_metadata() {
    let item = StreamItem::new(diagnostic_packet("mismatch"));

    let err = StreamEnvelope::from_item(&metadata(), 0, &item).unwrap_err();

    assert!(format!("{err}").contains("does not match packet media"));
}

#[test]
fn pull_spine_yields_finite_cursor_then_nil() {
    let mut cx = cx();
    let first = item("one");
    let second = item("two");
    let stream = Arc::new(crate::StreamValue::pull(
        metadata(),
        vec![first.clone(), second.clone()],
    ));
    let value = cx.factory().opaque(stream.clone()).unwrap();

    assert_eq!(stream.peek_packet().unwrap(), Some(first.clone()));
    assert_eq!(stream_next_bang(&stream).unwrap(), Some(first));
    assert!(seq_next_value(&mut cx, &value).unwrap().is_some());
    assert!(seq_next_value(&mut cx, &value).unwrap().is_none());
    assert!(stream.is_done().unwrap());
}

#[test]
fn push_spine_yields_producer_packets_then_nil() {
    let stream = crate::StreamValue::push(metadata());
    let first = item("first");
    let second = item("second");

    assert_eq!(
        stream.push_packet(first.clone()).unwrap(),
        PushResult::Accepted
    );
    assert_eq!(
        stream.push_packet(second.clone()).unwrap(),
        PushResult::Accepted
    );
    stream.close_push().unwrap();

    assert_eq!(stream.next_packet().unwrap(), Some(first));
    assert_eq!(stream.next_packet().unwrap(), Some(second));
    assert_eq!(stream.next_packet().unwrap(), None);
    assert!(stream.is_done().unwrap());
}

#[test]
fn overflow_policies_behave_exactly() {
    let newest = crate::StreamValue::push(metadata_with_overflow(BufferOverflowPolicy::DropNewest));
    let one = item("one");
    let two = item("two");
    let three = item("three");
    newest.push_packet(one.clone()).unwrap();
    newest.push_packet(two.clone()).unwrap();
    assert_eq!(
        newest.push_packet(three.clone()).unwrap(),
        PushResult::DroppedNewest(three.clone())
    );
    assert_eq!(
        newest.push_packet(item("four")).unwrap().outcome(),
        BackpressureOutcome::DroppedNewest
    );
    newest.close_push().unwrap();
    assert_eq!(
        newest.take_packets(4).unwrap(),
        vec![one.clone(), two.clone()]
    );

    let oldest = crate::StreamValue::push(metadata_with_overflow(BufferOverflowPolicy::DropOldest));
    oldest.push_packet(one.clone()).unwrap();
    oldest.push_packet(two.clone()).unwrap();
    assert_eq!(
        oldest.push_packet(three.clone()).unwrap(),
        PushResult::DroppedOldest(one.clone())
    );
    assert_eq!(
        oldest.push_packet(item("four")).unwrap().outcome(),
        BackpressureOutcome::DroppedOldest
    );
    oldest.close_push().unwrap();
    assert_eq!(oldest.take_packets(4).unwrap(), vec![three, item("four")]);

    let errors = crate::StreamValue::push(metadata_with_overflow(BufferOverflowPolicy::Error));
    errors.push_packet(one.clone()).unwrap();
    errors.push_packet(two.clone()).unwrap();
    let rejected = errors.push_packet(item("overflow")).unwrap();
    assert_eq!(rejected.outcome(), BackpressureOutcome::Rejected);
    errors.close_push().unwrap();
    assert_eq!(errors.stats().unwrap().rejected, 1);
    assert_eq!(errors.take_packets(4).unwrap(), vec![one, two]);
}

#[test]
fn timeout_does_not_spin() {
    let stream = crate::StreamValue::push(metadata());

    assert_eq!(
        stream
            .next_packet_timeout(Duration::from_millis(1))
            .unwrap(),
        None
    );
    assert_eq!(stream.stats().unwrap().timeouts, 1);
    assert_eq!(stream.stats().unwrap().timed_out, 1);
    assert!(!stream.is_done().unwrap());
}

#[test]
fn forced_packets_project_to_chunk_events_with_refs_and_ticks() {
    let mut cx = cx();
    let tick = Tick::new(
        Symbol::qualified("clock", "sample"),
        Ref::Content(ContentId::from_bytes(
            Symbol::qualified("core", "sha256"),
            [5; 32],
        )),
    );
    let packet =
        StreamItem::with_ticks(item("forced").packet().clone(), vec![tick.clone()]).unwrap();
    let stream = crate::StreamValue::pull(metadata(), vec![packet]);
    let events = stream_run_bang(
        &stream,
        &mut cx,
        Ref::Symbol(Symbol::qualified("run", "stream")),
        10,
    )
    .unwrap();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].ticks, vec![tick]);
    let EventKind::Chunk { payload } = &events[0].kind else {
        panic!("first stream event should be a chunk");
    };
    let Ref::Content(id) = payload else {
        panic!("chunk payload should be content-addressed");
    };
    assert!(cx.datum_store().contains(id));
    assert!(matches!(events[1].kind, EventKind::Done));
}

#[test]
fn stream_event_source_projects_packets_until_done() {
    let mut cx = cx();
    let stream = Arc::new(crate::StreamValue::pull(metadata(), vec![item("source")]));
    let source = StreamEventSource::new(stream, Ref::Symbol(Symbol::qualified("run", "source")), 3);

    let first = source.next(&mut cx).unwrap().unwrap();
    let second = source.next(&mut cx).unwrap().unwrap();
    let third = source.next(&mut cx).unwrap();

    assert_eq!(first.seq, 3);
    assert!(matches!(first.kind, EventKind::Chunk { .. }));
    assert_eq!(second.seq, 4);
    assert!(matches!(second.kind, EventKind::Done));
    assert!(third.is_none());
}

#[test]
fn event_source_close_cancels_stream_value() {
    let mut cx = cx();
    let stream = Arc::new(crate::StreamValue::push(metadata()));
    let source = StreamEventSource::new(
        Arc::clone(&stream),
        Ref::Symbol(Symbol::qualified("run", "cancel")),
        0,
    );

    source.close(&mut cx).unwrap();

    let closed = stream.push_packet(item("late")).unwrap();
    assert_eq!(closed.outcome(), BackpressureOutcome::Closed);
    let stats = stream.stats().unwrap();
    assert!(stats.closed);
    assert!(stats.cancelled);
}

fn metadata() -> StreamMetadata {
    metadata_with_overflow(BufferOverflowPolicy::DropOldest)
}

fn metadata_with_overflow(overflow: BufferOverflowPolicy) -> StreamMetadata {
    StreamMetadata::new(
        Symbol::qualified("stream", "demo"),
        StreamMedia::Pcm,
        StreamDirection::Source,
        Symbol::qualified("clock", "sample"),
        BufferPolicy::bounded_with_overflow(2, overflow).unwrap(),
    )
}

fn diagnostic_metadata() -> StreamMetadata {
    StreamMetadata::new(
        Symbol::qualified("stream", "diagnostics"),
        StreamMedia::Diagnostic,
        StreamDirection::Source,
        Symbol::qualified("clock", "sample"),
        BufferPolicy::bounded_with_overflow(2, BufferOverflowPolicy::DropOldest).unwrap(),
    )
}

fn item(message: &str) -> StreamItem {
    StreamItem::new(diagnostic_packet(message))
}

fn ticked_item(message: &str, index: u8) -> StreamItem {
    StreamItem::with_ticks(
        diagnostic_packet(message),
        vec![Tick::new(
            Symbol::qualified("clock", "sample"),
            Ref::Content(ContentId::from_bytes(
                Symbol::qualified("core", "sha256"),
                [index; 32],
            )),
        )],
    )
    .unwrap()
}

fn diagnostic_packet(message: &str) -> StreamPacket {
    StreamPacket::Diagnostic(crate::StreamDiagnostic::new(
        Symbol::qualified("stream/test", "packet"),
        message,
    ))
}

fn assert_has_claim(cx: &mut Cx, subject: Ref, predicate: Symbol, object: Ref) {
    let claims = cx
        .query_facts(ClaimPattern::exact(subject, predicate, object))
        .unwrap();
    assert_eq!(claims.len(), 1);
}

fn table_value<'a>(expr: &'a Expr, key: &str) -> Option<&'a Expr> {
    let Expr::Map(entries) = expr else {
        return None;
    };
    entries.iter().find_map(|(entry_key, entry_value)| {
        let Expr::Symbol(entry_key) = entry_key else {
            return None;
        };
        (entry_key.name.as_ref() == key).then_some(entry_value)
    })
}

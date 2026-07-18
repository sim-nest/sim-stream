use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use sim_kernel::{
    ContentId, DefaultFactory, Diagnostic, EventLedger, Expr, NoopEvalPolicy, Ref, Severity,
    Symbol, Tick,
};
use sim_lib_stream_clock::{Clock, ClockIndex};
use sim_lib_stream_core::{
    BufferOverflowPolicy, BufferPolicy, ClockDomain, StreamDiagnostic, StreamDirection, StreamItem,
    StreamMedia, StreamMetadata, StreamPacket, TransportProfile,
};

use crate::{
    SeekTarget, Stream, StreamNode, clock_convert, fan, filter_data_kind, filter_data_shape,
    identity, map, map_data_expr, merge_by_clock, pipe, record_bang, record_bang_bounded,
    record_cassette_bang, record_ledger_slice, replay, replay_cassette, run_bang, seek,
    stream_cell, stream_window_data_kind, tap_diagnostics, window_by_count,
};

fn metadata() -> StreamMetadata {
    StreamMetadata::new(
        Symbol::qualified("stream", "combinator-test"),
        StreamMedia::Diagnostic,
        StreamDirection::Source,
        clock_symbol(),
        BufferPolicy::bounded_with_overflow(8, BufferOverflowPolicy::DropNewest).unwrap(),
    )
}

fn data_metadata() -> StreamMetadata {
    StreamMetadata::new(
        Symbol::qualified("stream", "data-combinator-test"),
        StreamMedia::Data,
        StreamDirection::Source,
        clock_symbol(),
        BufferPolicy::bounded_with_overflow(8, BufferOverflowPolicy::DropNewest).unwrap(),
    )
}

fn clock_symbol() -> Symbol {
    ClockDomain::Control.symbol()
}

#[test]
fn pipe_source_identity_equals_source_packets() {
    let items = vec![packet("one"), packet("two"), packet("three")];
    let stream = Stream::pull(metadata(), items.clone());

    let out = pipe(stream, vec![identity()]);

    assert_eq!(out.take_packets(8).unwrap(), items);
}

#[test]
fn map_is_lazy() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let stream = Stream::pull(metadata(), vec![packet("one"), packet("two")]);

    let mapped = map(stream, move |item| {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok(item)
    });

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(message(&mapped.next_packet().unwrap().unwrap()), "one");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn merge_interleaves_by_clock() {
    let left = Stream::pull(
        metadata(),
        vec![ticked_packet("left-1", 1), ticked_packet("left-3", 3)],
    );
    let right = Stream::pull(
        metadata(),
        vec![ticked_packet("right-2", 2), ticked_packet("right-4", 4)],
    );

    let merged = merge_by_clock(left, right, clock_symbol());

    assert_eq!(
        messages(merged.take_packets(8).unwrap()),
        vec!["left-1", "right-2", "left-3", "right-4"]
    );
}

#[test]
fn merge_orders_clock_indexes_semantically() {
    let left = Stream::pull(metadata(), vec![ticked_packet("left-10", 10)]);
    let right = Stream::pull(metadata(), vec![ticked_packet("right-2", 2)]);

    let merged = merge_by_clock(left, right, clock_symbol());

    assert_eq!(
        messages(merged.take_packets(8).unwrap()),
        vec!["right-2", "left-10"]
    );
}

#[test]
fn merge_rejects_incomparable_clock_index_refs() {
    let left = Stream::pull(
        metadata(),
        vec![content_ref_ticked_packet("content-ref", 1)],
    );
    let right = Stream::pull(metadata(), vec![ticked_packet("semantic", 2)]);

    let merged = merge_by_clock(left, right, clock_symbol());

    let err = merged.take_packets(8).unwrap_err();
    assert!(format!("{err}").contains("incomparable index"));
}

#[test]
fn fan_lets_two_readers_see_every_packet() {
    let stream = Stream::pull(
        metadata(),
        vec![packet("one"), packet("two"), packet("three")],
    );

    let fanout = fan(stream);

    assert_eq!(
        messages(fanout.left.take_packets(8).unwrap()),
        vec!["one", "two", "three"]
    );
    assert_eq!(
        messages(fanout.right.take_packets(8).unwrap()),
        vec!["one", "two", "three"]
    );
}

#[test]
fn clock_convert_emits_lossy_diagnostics() {
    let converted = clock_convert(Stream::pull(metadata(), vec![packet("one")]), |_item| {
        Ok((
            vec![tick(9)],
            vec![Diagnostic {
                severity: Severity::Warning,
                message: "lossy clock conversion".to_owned(),
                source: None,
                span: None,
                code: Some(Symbol::qualified("stream/clock", "lossy")),
                related: Vec::new(),
            }],
        ))
    });

    let packet = converted.next_packet().unwrap().unwrap();
    let diagnostics = converted.diagnostics().unwrap();

    assert_eq!(packet.ticks(), &[tick(9)]);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("lossy"));
}

#[test]
fn stale_cell_version_fails_and_current_version_succeeds() {
    let cell = stream_cell("first".to_owned());
    let initial = cell.get().unwrap();

    assert_eq!(initial.version, 0);
    assert_eq!(cell.set("second".to_owned(), initial.version).unwrap(), 1);
    let err = cell.set("stale".to_owned(), initial.version).unwrap_err();
    assert!(format!("{err}").contains("stale stream cell version"));

    let current = cell.get().unwrap();
    assert_eq!(current.value, "second");
    assert_eq!(cell.set("third".to_owned(), current.version).unwrap(), 2);
    assert_eq!(cell.get().unwrap().value, "third");
}

#[test]
fn run_bang_projects_combinator_stream_events() {
    let mut cx = sim_kernel::Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));
    let stream = Stream::pull(metadata(), vec![packet("one")]);

    let events = run_bang(
        &stream,
        &mut cx,
        Ref::Symbol(Symbol::qualified("run", "combinator")),
        0,
    )
    .unwrap();

    assert_eq!(events.len(), 2);
}

#[test]
fn record_then_replay_reproduces_exact_packet_sequence() {
    let items = vec![packet("one"), ticked_packet("two", 2), packet("three")];
    let stream = Stream::pull(metadata(), items.clone());

    let recording = record_bang(&stream).unwrap();

    assert_eq!(recording.items(), items.as_slice());
    assert_eq!(replay(&recording).take_packets(8).unwrap(), items);
    assert_eq!(recording.replay().take_packets(8).unwrap(), items);
}

#[test]
fn cassette_recording_replays_in_memory_streams_as_golden_fixtures() {
    let items = vec![packet("one"), packet("two"), packet("three")];
    let stream = Stream::pull(metadata(), items.clone());

    let cassette = record_cassette_bang(&stream, TransportProfile::memory_local()).unwrap();
    let replayed = replay_cassette(&cassette).unwrap();

    assert_eq!(cassette.envelopes().len(), 3);
    assert_eq!(replayed.take_packets(8).unwrap(), items);
    assert!(
        cassette
            .validate_golden_fixture("fixtures/streams/golden/combinator.simcassette")
            .is_ok()
    );
}

#[test]
fn seek_skips_earlier_packets() {
    let items = vec![
        ticked_packet("one", 1),
        ticked_packet("two", 2),
        ticked_packet("three", 3),
    ];
    let recording = record_bang(&Stream::pull(metadata(), items)).unwrap();

    let by_packet = seek(recording.replay(), SeekTarget::packet_index(2));
    assert_eq!(messages(by_packet.take_packets(8).unwrap()), vec!["three"]);

    let target_tick = tick(2);
    let by_clock = recording.seek(SeekTarget::clock_index(
        target_tick.clock.clone(),
        target_tick.index.clone(),
    ));
    assert_eq!(
        messages(by_clock.take_packets(8).unwrap()),
        vec!["two", "three"]
    );
}

#[test]
fn replay_of_recorded_remote_stream_is_deterministic_offline() {
    let mut cx = sim_kernel::Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));
    let run = Ref::Symbol(Symbol::qualified("run", "recorded-remote"));
    let expected = vec![
        ticked_packet("remote-one", 1),
        ticked_packet("remote-two", 2),
    ];
    let stream = Stream::pull(metadata(), expected.clone());
    let events = run_bang(&stream, &mut cx, run.clone(), 0).unwrap();
    let mut ledger = EventLedger::new();
    for event in events {
        ledger
            .push_with_ticks(event.run, event.ticks, event.kind)
            .unwrap();
    }

    let recording = record_ledger_slice(&mut cx, metadata(), &ledger, &run, 0..3).unwrap();

    assert_eq!(recording.items(), expected.as_slice());
    assert_eq!(recording.replay().take_packets(8).unwrap(), expected);
    assert_eq!(recording.replay().take_packets(8).unwrap(), expected);
}

#[test]
fn data_combinators_filter_map_window_and_tap_diagnostics() {
    let diagnostic_count = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&diagnostic_count);
    let stream = Stream::pull(
        data_metadata(),
        vec![
            data_packet(
                Symbol::qualified("stream/data", "model-event"),
                Expr::Map(vec![(field("text"), Expr::String("hello".to_owned()))]),
            ),
            data_packet(
                Symbol::qualified("stream/data", "rank-frontier"),
                Expr::Map(vec![(field("rank"), Expr::String("frontier-1".to_owned()))]),
            ),
            packet("stream diagnostic"),
        ],
    );

    let tapped = tap_diagnostics(stream, move |_diagnostic| {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
    let model_events = filter_data_kind(tapped, Symbol::qualified("stream/data", "model-event"));
    let mapped = map_data_expr(model_events, |payload| match payload {
        Expr::Map(mut entries) => {
            entries.push((field("mapped"), Expr::Bool(true)));
            Ok(Expr::Map(entries))
        }
        other => Ok(other),
    });

    let out = mapped.take_packets(8).unwrap();

    assert_eq!(diagnostic_count.load(Ordering::SeqCst), 1);
    assert_eq!(out.len(), 1);
    assert_eq!(
        data_kind(&out[0]),
        Some(Symbol::qualified("stream/data", "model-event"))
    );
    let payload = data_payload(&out[0]).unwrap();
    assert_eq!(
        map_value(payload, "text"),
        Some(&Expr::String("hello".to_owned()))
    );
    assert_eq!(map_value(payload, "mapped"), Some(&Expr::Bool(true)));

    let rank_frontier = Stream::pull(
        data_metadata(),
        vec![
            data_packet(
                Symbol::qualified("stream/data", "rank-frontier"),
                Expr::Map(vec![(field("rank"), Expr::String("frontier-2".to_owned()))]),
            ),
            data_packet(
                Symbol::qualified("stream/data", "model-event"),
                Expr::Map(vec![(field("text"), Expr::String("ignored".to_owned()))]),
            ),
        ],
    );
    let shaped = filter_data_shape(rank_frontier, |payload| {
        Ok(map_value(payload, "rank").is_some())
    });
    let shaped = shaped.take_packets(8).unwrap();

    assert_eq!(shaped.len(), 1);
    assert_eq!(
        data_kind(&shaped[0]),
        Some(Symbol::qualified("stream/data", "rank-frontier"))
    );

    let windowed = window_by_count(
        Stream::pull(
            data_metadata(),
            vec![
                data_packet(Symbol::qualified("stream/data", "model-event"), Expr::Nil),
                data_packet(Symbol::qualified("stream/data", "rank-frontier"), Expr::Nil),
                data_packet(Symbol::qualified("stream/data", "model-event"), Expr::Nil),
            ],
        ),
        2,
    );
    let windows = windowed.take_packets(8).unwrap();

    assert_eq!(windows.len(), 2);
    assert_eq!(data_kind(&windows[0]), Some(stream_window_data_kind()));
    assert_eq!(window_len(&windows[0]), Some(2));
    assert_eq!(window_len(&windows[1]), Some(1));
}

#[test]
fn record_replay_and_seek_preserve_data_packets_exactly() {
    let expected = vec![
        ticked_data_packet(
            Symbol::qualified("stream/data", "model-event"),
            Expr::Map(vec![(field("delta"), Expr::String("a".to_owned()))]),
            1,
        ),
        ticked_data_packet(
            Symbol::qualified("stream/data", "rank-frontier"),
            Expr::Map(vec![(field("rank"), Expr::String("b".to_owned()))]),
            2,
        ),
        ticked_data_packet(
            Symbol::qualified("stream/data", "model-event"),
            Expr::Map(vec![(field("final"), Expr::Bool(true))]),
            3,
        ),
    ];
    let recording = record_bang(&Stream::pull(data_metadata(), expected.clone())).unwrap();

    assert_eq!(recording.items(), expected.as_slice());
    assert_eq!(recording.replay().take_packets(8).unwrap(), expected);

    let target_tick = tick(2);
    let by_clock = recording.seek(SeekTarget::clock_index(
        target_tick.clock.clone(),
        target_tick.index.clone(),
    ));
    assert_eq!(by_clock.take_packets(8).unwrap(), expected[1..].to_vec());
}

#[test]
fn record_ledger_slice_preserves_data_payload_equality() {
    let mut cx = sim_kernel::Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));
    let run = Ref::Symbol(Symbol::qualified("run", "data-recorded-remote"));
    let expected = vec![
        ticked_data_packet(
            Symbol::qualified("stream/data", "model-event"),
            Expr::Map(vec![(field("delta"), Expr::String("one".to_owned()))]),
            1,
        ),
        ticked_data_packet(
            Symbol::qualified("stream/data", "rank-frontier"),
            Expr::List(vec![Expr::String("frontier".to_owned())]),
            2,
        ),
    ];
    let stream = Stream::pull(data_metadata(), expected.clone());
    let events = run_bang(&stream, &mut cx, run.clone(), 0).unwrap();
    let mut ledger = EventLedger::new();
    for event in events {
        ledger
            .push_with_ticks(event.run, event.ticks, event.kind)
            .unwrap();
    }

    let recording = record_ledger_slice(&mut cx, data_metadata(), &ledger, &run, 0..3).unwrap();

    assert_eq!(recording.items(), expected.as_slice());
    assert_eq!(recording.replay().take_packets(8).unwrap(), expected);
}

#[test]
fn record_bang_bounded_returns_at_the_bound_on_a_live_source() {
    let source = Stream::new(InfiniteSource {
        metadata: metadata(),
    });

    let err = record_bang_bounded(&source, 16).unwrap_err();

    assert!(format!("{err}").contains("cannot record more than 16"));
}

#[test]
fn record_bang_bounded_captures_a_finite_source_within_the_bound() {
    let items = vec![packet("one"), packet("two")];

    let recording = record_bang_bounded(&Stream::pull(metadata(), items.clone()), 16).unwrap();

    assert_eq!(recording.items(), items.as_slice());
}

/// A never-ending diagnostic source that never reaches `done`.
struct InfiniteSource {
    metadata: StreamMetadata,
}

impl StreamNode for InfiniteSource {
    fn metadata(&self) -> &StreamMetadata {
        &self.metadata
    }

    fn next_packet(&self) -> sim_kernel::Result<Option<StreamItem>> {
        Ok(Some(packet("tick")))
    }

    fn is_done(&self) -> sim_kernel::Result<bool> {
        Ok(false)
    }
}

fn packet(message: &str) -> StreamItem {
    StreamItem::new(StreamPacket::Diagnostic(StreamDiagnostic::new(
        Symbol::qualified("stream/test", "packet"),
        message,
    )))
}

fn ticked_packet(message: &str, index: u8) -> StreamItem {
    StreamItem::with_ticks(packet(message).packet().clone(), vec![tick(index)]).unwrap()
}

fn data_packet(kind: Symbol, payload: Expr) -> StreamItem {
    StreamItem::new(StreamPacket::data(kind, payload))
}

fn ticked_data_packet(kind: Symbol, payload: Expr, index: u8) -> StreamItem {
    StreamItem::with_ticks(StreamPacket::data(kind, payload), vec![tick(index)]).unwrap()
}

fn tick(index: u8) -> Tick {
    let mut cx = sim_kernel::Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));
    test_clock()
        .tick_for_index(&mut cx, ClockIndex::new(u64::from(index)))
        .unwrap()
}

fn content_ref_ticked_packet(message: &str, index: u8) -> StreamItem {
    StreamItem::with_ticks(
        packet(message).packet().clone(),
        vec![content_ref_tick(index)],
    )
    .unwrap()
}

fn content_ref_tick(index: u8) -> Tick {
    Tick::new(
        clock_symbol(),
        Ref::Content(ContentId::from_bytes(
            Symbol::qualified("core", "sha256"),
            [index; 32],
        )),
    )
}

fn test_clock() -> Clock {
    Clock::frame_with_domain(clock_symbol(), ClockDomain::Control, 1).unwrap()
}

fn messages(items: Vec<StreamItem>) -> Vec<String> {
    items
        .into_iter()
        .map(|item| message(&item).to_owned())
        .collect()
}

fn message(item: &StreamItem) -> &str {
    let StreamPacket::Diagnostic(diagnostic) = item.packet() else {
        panic!("expected diagnostic packet");
    };
    diagnostic.message()
}

fn data_kind(item: &StreamItem) -> Option<Symbol> {
    match item.packet() {
        StreamPacket::Data(packet) => Some(packet.kind.clone()),
        _ => None,
    }
}

fn data_payload(item: &StreamItem) -> Option<&Expr> {
    match item.packet() {
        StreamPacket::Data(packet) => Some(&packet.payload),
        _ => None,
    }
}

fn map_value<'a>(expr: &'a Expr, name: &str) -> Option<&'a Expr> {
    let Expr::Map(entries) = expr else {
        return None;
    };
    entries.iter().find_map(|(key, value)| match key {
        Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == name => {
            Some(value)
        }
        _ => None,
    })
}

fn window_len(item: &StreamItem) -> Option<usize> {
    let Expr::List(items) = data_payload(item)? else {
        return None;
    };
    Some(items.len())
}

fn field(name: &str) -> Expr {
    Expr::Symbol(Symbol::new(name))
}

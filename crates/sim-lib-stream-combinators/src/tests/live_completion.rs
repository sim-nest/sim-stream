use std::sync::Arc;

use sim_kernel::{Expr, Symbol};
use sim_lib_stream_core::{StreamValue, TransportProfile, spine::PushResult};

use crate::{SeekTarget, Stream, fan, record_bang, record_cassette_bang, seek, window_by_count};

use super::{
    data_metadata, data_packet, message, metadata, packet, tick, ticked_packet, window_len,
};

#[test]
fn fan_keeps_empty_open_push_streams_live() {
    let source = Arc::new(StreamValue::push(metadata()));
    let fanout = fan(Stream::from_value(Arc::clone(&source)));

    assert_eq!(fanout.left.next_packet().unwrap(), None);
    assert!(!fanout.left.is_done().unwrap());
    assert_eq!(
        source.push_packet(packet("late")).unwrap(),
        PushResult::Accepted
    );

    let left = fanout.left.next_packet().unwrap().unwrap();
    assert_eq!(message(&left), "late");
    let right = fanout.right.next_packet().unwrap().unwrap();
    assert_eq!(message(&right), "late");

    source.close_push().unwrap();
    assert_eq!(fanout.left.next_packet().unwrap(), None);
    assert!(fanout.left.is_done().unwrap());
    assert_eq!(fanout.right.next_packet().unwrap(), None);
    assert!(fanout.right.is_done().unwrap());
}

#[test]
fn window_by_count_holds_partial_window_until_source_done() {
    let source = Arc::new(StreamValue::push(data_metadata()));
    let windowed = window_by_count(Stream::from_value(Arc::clone(&source)), 2);

    assert_eq!(
        source
            .push_packet(data_packet(
                Symbol::qualified("stream/data", "model-event"),
                Expr::String("one".to_owned()),
            ))
            .unwrap(),
        PushResult::Accepted
    );
    assert_eq!(windowed.next_packet().unwrap(), None);
    assert!(!windowed.is_done().unwrap());

    assert_eq!(
        source
            .push_packet(data_packet(
                Symbol::qualified("stream/data", "rank-frontier"),
                Expr::String("two".to_owned()),
            ))
            .unwrap(),
        PushResult::Accepted
    );
    let full = windowed.next_packet().unwrap().unwrap();
    assert_eq!(window_len(&full), Some(2));

    assert_eq!(
        source
            .push_packet(data_packet(
                Symbol::qualified("stream/data", "model-event"),
                Expr::String("three".to_owned()),
            ))
            .unwrap(),
        PushResult::Accepted
    );
    assert_eq!(windowed.next_packet().unwrap(), None);
    source.close_push().unwrap();
    let partial = windowed.next_packet().unwrap().unwrap();
    assert_eq!(window_len(&partial), Some(1));
    assert!(windowed.is_done().unwrap());
}

#[test]
fn seek_keeps_open_push_gap_pending_until_target_arrives() {
    let source = Arc::new(StreamValue::push(metadata()));
    let sought = seek(
        Stream::from_value(Arc::clone(&source)),
        SeekTarget::packet_index(1),
    );

    assert_eq!(
        source.push_packet(packet("skip")).unwrap(),
        PushResult::Accepted
    );
    assert_eq!(sought.next_packet().unwrap(), None);
    assert!(!sought.is_done().unwrap());

    assert_eq!(
        source.push_packet(packet("target")).unwrap(),
        PushResult::Accepted
    );
    let target = sought.next_packet().unwrap().unwrap();
    assert_eq!(message(&target), "target");

    assert_eq!(
        source.push_packet(packet("after")).unwrap(),
        PushResult::Accepted
    );
    let after = sought.next_packet().unwrap().unwrap();
    assert_eq!(message(&after), "after");
}

#[test]
fn seek_by_clock_keeps_open_push_gap_pending_until_target_arrives() {
    let source = Arc::new(StreamValue::push(metadata()));
    let target_tick = tick(2);
    let sought = seek(
        Stream::from_value(Arc::clone(&source)),
        SeekTarget::clock_index(target_tick.clock.clone(), target_tick.index.clone()),
    );

    assert_eq!(
        source.push_packet(ticked_packet("skip", 1)).unwrap(),
        PushResult::Accepted
    );
    assert_eq!(sought.next_packet().unwrap(), None);
    assert!(!sought.is_done().unwrap());

    assert_eq!(
        source.push_packet(ticked_packet("target", 2)).unwrap(),
        PushResult::Accepted
    );
    let target = sought.next_packet().unwrap().unwrap();
    assert_eq!(message(&target), "target");
}

#[test]
fn record_and_cassette_reject_live_gap_without_closing_source() {
    let source = Arc::new(StreamValue::push(metadata()));
    let stream = Stream::from_value(Arc::clone(&source));
    assert_eq!(
        source.push_packet(packet("early")).unwrap(),
        PushResult::Accepted
    );

    let err = record_bang(&stream).unwrap_err();

    assert!(format!("{err}").contains("has not reached done"));
    assert!(!stream.is_done().unwrap());
    assert_eq!(
        source.push_packet(packet("late")).unwrap(),
        PushResult::Accepted
    );
    source.close_push().unwrap();
    let late = stream.next_packet().unwrap().unwrap();
    assert_eq!(message(&late), "late");
    assert!(stream.is_done().unwrap());

    let cassette_source = Arc::new(StreamValue::push(metadata()));
    let cassette_stream = Stream::from_value(Arc::clone(&cassette_source));
    assert_eq!(
        cassette_source.push_packet(packet("early")).unwrap(),
        PushResult::Accepted
    );

    let err = record_cassette_bang(&cassette_stream, TransportProfile::memory_local()).unwrap_err();

    assert!(format!("{err}").contains("has not reached done"));
    assert!(!cassette_stream.is_done().unwrap());
    assert_eq!(
        cassette_source.push_packet(packet("late")).unwrap(),
        PushResult::Accepted
    );
    cassette_source.close_push().unwrap();
    let late = cassette_stream.next_packet().unwrap().unwrap();
    assert_eq!(message(&late), "late");
    assert!(cassette_stream.is_done().unwrap());
}

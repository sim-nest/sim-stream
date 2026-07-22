use crate::{StreamCassette, TransportProfile, spine::PushResult};

use super::{diagnostic_metadata, item};

#[test]
fn stream_cassette_rejects_open_push_stream_after_temporary_gap() {
    let stream = crate::StreamValue::push(diagnostic_metadata());
    assert_eq!(
        stream.push_packet(item("early")).unwrap(),
        PushResult::Accepted
    );

    let err =
        StreamCassette::from_stream_value(&stream, TransportProfile::memory_local()).unwrap_err();

    assert!(format!("{err}").contains("has not reached done"));
    assert!(!stream.is_done().unwrap());
    assert_eq!(
        stream.push_packet(item("late")).unwrap(),
        PushResult::Accepted
    );
    stream.close_push().unwrap();
    assert_eq!(stream.next_packet().unwrap(), Some(item("late")));
    assert!(stream.is_done().unwrap());
}

use sim_kernel::{Expr, Ref, Symbol, Tick};

use crate::{
    BufferOverflowPolicy, BufferPolicy, StreamDirection, StreamFaultKind, StreamFaultPlan,
    StreamFaultSpec, StreamInspectorSnapshot, StreamInspectorStatus, StreamItem, StreamMedia,
    StreamMetadata, StreamPacket, TransportProfile, stream_fault_symbols,
    stream_inspector_model_symbol, stream_inspector_route_local_symbol,
};

#[test]
fn stream_inspector_snapshot_reports_queue_status_and_diagnostics() {
    let stream = crate::StreamValue::pull(diagnostic_metadata(), vec![item("one"), item("two")]);
    let snapshot = StreamInspectorSnapshot::from_stream_value(
        &stream,
        stream_inspector_route_local_symbol(),
        &TransportProfile::memory_local(),
        vec![Symbol::qualified("stream/test", "recent")],
    )
    .unwrap();

    assert_eq!(
        snapshot.stream_id,
        Symbol::qualified("stream", "diagnostics")
    );
    assert_eq!(snapshot.route, stream_inspector_route_local_symbol());
    assert_eq!(snapshot.media, StreamMedia::Diagnostic);
    assert_eq!(
        snapshot.profile,
        Symbol::qualified("stream/profile", "memory-local")
    );
    assert_eq!(snapshot.clock, Symbol::qualified("clock", "sample"));
    assert_eq!(snapshot.status, StreamInspectorStatus::Live);
    assert_eq!(snapshot.queue_depth, 2);
    assert_eq!(snapshot.last_sequence, Some(1));
    assert_eq!(
        table_value(&snapshot.to_expr(), "inspector"),
        Some(&Expr::Symbol(stream_inspector_model_symbol()))
    );

    let _ = stream.take_packets(2).unwrap();
    let ended = StreamInspectorSnapshot::from_stream_value(
        &stream,
        stream_inspector_route_local_symbol(),
        &TransportProfile::memory_local(),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(ended.status, StreamInspectorStatus::Ended);
    assert_eq!(ended.queue_depth, 0);
}

#[test]
fn stream_fault_plan_names_and_applies_required_faults() {
    let items = vec![item("one"), item("two"), item("three")];
    let plan = StreamFaultPlan::new(vec![
        StreamFaultSpec::new(StreamFaultKind::Drop, 1),
        StreamFaultSpec::new(StreamFaultKind::Reorder, 1),
        StreamFaultSpec::new(StreamFaultKind::Duplicate, 1),
        StreamFaultSpec::new(StreamFaultKind::Delay, 1),
        StreamFaultSpec::new(StreamFaultKind::Cancel, 1),
        StreamFaultSpec::new(StreamFaultKind::Timeout, 1),
        StreamFaultSpec::new(StreamFaultKind::Disconnect, 1),
        StreamFaultSpec::new(StreamFaultKind::Reconnect, 1),
        StreamFaultSpec::new(StreamFaultKind::UnsupportedProfile, 1),
    ]);
    let result = plan.apply(&items);

    assert_eq!(stream_fault_symbols().len(), 9);
    assert_eq!(result.diagnostics, stream_fault_symbols().to_vec());
    assert_eq!(result.items.len(), 3);
    let Expr::List(faults) = plan.to_expr() else {
        panic!("fault plan should encode as a list");
    };
    assert_eq!(
        table_value(&faults[0], "fault"),
        Some(&Expr::Symbol(StreamFaultKind::Drop.symbol()))
    );
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
    StreamItem::with_ticks(
        StreamPacket::Diagnostic(crate::StreamDiagnostic::new(
            Symbol::qualified("stream/test", "packet"),
            message,
        )),
        vec![Tick::new(
            Symbol::qualified("clock", "sample"),
            Ref::Symbol(Symbol::qualified("stream/test", message)),
        )],
    )
    .unwrap()
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

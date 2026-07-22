mod lazy;
mod live_catalog;
mod support;

use std::sync::Arc;

use sim_codec::encode_with_codec;
use sim_kernel::{Error, Expr, Symbol, force_list_to_vec};
use sim_lib_stream_core::StreamPacket;

use crate::{
    stream_cancel_capability, stream_control_capability, stream_open_capability,
    stream_push_capability, stream_read_capability, stream_stats_capability,
    stream_transform_capability, stream_write_capability,
};

use support::*;

const STREAM_CARD_FIELDS: &[&str] = &[
    "subject",
    "kind",
    "help",
    "args",
    "result",
    "tests",
    "ops",
    "requires",
    "see-also",
    "shape-known",
    "facets",
    "coverage",
    "provenance",
    "freshness",
];

#[test]
fn lisp_opens_midi_memory_source_and_pulls_packets() {
    let mut cx = cx(&[stream_open_capability(), stream_read_capability()]);
    let source = eval_lisp(&mut cx, &midi_source_form("stream/test-midi")).unwrap();
    cx.env_mut().define(Symbol::new("src"), source);

    let packet = eval_lisp(
        &mut cx,
        "(expr:call (expr:symbol \"stream\" \"next!\") src)",
    )
    .unwrap();
    let packet = value_expr(&mut cx, packet);
    assert_eq!(
        table_value(&packet, "packet"),
        Some(&Expr::Symbol(Symbol::qualified("stream/packet", "midi")))
    );
    let Some(Expr::List(events)) = table_value(&packet, "events") else {
        panic!("expected MIDI events");
    };
    assert_eq!(events.len(), 2);
}

#[test]
fn lisp_opens_pcm_memory_sink_and_writes_packets() {
    let mut cx = cx(&[
        stream_open_capability(),
        stream_push_capability(),
        stream_stats_capability(),
    ]);
    let sink = eval_lisp(&mut cx, &pcm_sink_form("stream/test-pcm-sink")).unwrap();
    cx.env_mut().define(Symbol::new("sink"), sink);

    let wrote = eval_lisp(
        &mut cx,
        concat!(
            "(expr:call (expr:symbol \"stream\" \"write!\") sink ",
            "(quote (expr:map [packet stream/packet/pcm] [channels \"2\"] ",
            "[frames \"1\"] [sample-format pcm/i16] [samples (\"7\" \"-7\")])))"
        ),
    )
    .unwrap();
    let wrote = value_expr(&mut cx, wrote);
    assert_eq!(wrote, Expr::Bool(true));

    let stats = eval_lisp(&mut cx, "(stream/stats sink)").unwrap();
    let stats = value_expr(&mut cx, stats);
    assert_eq!(
        table_value(&stats, "pushed"),
        Some(&Expr::String("1".to_owned()))
    );
}

#[test]
fn full_memory_pipeline_runs() {
    let mut cx = cx(&[
        stream_open_capability(),
        stream_read_capability(),
        stream_push_capability(),
    ]);
    let source = eval_lisp(&mut cx, &pcm_source_form("stream/test-pcm-source")).unwrap();
    let sink = eval_lisp(&mut cx, &pcm_sink_form("stream/test-pcm-sink")).unwrap();
    cx.env_mut().define(Symbol::new("src"), source);
    cx.env_mut().define(Symbol::new("sink"), sink.clone());

    let report = eval_lisp(
        &mut cx,
        concat!(
            "(expr:call (expr:symbol \"stream\" \"run!\") ",
            "(expr:call stream/pipe src (expr:call stream/identity) sink))"
        ),
    )
    .unwrap();
    let report = value_expr(&mut cx, report);
    assert_eq!(
        table_value(&report, "packets"),
        Some(&Expr::String("2".to_owned()))
    );
    assert_eq!(
        table_value(&report, "written"),
        Some(&Expr::String("2".to_owned()))
    );

    let packets = eval_lisp(&mut cx, "(stream/sink-packets sink)").unwrap();
    let packets = value_expr(&mut cx, packets);
    let Expr::List(packets) = packets else {
        panic!("expected sink packet list");
    };
    assert_eq!(packets.len(), 2);
}

#[test]
fn stream_card_shows_metadata_stats_done_and_cancelled() {
    let mut cx = cx(&[
        stream_open_capability(),
        stream_read_capability(),
        stream_cancel_capability(),
        stream_stats_capability(),
    ]);
    let source = eval_lisp(&mut cx, &midi_source_form("stream/card-midi")).unwrap();
    cx.env_mut().define(Symbol::new("src"), source);
    eval_lisp(
        &mut cx,
        "(expr:call (expr:symbol \"stream\" \"next!\") src)",
    )
    .unwrap();
    eval_lisp(
        &mut cx,
        "(expr:call (expr:symbol \"stream\" \"cancel!\") src)",
    )
    .unwrap();

    let card = eval_lisp(&mut cx, "(stream/card src)").unwrap();
    let card = value_expr(&mut cx, card);
    assert_eq!(
        table_value(&card, "kind"),
        Some(&Expr::Symbol(Symbol::qualified("stream", "handle")))
    );
    assert!(table_value(&card, "metadata").is_some());
    assert_eq!(table_value(&card, "done"), Some(&Expr::Bool(true)));
    assert_eq!(table_value(&card, "cancelled"), Some(&Expr::Bool(true)));
    let Some(stats) = table_value(&card, "stats") else {
        panic!("expected stream stats");
    };
    assert_eq!(
        table_value(stats, "yielded"),
        Some(&Expr::String("1".to_owned()))
    );
    let Some(Expr::List(requires)) = table_value(&card, "requires") else {
        panic!("expected card requires");
    };
    assert!(has_symbol(requires, "capability", "stream.read"));
    assert!(has_symbol(requires, "capability", "stream.push"));
    assert!(has_symbol(requires, "capability", "stream.cancel"));
    assert!(has_symbol(requires, "capability", "stream.stats"));
    assert!(!has_symbol(requires, "capability", "stream.write"));
}

#[test]
fn stream_browse_schema_is_stable() {
    let mut cx = cx(&[
        stream_open_capability(),
        stream_read_capability(),
        stream_stats_capability(),
    ]);
    eval_lisp(&mut cx, &midi_source_form("stream/schema-midi")).unwrap();

    let list = eval_lisp(&mut cx, "(expr:call (expr:symbol \"stream\" \"list\"))").unwrap();
    let list_value = list
        .object()
        .as_list()
        .expect("stream/list result is a list");
    let cards = force_list_to_vec(&mut cx, list_value, "stream/list").unwrap();
    let first = cards
        .first()
        .expect("expected one live stream card")
        .object()
        .as_expr(&mut cx)
        .unwrap();
    assert_eq!(
        field_names(&first)[..STREAM_CARD_FIELDS.len()],
        *STREAM_CARD_FIELDS
    );
    assert_eq!(
        table_value(&first, "kind"),
        Some(&Expr::Symbol(Symbol::qualified("stream", "handle")))
    );
    assert!(table_value(&first, "facets").is_some());
    assert!(table_value(&first, "freshness").is_some());
}

#[test]
fn graph_lisp_round_trips_a_pipeline() {
    let mut cx = cx(&[
        stream_open_capability(),
        stream_read_capability(),
        stream_push_capability(),
    ]);
    let source = eval_lisp(&mut cx, &pcm_source_form("stream/test-pcm-source")).unwrap();
    let sink = eval_lisp(&mut cx, &pcm_sink_form("stream/test-pcm-sink")).unwrap();
    cx.env_mut().define(Symbol::new("src"), source);
    cx.env_mut().define(Symbol::new("sink"), sink);

    let graph = eval_lisp(&mut cx, "(stream/graph-lisp (stream/pipe src sink))").unwrap();
    let graph = value_expr(&mut cx, graph);
    let encoded = encode_with_codec(
        &mut cx,
        &Symbol::qualified("codec", "lisp"),
        &graph,
        Default::default(),
    )
    .unwrap()
    .into_text()
    .unwrap();
    assert_eq!(
        encoded,
        "(expr:call stream/pipe \"stream/test-pcm-source\" \"stream/test-pcm-sink\")"
    );

    let decoded = decode_expr(&mut cx, &encoded);
    assert!(decoded.canonical_eq(&graph));
}

#[test]
fn diagnostic_explanation_includes_stream_id_and_kind() {
    let mut cx = cx(&[stream_read_capability()]);
    let explanation = eval_lisp(
        &mut cx,
        concat!(
            "(stream/explain-diagnostic ",
            "(quote (expr:map [packet stream/packet/diagnostic] ",
            "[kind stream/diagnostic/silence] [message \"rms below threshold\"])) ",
            "\"audio-3\")"
        ),
    )
    .unwrap();
    let explanation = value_expr(&mut cx, explanation);
    assert_eq!(
        table_value(&explanation, "stream-id"),
        Some(&Expr::String("audio-3".to_owned()))
    );
    assert_eq!(
        table_value(&explanation, "kind"),
        Some(&Expr::Symbol(Symbol::qualified(
            "stream/diagnostic",
            "silence"
        )))
    );
    let Some(Expr::String(text)) = table_value(&explanation, "explanation") else {
        panic!("expected diagnostic explanation text");
    };
    assert!(text.contains("audio-3"));
    assert!(text.contains("stream/diagnostic/silence"));
}

#[test]
fn stream_describe_shows_data_packet_kind_and_payload() {
    let mut cx = cx(&[stream_read_capability()]);
    let card = eval_lisp(
        &mut cx,
        concat!(
            "(stream/describe ",
            "(quote (expr:map [packet stream/packet/data] ",
            "[kind stream/data/model-event] ",
            "[payload (expr:map [text \"hello\"])])))"
        ),
    )
    .unwrap();
    let card = value_expr(&mut cx, card);

    assert_eq!(
        table_value(&card, "packet-kind"),
        Some(&Expr::Symbol(Symbol::qualified("stream/packet", "data")))
    );
    assert_eq!(
        table_value(&card, "data-kind"),
        Some(&Expr::Symbol(Symbol::qualified(
            "stream/data",
            "model-event"
        )))
    );
    assert_eq!(
        table_value(&card, "payload-shape"),
        Some(&Expr::Symbol(Symbol::qualified("core", "Map")))
    );
    let Some(payload) = table_value(&card, "payload") else {
        panic!("expected data packet payload");
    };
    assert_eq!(
        table_value(payload, "text"),
        Some(&Expr::String("hello".to_owned()))
    );
}

#[test]
fn lisp_data_stream_combinators_filter_map_shape_and_window() {
    let mut cx = cx(&[stream_read_capability(), stream_transform_capability()]);
    let source = data_source(
        &mut cx,
        "stream/data-combinators",
        vec![
            StreamPacket::model_event(Expr::Map(vec![(
                field_expr("text"),
                Expr::String("hello".to_owned()),
            )])),
            StreamPacket::rank_frontier(Expr::Map(vec![(
                field_expr("rank"),
                Expr::String("frontier-1".to_owned()),
            )])),
            StreamPacket::model_event(Expr::Map(vec![(
                field_expr("text"),
                Expr::String("bye".to_owned()),
            )])),
        ],
    );
    cx.env_mut().define(Symbol::new("src"), source);
    let mark = cx.factory().opaque(Arc::new(MarkFn)).unwrap();
    cx.env_mut().define(Symbol::new("mark"), mark);
    let has_rank = cx.factory().opaque(Arc::new(HasRankShape)).unwrap();
    cx.env_mut().define(Symbol::new("has-rank"), has_rank);

    let model_stream =
        eval_lisp(&mut cx, "(stream/filter-kind src 'stream/data/model-event)").unwrap();
    cx.env_mut().define(Symbol::new("models"), model_stream);

    let mapped = eval_lisp(&mut cx, "(stream/map-expr models mark)").unwrap();
    cx.env_mut().define(Symbol::new("mapped"), mapped);

    let first = eval_lisp(
        &mut cx,
        "(expr:call (expr:symbol \"stream\" \"next!\") mapped)",
    )
    .unwrap();
    let first = value_expr(&mut cx, first);
    let payload = packet_payload(&first)
        .unwrap_or_else(|| panic!("expected mapped data payload, got {first:?}"));
    assert_eq!(
        table_value(payload, "text"),
        Some(&Expr::String("hello".to_owned()))
    );
    assert_eq!(table_value(payload, "mapped"), Some(&Expr::Bool(true)));
    let second = eval_lisp(
        &mut cx,
        "(expr:call (expr:symbol \"stream\" \"next!\") mapped)",
    )
    .unwrap();
    assert!(matches!(value_expr(&mut cx, second), Expr::Map(_)));
    let done = eval_lisp(
        &mut cx,
        "(expr:call (expr:symbol \"stream\" \"next!\") mapped)",
    )
    .unwrap();
    assert_eq!(value_expr(&mut cx, done), Expr::Nil);

    let rank_source = data_source(
        &mut cx,
        "stream/rank-shape",
        vec![
            StreamPacket::rank_frontier(Expr::Map(vec![(
                field_expr("rank"),
                Expr::String("frontier-2".to_owned()),
            )])),
            StreamPacket::model_event(Expr::Map(vec![(
                field_expr("text"),
                Expr::String("ignored".to_owned()),
            )])),
        ],
    );
    cx.env_mut().define(Symbol::new("rank-src"), rank_source);
    let shaped = eval_lisp(&mut cx, "(stream/filter-shape rank-src has-rank)").unwrap();
    cx.env_mut().define(Symbol::new("ranked"), shaped);
    let rank = eval_lisp(
        &mut cx,
        "(expr:call (expr:symbol \"stream\" \"next!\") ranked)",
    )
    .unwrap();
    let rank = value_expr(&mut cx, rank);
    assert_eq!(
        packet_kind(&rank),
        Some(Symbol::qualified("stream/data", "rank-frontier"))
    );
    assert_eq!(
        table_value(packet_payload(&rank).unwrap(), "rank"),
        Some(&Expr::String("frontier-2".to_owned()))
    );
    let done = eval_lisp(
        &mut cx,
        "(expr:call (expr:symbol \"stream\" \"next!\") ranked)",
    )
    .unwrap();
    assert_eq!(value_expr(&mut cx, done), Expr::Nil);

    let window_source = data_source(
        &mut cx,
        "stream/window-source",
        vec![
            StreamPacket::model_event(Expr::String("one".to_owned())),
            StreamPacket::rank_frontier(Expr::String("two".to_owned())),
            StreamPacket::model_event(Expr::String("three".to_owned())),
        ],
    );
    cx.env_mut()
        .define(Symbol::new("window-src"), window_source);
    let windowed = eval_lisp(&mut cx, "(stream/window window-src 2)").unwrap();
    cx.env_mut().define(Symbol::new("windowed"), windowed);
    let window = eval_lisp(
        &mut cx,
        "(expr:call (expr:symbol \"stream\" \"next!\") windowed)",
    )
    .unwrap();
    let window = value_expr(&mut cx, window);
    assert_eq!(
        packet_kind(&window),
        Some(Symbol::qualified("stream/data", "window"))
    );
    let Expr::List(items) = packet_payload(&window).unwrap() else {
        panic!("expected window payload list");
    };
    assert_eq!(items.len(), 2);
}

#[test]
fn map_expr_requires_stream_transform_capability() {
    let mut cx = cx(&[stream_read_capability()]);
    let source = data_source(
        &mut cx,
        "stream/map-no-cap",
        vec![StreamPacket::model_event(Expr::String("hello".to_owned()))],
    );
    cx.env_mut().define(Symbol::new("src"), source);
    let mark = cx.factory().opaque(Arc::new(MarkFn)).unwrap();
    cx.env_mut().define(Symbol::new("mark"), mark);

    let err = eval_lisp(&mut cx, "(stream/map-expr src mark)").unwrap_err();

    match err {
        Error::CapabilityDenied { capability } if capability == stream_transform_capability() => {}
        other => panic!("expected stream.transform denial, got {other:?}"),
    }
}

#[test]
fn cell_edit_requires_control_capability() {
    let mut cx = cx(&[stream_open_capability(), stream_read_capability()]);
    let cell = eval_lisp(&mut cx, "(stream/cell \"gain-1\" 1.0)").unwrap();
    cx.env_mut().define(Symbol::new("gain"), cell);

    let err = eval_lisp(
        &mut cx,
        "(expr:call (expr:symbol \"stream\" \"cell-set!\") gain 0.5)",
    )
    .unwrap_err();
    match err {
        Error::CapabilityDenied { capability } if capability == stream_control_capability() => {}
        other => panic!("expected stream.control denial, got {other:?}"),
    }

    cx.grant(stream_control_capability());
    let snapshot = eval_lisp(
        &mut cx,
        "(expr:call (expr:symbol \"stream\" \"cell-set!\") gain 0.5)",
    )
    .unwrap();
    let snapshot = value_expr(&mut cx, snapshot);
    assert_eq!(
        table_value(&snapshot, "value"),
        Some(&Expr::String("0.5".to_owned()))
    );
    assert_eq!(
        table_value(&snapshot, "version"),
        Some(&Expr::String("1".to_owned()))
    );

    let card = eval_lisp(&mut cx, "(stream/describe gain)").unwrap();
    let card = value_expr(&mut cx, card);
    assert_eq!(
        table_value(&card, "kind"),
        Some(&Expr::Symbol(Symbol::qualified("stream", "cell")))
    );
}

#[test]
fn write_requires_canonical_stream_push_capability() {
    assert_eq!(stream_write_capability(), stream_push_capability());

    let mut cx = cx(&[stream_open_capability()]);
    let sink = eval_lisp(&mut cx, &pcm_sink_form("stream/write-cap-sink")).unwrap();
    cx.env_mut().define(Symbol::new("sink"), sink);

    let write_form = concat!(
        "(expr:call (expr:symbol \"stream\" \"write!\") sink ",
        "(quote (expr:map [packet stream/packet/pcm] [channels \"2\"] ",
        "[frames \"1\"] [sample-format pcm/i16] [samples (\"7\" \"-7\")])))"
    );
    let err = eval_lisp(&mut cx, write_form).unwrap_err();
    match err {
        Error::CapabilityDenied { capability } if capability == stream_push_capability() => {}
        other => panic!("expected stream.push denial, got {other:?}"),
    }

    cx.grant(stream_push_capability());
    let wrote = eval_lisp(&mut cx, write_form).unwrap();
    assert_eq!(value_expr(&mut cx, wrote), Expr::Bool(true));
}

#[test]
fn cancel_requires_stream_cancel_capability() {
    let mut cx = cx(&[stream_open_capability()]);
    let source = eval_lisp(&mut cx, &midi_source_form("stream/cancel-cap-midi")).unwrap();
    cx.env_mut().define(Symbol::new("src"), source);

    let err = eval_lisp(
        &mut cx,
        "(expr:call (expr:symbol \"stream\" \"cancel!\") src)",
    )
    .unwrap_err();
    match err {
        Error::CapabilityDenied { capability } if capability == stream_cancel_capability() => {}
        other => panic!("expected stream.cancel denial, got {other:?}"),
    }

    cx.grant(stream_cancel_capability());
    eval_lisp(
        &mut cx,
        "(expr:call (expr:symbol \"stream\" \"cancel!\") src)",
    )
    .unwrap();
}

#[test]
fn cancel_older_than_requires_stream_control_capability() {
    let mut cx = cx(&[]);
    let err = eval_lisp(&mut cx, "(stream/cancel-older-than! 0)").unwrap_err();
    match err {
        Error::CapabilityDenied { capability } if capability == stream_control_capability() => {}
        other => panic!("expected stream.control denial, got {other:?}"),
    }
}

#[test]
fn stats_requires_stream_stats_capability() {
    let mut cx = cx(&[stream_open_capability()]);
    let source = eval_lisp(&mut cx, &midi_source_form("stream/stats-cap-midi")).unwrap();
    cx.env_mut().define(Symbol::new("src"), source);

    let err = eval_lisp(&mut cx, "(stream/stats src)").unwrap_err();
    match err {
        Error::CapabilityDenied { capability } if capability == stream_stats_capability() => {}
        other => panic!("expected stream.stats denial, got {other:?}"),
    }

    cx.grant(stream_stats_capability());
    let stats = eval_lisp(&mut cx, "(stream/stats src)").unwrap();
    let stats = value_expr(&mut cx, stats);
    assert!(table_value(&stats, "yielded").is_some());
}

#[test]
fn metadata_requires_stream_read_capability() {
    let mut cx = cx(&[stream_open_capability()]);
    let source = eval_lisp(&mut cx, &midi_source_form("stream/metadata-cap-midi")).unwrap();
    cx.env_mut().define(Symbol::new("src"), source);

    let err = eval_lisp(&mut cx, "(stream/metadata src)").unwrap_err();
    match err {
        Error::CapabilityDenied { capability } if capability == stream_read_capability() => {}
        other => panic!("expected stream.read denial, got {other:?}"),
    }

    cx.grant(stream_read_capability());
    let metadata = eval_lisp(&mut cx, "(stream/metadata src)").unwrap();
    let metadata = value_expr(&mut cx, metadata);
    assert_eq!(
        table_value(&metadata, "id"),
        Some(&Expr::String("stream/metadata-cap-midi".to_owned()))
    );
}

#[test]
fn card_requires_stream_read_and_stats_capabilities() {
    let mut cx_with_stats = cx(&[stream_open_capability(), stream_stats_capability()]);
    let source = eval_lisp(
        &mut cx_with_stats,
        &midi_source_form("stream/card-read-cap"),
    )
    .unwrap();
    cx_with_stats.env_mut().define(Symbol::new("src"), source);
    let err = eval_lisp(&mut cx_with_stats, "(stream/card src)").unwrap_err();
    match err {
        Error::CapabilityDenied { capability } if capability == stream_read_capability() => {}
        other => panic!("expected stream.read denial, got {other:?}"),
    }

    let mut cx_with_read = cx(&[stream_open_capability(), stream_read_capability()]);
    let source = eval_lisp(
        &mut cx_with_read,
        &midi_source_form("stream/card-stats-cap"),
    )
    .unwrap();
    cx_with_read.env_mut().define(Symbol::new("src"), source);
    let err = eval_lisp(&mut cx_with_read, "(stream/card src)").unwrap_err();
    match err {
        Error::CapabilityDenied { capability } if capability == stream_stats_capability() => {}
        other => panic!("expected stream.stats denial, got {other:?}"),
    }

    cx_with_read.grant(stream_stats_capability());
    let card = eval_lisp(&mut cx_with_read, "(stream/card src)").unwrap();
    assert!(table_value(&value_expr(&mut cx_with_read, card), "stats").is_some());
}

#[test]
fn missing_capability_is_rejected() {
    let mut cx = cx(&[]);
    let err = eval_lisp(&mut cx, &midi_source_form("stream/no-cap")).unwrap_err();
    assert!(matches!(
        err,
        Error::CapabilityDenied { capability } if capability == stream_open_capability()
    ));
}

fn has_symbol(values: &[Expr], namespace: &str, name: &str) -> bool {
    values.iter().any(|value| {
        matches!(
            value,
            Expr::Symbol(symbol)
                if symbol.namespace.as_deref() == Some(namespace) && symbol.name.as_ref() == name
        )
    })
}

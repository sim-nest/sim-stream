use std::sync::Arc;

use sim_kernel::{Expr, Symbol};
use sim_lib_stream_core::{StreamItem, StreamPacket};

use crate::{stream_read_capability, stream_transform_capability};

use super::support::{
    HasRankShape, MarkFn, cx, eval_lisp, field_expr, live_data_source, packet_kind, packet_payload,
    table_value, value_expr,
};

#[test]
fn map_and_filter_transforms_do_not_drain_live_streams_at_construction() {
    let mut cx = cx(&[stream_read_capability(), stream_transform_capability()]);
    let (map_source, map_stream) = live_data_source(&mut cx, "stream/live-map");
    cx.env_mut().define(Symbol::new("map-src"), map_source);
    let mark = cx.factory().opaque(Arc::new(MarkFn)).unwrap();
    cx.env_mut().define(Symbol::new("mark"), mark);

    let mapped = eval_lisp(&mut cx, "(stream/map-expr map-src mark)").unwrap();
    cx.env_mut().define(Symbol::new("mapped"), mapped);
    let empty = eval_lisp(&mut cx, "(stream/next! mapped)").unwrap();
    assert_eq!(value_expr(&mut cx, empty), Expr::Nil);

    map_stream
        .push_packet(StreamItem::new(StreamPacket::model_event(Expr::Map(vec![
            (field_expr("text"), Expr::String("late".to_owned())),
        ]))))
        .unwrap();
    let packet = eval_lisp(&mut cx, "(stream/next! mapped)").unwrap();
    let packet = value_expr(&mut cx, packet);
    let payload = packet_payload(&packet)
        .unwrap_or_else(|| panic!("expected mapped payload after live push, got {packet:?}"));
    assert_eq!(
        table_value(payload, "text"),
        Some(&Expr::String("late".to_owned()))
    );
    assert_eq!(table_value(payload, "mapped"), Some(&Expr::Bool(true)));

    let (shape_source, shape_stream) = live_data_source(&mut cx, "stream/live-shape");
    cx.env_mut().define(Symbol::new("shape-src"), shape_source);
    let has_rank = cx.factory().opaque(Arc::new(HasRankShape)).unwrap();
    cx.env_mut().define(Symbol::new("has-rank"), has_rank);
    let filtered = eval_lisp(&mut cx, "(stream/filter-shape shape-src has-rank)").unwrap();
    cx.env_mut().define(Symbol::new("filtered"), filtered);
    shape_stream
        .push_packet(StreamItem::new(StreamPacket::model_event(Expr::Map(vec![
            (field_expr("text"), Expr::String("ignored".to_owned())),
        ]))))
        .unwrap();
    shape_stream
        .push_packet(StreamItem::new(StreamPacket::rank_frontier(Expr::Map(
            vec![(field_expr("rank"), Expr::String("frontier-live".to_owned()))],
        ))))
        .unwrap();

    let packet = eval_lisp(&mut cx, "(stream/next! filtered)").unwrap();
    let packet = value_expr(&mut cx, packet);
    assert_eq!(
        packet_kind(&packet),
        Some(Symbol::qualified("stream/data", "rank-frontier"))
    );
    assert_eq!(
        table_value(packet_payload(&packet).unwrap(), "rank"),
        Some(&Expr::String("frontier-live".to_owned()))
    );
}

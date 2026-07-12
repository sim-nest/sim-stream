use sim_kernel::{Cx, Expr, Symbol, Value};

use crate::{
    BufferOverflowPolicy, BufferPolicy, StreamDirection, StreamEnvelope, StreamItem, StreamMedia,
    StreamMetadata, StreamPacket, install_stream_core_shapes_lib, stream_envelope_shape_symbol,
    stream_metadata_shape_symbol,
};

#[test]
fn stream_metadata_shape_accepts_canonical_metadata_table() {
    let mut cx = cx();
    install_stream_core_shapes_lib(&mut cx).unwrap();
    let expr = metadata().table_expr();

    assert_shape_accepts(&mut cx, stream_metadata_shape_symbol(), &expr);
}

#[test]
fn stream_metadata_shape_rejects_malformed_metadata() {
    let mut cx = cx();
    install_stream_core_shapes_lib(&mut cx).unwrap();
    let metadata_shape = stream_metadata_shape_symbol();

    assert_shape_rejects(&mut cx, metadata_shape.clone(), &without_field("media"));
    assert_shape_rejects(
        &mut cx,
        metadata_shape.clone(),
        &with_field("media", Expr::String("pcm".to_owned())),
    );
    assert_shape_rejects(
        &mut cx,
        metadata_shape,
        &with_field(
            "media",
            Expr::Symbol(Symbol::qualified("stream/media", "video")),
        ),
    );
}

#[test]
fn stream_envelope_shape_accepts_canonical_envelope_table() {
    let mut cx = cx();
    install_stream_core_shapes_lib(&mut cx).unwrap();
    let expr = envelope().to_expr();

    assert_shape_accepts(&mut cx, stream_envelope_shape_symbol(), &expr);
}

fn cx() -> Cx {
    sim_kernel::testing::bare_cx()
}

fn metadata() -> StreamMetadata {
    StreamMetadata::new(
        Symbol::qualified("stream", "shape-test"),
        StreamMedia::Diagnostic,
        StreamDirection::Source,
        Symbol::qualified("clock", "sample"),
        BufferPolicy::bounded_with_overflow(4, BufferOverflowPolicy::DropOldest).unwrap(),
    )
}

fn envelope() -> StreamEnvelope {
    let item = StreamItem::new(StreamPacket::Diagnostic(crate::StreamDiagnostic::new(
        Symbol::qualified("stream/diagnostic", "shape-test"),
        "shape test",
    )));
    StreamEnvelope::from_item(&metadata(), 7, &item).unwrap()
}

fn without_field(name: &str) -> Expr {
    let Expr::Map(mut entries) = metadata().table_expr() else {
        panic!("metadata should encode as a map");
    };
    entries.retain(|(key, _)| !is_key(key, name));
    Expr::Map(entries)
}

fn with_field(name: &str, value: Expr) -> Expr {
    let Expr::Map(mut entries) = metadata().table_expr() else {
        panic!("metadata should encode as a map");
    };
    let Some((_, field)) = entries.iter_mut().find(|(key, _)| is_key(key, name)) else {
        panic!("missing metadata field {name}");
    };
    *field = value;
    Expr::Map(entries)
}

fn is_key(expr: &Expr, name: &str) -> bool {
    matches!(expr, Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == name)
}

fn registered_shape(cx: &Cx, symbol: Symbol) -> Value {
    cx.registry()
        .shape_by_symbol(&symbol)
        .expect("registered stream shape")
        .clone()
}

fn assert_shape_accepts(cx: &mut Cx, symbol: Symbol, expr: &Expr) {
    let shape = registered_shape(cx, symbol);
    let matched = shape
        .object()
        .as_shape()
        .expect("shape protocol")
        .check_expr(cx, expr)
        .unwrap();
    assert!(
        matched.accepted,
        "{expr:?} rejected: {:?}",
        matched.diagnostics
    );
}

fn assert_shape_rejects(cx: &mut Cx, symbol: Symbol, expr: &Expr) {
    let shape = registered_shape(cx, symbol);
    let matched = shape
        .object()
        .as_shape()
        .expect("shape protocol")
        .check_expr(cx, expr)
        .unwrap();
    assert!(
        !matched.accepted,
        "{expr:?} unexpectedly matched with score {:?}",
        matched.score
    );
}

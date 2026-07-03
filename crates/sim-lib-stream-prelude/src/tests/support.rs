use std::sync::Arc;

use sim_codec::{DecodePosition, DecodedForm, Input, decode_default_with_codec};
use sim_codec_lisp::LispCodecLib;
use sim_kernel::{
    Args, Callable, CapabilityName, ClassRef, Cx, DefaultFactory, EagerPolicy, Error, Expr,
    MatchScore, Object, ObjectCompat, ReadPolicy, Result, Shape, ShapeDoc, ShapeMatch, Symbol,
    Value,
};
use sim_lib_stream_core::{
    BufferPolicy, StreamDirection, StreamItem, StreamMedia, StreamMetadata, StreamPacket,
    StreamValue,
};

use crate::{StreamHandle, install_stream_prelude_lib};

pub(super) fn cx(capabilities: &[CapabilityName]) -> Cx {
    let mut cx = Cx::new(Arc::new(EagerPolicy), Arc::new(DefaultFactory));
    install_lisp_codec(&mut cx);
    install_stream_prelude_lib(&mut cx).unwrap();
    for capability in capabilities {
        cx.grant(capability.clone());
    }
    cx
}

pub(super) fn eval_lisp(cx: &mut Cx, source: &str) -> sim_kernel::Result<Value> {
    let decoded = decode_default_with_codec(
        cx,
        &Symbol::qualified("codec", "lisp"),
        Input::Text(source.to_owned()),
        ReadPolicy::default(),
        DecodePosition::Eval,
    )?;
    let expr = match decoded {
        DecodedForm::Term(term) => Expr::from(term),
        DecodedForm::Datum(datum) => Expr::from(datum),
    };
    cx.eval_expr(expr)
}

pub(super) fn decode_expr(cx: &mut Cx, source: &str) -> Expr {
    let decoded = decode_default_with_codec(
        cx,
        &Symbol::qualified("codec", "lisp"),
        Input::Text(source.to_owned()),
        ReadPolicy::default(),
        DecodePosition::Eval,
    )
    .unwrap();
    match decoded {
        DecodedForm::Term(term) => Expr::from(term),
        DecodedForm::Datum(datum) => Expr::from(datum),
    }
}

pub(super) fn midi_source_form(id: &str) -> String {
    format!(
        concat!(
            "(stream/open (quote (expr:map [kind stream/memory-midi-source] ",
            "[id \"{id}\"] [tpq \"480\"] [batch-events \"2\"] ",
            "[events [(expr:map [ticks \"0\"] [bytes [\"144\" \"60\" \"100\"]]) ",
            "(expr:map [ticks \"240\"] [bytes [\"128\" \"60\" \"0\"]])]])))"
        ),
        id = id
    )
}

pub(super) fn pcm_source_form(id: &str) -> String {
    format!(
        concat!(
            "(stream/open (quote (expr:map [kind stream/memory-pcm-source] ",
            "[id \"{id}\"] [channels \"2\"] [sample-rate-hz \"48000\"] ",
            "[buffers [(expr:map [frames \"1\"] [samples (\"1\" \"-1\")]) ",
            "(expr:map [frames \"1\"] [samples (\"2\" \"-2\")])]])))"
        ),
        id = id
    )
}

pub(super) fn pcm_sink_form(id: &str) -> String {
    format!(
        concat!(
            "(stream/open (quote (expr:map [kind stream/memory-pcm-sink] ",
            "[id \"{id}\"] [channels \"2\"] [sample-rate-hz \"48000\"])))"
        ),
        id = id
    )
}

pub(super) fn data_source(cx: &mut Cx, id: &str, packets: Vec<StreamPacket>) -> Value {
    let metadata = StreamMetadata::new(
        Symbol::new(id),
        StreamMedia::Data,
        StreamDirection::Source,
        Symbol::qualified("clock", "data"),
        BufferPolicy::bounded(8).unwrap(),
    );
    let items = packets.into_iter().map(StreamItem::new).collect();
    let stream = Arc::new(StreamValue::pull(metadata.clone(), items));
    stream.publish_claims(cx, metadata.subject_ref()).unwrap();
    cx.factory()
        .opaque(Arc::new(StreamHandle::source(metadata, stream)))
        .unwrap()
}

pub(super) struct MarkFn;

impl Callable for MarkFn {
    fn call(&self, cx: &mut Cx, args: Args) -> Result<Value> {
        let [payload] = args.values() else {
            return Err(Error::Eval("mark expects one payload".to_owned()));
        };
        let mut expr = payload.object().as_expr(cx)?;
        if let Expr::Map(entries) = &mut expr {
            entries.push((field_expr("mapped"), Expr::Bool(true)));
        }
        cx.factory().expr(expr)
    }
}

impl Object for MarkFn {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok("#<function test/mark>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for MarkFn {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        cx.factory().class_stub(
            sim_kernel::CORE_FUNCTION_CLASS_ID,
            Symbol::qualified("test", "MarkFn"),
        )
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }
}

pub(super) struct HasRankShape;

impl Shape for HasRankShape {
    fn check_value(&self, cx: &mut Cx, value: Value) -> Result<ShapeMatch> {
        let expr = value.object().as_expr(cx)?;
        self.check_expr(cx, &expr)
    }

    fn check_expr(&self, _cx: &mut Cx, expr: &Expr) -> Result<ShapeMatch> {
        if table_value(expr, "rank").is_some() {
            Ok(ShapeMatch::accept(MatchScore::exact(10)))
        } else {
            Ok(ShapeMatch::reject("rank field missing"))
        }
    }

    fn describe(&self, _cx: &mut Cx) -> Result<ShapeDoc> {
        Ok(ShapeDoc::new("HasRank"))
    }
}

pub(super) fn value_expr(cx: &mut Cx, value: Value) -> Expr {
    value.object().as_expr(cx).unwrap()
}

pub(super) fn packet_kind(expr: &Expr) -> Option<Symbol> {
    match table_value(expr, "kind") {
        Some(Expr::Symbol(symbol)) => Some(symbol.clone()),
        _ => None,
    }
}

pub(super) fn packet_payload(expr: &Expr) -> Option<&Expr> {
    table_value(expr, "payload")
}

pub(super) fn table_value<'a>(expr: &'a Expr, field: &str) -> Option<&'a Expr> {
    let Expr::Map(entries) = expr else {
        return None;
    };
    entries.iter().find_map(|(key, value)| match key {
        Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == field => {
            Some(value)
        }
        _ => None,
    })
}

pub(super) fn field_names(expr: &Expr) -> Vec<String> {
    let Expr::Map(entries) = expr else {
        panic!("expected table, got {expr:?}");
    };
    entries
        .iter()
        .map(|(key, _)| match key {
            Expr::Symbol(symbol) => symbol.to_string(),
            other => panic!("expected symbol table key, got {other:?}"),
        })
        .collect()
}

pub(super) fn field_expr(name: &str) -> Expr {
    Expr::Symbol(Symbol::new(name))
}

fn install_lisp_codec(cx: &mut Cx) {
    let codec_id = cx.registry_mut().fresh_codec_id();
    cx.load_lib(&LispCodecLib::new(codec_id).unwrap()).unwrap();
}

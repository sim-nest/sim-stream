//! Built-in topology adapters for kernel protocols and installed runtimes.

use std::sync::Arc;

use sim_codec::{CodecRuntime, Input, Output, decode_with_codec, encode_with_codec};
use sim_kernel::{
    Args, Consistency, Cx, EncodeOptions, Error, EvalMode, EvalRequest, Expr, ReadPolicy, Result,
    Symbol, Value, list::force_list_to_vec, shape_match_value,
};

use crate::TopologyConnection;

use super::{TopologyAdapter, call_target_expr, resolve_target};

pub(super) fn core_adapters() -> Vec<Arc<dyn TopologyAdapter>> {
    vec![
        Arc::new(TopologyConnectionAdapter),
        Arc::new(ShapeAdapter),
        Arc::new(CodecAdapter),
        Arc::new(TableAdapter),
        Arc::new(ListAdapter),
        Arc::new(StreamAdapter),
        Arc::new(FabricAdapter),
        Arc::new(CallableAdapter),
    ]
}

struct TopologyConnectionAdapter;
struct ShapeAdapter;
struct CodecAdapter;
struct TableAdapter;
struct ListAdapter;
struct StreamAdapter;
struct FabricAdapter;
struct CallableAdapter;

impl TopologyAdapter for TopologyConnectionAdapter {
    fn name(&self) -> Symbol {
        adapter_symbol("topology")
    }

    fn accepts(&self, _cx: &mut Cx, target: &Value) -> bool {
        target
            .object()
            .downcast_ref::<TopologyConnection>()
            .is_some_and(|connection| connection.site_kind() == "topology")
    }

    fn call(
        &self,
        cx: &mut Cx,
        target: &Value,
        input: Expr,
        _role: Option<&Symbol>,
    ) -> Result<Expr> {
        call_eval_fabric(cx, target, input)
    }
}

impl TopologyAdapter for ShapeAdapter {
    fn name(&self) -> Symbol {
        adapter_symbol("shape")
    }

    fn accepts(&self, _cx: &mut Cx, target: &Value) -> bool {
        target.object().as_shape().is_some()
    }

    fn call(
        &self,
        cx: &mut Cx,
        target: &Value,
        input: Expr,
        _role: Option<&Symbol>,
    ) -> Result<Expr> {
        let shape = target.object().as_shape().ok_or(Error::TypeMismatch {
            expected: "shape",
            found: "non-shape",
        })?;
        let value = cx.factory().expr(input)?;
        let matched = shape.check_value(cx, value)?;
        if matched.captures.values().is_empty() && matched.captures.exprs().is_empty() {
            Ok(Expr::Bool(matched.accepted))
        } else {
            shape_match_value(cx, matched)?.object().as_expr(cx)
        }
    }
}

impl TopologyAdapter for CodecAdapter {
    fn name(&self) -> Symbol {
        adapter_symbol("codec")
    }

    fn accepts(&self, _cx: &mut Cx, target: &Value) -> bool {
        target.object().downcast_ref::<CodecRuntime>().is_some()
    }

    fn call(
        &self,
        cx: &mut Cx,
        target: &Value,
        input: Expr,
        _role: Option<&Symbol>,
    ) -> Result<Expr> {
        let codec = target
            .object()
            .downcast_ref::<CodecRuntime>()
            .ok_or(Error::TypeMismatch {
                expected: "codec",
                found: "non-codec",
            })?;
        match codec_op(&input) {
            CodecOp::Decode => decode_with_codec(
                cx,
                &codec.symbol,
                codec_input(&input)?,
                ReadPolicy::default(),
            ),
            CodecOp::Encode => {
                let output = encode_with_codec(
                    cx,
                    &codec.symbol,
                    codec_expr(&input),
                    EncodeOptions::default(),
                )?;
                Ok(match output {
                    Output::Text(text) => Expr::String(text),
                    Output::Bytes(bytes) => Expr::Bytes(bytes),
                })
            }
        }
    }
}

impl TopologyAdapter for TableAdapter {
    fn name(&self) -> Symbol {
        adapter_symbol("table")
    }

    fn accepts(&self, _cx: &mut Cx, target: &Value) -> bool {
        target.object().as_table_impl().is_some()
    }

    fn call(
        &self,
        cx: &mut Cx,
        target: &Value,
        input: Expr,
        _role: Option<&Symbol>,
    ) -> Result<Expr> {
        let table = target.object().as_table_impl().ok_or(Error::TypeMismatch {
            expected: "table",
            found: "non-table",
        })?;
        match table_op(&input) {
            TableOp::Get(key) => table.get(cx, key)?.object().as_expr(cx),
            TableOp::Put { key, value } => {
                let runtime_value = cx.factory().expr(value.clone())?;
                table.set(cx, key, runtime_value)?;
                Ok(value)
            }
            TableOp::Scan => table.as_table_expr(cx),
        }
    }
}

impl TopologyAdapter for ListAdapter {
    fn name(&self) -> Symbol {
        adapter_symbol("list")
    }

    fn accepts(&self, _cx: &mut Cx, target: &Value) -> bool {
        target.object().as_list().is_some()
    }

    fn call(
        &self,
        cx: &mut Cx,
        target: &Value,
        input: Expr,
        _role: Option<&Symbol>,
    ) -> Result<Expr> {
        let list = target.object().as_list().ok_or(Error::TypeMismatch {
            expected: "list",
            found: "non-list",
        })?;
        let values = force_list_to_vec(cx, list, "topology list adapter")?;
        match list_op(&input) {
            ListOp::Map(mapper) => {
                let mapper = resolve_target(cx, &mapper)?;
                let mut mapped = Vec::new();
                for value in values {
                    let expr = value.object().as_expr(cx)?;
                    mapped.push(call_target_expr(cx, mapper.clone(), expr)?);
                }
                Ok(Expr::List(mapped))
            }
            ListOp::Fold { folder, initial } => {
                let folder = resolve_target(cx, &folder)?;
                let mut acc = initial;
                for value in values {
                    let expr = value.object().as_expr(cx)?;
                    acc = call_target_expr(cx, folder.clone(), Expr::List(vec![acc, expr]))?;
                }
                Ok(acc)
            }
            ListOp::Items => values
                .into_iter()
                .map(|value| value.object().as_expr(cx))
                .collect::<Result<Vec<_>>>()
                .map(Expr::List),
        }
    }
}

impl TopologyAdapter for StreamAdapter {
    fn name(&self) -> Symbol {
        adapter_symbol("stream")
    }

    fn accepts(&self, _cx: &mut Cx, target: &Value) -> bool {
        target.object().as_stream().is_some()
    }

    fn call(
        &self,
        cx: &mut Cx,
        target: &Value,
        input: Expr,
        _role: Option<&Symbol>,
    ) -> Result<Expr> {
        let stream = target.object().as_stream().ok_or(Error::TypeMismatch {
            expected: "stream",
            found: "non-stream",
        })?;
        match op_name(&input).as_deref().unwrap_or("next") {
            "next" | "source" => match stream.next(cx)? {
                Some(value) => value.object().as_expr(cx),
                None => Ok(Expr::Nil),
            },
            "close" => {
                stream.close(cx)?;
                Ok(Expr::Nil)
            }
            other => Err(Error::Eval(format!(
                "topology stream adapter: unsupported op {other}"
            ))),
        }
    }
}

impl TopologyAdapter for FabricAdapter {
    fn name(&self) -> Symbol {
        adapter_symbol("fabric")
    }

    fn accepts(&self, _cx: &mut Cx, target: &Value) -> bool {
        target.object().as_eval_fabric().is_some()
    }

    fn call(
        &self,
        cx: &mut Cx,
        target: &Value,
        input: Expr,
        _role: Option<&Symbol>,
    ) -> Result<Expr> {
        call_eval_fabric(cx, target, input)
    }
}

impl TopologyAdapter for CallableAdapter {
    fn name(&self) -> Symbol {
        adapter_symbol("callable")
    }

    fn accepts(&self, _cx: &mut Cx, target: &Value) -> bool {
        target.object().as_callable().is_some()
    }

    fn call(
        &self,
        cx: &mut Cx,
        target: &Value,
        input: Expr,
        _role: Option<&Symbol>,
    ) -> Result<Expr> {
        let input = cx.factory().expr(input)?;
        let value = cx.call_value(target.clone(), Args::new(vec![input]))?;
        value.object().as_expr(cx)
    }
}

enum CodecOp {
    Decode,
    Encode,
}

enum TableOp {
    Get(Symbol),
    Put { key: Symbol, value: Expr },
    Scan,
}

enum ListOp {
    Items,
    Map(Expr),
    Fold { folder: Expr, initial: Expr },
}

fn adapter_symbol(name: &str) -> Symbol {
    Symbol::qualified("topology/adapter", name)
}

fn call_eval_fabric(cx: &mut Cx, target: &Value, input: Expr) -> Result<Expr> {
    let fabric = target
        .object()
        .as_eval_fabric()
        .ok_or(Error::TypeMismatch {
            expected: "eval fabric",
            found: "non-fabric",
        })?;
    let reply = fabric.realize(
        cx,
        EvalRequest {
            expr: input,
            result_shape: None,
            required_capabilities: Vec::new(),
            deadline: None,
            consistency: Consistency::LocalFirst,
            mode: EvalMode::Eval,
            answer_limit: None,
            stream_buffer: None,
            stream: false,
            trace: false,
        },
    )?;
    reply.value.object().as_expr(cx)
}

fn codec_op(input: &Expr) -> CodecOp {
    match op_name(input).as_deref() {
        Some("decode") => CodecOp::Decode,
        Some("encode") => CodecOp::Encode,
        _ => match input {
            Expr::String(_) | Expr::Bytes(_) => CodecOp::Decode,
            _ => CodecOp::Encode,
        },
    }
}

fn codec_input(input: &Expr) -> Result<Input> {
    match field(input, "text")
        .or_else(|| field(input, "bytes"))
        .unwrap_or(input)
    {
        Expr::String(text) => Ok(Input::Text(text.clone())),
        Expr::Bytes(bytes) => Ok(Input::Bytes(bytes.clone())),
        other => Err(Error::Eval(format!(
            "topology codec adapter decode expects text or bytes, got {other:?}"
        ))),
    }
}

fn codec_expr(input: &Expr) -> &Expr {
    field(input, "expr").unwrap_or(input)
}

fn table_op(input: &Expr) -> TableOp {
    let op = op_name(input);
    match op.as_deref() {
        Some("get") => field(input, "key")
            .and_then(symbolish)
            .map(TableOp::Get)
            .unwrap_or(TableOp::Scan),
        Some("put") | Some("set") => match (field(input, "key"), field(input, "value")) {
            (Some(key), Some(value)) => symbolish(key)
                .map(|key| TableOp::Put {
                    key,
                    value: value.clone(),
                })
                .unwrap_or(TableOp::Scan),
            _ => TableOp::Scan,
        },
        Some("scan") | Some("entries") => TableOp::Scan,
        _ => symbolish(input).map(TableOp::Get).unwrap_or(TableOp::Scan),
    }
}

fn list_op(input: &Expr) -> ListOp {
    match op_name(input).as_deref() {
        Some("map") => field(input, "target")
            .cloned()
            .map(ListOp::Map)
            .unwrap_or(ListOp::Items),
        Some("fold") => match field(input, "target").cloned() {
            Some(folder) => ListOp::Fold {
                folder,
                initial: field(input, "initial").cloned().unwrap_or(Expr::Nil),
            },
            None => ListOp::Items,
        },
        _ => ListOp::Items,
    }
}

fn op_name(input: &Expr) -> Option<String> {
    field(input, "op")
        .or_else(|| field(input, "mode"))
        .and_then(symbolish)
        .map(|symbol| symbol.name.to_string())
}

fn field<'a>(expr: &'a Expr, name: &str) -> Option<&'a Expr> {
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

fn symbolish(expr: &Expr) -> Option<Symbol> {
    match expr {
        Expr::Symbol(symbol) => Some(symbol.clone()),
        Expr::String(text) => Some(Symbol::new(text.clone())),
        _ => None,
    }
}

//! Read/construct integration for stream metadata.
//!
//! The kernel defines the class, object, and read-constructor contracts
//! (`Class`, `Object`, `ReadConstructor`); this module supplies the concrete
//! `stream/Metadata` class so stream metadata can be parsed and constructed
//! through the kernel read pipeline. [`StreamMetadataValue`] is the constructed
//! object, [`stream_metadata_class_symbol`] names the class, and
//! [`install_stream_core_classes`] registers it in a context's class registry.

use std::sync::Arc;

use sim_kernel::{
    Args, CORE_CLASS_CLASS_ID, CORE_FUNCTION_CLASS_ID, Callable, Class, ClassId, ClassRef, Cx,
    DefaultFactory, Expr, Factory, Linker, Object, ObjectCompat, ObjectEncode, ObjectEncoding,
    ReadConstructor, ReadConstructorRef, Result, ShapeRef, Symbol, TableRef, Value,
};

use crate::{
    BufferPolicy,
    metadata::{StreamDirection, StreamMedia, StreamMetadata},
};

const STREAM_METADATA_CLASS_ID: ClassId = ClassId(6200);

/// Runtime object wrapping [`StreamMetadata`] as a constructed `stream/Metadata`
/// value.
///
/// This is the object the read/construct path produces; it exposes the metadata
/// as an expression, table, and constructor encoding.
#[derive(Clone)]
pub struct StreamMetadataValue {
    metadata: StreamMetadata,
}

impl StreamMetadataValue {
    /// Wraps stream metadata as a constructed value.
    pub fn new(metadata: StreamMetadata) -> Self {
        Self { metadata }
    }

    /// Returns the wrapped stream metadata.
    pub fn metadata(&self) -> &StreamMetadata {
        &self.metadata
    }
}

impl Object for StreamMetadataValue {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok(format!("#<stream-metadata {}>", self.metadata.id()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for StreamMetadataValue {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        class_value_or_stub(cx)
    }

    fn as_expr(&self, _cx: &mut Cx) -> Result<Expr> {
        Ok(Expr::Call {
            operator: Box::new(Expr::Symbol(stream_metadata_class_symbol())),
            args: self.metadata.to_constructor_args(),
        })
    }

    fn as_table(&self, cx: &mut Cx) -> Result<Value> {
        let Expr::Map(entries) = self.metadata.table_expr() else {
            unreachable!("metadata table_expr returns a map");
        };
        cx.factory().table(
            entries
                .into_iter()
                .map(|(key, value)| match key {
                    Expr::Symbol(symbol) => Ok((symbol, cx.factory().expr(value)?)),
                    _ => unreachable!("metadata table keys are symbols"),
                })
                .collect::<Result<Vec<_>>>()?,
        )
    }

    fn as_object_encoder(&self) -> Option<&dyn ObjectEncode> {
        Some(self)
    }
}

impl ObjectEncode for StreamMetadataValue {
    fn object_encoding(&self, _cx: &mut Cx) -> Result<ObjectEncoding> {
        Ok(ObjectEncoding::Constructor {
            class: stream_metadata_class_symbol(),
            args: self.metadata.to_constructor_args(),
        })
    }
}

impl sim_citizen::Citizen for StreamMetadataValue {
    fn citizen_symbol() -> Symbol {
        stream_metadata_class_symbol()
    }

    fn citizen_version() -> u32 {
        0
    }

    fn citizen_arity() -> usize {
        5
    }

    fn citizen_fields() -> &'static [&'static str] {
        &["id", "media", "direction", "clock", "buffer"]
    }
}

/// Returns the qualified `stream/Metadata` class symbol.
pub fn stream_metadata_class_symbol() -> Symbol {
    Symbol::qualified("stream", "Metadata")
}

/// Registers the stream-core classes in a context's registry.
///
/// Installs the `stream/Metadata` class, which carries the read constructor used
/// by the kernel read pipeline. Idempotent: returns immediately if the class is
/// already registered.
pub fn install_stream_core_classes(cx: &mut Cx) -> Result<()> {
    if cx
        .registry()
        .class_by_symbol(&stream_metadata_class_symbol())
        .is_some()
    {
        return Ok(());
    }
    let class = cx.factory().opaque(Arc::new(StreamMetadataClass))?;
    cx.registry_mut()
        .register_class_value(stream_metadata_class_symbol(), class)?;
    Ok(())
}

#[derive(Clone)]
struct StreamMetadataClass;

impl Object for StreamMetadataClass {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok("#<class stream/Metadata>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for StreamMetadataClass {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        let symbol = Symbol::qualified("core", "Class");
        if let Some(value) = cx.registry().class_by_symbol(&symbol) {
            return Ok(value.clone());
        }
        cx.factory().class_stub(CORE_CLASS_CLASS_ID, symbol)
    }

    fn as_expr(&self, _cx: &mut Cx) -> Result<Expr> {
        Ok(Expr::Symbol(stream_metadata_class_symbol()))
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }

    fn as_class(&self) -> Option<&dyn Class> {
        Some(self)
    }
}

impl Callable for StreamMetadataClass {
    fn call(&self, cx: &mut Cx, args: Args) -> Result<Value> {
        construct_stream_metadata_value(cx, args.into_vec())
    }
}

impl Class for StreamMetadataClass {
    fn id(&self) -> ClassId {
        STREAM_METADATA_CLASS_ID
    }

    fn symbol(&self) -> Symbol {
        stream_metadata_class_symbol()
    }

    fn constructor_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        cx.factory().nil()
    }

    fn instance_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        cx.factory().nil()
    }

    fn read_constructor(&self, _cx: &mut Cx) -> Result<Option<ReadConstructorRef>> {
        Ok(Some(
            DefaultFactory.opaque(Arc::new(StreamMetadataReadConstructor))?,
        ))
    }

    fn members(&self, cx: &mut Cx) -> Result<TableRef> {
        cx.factory().table(Vec::new())
    }
}

#[derive(Clone)]
struct StreamMetadataReadConstructor;

impl Object for StreamMetadataReadConstructor {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok("#<read-constructor stream/Metadata>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for StreamMetadataReadConstructor {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        let symbol = Symbol::qualified("core", "Function");
        if let Some(value) = cx.registry().class_by_symbol(&symbol) {
            return Ok(value.clone());
        }
        cx.factory().class_stub(CORE_FUNCTION_CLASS_ID, symbol)
    }

    fn as_read_constructor(&self) -> Option<&dyn ReadConstructor> {
        Some(self)
    }
}

impl ReadConstructor for StreamMetadataReadConstructor {
    fn symbol(&self) -> Symbol {
        stream_metadata_class_symbol()
    }

    fn args_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        cx.factory().nil()
    }

    fn construct_read(&self, cx: &mut Cx, args: Vec<Value>) -> Result<Value> {
        construct_stream_metadata_value(cx, args)
    }
}

fn construct_stream_metadata_value(cx: &mut Cx, args: Vec<Value>) -> Result<Value> {
    let mut exprs = Vec::with_capacity(args.len());
    for value in args {
        exprs.push(value.object().as_expr(cx)?);
    }
    cx.factory().opaque(Arc::new(StreamMetadataValue::new(
        StreamMetadata::from_constructor_args(exprs)?,
    )))
}

fn register_stream_metadata_class(linker: &mut Linker<'_>) -> Result<()> {
    let class = DefaultFactory
        .opaque(Arc::new(StreamMetadataClass))
        .expect("stream metadata class should be boxable");
    linker.class_value(stream_metadata_class_symbol(), class)?;
    Ok(())
}

fn install_stream_metadata_citizen(linker: &mut Linker<'_>) -> Result<()> {
    register_stream_metadata_class(linker)
}

fn conformance_stream_metadata_citizen(cx: &mut Cx) -> Result<()> {
    let metadata = StreamMetadata::new(
        Symbol::qualified("stream-citizen", "metadata"),
        StreamMedia::Data,
        StreamDirection::Source,
        Symbol::qualified("stream-clock", "logical"),
        BufferPolicy::bounded(8)?,
    );
    let value = cx
        .factory()
        .opaque(Arc::new(StreamMetadataValue::new(metadata)))?;
    sim_citizen::check_value_fixture(cx, value)
}

sim_citizen::inventory::submit! {
    sim_citizen::CitizenInfo {
        symbol: "stream/Metadata",
        version: 0,
        crate_name: env!("CARGO_PKG_NAME"),
        arity: 5,
        install: install_stream_metadata_citizen,
        conformance: conformance_stream_metadata_citizen,
    }
}

fn class_value_or_stub(cx: &mut Cx) -> Result<Value> {
    if let Some(value) = cx
        .registry()
        .class_by_symbol(&stream_metadata_class_symbol())
    {
        return Ok(value.clone());
    }
    cx.factory()
        .class_stub(STREAM_METADATA_CLASS_ID, stream_metadata_class_symbol())
}

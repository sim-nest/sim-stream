//! Runtime class and read-construct support for worn event values.

use std::sync::Arc;

use sim_kernel::{
    AbiVersion, Args, CORE_CLASS_CLASS_ID, CORE_FUNCTION_CLASS_ID, Callable, Class, ClassId,
    ClassRef, Cx, DefaultFactory, Dependency, Export, Expr, Factory, Lib, LibManifest, LibTarget,
    Linker, Object, ObjectCompat, ObjectEncode, ObjectEncoding, ReadConstructor,
    ReadConstructorRef, Result, ShapeRef, Symbol, TableRef, Value, Version,
};
use sim_lib_stream_device::{
    DeviceSample, device_stream_base_manifest_symbol, install_device_stream_base,
};

use crate::{
    WornEvent, WornSensor,
    worn::{decode_known_worn_event, worn_constructor_args},
};

const WORN_EVENT_CLASS_ID: ClassId = ClassId(6202);

/// Runtime object wrapping a worn event sample expression.
#[derive(Clone)]
pub struct WornEventValue {
    sample: Expr,
}

impl WornEventValue {
    /// Validates and wraps a worn event expression.
    pub fn new(sample: Expr) -> Result<Self> {
        decode_known_worn_event(&sample)?;
        Ok(Self { sample })
    }

    /// Returns the wrapped sample expression.
    pub fn sample(&self) -> &Expr {
        &self.sample
    }

    /// Decodes the wrapped expression as a worn event.
    pub fn worn_event(&self) -> Result<WornEvent> {
        Ok(WornEvent::from_expr(&self.sample)?)
    }
}

impl Object for WornEventValue {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok("#<stream-worn-event>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for WornEventValue {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        class_value_or_stub(cx)
    }

    fn as_expr(&self, _cx: &mut Cx) -> Result<Expr> {
        Ok(Expr::Call {
            operator: Box::new(Expr::Symbol(worn_event_class_symbol())),
            args: worn_constructor_args(&self.sample)?,
        })
    }

    fn as_table(&self, cx: &mut Cx) -> Result<Value> {
        let Expr::Map(entries) = self.sample.clone() else {
            unreachable!("validated worn events are maps");
        };
        cx.factory().table(
            entries
                .into_iter()
                .map(|(key, value)| match key {
                    Expr::Symbol(symbol) => Ok((symbol, cx.factory().expr(value)?)),
                    _ => unreachable!("worn event map keys are symbols"),
                })
                .collect::<Result<Vec<_>>>()?,
        )
    }

    fn as_object_encoder(&self) -> Option<&dyn ObjectEncode> {
        Some(self)
    }
}

impl ObjectEncode for WornEventValue {
    fn object_encoding(&self, _cx: &mut Cx) -> Result<ObjectEncoding> {
        Ok(ObjectEncoding::Constructor {
            class: worn_event_class_symbol(),
            args: worn_constructor_args(&self.sample)?,
        })
    }
}

impl sim_citizen::Citizen for WornEventValue {
    fn citizen_symbol() -> Symbol {
        worn_event_class_symbol()
    }

    fn citizen_version() -> u32 {
        0
    }

    fn citizen_arity() -> usize {
        1
    }

    fn citizen_fields() -> &'static [&'static str] {
        &["sample"]
    }
}

/// Host-registered library that installs worn stream contracts.
pub struct WristStreamLib;

impl Lib for WristStreamLib {
    fn manifest(&self) -> LibManifest {
        LibManifest {
            id: wrist_stream_manifest_symbol(),
            version: Version(env!("CARGO_PKG_VERSION").to_owned()),
            abi: AbiVersion { major: 0, minor: 1 },
            target: LibTarget::HostRegistered,
            requires: vec![Dependency {
                id: device_stream_base_manifest_symbol(),
                minimum_version: None,
            }],
            capabilities: Vec::new(),
            exports: wrist_stream_exports(),
        }
    }

    fn load(&self, cx: &mut sim_kernel::LoadCx, linker: &mut Linker<'_>) -> Result<()> {
        register_worn_event_class(linker)?;
        linker.value(
            crate::worn_event_sample_kind_symbol(),
            cx.factory()
                .expr(Expr::Symbol(crate::worn_event_sample_kind_symbol()))?,
        )?;
        for sensor in WornSensor::all() {
            let symbol = sensor.symbol();
            linker.value(symbol.clone(), cx.factory().expr(Expr::Symbol(symbol))?)?;
        }
        Ok(())
    }
}

/// Installs the base device contracts and then the wrist stream library.
pub fn install_wrist_stream_lib(cx: &mut Cx) -> Result<()> {
    install_device_stream_base(cx)?;
    sim_lib_core::install_once(cx, &WristStreamLib).map(|_| ())
}

/// Export records advertised by [`WristStreamLib`].
pub fn wrist_stream_exports() -> Vec<Export> {
    let mut exports = vec![
        Export::Class {
            symbol: worn_event_class_symbol(),
            class_id: None,
        },
        Export::Value {
            symbol: crate::worn_event_sample_kind_symbol(),
        },
    ];
    exports.extend(WornSensor::all().iter().map(|sensor| Export::Value {
        symbol: sensor.symbol(),
    }));
    exports
}

/// Returns the manifest id for the wrist stream library.
pub fn wrist_stream_manifest_symbol() -> Symbol {
    Symbol::qualified("stream", "wrist")
}

/// Returns the read-construct class symbol for worn event values.
pub fn worn_event_class_symbol() -> Symbol {
    Symbol::qualified("stream", "WornEvent")
}

fn register_worn_event_class(linker: &mut Linker<'_>) -> Result<()> {
    let class = DefaultFactory
        .opaque(Arc::new(WornEventClass))
        .expect("worn event class should be boxable");
    linker.class_value(worn_event_class_symbol(), class)?;
    Ok(())
}

fn install_worn_event_citizen(linker: &mut Linker<'_>) -> Result<()> {
    register_worn_event_class(linker)
}

fn conformance_worn_event_citizen(cx: &mut Cx) -> Result<()> {
    let value = cx.factory().opaque(Arc::new(WornEventValue::new(
        WornEvent::heart_rate(0, 72)?.to_expr(),
    )?))?;
    sim_citizen::check_value_fixture(cx, value)
}

sim_citizen::inventory::submit! {
    sim_citizen::CitizenInfo {
        symbol: "stream/WornEvent",
        version: 0,
        crate_name: env!("CARGO_PKG_NAME"),
        arity: 1,
        install: install_worn_event_citizen,
        conformance: conformance_worn_event_citizen,
    }
}

#[derive(Clone)]
struct WornEventClass;

impl Object for WornEventClass {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok("#<class stream/WornEvent>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for WornEventClass {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        let symbol = Symbol::qualified("core", "Class");
        if let Some(value) = cx.registry().class_by_symbol(&symbol) {
            return Ok(value.clone());
        }
        cx.factory().class_stub(CORE_CLASS_CLASS_ID, symbol)
    }

    fn as_expr(&self, _cx: &mut Cx) -> Result<Expr> {
        Ok(Expr::Symbol(worn_event_class_symbol()))
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }

    fn as_class(&self) -> Option<&dyn Class> {
        Some(self)
    }
}

impl Callable for WornEventClass {
    fn call(&self, cx: &mut Cx, args: Args) -> Result<Value> {
        construct_worn_event_value(cx, args.into_vec())
    }
}

impl Class for WornEventClass {
    fn id(&self) -> ClassId {
        WORN_EVENT_CLASS_ID
    }

    fn symbol(&self) -> Symbol {
        worn_event_class_symbol()
    }

    fn constructor_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        cx.factory().nil()
    }

    fn instance_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        cx.factory().nil()
    }

    fn read_constructor(&self, _cx: &mut Cx) -> Result<Option<ReadConstructorRef>> {
        Ok(Some(
            DefaultFactory.opaque(Arc::new(WornEventReadConstructor))?,
        ))
    }

    fn members(&self, cx: &mut Cx) -> Result<TableRef> {
        cx.factory().table(Vec::new())
    }
}

#[derive(Clone)]
struct WornEventReadConstructor;

impl Object for WornEventReadConstructor {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok("#<read-constructor stream/WornEvent>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for WornEventReadConstructor {
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

impl ReadConstructor for WornEventReadConstructor {
    fn symbol(&self) -> Symbol {
        worn_event_class_symbol()
    }

    fn args_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        cx.factory().nil()
    }

    fn construct_read(&self, cx: &mut Cx, args: Vec<Value>) -> Result<Value> {
        construct_worn_event_value(cx, args)
    }
}

fn construct_worn_event_value(cx: &mut Cx, args: Vec<Value>) -> Result<Value> {
    let [sample] = args.as_slice() else {
        return Err(sim_kernel::Error::Eval(
            "stream/WornEvent expects one constructor argument".to_owned(),
        ));
    };
    let expr = sample.object().as_expr(cx)?;
    cx.factory().opaque(Arc::new(WornEventValue::new(expr)?))
}

fn class_value_or_stub(cx: &mut Cx) -> Result<Value> {
    if let Some(value) = cx.registry().class_by_symbol(&worn_event_class_symbol()) {
        return Ok(value.clone());
    }
    cx.factory()
        .class_stub(WORN_EVENT_CLASS_ID, worn_event_class_symbol())
}

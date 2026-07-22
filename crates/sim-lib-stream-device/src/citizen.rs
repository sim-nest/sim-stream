//! Runtime class and read-construct support for device sample values.

use std::sync::Arc;

use sim_kernel::{
    AbiVersion, Args, CORE_CLASS_CLASS_ID, CORE_FUNCTION_CLASS_ID, Callable, Class, ClassId,
    ClassRef, Cx, DefaultFactory, Export, Expr, Factory, Lib, LibManifest, LibTarget, Linker,
    Object, ObjectCompat, ObjectEncode, ObjectEncoding, ReadConstructor, ReadConstructorRef,
    Result, ShapeRef, Symbol, TableRef, Value, Version,
};

use crate::{
    DeviceCaps, DeviceSample,
    sample::{decode_known_sample, sample_constructor_args},
};

const DEVICE_SAMPLE_CLASS_ID: ClassId = ClassId(6201);

/// Runtime object wrapping a device sample expression.
///
/// The value validates the sample expression on construction and encodes as a
/// read constructor, allowing quoted device sample data to round-trip through
/// the kernel object surface.
#[derive(Clone)]
pub struct DeviceSampleValue {
    sample: Expr,
}

impl DeviceSampleValue {
    /// Validates and wraps a device sample expression.
    pub fn new(sample: Expr) -> Result<Self> {
        decode_known_sample(&sample)?;
        Ok(Self { sample })
    }

    /// Returns the wrapped sample expression.
    pub fn sample(&self) -> &Expr {
        &self.sample
    }

    /// Decodes the wrapped expression as the base device capabilities sample.
    pub fn device_caps(&self) -> Result<DeviceCaps> {
        Ok(DeviceCaps::from_expr(&self.sample)?)
    }
}

impl Object for DeviceSampleValue {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok("#<stream-device-sample>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for DeviceSampleValue {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        class_value_or_stub(cx)
    }

    fn as_expr(&self, _cx: &mut Cx) -> Result<Expr> {
        Ok(Expr::Call {
            operator: Box::new(Expr::Symbol(device_sample_class_symbol())),
            args: sample_constructor_args(&self.sample)?,
        })
    }

    fn as_table(&self, cx: &mut Cx) -> Result<Value> {
        let Expr::Map(entries) = self.sample.clone() else {
            unreachable!("validated device samples are maps");
        };
        cx.factory().table(
            entries
                .into_iter()
                .map(|(key, value)| match key {
                    Expr::Symbol(symbol) => Ok((symbol, cx.factory().expr(value)?)),
                    _ => unreachable!("device sample map keys are symbols"),
                })
                .collect::<Result<Vec<_>>>()?,
        )
    }

    fn as_object_encoder(&self) -> Option<&dyn ObjectEncode> {
        Some(self)
    }
}

impl ObjectEncode for DeviceSampleValue {
    fn object_encoding(&self, _cx: &mut Cx) -> Result<ObjectEncoding> {
        Ok(ObjectEncoding::Constructor {
            class: device_sample_class_symbol(),
            args: sample_constructor_args(&self.sample)?,
        })
    }
}

impl sim_citizen::Citizen for DeviceSampleValue {
    fn citizen_symbol() -> Symbol {
        device_sample_class_symbol()
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

/// Host-registered library that installs the device stream base class.
pub struct DeviceStreamBaseLib;

impl Lib for DeviceStreamBaseLib {
    fn manifest(&self) -> LibManifest {
        LibManifest {
            id: device_stream_base_manifest_symbol(),
            version: Version(env!("CARGO_PKG_VERSION").to_owned()),
            abi: AbiVersion { major: 0, minor: 1 },
            target: LibTarget::HostRegistered,
            requires: Vec::new(),
            capabilities: Vec::new(),
            exports: device_stream_base_exports(),
        }
    }

    fn load(&self, cx: &mut sim_kernel::LoadCx, linker: &mut Linker<'_>) -> Result<()> {
        register_device_sample_class(linker)?;
        linker.value(
            crate::device_caps_sample_kind_symbol(),
            cx.factory()
                .expr(Expr::Symbol(crate::device_caps_sample_kind_symbol()))?,
        )?;
        Ok(())
    }
}

/// Installs the device stream base into a context exactly once.
pub fn install_device_stream_base(cx: &mut Cx) -> Result<()> {
    sim_lib_core::install_once(cx, &DeviceStreamBaseLib).map(|_| ())
}

/// Export records advertised by [`DeviceStreamBaseLib`].
pub fn device_stream_base_exports() -> Vec<Export> {
    vec![
        Export::Class {
            symbol: device_sample_class_symbol(),
            class_id: Some(DEVICE_SAMPLE_CLASS_ID),
        },
        Export::Value {
            symbol: crate::device_caps_sample_kind_symbol(),
        },
    ]
}

/// Returns the manifest id for the device stream base library.
pub fn device_stream_base_manifest_symbol() -> Symbol {
    Symbol::qualified("stream", "device-base")
}

/// Returns the read-construct class symbol for device sample values.
pub fn device_sample_class_symbol() -> Symbol {
    Symbol::qualified("stream", "DeviceSample")
}

fn register_device_sample_class(linker: &mut Linker<'_>) -> Result<()> {
    let class = DefaultFactory
        .opaque(Arc::new(DeviceSampleClass))
        .expect("device sample class should be boxable");
    let id = linker.class_with_id(device_sample_class_symbol(), DEVICE_SAMPLE_CLASS_ID)?;
    linker.bind_class_value(id, class)?;
    Ok(())
}

fn install_device_sample_citizen(linker: &mut Linker<'_>) -> Result<()> {
    register_device_sample_class(linker)
}

fn conformance_device_sample_citizen(cx: &mut Cx) -> Result<()> {
    let value = cx.factory().opaque(Arc::new(DeviceSampleValue::new(
        DeviceCaps::demo(0).to_expr(),
    )?))?;
    sim_citizen::check_value_fixture(cx, value)
}

sim_citizen::inventory::submit! {
    sim_citizen::CitizenInfo {
        symbol: "stream/DeviceSample",
        version: 0,
        crate_name: env!("CARGO_PKG_NAME"),
        arity: 1,
        install: install_device_sample_citizen,
        conformance: conformance_device_sample_citizen,
    }
}

#[derive(Clone)]
struct DeviceSampleClass;

impl Object for DeviceSampleClass {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok("#<class stream/DeviceSample>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for DeviceSampleClass {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        let symbol = Symbol::qualified("core", "Class");
        if let Some(value) = cx.registry().class_by_symbol(&symbol) {
            return Ok(value.clone());
        }
        cx.factory().class_stub(CORE_CLASS_CLASS_ID, symbol)
    }

    fn as_expr(&self, _cx: &mut Cx) -> Result<Expr> {
        Ok(Expr::Symbol(device_sample_class_symbol()))
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }

    fn as_class(&self) -> Option<&dyn Class> {
        Some(self)
    }
}

impl Callable for DeviceSampleClass {
    fn call(&self, cx: &mut Cx, args: Args) -> Result<Value> {
        construct_device_sample_value(cx, args.into_vec())
    }
}

impl Class for DeviceSampleClass {
    fn id(&self) -> ClassId {
        DEVICE_SAMPLE_CLASS_ID
    }

    fn symbol(&self) -> Symbol {
        device_sample_class_symbol()
    }

    fn constructor_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        cx.factory().nil()
    }

    fn instance_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        cx.factory().nil()
    }

    fn read_constructor(&self, _cx: &mut Cx) -> Result<Option<ReadConstructorRef>> {
        Ok(Some(
            DefaultFactory.opaque(Arc::new(DeviceSampleReadConstructor))?,
        ))
    }

    fn members(&self, cx: &mut Cx) -> Result<TableRef> {
        cx.factory().table(Vec::new())
    }
}

#[derive(Clone)]
struct DeviceSampleReadConstructor;

impl Object for DeviceSampleReadConstructor {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok("#<read-constructor stream/DeviceSample>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for DeviceSampleReadConstructor {
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

impl ReadConstructor for DeviceSampleReadConstructor {
    fn symbol(&self) -> Symbol {
        device_sample_class_symbol()
    }

    fn args_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        cx.factory().nil()
    }

    fn construct_read(&self, cx: &mut Cx, args: Vec<Value>) -> Result<Value> {
        construct_device_sample_value(cx, args)
    }
}

fn construct_device_sample_value(cx: &mut Cx, args: Vec<Value>) -> Result<Value> {
    let [sample] = args.as_slice() else {
        return Err(sim_kernel::Error::Eval(
            "stream/DeviceSample expects one constructor argument".to_owned(),
        ));
    };
    let expr = sample.object().as_expr(cx)?;
    cx.factory().opaque(Arc::new(DeviceSampleValue::new(expr)?))
}

fn class_value_or_stub(cx: &mut Cx) -> Result<Value> {
    if let Some(value) = cx.registry().class_by_symbol(&device_sample_class_symbol()) {
        return Ok(value.clone());
    }
    cx.factory()
        .class_stub(DEVICE_SAMPLE_CLASS_ID, device_sample_class_symbol())
}

//! Runtime class and read-construct support for XR sample values.

use std::sync::Arc;

use sim_kernel::{
    AbiVersion, Args, CORE_CLASS_CLASS_ID, CORE_FUNCTION_CLASS_ID, Callable, Class, ClassId,
    ClassRef, Cx, DefaultFactory, Dependency, Export, Expr, Factory, Lib, LibManifest, LibTarget,
    Linker, Object, ObjectCompat, ObjectEncode, ObjectEncoding, ReadConstructor,
    ReadConstructorRef, Result, ShapeRef, Symbol, TableRef, Value, Version,
};
use sim_lib_stream_device::{
    DeviceSample, ModeledSource, device_stream_base_manifest_symbol, install_device_stream_base,
};

use crate::{
    XrCameraFrameRef, XrHandSample, XrMicChunkRef, XrPoseSample, XrTapSample, XrTrackingStatus,
    camera::decode_known_camera_frame, hand::decode_known_hand, mic::decode_known_mic_chunk,
    pose::decode_known_pose, tap::decode_known_tap,
};

const XR_SAMPLE_CLASS_ID: ClassId = ClassId(6203);

/// Runtime object wrapping an XR sample expression.
#[derive(Clone)]
pub struct XrSampleValue {
    sample: Expr,
}

impl XrSampleValue {
    /// Validates and wraps an XR sample expression.
    pub fn new(sample: Expr) -> Result<Self> {
        decode_known_xr_sample(&sample)?;
        Ok(Self { sample })
    }

    /// Returns the wrapped sample expression.
    pub fn sample(&self) -> &Expr {
        &self.sample
    }

    /// Decodes the wrapped expression as an XR pose sample.
    pub fn pose(&self) -> Result<XrPoseSample> {
        Ok(XrPoseSample::from_expr(&self.sample)?)
    }

    /// Decodes the wrapped expression as an XR camera frame reference.
    pub fn camera_frame(&self) -> Result<XrCameraFrameRef> {
        Ok(XrCameraFrameRef::from_expr(&self.sample)?)
    }

    /// Decodes the wrapped expression as an XR hand sample.
    pub fn hand(&self) -> Result<XrHandSample> {
        Ok(XrHandSample::from_expr(&self.sample)?)
    }

    /// Decodes the wrapped expression as an XR tap sample.
    pub fn tap(&self) -> Result<XrTapSample> {
        Ok(XrTapSample::from_expr(&self.sample)?)
    }

    /// Decodes the wrapped expression as an XR microphone chunk reference.
    pub fn mic_chunk(&self) -> Result<XrMicChunkRef> {
        Ok(XrMicChunkRef::from_expr(&self.sample)?)
    }
}

impl Object for XrSampleValue {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok("#<stream-xr-sample>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for XrSampleValue {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        class_value_or_stub(cx)
    }

    fn as_expr(&self, _cx: &mut Cx) -> Result<Expr> {
        Ok(Expr::Call {
            operator: Box::new(Expr::Symbol(xr_sample_class_symbol())),
            args: xr_constructor_args(&self.sample)?,
        })
    }

    fn as_table(&self, cx: &mut Cx) -> Result<Value> {
        let Expr::Map(entries) = self.sample.clone() else {
            unreachable!("validated XR samples are maps");
        };
        cx.factory().table(
            entries
                .into_iter()
                .map(|(key, value)| match key {
                    Expr::Symbol(symbol) => Ok((symbol, cx.factory().expr(value)?)),
                    _ => unreachable!("XR sample map keys are symbols"),
                })
                .collect::<Result<Vec<_>>>()?,
        )
    }

    fn as_object_encoder(&self) -> Option<&dyn ObjectEncode> {
        Some(self)
    }
}

impl ObjectEncode for XrSampleValue {
    fn object_encoding(&self, _cx: &mut Cx) -> Result<ObjectEncoding> {
        Ok(ObjectEncoding::Constructor {
            class: xr_sample_class_symbol(),
            args: xr_constructor_args(&self.sample)?,
        })
    }
}

impl sim_citizen::Citizen for XrSampleValue {
    fn citizen_symbol() -> Symbol {
        xr_sample_class_symbol()
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

/// Host-registered library that installs XR stream contracts.
pub struct XrStreamLib;

impl Lib for XrStreamLib {
    fn manifest(&self) -> LibManifest {
        LibManifest {
            id: xr_stream_manifest_symbol(),
            version: Version(env!("CARGO_PKG_VERSION").to_owned()),
            abi: AbiVersion { major: 0, minor: 1 },
            target: LibTarget::HostRegistered,
            requires: vec![Dependency {
                id: device_stream_base_manifest_symbol(),
                minimum_version: None,
            }],
            capabilities: Vec::new(),
            exports: xr_stream_exports(),
        }
    }

    fn load(&self, cx: &mut sim_kernel::LoadCx, linker: &mut Linker<'_>) -> Result<()> {
        register_xr_sample_class(linker)?;
        for symbol in xr_value_symbols() {
            linker.value(symbol.clone(), cx.factory().expr(Expr::Symbol(symbol))?)?;
        }
        Ok(())
    }
}

/// Installs the base device contracts and then the XR stream library.
pub fn install_xr_stream_lib(cx: &mut Cx) -> Result<()> {
    install_device_stream_base(cx)?;
    sim_lib_core::install_once(cx, &XrStreamLib).map(|_| ())
}

/// Export records advertised by [`XrStreamLib`].
pub fn xr_stream_exports() -> Vec<Export> {
    let mut exports = vec![Export::Class {
        symbol: xr_sample_class_symbol(),
        class_id: None,
    }];
    exports.extend(
        xr_value_symbols()
            .into_iter()
            .map(|symbol| Export::Value { symbol }),
    );
    exports
}

/// Returns the manifest id for the XR stream library.
pub fn xr_stream_manifest_symbol() -> Symbol {
    Symbol::qualified("stream", "xr")
}

/// Returns the read-construct class symbol for XR sample values.
pub fn xr_sample_class_symbol() -> Symbol {
    Symbol::qualified("stream", "XrSample")
}

pub(crate) fn decode_known_xr_sample(expr: &Expr) -> sim_lib_stream_device::DeviceSampleResult<()> {
    let entries = crate::wire::map_entries(expr, "XR sample map")?;
    let kind = crate::wire::symbol_field(entries, "sample", "XR sample")?;
    if kind == &crate::xr_pose_sample_kind_symbol() {
        return decode_known_pose(expr);
    }
    if kind == &crate::xr_camera_frame_sample_kind_symbol() {
        return decode_known_camera_frame(expr);
    }
    if kind == &crate::xr_hand_sample_kind_symbol() {
        return decode_known_hand(expr);
    }
    if kind == &crate::xr_tap_sample_kind_symbol() {
        return decode_known_tap(expr);
    }
    if kind == &crate::xr_mic_chunk_sample_kind_symbol() {
        return decode_known_mic_chunk(expr);
    }
    Err(sim_lib_stream_device::DeviceSampleError::new(format!(
        "unknown XR sample kind {kind}"
    )))
}

fn xr_constructor_args(expr: &Expr) -> sim_lib_stream_device::DeviceSampleResult<Vec<Expr>> {
    decode_known_xr_sample(expr)?;
    Ok(vec![expr.clone()])
}

fn xr_value_symbols() -> Vec<Symbol> {
    let mut symbols = vec![
        crate::xr_pose_sample_kind_symbol(),
        crate::xr_camera_frame_sample_kind_symbol(),
        crate::xr_hand_sample_kind_symbol(),
        crate::xr_tap_sample_kind_symbol(),
        crate::xr_mic_chunk_sample_kind_symbol(),
    ];
    symbols.extend(XrTrackingStatus::all().iter().map(|status| status.symbol()));
    symbols
}

fn register_xr_sample_class(linker: &mut Linker<'_>) -> Result<()> {
    let class = DefaultFactory
        .opaque(Arc::new(XrSampleClass))
        .expect("XR sample class should be boxable");
    linker.class_value(xr_sample_class_symbol(), class)?;
    Ok(())
}

fn install_xr_sample_citizen(linker: &mut Linker<'_>) -> Result<()> {
    register_xr_sample_class(linker)
}

fn conformance_xr_sample_citizen(cx: &mut Cx) -> Result<()> {
    let value = cx.factory().opaque(Arc::new(XrSampleValue::new(
        crate::ModeledViturePoseSource.at(0).to_expr(),
    )?))?;
    sim_citizen::check_value_fixture(cx, value)
}

sim_citizen::inventory::submit! {
    sim_citizen::CitizenInfo {
        symbol: "stream/XrSample",
        version: 0,
        crate_name: env!("CARGO_PKG_NAME"),
        arity: 1,
        install: install_xr_sample_citizen,
        conformance: conformance_xr_sample_citizen,
    }
}

#[derive(Clone)]
struct XrSampleClass;

impl Object for XrSampleClass {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok("#<class stream/XrSample>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for XrSampleClass {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        let symbol = Symbol::qualified("core", "Class");
        if let Some(value) = cx.registry().class_by_symbol(&symbol) {
            return Ok(value.clone());
        }
        cx.factory().class_stub(CORE_CLASS_CLASS_ID, symbol)
    }

    fn as_expr(&self, _cx: &mut Cx) -> Result<Expr> {
        Ok(Expr::Symbol(xr_sample_class_symbol()))
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }

    fn as_class(&self) -> Option<&dyn Class> {
        Some(self)
    }
}

impl Callable for XrSampleClass {
    fn call(&self, cx: &mut Cx, args: Args) -> Result<Value> {
        construct_xr_sample_value(cx, args.into_vec())
    }
}

impl Class for XrSampleClass {
    fn id(&self) -> ClassId {
        XR_SAMPLE_CLASS_ID
    }

    fn symbol(&self) -> Symbol {
        xr_sample_class_symbol()
    }

    fn constructor_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        cx.factory().nil()
    }

    fn instance_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        cx.factory().nil()
    }

    fn read_constructor(&self, _cx: &mut Cx) -> Result<Option<ReadConstructorRef>> {
        Ok(Some(
            DefaultFactory.opaque(Arc::new(XrSampleReadConstructor))?,
        ))
    }

    fn members(&self, cx: &mut Cx) -> Result<TableRef> {
        cx.factory().table(Vec::new())
    }
}

#[derive(Clone)]
struct XrSampleReadConstructor;

impl Object for XrSampleReadConstructor {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok("#<read-constructor stream/XrSample>".to_owned())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for XrSampleReadConstructor {
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

impl ReadConstructor for XrSampleReadConstructor {
    fn symbol(&self) -> Symbol {
        xr_sample_class_symbol()
    }

    fn args_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        cx.factory().nil()
    }

    fn construct_read(&self, cx: &mut Cx, args: Vec<Value>) -> Result<Value> {
        construct_xr_sample_value(cx, args)
    }
}

fn construct_xr_sample_value(cx: &mut Cx, args: Vec<Value>) -> Result<Value> {
    let [sample] = args.as_slice() else {
        return Err(sim_kernel::Error::Eval(
            "stream/XrSample expects one constructor argument".to_owned(),
        ));
    };
    let expr = sample.object().as_expr(cx)?;
    cx.factory().opaque(Arc::new(XrSampleValue::new(expr)?))
}

fn class_value_or_stub(cx: &mut Cx) -> Result<Value> {
    if let Some(value) = cx.registry().class_by_symbol(&xr_sample_class_symbol()) {
        return Ok(value.clone());
    }
    cx.factory()
        .class_stub(XR_SAMPLE_CLASS_ID, xr_sample_class_symbol())
}

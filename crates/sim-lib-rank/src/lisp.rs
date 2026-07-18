//! Installs the rank library into the runtime.
//!
//! Defines [`RankLib`] and its exports -- the `rank`, `unrank`,
//! `rank/enumerate`, and `rank/mutate` callables plus the rank space, node, and
//! coordinate classes -- and the helpers that register them on a [`Cx`].

use std::sync::Arc;

use sim_kernel::{
    AbiVersion, Args, CORE_FUNCTION_CLASS_ID, Callable, ClassRef, Cx, DefaultFactory, Error,
    Export, Factory, Lib, LibManifest, LibTarget, Linker, LoadCx, Object, ObjectCompat, Result,
    Step, Symbol, Value, Version, invoke_op,
};

use crate::{
    GenericNodeNeighborhood, Nat, RankGrammar, RankLimits, RankNeighborhood, RankNode, RankSpace,
    RankSpaceCardMetadata,
    cap::{
        rank_enumerate_capability, rank_heavy_capability, rank_neighbor_capability,
        rank_read_capability,
    },
    claims::publish_space_card_claims,
    cookbook_runtime::{install_rank_cookbook_functions, rank_cookbook_exports},
    lisp_class::{RankClass, RankClassKind, class_value_or_stub},
    nat::intern_ordinal,
    read_construct::{nat_from_value, symbol_from_value, u64_from_value, usize_from_value},
    space::{coordinate_from_value, rank_coordinate_value, rank_node_from_value, rank_node_value},
};

pub use crate::lisp_class::{
    rank_coordinate_class_symbol, rank_node_class_symbol, rank_space_class_symbol,
};
pub(crate) use crate::lisp_class::{
    rank_coordinate_class_value_or_stub, rank_node_class_value_or_stub,
    rank_space_class_value_or_stub,
};
pub use crate::read_construct::{rank_coordinate_constructor_args, rank_node_constructor_args};

/// The rank library: registers rank functions and classes on load.
pub struct RankLib;

impl Lib for RankLib {
    fn manifest(&self) -> LibManifest {
        LibManifest {
            id: manifest_name(),
            version: Version(env!("CARGO_PKG_VERSION").to_owned()),
            abi: AbiVersion { major: 0, minor: 1 },
            target: LibTarget::HostRegistered,
            requires: Vec::new(),
            capabilities: Vec::new(),
            exports: rank_exports(),
        }
    }

    fn load(&self, cx: &mut LoadCx, linker: &mut Linker<'_>) -> Result<()> {
        for kind in [
            RankFunctionKind::Rank,
            RankFunctionKind::Unrank,
            RankFunctionKind::Enumerate,
            RankFunctionKind::Mutate,
        ] {
            let function = RankFunction { kind };
            linker.function_value(function.symbol(), cx.factory().opaque(Arc::new(function))?)?;
        }
        for kind in [
            RankClassKind::Space,
            RankClassKind::Node,
            RankClassKind::Coordinate,
        ] {
            register_rank_class(linker, kind)?;
        }
        install_rank_cookbook_functions(cx, linker)?;
        Ok(())
    }
}

/// Installs [`RankLib`] into `cx` exactly once.
pub fn install_rank_lib(cx: &mut Cx) -> Result<()> {
    sim_lib_core::install_once(cx, &RankLib)?;
    Ok(())
}

fn register_rank_class(linker: &mut Linker<'_>, kind: RankClassKind) -> Result<()> {
    linker.class_value(
        kind.symbol(),
        DefaultFactory
            .opaque(Arc::new(RankClass { kind }))
            .expect("rank class should be boxable"),
    )?;
    Ok(())
}

fn install_rank_space_citizen(linker: &mut Linker<'_>) -> Result<()> {
    register_rank_class(linker, RankClassKind::Space)
}

fn install_rank_node_citizen(linker: &mut Linker<'_>) -> Result<()> {
    register_rank_class(linker, RankClassKind::Node)
}

fn install_rank_coordinate_citizen(linker: &mut Linker<'_>) -> Result<()> {
    register_rank_class(linker, RankClassKind::Coordinate)
}

fn conformance_rank_space_citizen(cx: &mut Cx) -> Result<()> {
    let space = RankSpace::group(
        Symbol::qualified("rank-citizen", "unit-space"),
        RankGrammar::Unit,
    );
    let value = space.value(cx)?;
    if cx.registry().value_by_symbol(space.symbol()).is_none() {
        cx.registry_mut()
            .register_value(space.symbol().clone(), value.clone())?;
    }
    sim_citizen::check_value_fixture(cx, value)
}

fn conformance_rank_node_citizen(cx: &mut Cx) -> Result<()> {
    let value = rank_node_value(cx, RankNode::Nat(Nat::from(7_usize)))?;
    sim_citizen::check_value_fixture(cx, value)
}

fn conformance_rank_coordinate_citizen(cx: &mut Cx) -> Result<()> {
    let ordinal = intern_ordinal(cx, &Nat::from(3_usize))?;
    let value = rank_coordinate_value(
        cx,
        sim_kernel::Coordinate {
            space: Symbol::qualified("rank-citizen", "unit-space"),
            ordinal,
        },
    )?;
    sim_citizen::check_value_fixture(cx, value)
}

sim_citizen::inventory::submit! {
    sim_citizen::CitizenInfo {
        symbol: "rank/Space",
        version: 0,
        crate_name: env!("CARGO_PKG_NAME"),
        arity: 1,
        install: install_rank_space_citizen,
        conformance: conformance_rank_space_citizen,
    }
}

sim_citizen::inventory::submit! {
    sim_citizen::CitizenInfo {
        symbol: "rank/Node",
        version: 0,
        crate_name: env!("CARGO_PKG_NAME"),
        arity: 1,
        install: install_rank_node_citizen,
        conformance: conformance_rank_node_citizen,
    }
}

sim_citizen::inventory::submit! {
    sim_citizen::CitizenInfo {
        symbol: "rank/Coord",
        version: 0,
        crate_name: env!("CARGO_PKG_NAME"),
        arity: 2,
        install: install_rank_coordinate_citizen,
        conformance: conformance_rank_coordinate_citizen,
    }
}

/// Installs the rank library and registers `space` as a named runtime value.
///
/// Publishes the space's card claims with `metadata` and optional `help`, then
/// registers the space value under its symbol if not already present.
pub fn install_rank_space(
    cx: &mut Cx,
    space: RankSpace,
    metadata: RankSpaceCardMetadata,
    help: Option<&str>,
) -> Result<()> {
    install_rank_lib(cx)?;
    publish_space_card_claims(cx, &space, &metadata, help)?;
    if cx.registry().value_by_symbol(space.symbol()).is_none() {
        let value = space.value(cx)?;
        cx.registry_mut()
            .register_value(space.symbol().clone(), value)?;
    }
    Ok(())
}

/// Returns the library's exports: the rank functions and the rank classes.
pub fn rank_exports() -> Vec<Export> {
    [
        rank_fn_symbol(),
        unrank_fn_symbol(),
        rank_enumerate_symbol(),
        rank_mutate_symbol(),
    ]
    .into_iter()
    .map(|symbol| Export::Function {
        symbol,
        function_id: None,
    })
    .chain(rank_cookbook_exports())
    .chain(
        [
            rank_space_class_symbol(),
            rank_node_class_symbol(),
            rank_coordinate_class_symbol(),
        ]
        .into_iter()
        .map(|symbol| Export::Class {
            symbol,
            class_id: None,
        }),
    )
    .collect()
}

/// Returns the manifest symbol `sim/rank` naming this library.
pub fn manifest_name() -> Symbol {
    Symbol::qualified("sim", "rank")
}

/// Returns the symbol of the `rank` function (value -> coordinate).
pub fn rank_fn_symbol() -> Symbol {
    Symbol::new("rank")
}

/// Returns the symbol of the `unrank` function (coordinate -> value).
pub fn unrank_fn_symbol() -> Symbol {
    Symbol::new("unrank")
}

/// Returns the symbol of the `rank/enumerate` function.
pub fn rank_enumerate_symbol() -> Symbol {
    Symbol::qualified("rank", "enumerate")
}

/// Returns the symbol of the `rank/mutate` function.
pub fn rank_mutate_symbol() -> Symbol {
    Symbol::qualified("rank", "mutate")
}

#[derive(Clone)]
struct RankFunction {
    kind: RankFunctionKind,
}

#[derive(Clone, Copy)]
enum RankFunctionKind {
    Rank,
    Unrank,
    Enumerate,
    Mutate,
}

impl RankFunction {
    fn symbol(&self) -> Symbol {
        match self.kind {
            RankFunctionKind::Rank => rank_fn_symbol(),
            RankFunctionKind::Unrank => unrank_fn_symbol(),
            RankFunctionKind::Enumerate => rank_enumerate_symbol(),
            RankFunctionKind::Mutate => rank_mutate_symbol(),
        }
    }
}

impl Object for RankFunction {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok(format!("#<function {}>", self.symbol()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for RankFunction {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        class_value_or_stub(
            cx,
            CORE_FUNCTION_CLASS_ID,
            Symbol::qualified("core", "Function"),
        )
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }
}

impl Callable for RankFunction {
    fn call(&self, cx: &mut Cx, args: Args) -> Result<Value> {
        match self.kind {
            RankFunctionKind::Rank => call_rank(cx, args.into_vec()),
            RankFunctionKind::Unrank => call_unrank(cx, args.into_vec()),
            RankFunctionKind::Enumerate => call_enumerate(cx, args.into_vec()),
            RankFunctionKind::Mutate => call_mutate(cx, args.into_vec()),
        }
    }
}

fn call_rank(cx: &mut Cx, args: Vec<Value>) -> Result<Value> {
    cx.require(&rank_read_capability())?;
    let [space_arg, value_arg] = args.as_slice() else {
        return Err(arity_error("rank", "space value"));
    };
    let space = space_from_value(cx, space_arg)?;
    let target = space.value(cx)?;
    match invoke_op(
        cx,
        target,
        &sim_kernel::rank::rank_rank_op_key(),
        value_arg.clone(),
    )? {
        Step::Value(value) => Ok(value),
        _ => Err(Error::Eval("rank op did not return a value".to_owned())),
    }
}

fn call_unrank(cx: &mut Cx, args: Vec<Value>) -> Result<Value> {
    cx.require(&rank_read_capability())?;
    let [space_arg, ordinal_arg] = args.as_slice() else {
        return Err(arity_error("unrank", "space ordinal-or-coordinate"));
    };
    let space = space_from_value(cx, space_arg)?;
    let input = coordinate_input(cx, &space, ordinal_arg)?;
    let target = space.value(cx)?;
    match invoke_op(cx, target, &sim_kernel::rank::rank_unrank_op_key(), input)? {
        Step::Value(value) => Ok(value),
        _ => Err(Error::Eval("unrank op did not return a value".to_owned())),
    }
}

fn call_enumerate(cx: &mut Cx, args: Vec<Value>) -> Result<Value> {
    cx.require(&rank_enumerate_capability())?;
    let Some(space_arg) = args.first() else {
        return Err(arity_error("rank/enumerate", "space :limit n"));
    };
    let space = space_from_value(cx, space_arg)?;
    let limit = enumerate_limit(cx, &args[1..])?;
    let mut limits = RankLimits::default();
    authorize_enumeration_limit(cx, limit, &mut limits)?;
    let mut values = Vec::new();
    values
        .try_reserve(limit)
        .map_err(|_| Error::Eval("rank/enumerate limit exceeds allocation budget".to_owned()))?;
    for index in 0..limit {
        limits.consume(1, "rank.enumerate").map_err(Error::from)?;
        let ordinal = Nat::from(index);
        if !space.codec().r_ok(&ordinal) {
            break;
        }
        limits
            .check_nat(&ordinal, "rank.enumerate")
            .map_err(Error::from)?;
        values.push(rank_node_value(
            cx,
            space.codec().unrank_node(&ordinal).map_err(Error::from)?,
        )?);
    }
    cx.factory().list(values)
}

fn call_mutate(cx: &mut Cx, args: Vec<Value>) -> Result<Value> {
    cx.require(&rank_neighbor_capability())?;
    if args.len() < 2 {
        return Err(arity_error("rank/mutate", "space value [:seed n]"));
    }
    let space = space_from_value(cx, &args[0])?;
    let seed = seed_arg(cx, &args[2..])?;
    let node = rank_node_from_value(&args[1])?;
    let ordinal = space.codec().rank_node(&node).map_err(Error::from)?;
    let metric = GenericNodeNeighborhood::default();
    let mut limits = crate::RankLimits::default();
    let mutated = metric
        .mutate(space.codec().as_ref(), &ordinal, seed, &mut limits)
        .map_err(Error::from)?;
    rank_node_value(
        cx,
        space.codec().unrank_node(&mutated).map_err(Error::from)?,
    )
}

fn coordinate_input(cx: &mut Cx, space: &RankSpace, value: &Value) -> Result<Value> {
    if coordinate_from_value(value).is_ok() {
        return Ok(value.clone());
    }
    let ordinal = nat_from_value(cx, value)?;
    let coordinate = sim_kernel::Coordinate {
        space: space.symbol().clone(),
        ordinal: intern_ordinal(cx, &ordinal)?,
    };
    rank_coordinate_value(cx, coordinate)
}

fn space_from_value(cx: &mut Cx, value: &Value) -> Result<RankSpace> {
    if let Some(space) = value.object().downcast_ref::<RankSpace>() {
        return Ok(space.clone());
    }
    let symbol = symbol_from_value(cx, value, "rank space")?;
    let resolved = cx.resolve_value(&symbol)?;
    resolved
        .object()
        .downcast_ref::<RankSpace>()
        .cloned()
        .ok_or_else(|| Error::Eval(format!("symbol {symbol} is not a rank space")))
}

fn enumerate_limit(cx: &mut Cx, args: &[Value]) -> Result<usize> {
    let mut limit = None;
    let mut index = 0;
    while index < args.len() {
        if is_keyword(cx, &args[index], "limit")? {
            index += 1;
            let Some(value) = args.get(index) else {
                return Err(arity_error("rank/enumerate", ":limit n"));
            };
            limit = Some(usize_from_value(cx, value, "rank/enumerate limit")?);
        } else if is_keyword(cx, &args[index], "order")? || is_keyword(cx, &args[index], "context")?
        {
            index += 1;
            if args.get(index).is_none() {
                return Err(arity_error("rank/enumerate", "keyword value"));
            }
        } else if limit.is_none() {
            limit = Some(usize_from_value(cx, &args[index], "rank/enumerate limit")?);
        } else {
            return Err(Error::Eval("rank/enumerate requires :limit".to_owned()));
        }
        index += 1;
    }
    limit.ok_or_else(|| Error::Eval("rank/enumerate requires :limit".to_owned()))
}

fn authorize_enumeration_limit(cx: &mut Cx, limit: usize, limits: &mut RankLimits) -> Result<()> {
    if limit > RankLimits::ORDINARY_ENUMERATION_LIMIT {
        cx.require(&rank_heavy_capability())?;
    }
    limits
        .check_count(limit, "rank.enumerate.limit")
        .map_err(Error::from)
}

fn seed_arg(cx: &mut Cx, args: &[Value]) -> Result<u64> {
    let mut seed = 0;
    let mut index = 0;
    while index < args.len() {
        if is_keyword(cx, &args[index], "seed")? {
            index += 1;
            let Some(value) = args.get(index) else {
                return Err(arity_error("rank/mutate", ":seed n"));
            };
            seed = u64_from_value(cx, value, "rank/mutate seed")?;
        } else {
            return Err(Error::Eval("rank/mutate only accepts :seed".to_owned()));
        }
        index += 1;
    }
    Ok(seed)
}

fn is_keyword(cx: &mut Cx, value: &Value, expected: &str) -> Result<bool> {
    let sim_kernel::Expr::Symbol(symbol) = value.object().as_expr(cx)? else {
        return Ok(false);
    };
    let name = symbol.name.as_ref();
    Ok(name == expected || name.strip_prefix(':') == Some(expected))
}

fn arity_error(function: &'static str, expected: &'static str) -> Error {
    Error::Eval(format!("{function} expects {expected}"))
}

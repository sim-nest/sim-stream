//! Runtime classes for rank spaces, nodes, and coordinates.
//!
//! Defines the class and read-constructor objects that let the runtime
//! construct and read rank space, node, and coordinate values.

use std::sync::Arc;

use sim_kernel::{
    Args, CORE_CLASS_CLASS_ID, CORE_FUNCTION_CLASS_ID, Callable, Class, ClassId, ClassRef, Cx,
    DefaultFactory, Error, Expr, Factory, MatchScore, Object, ObjectCompat, ReadConstructor,
    ReadConstructorRef, Result, Shape, ShapeDoc, ShapeMatch, ShapeRef, Symbol, TableRef, Value,
};

use crate::{
    Nat,
    nat::intern_ordinal,
    read_construct::{nat_from_value, rank_node_from_expr, symbol_from_value, value_exprs},
    space::{
        RankSpace, coordinate_from_value, rank_coordinate_value, rank_node_from_value,
        rank_node_value,
    },
};

const RANK_SPACE_CLASS_ID: ClassId = ClassId(6300);
const RANK_NODE_CLASS_ID: ClassId = ClassId(6301);
const RANK_COORDINATE_CLASS_ID: ClassId = ClassId(6302);

#[derive(Clone, Copy)]
pub(crate) enum RankClassKind {
    Space,
    Node,
    Coordinate,
}

impl RankClassKind {
    fn id(self) -> ClassId {
        match self {
            Self::Space => RANK_SPACE_CLASS_ID,
            Self::Node => RANK_NODE_CLASS_ID,
            Self::Coordinate => RANK_COORDINATE_CLASS_ID,
        }
    }

    pub(crate) fn symbol(self) -> Symbol {
        match self {
            Self::Space => rank_space_class_symbol(),
            Self::Node => rank_node_class_symbol(),
            Self::Coordinate => rank_coordinate_class_symbol(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct RankClass {
    pub(crate) kind: RankClassKind,
}

impl Object for RankClass {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok(format!("#<class {}>", self.kind.symbol()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for RankClass {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        class_value_or_stub(cx, CORE_CLASS_CLASS_ID, Symbol::qualified("core", "Class"))
    }

    fn as_expr(&self, _cx: &mut Cx) -> Result<sim_kernel::Expr> {
        Ok(sim_kernel::Expr::Symbol(self.kind.symbol()))
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }

    fn as_class(&self) -> Option<&dyn Class> {
        Some(self)
    }
}

impl Callable for RankClass {
    fn call(&self, cx: &mut Cx, args: Args) -> Result<Value> {
        construct_rank_value(cx, self.kind, args.into_vec())
    }
}

impl Class for RankClass {
    fn id(&self) -> ClassId {
        self.kind.id()
    }

    fn symbol(&self) -> Symbol {
        self.kind.symbol()
    }

    fn constructor_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        shape_ref(cx, RankArgsShape { kind: self.kind })
    }

    fn instance_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        shape_ref(cx, RankValueShape { kind: self.kind })
    }

    fn read_constructor(&self, _cx: &mut Cx) -> Result<Option<ReadConstructorRef>> {
        Ok(Some(DefaultFactory.opaque(Arc::new(
            RankReadConstructor { kind: self.kind },
        ))?))
    }

    fn members(&self, cx: &mut Cx) -> Result<TableRef> {
        cx.factory().table(Vec::new())
    }
}

#[derive(Clone)]
struct RankReadConstructor {
    kind: RankClassKind,
}

impl Object for RankReadConstructor {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok(format!("#<read-constructor {}>", self.kind.symbol()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for RankReadConstructor {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        class_value_or_stub(
            cx,
            CORE_FUNCTION_CLASS_ID,
            Symbol::qualified("core", "Function"),
        )
    }

    fn as_read_constructor(&self) -> Option<&dyn ReadConstructor> {
        Some(self)
    }
}

impl ReadConstructor for RankReadConstructor {
    fn symbol(&self) -> Symbol {
        self.kind.symbol()
    }

    fn args_shape(&self, cx: &mut Cx) -> Result<ShapeRef> {
        shape_ref(cx, RankArgsShape { kind: self.kind })
    }

    fn construct_read(&self, cx: &mut Cx, args: Vec<Value>) -> Result<Value> {
        construct_rank_value(cx, self.kind, args)
    }
}

/// Returns the symbol `rank/Space` naming the rank space class.
pub fn rank_space_class_symbol() -> Symbol {
    Symbol::qualified("rank", "Space")
}

/// Returns the symbol `rank/Node` naming the rank node class.
pub fn rank_node_class_symbol() -> Symbol {
    Symbol::qualified("rank", "Node")
}

/// Returns the symbol `rank/Coord` naming the rank coordinate class.
pub fn rank_coordinate_class_symbol() -> Symbol {
    Symbol::qualified("rank", "Coord")
}

pub(crate) fn rank_space_class_value_or_stub(cx: &mut Cx) -> Result<Value> {
    class_value_or_stub(cx, RANK_SPACE_CLASS_ID, rank_space_class_symbol())
}

pub(crate) fn rank_node_class_value_or_stub(cx: &mut Cx) -> Result<Value> {
    class_value_or_stub(cx, RANK_NODE_CLASS_ID, rank_node_class_symbol())
}

pub(crate) fn rank_coordinate_class_value_or_stub(cx: &mut Cx) -> Result<Value> {
    class_value_or_stub(cx, RANK_COORDINATE_CLASS_ID, rank_coordinate_class_symbol())
}

pub(crate) fn class_value_or_stub(cx: &mut Cx, id: ClassId, symbol: Symbol) -> Result<Value> {
    if let Some(value) = cx.registry().class_by_symbol(&symbol) {
        return Ok(value.clone());
    }
    cx.factory().class_stub(id, symbol)
}

fn construct_rank_value(cx: &mut Cx, kind: RankClassKind, args: Vec<Value>) -> Result<Value> {
    match kind {
        RankClassKind::Space => {
            let [symbol] = args.as_slice() else {
                return Err(arity_error("rank/Space", "space-symbol"));
            };
            let symbol = symbol_from_value(cx, symbol, "rank space symbol")?;
            cx.resolve_value(&symbol)
        }
        RankClassKind::Node => {
            let exprs = value_exprs(cx, args)?;
            let [node] = exprs.as_slice() else {
                return Err(arity_error("rank/Node", "node-data"));
            };
            rank_node_value(cx, rank_node_from_expr(node.clone())?)
        }
        RankClassKind::Coordinate => {
            let [space, ordinal] = args.as_slice() else {
                return Err(arity_error("rank/Coord", "space ordinal"));
            };
            let ordinal = nat_from_value(cx, ordinal)?;
            let coordinate = sim_kernel::Coordinate {
                space: symbol_from_value(cx, space, "rank coordinate space")?,
                ordinal: intern_ordinal(cx, &ordinal)?,
            };
            rank_coordinate_value(cx, coordinate)
        }
    }
}

fn arity_error(function: &'static str, expected: &'static str) -> Error {
    Error::Eval(format!("{function} expects {expected}"))
}

fn shape_ref(cx: &mut Cx, shape: impl Shape + 'static) -> Result<ShapeRef> {
    cx.factory().opaque(Arc::new(shape))
}

#[derive(Clone)]
struct RankValueShape {
    kind: RankClassKind,
}

impl Shape for RankValueShape {
    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol::qualified("rank-shape", self.kind.shape_name()))
    }

    fn check_value(&self, _cx: &mut Cx, value: Value) -> Result<ShapeMatch> {
        if self.kind.value_matches(&value) {
            Ok(ShapeMatch::accept(MatchScore::exact(30)))
        } else {
            Ok(ShapeMatch::reject(format!(
                "{} instance shape expects {}",
                self.kind.symbol(),
                self.kind.value_label()
            )))
        }
    }

    fn check_expr(&self, _cx: &mut Cx, expr: &Expr) -> Result<ShapeMatch> {
        if self.kind.instance_expr_matches(expr) {
            Ok(ShapeMatch::accept(MatchScore::exact(25)))
        } else {
            Ok(ShapeMatch::reject(format!(
                "{} instance expression shape expects {}",
                self.kind.symbol(),
                self.kind.value_label()
            )))
        }
    }

    fn describe(&self, _cx: &mut Cx) -> Result<ShapeDoc> {
        Ok(ShapeDoc::new(format!("{} instance", self.kind.symbol()))
            .with_detail(self.kind.value_label()))
    }
}

#[derive(Clone)]
struct RankArgsShape {
    kind: RankClassKind,
}

impl Shape for RankArgsShape {
    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol::qualified(
            "rank-shape",
            format!("{}-args", self.kind.shape_name()),
        ))
    }

    fn check_value(&self, cx: &mut Cx, value: Value) -> Result<ShapeMatch> {
        let expr = value.object().as_expr(cx)?;
        self.check_expr(cx, &expr)
    }

    fn check_expr(&self, _cx: &mut Cx, expr: &Expr) -> Result<ShapeMatch> {
        if self.kind.args_expr_matches(expr) {
            Ok(ShapeMatch::accept(MatchScore::exact(30)))
        } else {
            Ok(ShapeMatch::reject(format!(
                "{} constructor shape expects {}",
                self.kind.symbol(),
                self.kind.args_label()
            )))
        }
    }

    fn describe(&self, _cx: &mut Cx) -> Result<ShapeDoc> {
        Ok(
            ShapeDoc::new(format!("{} constructor args", self.kind.symbol()))
                .with_detail(self.kind.args_label()),
        )
    }
}

impl RankClassKind {
    fn shape_name(self) -> &'static str {
        match self {
            Self::Space => "rank-space",
            Self::Node => "rank-node",
            Self::Coordinate => "rank-coordinate",
        }
    }

    fn value_label(self) -> &'static str {
        match self {
            Self::Space => "a rank space value",
            Self::Node => "a rank node value",
            Self::Coordinate => "a rank coordinate value",
        }
    }

    fn args_label(self) -> &'static str {
        match self {
            Self::Space => "one rank space symbol",
            Self::Node => "one rank node expression",
            Self::Coordinate => "one rank space symbol and one natural ordinal",
        }
    }

    fn value_matches(self, value: &Value) -> bool {
        match self {
            Self::Space => value.object().downcast_ref::<RankSpace>().is_some(),
            Self::Node => rank_node_from_value(value).is_ok(),
            Self::Coordinate => coordinate_from_value(value).is_ok(),
        }
    }

    fn instance_expr_matches(self, expr: &Expr) -> bool {
        match self {
            Self::Space => matches!(expr, Expr::Symbol(_)),
            Self::Node => rank_node_instance_expr_matches(expr),
            Self::Coordinate => coordinate_instance_expr_matches(expr),
        }
    }

    fn args_expr_matches(self, expr: &Expr) -> bool {
        let Some(args) = constructor_arg_exprs(self, expr) else {
            return false;
        };
        match self {
            Self::Space => matches!(args, [Expr::Symbol(_)]),
            Self::Node => {
                let [node] = args else {
                    return false;
                };
                rank_node_from_expr(node.clone()).is_ok()
            }
            Self::Coordinate => {
                let [Expr::Symbol(_), ordinal] = args else {
                    return false;
                };
                nat_expr_matches(ordinal)
            }
        }
    }
}

fn constructor_arg_exprs(kind: RankClassKind, expr: &Expr) -> Option<&[Expr]> {
    match expr {
        Expr::List(args) | Expr::Vector(args) => Some(args),
        Expr::Call { operator, args } if matches_class_operator(operator, kind.symbol()) => {
            Some(args)
        }
        _ => None,
    }
}

fn rank_node_instance_expr_matches(expr: &Expr) -> bool {
    if rank_node_from_expr(expr.clone()).is_ok() {
        return true;
    }
    let Expr::Call { operator, args } = expr else {
        return false;
    };
    let [node] = args.as_slice() else {
        return false;
    };
    matches_class_operator(operator, rank_node_class_symbol())
        && rank_node_from_expr(node.clone()).is_ok()
}

fn coordinate_instance_expr_matches(expr: &Expr) -> bool {
    let Expr::Call { operator, args } = expr else {
        return false;
    };
    matches_class_operator(operator, rank_coordinate_class_symbol())
        && matches!(args.as_slice(), [Expr::Symbol(_), ordinal] if nat_expr_matches(ordinal))
}

fn matches_class_operator(operator: &Expr, symbol: Symbol) -> bool {
    matches!(operator, Expr::Symbol(found) if found == &symbol)
}

fn nat_expr_matches(expr: &Expr) -> bool {
    match expr {
        Expr::Number(number) => Nat::from_number_literal(number).is_ok(),
        Expr::String(value) => value.parse::<Nat>().is_ok(),
        _ => false,
    }
}

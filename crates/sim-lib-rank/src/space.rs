//! Runtime objects for rank spaces, nodes, and coordinates.
//!
//! Wraps a grammar plus a codec as a first-class `RankSpace` runtime object
//! that exposes rank and unrank operations, and provides the object wrappers
//! that carry `RankNode` values and `Coordinate` references through the runtime.

use std::{any::Any, sync::Arc};

use sim_kernel::{
    CapabilityName, ClassRef, Cx, Expr, Object, ObjectCompat, ObjectEncode, ObjectEncoding,
    ObjectHeader, Ref, Result as KernelResult, Symbol, TrustLevel,
};

use crate::{
    RankCodec, RankGroupCodec, RankNode,
    grammar::RankGrammar,
    ops::{RankOp, RankOpKind},
};

/// Shared reference to a rank codec backing a space.
pub type RankCodecRef = Arc<dyn RankCodec>;

/// A rankable space: a grammar plus codec exposing rank/unrank operations.
#[derive(Clone, Debug)]
pub struct RankSpace {
    symbol: Symbol,
    grammar: RankGrammar,
    codec: RankCodecRef,
    header: ObjectHeader,
    rank_op: RankOp,
    unrank_op: RankOp,
}

impl RankSpace {
    /// Builds a space named `symbol` over `grammar` using the group codec.
    pub fn group(symbol: Symbol, grammar: RankGrammar) -> Self {
        Self::with_codec(
            symbol,
            grammar.clone(),
            Arc::new(RankGroupCodec::new(grammar)),
        )
    }

    /// Builds a space named `symbol` over `grammar` with an explicit `codec`.
    pub fn with_codec(symbol: Symbol, grammar: RankGrammar, codec: RankCodecRef) -> Self {
        let subject = Ref::Symbol(symbol.clone());
        Self {
            symbol: symbol.clone(),
            grammar,
            codec: codec.clone(),
            header: ObjectHeader {
                id: subject.clone(),
                kind: sim_kernel::rank::rank_space_kind(),
                trust: TrustLevel::HostInternal,
            },
            rank_op: RankOp::new(
                RankOpKind::Rank,
                subject.clone(),
                symbol.clone(),
                codec.clone(),
            ),
            unrank_op: RankOp::new(RankOpKind::Unrank, subject, symbol, codec),
        }
    }

    /// Returns the symbol naming this space.
    pub fn symbol(&self) -> &Symbol {
        &self.symbol
    }

    /// Returns the grammar describing this space's structure.
    pub fn grammar(&self) -> &RankGrammar {
        &self.grammar
    }

    /// Returns the codec backing this space's rank/unrank operations.
    pub fn codec(&self) -> &RankCodecRef {
        &self.codec
    }

    /// Wraps this space as an opaque runtime value.
    pub fn value(&self, cx: &mut Cx) -> KernelResult<sim_kernel::Value> {
        cx.factory().opaque(Arc::new(self.clone()))
    }
}

impl Object for RankSpace {
    fn header(&self) -> &ObjectHeader {
        &self.header
    }

    fn op(&self, key: &sim_kernel::OpKey) -> Option<&dyn sim_kernel::Op> {
        if key == &sim_kernel::rank::rank_rank_op_key() {
            Some(&self.rank_op)
        } else if key == &sim_kernel::rank::rank_unrank_op_key() {
            Some(&self.unrank_op)
        } else {
            None
        }
    }

    fn display(&self, _cx: &mut Cx) -> KernelResult<String> {
        Ok(format!("#<rank-space {}>", self.symbol))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ObjectCompat for RankSpace {
    fn class(&self, cx: &mut Cx) -> KernelResult<ClassRef> {
        crate::lisp::rank_space_class_value_or_stub(cx)
    }

    fn as_expr(&self, _cx: &mut Cx) -> KernelResult<Expr> {
        Ok(Expr::Symbol(self.symbol.clone()))
    }

    fn as_object_encoder(&self) -> Option<&dyn ObjectEncode> {
        Some(self)
    }
}

impl ObjectEncode for RankSpace {
    fn object_encoding(&self, _cx: &mut Cx) -> KernelResult<ObjectEncoding> {
        Ok(ObjectEncoding::Constructor {
            class: crate::lisp::rank_space_class_symbol(),
            args: vec![Expr::Symbol(self.symbol.clone())],
        })
    }
}

impl sim_citizen::Citizen for RankSpace {
    fn citizen_symbol() -> Symbol {
        crate::lisp::rank_space_class_symbol()
    }

    fn citizen_version() -> u32 {
        0
    }

    fn citizen_arity() -> usize {
        1
    }

    fn citizen_fields() -> &'static [&'static str] {
        &["symbol"]
    }
}

/// Runtime object wrapping a single `RankNode` value.
#[derive(Clone, Debug)]
pub struct RankNodeValue {
    node: RankNode,
}

impl RankNodeValue {
    /// Wraps `node` as a runtime node value.
    pub fn new(node: RankNode) -> Self {
        Self { node }
    }

    /// Borrows the wrapped node.
    pub fn node(&self) -> &RankNode {
        &self.node
    }
}

impl Object for RankNodeValue {
    fn display(&self, _cx: &mut Cx) -> KernelResult<String> {
        Ok(format!("#<rank-node {:?}>", self.node))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ObjectCompat for RankNodeValue {
    fn class(&self, cx: &mut Cx) -> KernelResult<ClassRef> {
        crate::lisp::rank_node_class_value_or_stub(cx)
    }

    fn as_expr(&self, _cx: &mut Cx) -> KernelResult<Expr> {
        Ok(Expr::Call {
            operator: Box::new(Expr::Symbol(crate::lisp::rank_node_class_symbol())),
            args: crate::lisp::rank_node_constructor_args(&self.node),
        })
    }

    fn as_object_encoder(&self) -> Option<&dyn ObjectEncode> {
        Some(self)
    }
}

impl ObjectEncode for RankNodeValue {
    fn object_encoding(&self, _cx: &mut Cx) -> KernelResult<ObjectEncoding> {
        Ok(ObjectEncoding::Constructor {
            class: crate::lisp::rank_node_class_symbol(),
            args: crate::lisp::rank_node_constructor_args(&self.node),
        })
    }
}

impl sim_citizen::Citizen for RankNodeValue {
    fn citizen_symbol() -> Symbol {
        crate::lisp::rank_node_class_symbol()
    }

    fn citizen_version() -> u32 {
        0
    }

    fn citizen_arity() -> usize {
        1
    }

    fn citizen_fields() -> &'static [&'static str] {
        &["node"]
    }
}

/// Wraps `node` as an opaque runtime value.
pub fn rank_node_value(cx: &mut Cx, node: RankNode) -> KernelResult<sim_kernel::Value> {
    cx.factory().opaque(Arc::new(RankNodeValue::new(node)))
}

/// Extracts a `RankNode` from a runtime value, erroring on a type mismatch.
pub fn rank_node_from_value(value: &sim_kernel::Value) -> KernelResult<RankNode> {
    value
        .object()
        .downcast_ref::<RankNodeValue>()
        .map(|value| value.node().clone())
        .ok_or(sim_kernel::Error::TypeMismatch {
            expected: "rank-node",
            found: "non-rank-node",
        })
}

/// Runtime object wrapping a space coordinate (space plus ordinal).
#[derive(Clone, Debug)]
pub struct RankCoordinateValue {
    coordinate: sim_kernel::Coordinate,
    header: ObjectHeader,
}

impl RankCoordinateValue {
    /// Wraps `coordinate` as a runtime coordinate value.
    pub fn new(coordinate: sim_kernel::Coordinate) -> Self {
        Self {
            header: ObjectHeader {
                id: Ref::Coord(coordinate.clone()),
                kind: sim_kernel::rank::rank_coordinate_kind(),
                trust: TrustLevel::HostInternal,
            },
            coordinate,
        }
    }

    /// Borrows the wrapped coordinate.
    pub fn coordinate(&self) -> &sim_kernel::Coordinate {
        &self.coordinate
    }
}

impl Object for RankCoordinateValue {
    fn header(&self) -> &ObjectHeader {
        &self.header
    }

    fn display(&self, _cx: &mut Cx) -> KernelResult<String> {
        Ok(format!(
            "#<rank-coordinate {} {:?}>",
            self.coordinate.space, self.coordinate.ordinal
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ObjectCompat for RankCoordinateValue {
    fn class(&self, cx: &mut Cx) -> KernelResult<ClassRef> {
        crate::lisp::rank_coordinate_class_value_or_stub(cx)
    }

    fn as_expr(&self, cx: &mut Cx) -> KernelResult<Expr> {
        Ok(Expr::Call {
            operator: Box::new(Expr::Symbol(crate::lisp::rank_coordinate_class_symbol())),
            args: crate::lisp::rank_coordinate_constructor_args(cx, &self.coordinate)?,
        })
    }

    fn as_object_encoder(&self) -> Option<&dyn ObjectEncode> {
        Some(self)
    }
}

impl ObjectEncode for RankCoordinateValue {
    fn object_encoding(&self, cx: &mut Cx) -> KernelResult<ObjectEncoding> {
        Ok(ObjectEncoding::Constructor {
            class: crate::lisp::rank_coordinate_class_symbol(),
            args: crate::lisp::rank_coordinate_constructor_args(cx, &self.coordinate)?,
        })
    }
}

impl sim_citizen::Citizen for RankCoordinateValue {
    fn citizen_symbol() -> Symbol {
        crate::lisp::rank_coordinate_class_symbol()
    }

    fn citizen_version() -> u32 {
        0
    }

    fn citizen_arity() -> usize {
        2
    }

    fn citizen_fields() -> &'static [&'static str] {
        &["space", "ordinal"]
    }
}

/// Wraps `coordinate` as an opaque runtime value.
pub fn rank_coordinate_value(
    cx: &mut Cx,
    coordinate: sim_kernel::Coordinate,
) -> KernelResult<sim_kernel::Value> {
    cx.factory()
        .opaque(Arc::new(RankCoordinateValue::new(coordinate)))
}

/// Extracts a `Coordinate` from a runtime value or coordinate reference.
///
/// Errors on a value that is neither a coordinate object nor a coordinate ref.
pub fn coordinate_from_value(value: &sim_kernel::Value) -> KernelResult<sim_kernel::Coordinate> {
    if let Some(value) = value.object().downcast_ref::<RankCoordinateValue>() {
        return Ok(value.coordinate().clone());
    }
    if let Ref::Coord(coordinate) = &value.header().id {
        return Ok(coordinate.clone());
    }
    Err(sim_kernel::Error::TypeMismatch {
        expected: "rank-coordinate",
        found: "non-rank-coordinate",
    })
}

pub(crate) fn no_rank_capabilities() -> Vec<CapabilityName> {
    vec![crate::rank_read_capability()]
}

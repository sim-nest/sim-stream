//! Runtime rank and unrank operations for a rank space.
//!
//! Wires a space's codec into kernel ops so values can be ranked into
//! coordinates and coordinates unranked back into values, publishing
//! coordinate claims along the way.

use sim_kernel::{
    ContentId, Cx, Datum, DatumStore, Op, OpKey, OpSpec, Ref, Result as KernelResult, Step, Symbol,
};

use crate::{
    Nat, RankCodec,
    claims::publish_coordinate_claims,
    nat::intern_ordinal,
    space::{
        RankCodecRef, coordinate_from_value, no_rank_capabilities, rank_coordinate_value,
        rank_node_from_value, rank_node_value,
    },
};

/// Which direction a [`RankOp`] runs: value to ordinal, or ordinal to value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RankOpKind {
    /// Rank a node value into a coordinate (value -> ordinal).
    Rank,
    /// Unrank a coordinate back into a node value (ordinal -> value).
    Unrank,
}

/// Runtime operation that ranks or unranks values for one rank space.
///
/// Bound to a space symbol and its codec, it implements the kernel `rank` or
/// `unrank` op key according to its [`RankOpKind`].
#[derive(Clone, Debug)]
pub struct RankOp {
    kind: RankOpKind,
    space: Symbol,
    codec: RankCodecRef,
    spec: OpSpec,
}

impl RankOp {
    /// Builds a rank or unrank op for `space` over `subject`, backed by `codec`.
    pub fn new(kind: RankOpKind, subject: Ref, space: Symbol, codec: RankCodecRef) -> Self {
        let key = match kind {
            RankOpKind::Rank => sim_kernel::rank::rank_rank_op_key(),
            RankOpKind::Unrank => sim_kernel::rank::rank_unrank_op_key(),
        };
        Self {
            kind,
            space,
            codec,
            spec: OpSpec::new(
                key,
                subject,
                sim_kernel::core_any_ref(),
                sim_kernel::core_any_ref(),
            )
            .with_requirements(no_rank_capabilities()),
        }
    }
}

impl Op for RankOp {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn invoke_authorized(&self, cx: &mut Cx, input: sim_kernel::Value) -> KernelResult<Step> {
        match self.kind {
            RankOpKind::Rank => rank_value(cx, self.space.clone(), self.codec.as_ref(), input),
            RankOpKind::Unrank => unrank_value(cx, &self.space, self.codec.as_ref(), input),
        }
    }
}

/// Returns the kernel op key for the rank (value -> coordinate) operation.
pub fn rank_rank_op_key() -> OpKey {
    sim_kernel::rank::rank_rank_op_key()
}

/// Returns the kernel op key for the unrank (coordinate -> value) operation.
pub fn rank_unrank_op_key() -> OpKey {
    sim_kernel::rank::rank_unrank_op_key()
}

fn rank_value(
    cx: &mut Cx,
    space: Symbol,
    codec: &dyn RankCodec,
    input: sim_kernel::Value,
) -> KernelResult<Step> {
    let node = rank_node_from_value(&input)?;
    let ordinal = codec.rank_node(&node)?;
    let ordinal = intern_ordinal(cx, &ordinal)?;
    let coordinate = sim_kernel::Coordinate { space, ordinal };
    publish_coordinate_claims(cx, coordinate.clone())?;
    rank_coordinate_value(cx, coordinate).map(Step::Value)
}

fn unrank_value(
    cx: &mut Cx,
    space: &Symbol,
    codec: &dyn RankCodec,
    input: sim_kernel::Value,
) -> KernelResult<Step> {
    let coordinate = coordinate_from_value(&input)?;
    if &coordinate.space != space {
        return Err(sim_kernel::Error::Eval(format!(
            "rank coordinate belongs to {}, not {}",
            coordinate.space, space
        )));
    }
    let ordinal = nat_from_content(cx, &coordinate.ordinal)?;
    let node = codec.unrank_node(&ordinal)?;
    rank_node_value(cx, node).map(Step::Value)
}

fn nat_from_content(cx: &mut Cx, id: &ContentId) -> KernelResult<Nat> {
    let datum = cx.datum_store().get(id)?.cloned().ok_or_else(|| {
        sim_kernel::Error::Eval(format!("rank ordinal content id {id:?} is missing"))
    })?;
    let Datum::Number(number) = datum else {
        return Err(sim_kernel::Error::Eval(
            "rank coordinate ordinal is not a number datum".to_owned(),
        ));
    };
    Ok(Nat::from_number_literal(&number)?)
}

//! Feature-gated universal fallback for ranking SIM values by storage identity.
//!
//! This surface maps a value's canonical `codec:binary` frame to a natural
//! ordinal and decodes that ordinal back to the same expression. It is a
//! storage-identity fallback for persistence and exhaustive byte traversal, not
//! a semantic ordering for the value's domain.

use num_bigint::BigUint;
use sim_kernel::{
    Cx, Expr, Ref, Result as KernelResult, Symbol, Value,
    card::{card_for_ref_with_fallback, card_requires_predicate},
};

use crate::{Nat, cap::rank_codec_capability};

/// Returns the symbol identifying the storage-identity fallback codec.
pub fn rank_codec_fallback_symbol() -> Symbol {
    Symbol::qualified("rank", "codec-fallback")
}

/// Returns the symbol of the binary codec whose frames define the fallback ordinal.
pub fn binary_codec_symbol() -> Symbol {
    Symbol::qualified("codec", "binary")
}

/// Interprets a big-endian binary frame as a natural ordinal.
pub fn binary_frame_to_nat(frame: &[u8]) -> Nat {
    Nat::from_biguint(BigUint::from_bytes_be(frame))
}

/// Renders a natural ordinal back to its big-endian binary frame.
pub fn binary_frame_from_nat(ordinal: &Nat) -> Vec<u8> {
    ordinal.as_biguint().to_bytes_be()
}

/// Ranks an expression to its storage-identity ordinal via its binary frame.
pub fn rank_expr_storage_identity(expr: &Expr) -> KernelResult<Nat> {
    let sim_codec_binary::BinaryFrame(frame) = sim_codec_binary::encode_frame(expr)?;
    Ok(binary_frame_to_nat(&frame))
}

/// Unranks a storage-identity ordinal back to its expression via its binary frame.
pub fn unrank_expr_storage_identity(ordinal: &Nat) -> KernelResult<Expr> {
    let frame = binary_frame_from_nat(ordinal);
    sim_codec_binary::decode_frame(sim_kernel::CodecId(0), &frame).map(|(_, expr)| expr)
}

/// Ranks an expression to its storage-identity ordinal after a capability check.
pub fn rank_expr_with_fallback(cx: &mut Cx, expr: &Expr) -> KernelResult<Nat> {
    cx.require(&rank_codec_capability())?;
    rank_expr_storage_identity(expr)
}

/// Ranks a value to its storage-identity ordinal after a capability check.
pub fn rank_value_with_fallback(cx: &mut Cx, value: &Value) -> KernelResult<Nat> {
    cx.require(&rank_codec_capability())?;
    let expr = value.object().as_expr(cx)?;
    rank_expr_storage_identity(&expr)
}

/// Unranks a storage-identity ordinal back to its expression after a capability check.
pub fn unrank_expr_with_fallback(cx: &mut Cx, ordinal: &Nat) -> KernelResult<Expr> {
    cx.require(&rank_codec_capability())?;
    unrank_expr_storage_identity(ordinal)
}

/// Builds the card describing the fallback codec, its required capability, and
/// its non-semantic storage-identity ordering.
pub fn rank_codec_fallback_card(cx: &mut Cx) -> KernelResult<Value> {
    let capability = rank_codec_capability().as_symbol();
    let requires = cx
        .factory()
        .list(vec![cx.factory().symbol(capability.clone())?])?;
    let fallback = cx.factory().table(vec![
        (field("codec"), cx.factory().symbol(binary_codec_symbol())?),
        (
            field("identity"),
            cx.factory().string("storage identity".to_owned())?,
        ),
        (
            field("order-semantics"),
            cx.factory().string("not semantic order".to_owned())?,
        ),
        (field("requires"), requires),
    ])?;
    cx.insert_fact(sim_kernel::Claim::public(
        Ref::Symbol(rank_codec_fallback_symbol()),
        card_requires_predicate(),
        Ref::Symbol(capability),
    ))?;
    card_for_ref_with_fallback(
        cx,
        Ref::Symbol(rank_codec_fallback_symbol()),
        Some(fallback),
        Some(rank_codec_fallback_symbol()),
    )
}

fn field(name: &str) -> Symbol {
    Symbol::new(name.to_owned())
}

//! Publishing of rank-space cards and coordinates into the knowledge store.
//!
//! Turns a `RankSpace` plus its metadata into claim facts and a card value,
//! recording orders, metrics, capabilities, grades, limits, and default
//! contexts under stable `rank/*` predicates.

use sim_kernel::{
    CapabilityName, Claim, ClaimPattern, Coordinate, Cx, Datum, DatumStore, Ref,
    Result as KernelResult, Symbol,
    card::{card_for_ref_with_fallback, card_requires_predicate, card_tests_predicate},
};

use crate::{
    RankLimits, RankVersion,
    cap::{
        rank_browse_capability, rank_enumerate_capability, rank_neighbor_capability,
        rank_read_capability,
    },
    context::{RankContext, default_context_claim_datum, order_symbol, standard_default_contexts},
    space::RankSpace,
};

/// Metadata describing a rank-space card published into the knowledge store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankSpaceCardMetadata {
    /// Version stamp of the space contract.
    pub version: RankVersion,
    /// Named orders the space exposes for ranking.
    pub orders: Vec<Symbol>,
    /// Named distance/similarity metrics over the space.
    pub metrics: Vec<Symbol>,
    /// Default order chosen per ranking context.
    pub default_contexts: Vec<(RankContext, Symbol)>,
    /// Capabilities required to read or traverse the space.
    pub capabilities: Vec<CapabilityName>,
    /// Named grade bands used for cost-ordered ranking.
    pub grades: Vec<Symbol>,
    /// Resource limits (fuel and bit budget) for ranking operations.
    pub limits: RankLimits,
    /// Named conformance tests advertised by the card.
    pub tests: Vec<Symbol>,
}

impl RankSpaceCardMetadata {
    /// Returns a baseline metadata set with the standard orders and capabilities.
    pub fn basic() -> Self {
        Self {
            version: RankVersion::v1(),
            orders: vec![order_symbol("canonical"), order_symbol("grade-first")],
            metrics: vec![Symbol::qualified("rank/metric", "generic-node")],
            default_contexts: standard_default_contexts(),
            capabilities: vec![
                rank_read_capability(),
                rank_enumerate_capability(),
                rank_browse_capability(),
                rank_neighbor_capability(),
            ],
            grades: vec![Symbol::qualified("rank-grade", "finite-bands")],
            limits: RankLimits::default(),
            tests: vec![Symbol::qualified("rank-test", "roundtrip")],
        }
    }
}

impl Default for RankSpaceCardMetadata {
    fn default() -> Self {
        Self::basic()
    }
}

/// Publishes the base claim facts identifying `space` in the knowledge store.
pub fn publish_space_claims(
    cx: &mut Cx,
    space: &RankSpace,
    help: Option<&str>,
) -> KernelResult<()> {
    sim_kernel::rank::publish_rank_space_claims(cx, space.symbol().clone(), help)
}

/// Publishes the full card claim set for `space` from its `metadata`.
pub fn publish_space_card_claims(
    cx: &mut Cx,
    space: &RankSpace,
    metadata: &RankSpaceCardMetadata,
    help: Option<&str>,
) -> KernelResult<()> {
    publish_space_claims(cx, space, help)?;
    let subject = Ref::Symbol(space.symbol().clone());
    let version_ref = intern(cx, Datum::String(metadata.version.to_string()))?;
    insert_once(cx, subject.clone(), rank_version_predicate(), version_ref)?;
    let grammar_ref = intern(cx, space.grammar().summary_datum())?;
    insert_once(cx, subject.clone(), rank_grammar_predicate(), grammar_ref)?;
    insert_once(
        cx,
        subject.clone(),
        rank_codec_predicate(),
        Ref::Symbol(space.codec().id()),
    )?;
    let limits_ref = intern(cx, limits_datum(&metadata.limits))?;
    insert_once(cx, subject.clone(), rank_limits_predicate(), limits_ref)?;
    for order in &metadata.orders {
        insert_once(
            cx,
            subject.clone(),
            rank_order_predicate(),
            Ref::Symbol(order.clone()),
        )?;
    }
    for metric in &metadata.metrics {
        insert_once(
            cx,
            subject.clone(),
            rank_metric_predicate(),
            Ref::Symbol(metric.clone()),
        )?;
    }
    for (context, order) in &metadata.default_contexts {
        let context_ref = intern(cx, default_context_claim_datum(context, order))?;
        insert_once(
            cx,
            subject.clone(),
            rank_default_context_predicate(),
            context_ref,
        )?;
    }
    for capability in &metadata.capabilities {
        insert_once(
            cx,
            subject.clone(),
            rank_capability_predicate(),
            Ref::Symbol(capability.as_symbol()),
        )?;
        insert_once(
            cx,
            subject.clone(),
            card_requires_predicate(),
            Ref::Symbol(capability.as_symbol()),
        )?;
    }
    for grade in &metadata.grades {
        insert_once(
            cx,
            subject.clone(),
            rank_grade_predicate(),
            Ref::Symbol(grade.clone()),
        )?;
    }
    for test in &metadata.tests {
        insert_once(
            cx,
            subject.clone(),
            card_tests_predicate(),
            Ref::Symbol(test.clone()),
        )?;
    }
    Ok(())
}

/// Publishes the card claims for `space` and returns its card value.
pub fn rank_space_card(
    cx: &mut Cx,
    space: &RankSpace,
    metadata: &RankSpaceCardMetadata,
    help: Option<&str>,
) -> KernelResult<sim_kernel::Value> {
    publish_space_card_claims(cx, space, metadata, help)?;
    let grammar_ref = intern(cx, space.grammar().summary_datum())?;
    let grammar = sim_kernel::card::ref_value(cx, &grammar_ref)?;
    let codec = cx.factory().symbol(space.codec().id())?;
    let orders = symbol_list(cx, &metadata.orders)?;
    let metrics = symbol_list(cx, &metadata.metrics)?;
    let default_contexts = default_contexts_value(cx, &metadata.default_contexts)?;
    let grades = symbol_list(cx, &metadata.grades)?;
    let limits = limits_value(cx, &metadata.limits)?;
    let fallback = cx.factory().table(vec![
        (field("grammar"), grammar),
        (field("codec"), codec),
        (field("orders"), orders),
        (field("metrics"), metrics),
        (field("default-contexts"), default_contexts),
        (field("grades"), grades),
        (field("limits"), limits),
    ])?;
    card_for_ref_with_fallback(
        cx,
        Ref::Symbol(space.symbol().clone()),
        Some(fallback),
        Some(sim_kernel::rank::rank_space_kind()),
    )
}

/// Publishes the claim facts identifying a single `coordinate`.
pub fn publish_coordinate_claims(cx: &mut Cx, coordinate: Coordinate) -> KernelResult<()> {
    sim_kernel::rank::publish_coordinate_claims(cx, coordinate)
}

/// Returns the `rank/version` predicate symbol.
pub fn rank_version_predicate() -> Symbol {
    rank_symbol("version")
}

/// Returns the `rank/grammar` predicate symbol.
pub fn rank_grammar_predicate() -> Symbol {
    rank_symbol("grammar")
}

/// Returns the `rank/codec` predicate symbol.
pub fn rank_codec_predicate() -> Symbol {
    rank_symbol("codec")
}

/// Returns the `rank/order` predicate symbol.
pub fn rank_order_predicate() -> Symbol {
    rank_symbol("order")
}

/// Returns the `rank/metric` predicate symbol.
pub fn rank_metric_predicate() -> Symbol {
    rank_symbol("metric")
}

/// Returns the `rank/default-context` predicate symbol.
pub fn rank_default_context_predicate() -> Symbol {
    rank_symbol("default-context")
}

/// Returns the `rank/capability` predicate symbol.
pub fn rank_capability_predicate() -> Symbol {
    rank_symbol("capability")
}

/// Returns the `rank/grade` predicate symbol.
pub fn rank_grade_predicate() -> Symbol {
    rank_symbol("grade")
}

/// Returns the `rank/limits` predicate symbol.
pub fn rank_limits_predicate() -> Symbol {
    rank_symbol("limits")
}

fn insert_once(cx: &mut Cx, subject: Ref, predicate: Symbol, object: Ref) -> KernelResult<()> {
    let exists = !cx
        .query_facts(ClaimPattern::exact(
            subject.clone(),
            predicate.clone(),
            object.clone(),
        ))?
        .is_empty();
    if !exists {
        cx.insert_fact(Claim::public(subject, predicate, object))?;
    }
    Ok(())
}

fn intern(cx: &mut Cx, datum: Datum) -> KernelResult<Ref> {
    Ok(Ref::Content(cx.datum_store_mut().intern(datum)?))
}

fn limits_datum(limits: &RankLimits) -> Datum {
    Datum::Node {
        tag: rank_symbol("limits"),
        fields: vec![
            (field("fuel"), number_datum(limits.remaining_fuel())),
            (field("max-bits"), number_datum(limits.max_bits())),
        ],
    }
}

fn number_datum(value: u64) -> Datum {
    Datum::Number(sim_kernel::NumberLiteral {
        domain: crate::bigint_number_domain(),
        canonical: value.to_string(),
    })
}

fn symbol_list(cx: &mut Cx, symbols: &[Symbol]) -> KernelResult<sim_kernel::Value> {
    let values = symbols
        .iter()
        .cloned()
        .map(|symbol| cx.factory().symbol(symbol))
        .collect::<KernelResult<Vec<_>>>()?;
    cx.factory().list(values)
}

fn default_contexts_value(
    cx: &mut Cx,
    contexts: &[(RankContext, Symbol)],
) -> KernelResult<sim_kernel::Value> {
    let values = contexts
        .iter()
        .map(|(context, order)| {
            cx.factory().table(vec![
                (field("context"), cx.factory().symbol(context.symbol())?),
                (field("order"), cx.factory().symbol(order.clone())?),
            ])
        })
        .collect::<KernelResult<Vec<_>>>()?;
    cx.factory().list(values)
}

fn limits_value(cx: &mut Cx, limits: &RankLimits) -> KernelResult<sim_kernel::Value> {
    cx.factory().table(vec![
        (
            field("fuel"),
            cx.factory().number_literal(
                crate::bigint_number_domain(),
                limits.remaining_fuel().to_string(),
            )?,
        ),
        (
            field("max-bits"),
            cx.factory()
                .number_literal(crate::bigint_number_domain(), limits.max_bits().to_string())?,
        ),
    ])
}

fn field(name: &str) -> Symbol {
    Symbol::new(name.to_owned())
}

fn rank_symbol(name: &str) -> Symbol {
    Symbol::qualified("rank", name.to_owned())
}

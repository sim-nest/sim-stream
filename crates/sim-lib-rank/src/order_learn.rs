//! Learned ranking orders driven by a frozen frequency model.
//!
//! This module captures an observed score distribution over a codec's ordinals
//! as a [`RankFrozenLearnedModel`], scores candidates from it, and derives an
//! exact retrieval order that ranks higher-frequency ordinals first. It also
//! publishes the model as a card and claims into the runtime knowledge store.

use sim_kernel::{
    Claim, ClaimPattern, Cx, Datum, DatumStore, Ref, Result as KernelResult, Symbol, Value,
    card::card_for_ref_with_fallback,
};

use crate::{Nat, RankCodec, RankError, RankLimits, RankNode, RankResult, RankVersion};

use super::{RankScore, ScoreValue};

/// Immutable learned scoring model over a codec's ordinals.
///
/// Holds a frozen frequency table mapping each observed ordinal to a score and
/// a deduplicated novelty archive. It implements [`RankScore`] so it can drive
/// ranking and retrieval directly, and carries an identity [`Symbol`] plus a
/// content digest so the model can be published and referenced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankFrozenLearnedModel {
    id: Symbol,
    version: RankVersion,
    digest: String,
    frequencies: std::collections::BTreeMap<Nat, ScoreValue>,
    novelty_archive: Vec<Nat>,
}

impl RankFrozenLearnedModel {
    /// Builds a frozen model from an identity, digest, frequency pairs, and a
    /// novelty archive. The archive is sorted and deduplicated; the version is
    /// pinned to [`RankVersion::v1`].
    pub fn new(
        id: Symbol,
        digest: impl Into<String>,
        frequencies: impl IntoIterator<Item = (Nat, ScoreValue)>,
        novelty_archive: impl IntoIterator<Item = Nat>,
    ) -> Self {
        let mut novelty_archive = novelty_archive.into_iter().collect::<Vec<_>>();
        novelty_archive.sort();
        novelty_archive.dedup();
        Self {
            id,
            version: RankVersion::v1(),
            digest: digest.into(),
            frequencies: frequencies.into_iter().collect(),
            novelty_archive,
        }
    }

    /// Returns the model's identity symbol.
    pub fn id(&self) -> &Symbol {
        &self.id
    }

    /// Returns the content digest that fingerprints the captured distribution.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Returns the frequency table mapping each ordinal to its learned score.
    pub fn frequency_table(&self) -> &std::collections::BTreeMap<Nat, ScoreValue> {
        &self.frequencies
    }

    /// Returns the sorted, deduplicated novelty archive of ordinals.
    pub fn novelty_archive(&self) -> &[Nat] {
        &self.novelty_archive
    }

    /// Returns the learned score for `ordinal`, or zero if it is unseen.
    pub fn score_ordinal(&self, ordinal: &Nat) -> ScoreValue {
        self.frequencies.get(ordinal).copied().unwrap_or(0)
    }
}

impl RankScore for RankFrozenLearnedModel {
    fn id(&self) -> Symbol {
        self.id.clone()
    }

    fn version(&self) -> RankVersion {
        self.version
    }

    fn score(
        &self,
        _codec: &dyn RankCodec,
        ordinal: &Nat,
        _node: &RankNode,
    ) -> RankResult<ScoreValue> {
        Ok(self.score_ordinal(ordinal))
    }
}

/// Builds an exact order that ranks a codec's ordinals by learned frequency.
///
/// Enumerates every ordinal of a finite `codec` (consuming fuel and checking
/// limits), then sorts them by descending model score with the ordinal itself
/// as a stable tie-break. Fails with [`RankError::InvalidNode`] when the codec
/// has no finite count.
pub fn learned_frequency_order(
    id: Symbol,
    codec: &dyn RankCodec,
    model: &RankFrozenLearnedModel,
    limits: &mut RankLimits,
) -> RankResult<crate::order::RankExactOrder> {
    let count = codec.count().ok_or_else(|| RankError::InvalidNode {
        message: "learned frequency order requires a finite codec count".to_owned(),
    })?;
    let mut ordinals = Vec::new();
    let mut ordinal = Nat::zero();
    while ordinal < count {
        limits.consume(1, "rank.learn.frequency-order")?;
        limits.check_nat(&ordinal, "rank.learn.frequency-order")?;
        codec.unrank_node(&ordinal)?;
        ordinals.push(ordinal.clone());
        ordinal = ordinal.checked_add(&Nat::one());
    }
    ordinals.sort_by(|left, right| {
        model
            .score_ordinal(right)
            .cmp(&model.score_ordinal(left))
            .then_with(|| left.cmp(right))
    });
    crate::order::RankExactOrder::new(id, ordinals)
}

/// Publishes `model` and returns its card value.
///
/// Records the model's claims and builds a fallback table (id, version, digest,
/// frequency count, novelty archive) for the card under the learned-model kind.
pub fn rank_learned_model_card(cx: &mut Cx, model: &RankFrozenLearnedModel) -> KernelResult<Value> {
    publish_learned_model_claims(cx, model)?;
    let archive = model
        .novelty_archive()
        .iter()
        .map(|ordinal| nat_value(cx, ordinal))
        .collect::<KernelResult<Vec<_>>>()?;
    let frequency_count = nat_value(cx, &Nat::from(model.frequency_table().len()))?;
    let fallback = cx.factory().table(vec![
        (field("model-id"), cx.factory().symbol(model.id().clone())?),
        (
            field("version"),
            cx.factory().string(model.version().to_string())?,
        ),
        (
            field("digest"),
            cx.factory().string(model.digest().to_owned())?,
        ),
        (field("frequency-count"), frequency_count),
        (field("novelty-archive"), cx.factory().list(archive)?),
    ])?;
    card_for_ref_with_fallback(
        cx,
        Ref::Symbol(model.id().clone()),
        Some(fallback),
        Some(rank_learned_model_kind()),
    )
}

/// Inserts the model's identity, version, and digest facts into the store.
///
/// Each claim is written at most once, so repeated publication is idempotent.
pub fn publish_learned_model_claims(
    cx: &mut Cx,
    model: &RankFrozenLearnedModel,
) -> KernelResult<()> {
    let subject = Ref::Symbol(model.id().clone());
    insert_once(
        cx,
        subject.clone(),
        rank_learned_model_id_predicate(),
        Ref::Symbol(model.id().clone()),
    )?;
    let version = intern(cx, Datum::String(model.version().to_string()))?;
    insert_once(
        cx,
        subject.clone(),
        rank_learned_model_version_predicate(),
        version,
    )?;
    let digest = intern(cx, Datum::String(model.digest().to_owned()))?;
    insert_once(cx, subject, rank_learned_model_digest_predicate(), digest)
}

/// Returns the card kind symbol for learned models (`rank/learned-model`).
pub fn rank_learned_model_kind() -> Symbol {
    Symbol::qualified("rank", "learned-model")
}

/// Returns the predicate symbol for a learned model's identity claim.
pub fn rank_learned_model_id_predicate() -> Symbol {
    Symbol::qualified("rank", "learned-model-id")
}

/// Returns the predicate symbol for a learned model's version claim.
pub fn rank_learned_model_version_predicate() -> Symbol {
    Symbol::qualified("rank", "learned-model-version")
}

/// Returns the predicate symbol for a learned model's digest claim.
pub fn rank_learned_model_digest_predicate() -> Symbol {
    Symbol::qualified("rank", "learned-model-digest")
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

fn nat_value(cx: &mut Cx, value: &Nat) -> KernelResult<Value> {
    cx.factory()
        .number_literal(crate::bigint_number_domain(), value.to_string())
}

fn field(name: &str) -> Symbol {
    Symbol::new(name.to_owned())
}

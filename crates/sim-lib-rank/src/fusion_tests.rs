use crate::{
    EmbeddingStore, FusionLimits, RankDropReason, RankedList, ranked_list_from_embeddings,
    reciprocal_rank_fusion,
};

fn list(source: &str, weight: f64, keys: &[&str]) -> RankedList<String> {
    RankedList::new(
        source,
        weight,
        keys.iter().map(|key| (*key).to_owned()).collect(),
    )
    .unwrap()
}

#[test]
fn fusion_is_permutation_stable_and_scores_recompute_exactly() {
    let sources = vec![
        list("semantic", 1.0, &["a", "b", "c"]),
        list("lexical", 2.0, &["b", "a", "d"]),
        list("freshness", 0.5, &["d", "a", "c"]),
    ];
    let limits = FusionLimits::new(10, 20, 20).unwrap();
    let first = reciprocal_rank_fusion(sources.clone(), 60, limits).unwrap();
    let reversed = reciprocal_rank_fusion(sources.into_iter().rev().collect(), 60, limits).unwrap();
    assert_eq!(first, reversed);
    for item in &first.items {
        assert_eq!(
            item.score,
            item.contributions.iter().map(|part| part.value).sum()
        );
        assert!(item.contributions.iter().all(|part| {
            part.value == part.source_weight / (part.rrf_k + part.source_rank) as f64
        }));
    }
}

#[test]
fn duplicates_cutoffs_and_ties_have_exact_receipts() {
    let fusion = reciprocal_rank_fusion(
        vec![
            list("a", 1.0, &["same", "same", "tail"]),
            list("b", 1.0, &["other"]),
        ],
        1,
        FusionLimits::new(2, 2, 1).unwrap(),
    )
    .unwrap();
    assert_eq!(fusion.items[0].key, "other");
    assert_eq!(fusion.output_cutoff, 1);
    assert!(
        fusion
            .drops
            .iter()
            .any(|drop| drop.reason == RankDropReason::Duplicate)
    );
    assert!(
        fusion
            .drops
            .iter()
            .any(|drop| drop.reason == RankDropReason::SourceCutoff)
    );
    let receipt_round_trip = fusion.clone();
    assert_eq!(receipt_round_trip, fusion);
}

#[test]
fn aggregate_candidate_cutoff_is_recorded() {
    let fusion = reciprocal_rank_fusion(
        vec![list("only", 1.0, &["a", "b"])],
        60,
        FusionLimits::new(2, 1, 1).unwrap(),
    )
    .unwrap();
    assert_eq!(fusion.drops[0].reason, RankDropReason::CandidateCutoff);
}

#[test]
fn equal_scores_use_best_rank_then_key_and_record_ties() {
    let fusion = reciprocal_rank_fusion(
        vec![
            list("a", 1.0, &["z", "a"]),
            list("b", 1.0, &["a", "z"]),
            list("c", 1.0, &["b"]),
        ],
        10,
        FusionLimits::new(10, 10, 10).unwrap(),
    )
    .unwrap();
    assert_eq!(
        fusion
            .items
            .iter()
            .map(|item| item.key.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "z", "b"]
    );
    assert_eq!(fusion.tie_breaks[0].decided_by, "stable_key");
}

#[test]
fn missing_sources_and_zero_output_are_valid() {
    let empty =
        reciprocal_rank_fusion::<String>(vec![], 60, FusionLimits::new(1, 1, 0).unwrap()).unwrap();
    assert!(empty.items.is_empty());
    let cut = reciprocal_rank_fusion(
        vec![list("a", 1.0, &["x"])],
        60,
        FusionLimits::new(usize::MAX, usize::MAX, 0).unwrap(),
    )
    .unwrap();
    assert_eq!(cut.output_cutoff, 1);
}

#[test]
fn rejects_invalid_numerics_bounds_and_source_identity() {
    for weight in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1] {
        assert!(RankedList::<String>::new("source", weight, vec![]).is_err());
    }
    assert!(RankedList::<String>::new("", 1.0, vec![]).is_err());
    assert!(FusionLimits::new(0, 1, 1).is_err());
    assert!(FusionLimits::new(1, 0, 1).is_err());
    assert!(
        reciprocal_rank_fusion(
            vec![list("a", 1.0, &[])],
            0,
            FusionLimits::new(1, 1, 1).unwrap()
        )
        .is_err()
    );
    assert!(
        reciprocal_rank_fusion(
            vec![list("a", 1.0, &["x"])],
            usize::MAX,
            FusionLimits::new(1, 1, 1).unwrap()
        )
        .is_err()
    );
    assert!(
        reciprocal_rank_fusion(
            vec![list("a", 1.0, &[]), list("a", 1.0, &[])],
            1,
            FusionLimits::new(1, 1, 1).unwrap()
        )
        .is_err()
    );
}

#[test]
fn embedding_adapter_preserves_retrieval_order_with_caller_keys() {
    let store =
        EmbeddingStore::with_entries([("x", vec![1.0, 0.0]), ("y", vec![0.0, 1.0])]).unwrap();
    let ranked = ranked_list_from_embeddings("semantic", 0.75, &store, &[1.0, 0.0], 2, |id| {
        format!("doc:{id}")
    })
    .unwrap();
    assert_eq!(ranked.items, vec!["doc:x", "doc:y"]);
}

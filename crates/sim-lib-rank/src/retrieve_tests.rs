use sim_kernel::Symbol;

use crate::{
    EmbeddingIndex, EmbeddingStore, GenericNodeNeighborhood, RankBuilder, RankCodec, RankError,
    RankLimits, RankNeighborhood, RankNode, retrieve, retrieve_limited, retrieve_rank_neighborhood,
};

#[test]
fn retrieve_orders_by_cosine_and_breaks_ties_by_id() {
    let store = EmbeddingStore::with_entries([
        ("beta", vec![1.0, 0.0]),
        ("gamma", vec![0.0, 1.0]),
        ("alpha", vec![1.0, 0.0]),
    ])
    .unwrap();

    let neighbors = retrieve(&store, &[1.0, 0.0], 3).unwrap();

    assert_eq!(
        neighbors
            .iter()
            .map(|neighbor| neighbor.id.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta", "gamma"]
    );
    assert_eq!(neighbors[0].score, 1.0);
    assert_eq!(neighbors[1].score, 1.0);
    assert_eq!(neighbors[2].score, 0.0);
}

#[test]
fn retrieve_respects_top_k_limit() {
    let store = EmbeddingStore::with_entries([
        ("best", vec![1.0, 0.0]),
        ("next", vec![0.8, 0.2]),
        ("last", vec![0.0, 1.0]),
    ])
    .unwrap();

    let neighbors = retrieve(&store, &[1.0, 0.0], 2).unwrap();

    assert_eq!(neighbors.len(), 2);
    assert_eq!(neighbors[0].id, "best");
    assert_eq!(neighbors[1].id, "next");
}

#[test]
fn retrieve_uses_only_in_memory_store() {
    let mut store = EmbeddingStore::new();
    store.insert("episode-a", vec![1.0, 1.0]).unwrap();
    store.insert("episode-b", vec![1.0, -1.0]).unwrap();

    let first = retrieve(&store, &[1.0, 1.0], 1).unwrap();
    let second = retrieve(&store, &[1.0, 1.0], 1).unwrap();

    assert_eq!(first, second);
    assert_eq!(first[0].id, "episode-a");
}

#[test]
fn retrieve_accepts_trait_backed_indexes() {
    struct StaticIndex {
        entries: Vec<(&'static str, Vec<f32>)>,
    }

    impl EmbeddingIndex for StaticIndex {
        fn len(&self) -> usize {
            self.entries.len()
        }

        fn dimensions(&self) -> Option<usize> {
            self.entries.first().map(|(_, embedding)| embedding.len())
        }

        fn embedding(&self, id: &str) -> Option<&[f32]> {
            self.entries
                .iter()
                .find_map(|(entry_id, embedding)| (*entry_id == id).then_some(embedding.as_slice()))
        }

        fn ids(&self) -> Box<dyn Iterator<Item = &str> + '_> {
            Box::new(self.entries.iter().map(|(id, _)| *id))
        }
    }

    let index = StaticIndex {
        entries: vec![("b", vec![0.0, 1.0]), ("a", vec![1.0, 0.0])],
    };
    let mut limits = RankLimits::new(4, 64);

    let neighbors = retrieve_limited(&index, &[1.0, 0.0], 1, &mut limits).unwrap();

    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].id, "a");
}

#[test]
fn retrieve_limited_fails_before_unbounded_candidate_traversal() {
    let store = EmbeddingStore::with_entries([
        ("a", vec![1.0, 0.0]),
        ("b", vec![0.0, 1.0]),
        ("c", vec![0.5, 0.5]),
    ])
    .unwrap();
    let mut limits = RankLimits::new(1, 64);

    let error = retrieve_limited(&store, &[1.0, 0.0], 1, &mut limits).unwrap_err();

    assert_eq!(
        error,
        RankError::LimitExceeded {
            limit: "rank.retrieve.candidate",
            needed: 1,
            remaining: 0
        }
    );
}

#[test]
fn retrieve_rank_neighborhood_scores_rank_candidates_only() {
    let codec = bool_pair_codec();
    let neighborhood = GenericNodeNeighborhood::default();
    let start = codec
        .rank_node(&RankNode::Product(vec![
            RankNode::Bool(false),
            RankNode::Bool(false),
        ]))
        .unwrap();
    let rank_candidates = neighborhood
        .neighbors(&codec, &start, &mut RankLimits::default())
        .unwrap();
    assert!(!rank_candidates.is_empty());

    let mut expected_ids = rank_candidates
        .iter()
        .chain(std::iter::once(&start))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    expected_ids.sort();
    expected_ids.dedup();

    let mut store = EmbeddingStore::new();
    store.insert("outside", vec![1.0, 0.0]).unwrap();
    for id in &expected_ids {
        store.insert(id.clone(), vec![0.0, 1.0]).unwrap();
    }

    assert_eq!(retrieve(&store, &[1.0, 0.0], 1).unwrap()[0].id, "outside");

    let neighbors = retrieve_rank_neighborhood(
        &store,
        &[1.0, 0.0],
        &neighborhood,
        &codec,
        &start,
        10,
        &mut RankLimits::default(),
    )
    .unwrap();

    assert_eq!(
        neighbors
            .iter()
            .map(|neighbor| neighbor.id.clone())
            .collect::<Vec<_>>(),
        expected_ids
    );
}

#[test]
fn embedding_store_rejects_invalid_entries() {
    let mut store = EmbeddingStore::new();

    assert_invalid(store.insert("", vec![1.0]));
    assert_invalid(store.insert("zero", vec![0.0, 0.0]));
    assert_invalid(store.insert("nan", vec![f32::NAN]));

    store.insert("valid", vec![1.0, 0.0]).unwrap();
    assert_invalid(store.insert("wrong-dim", vec![1.0, 0.0, 0.0]));
}

#[test]
fn retrieve_rejects_invalid_queries() {
    let store = EmbeddingStore::with_entries([("valid", vec![1.0, 0.0])]).unwrap();

    assert_invalid(retrieve(&store, &[], 1));
    assert_invalid(retrieve(&store, &[0.0, 0.0], 1));
    assert_invalid(retrieve(&store, &[f32::INFINITY, 0.0], 1));
    assert_invalid(retrieve(&store, &[1.0], 1));
}

fn assert_invalid<T>(result: Result<T, RankError>) {
    assert!(matches!(result, Err(RankError::InvalidNode { .. })));
}

fn bool_pair_codec() -> crate::RankPrimitiveCodec {
    crate::RankPrimitiveCodec::new(
        RankBuilder::product(Symbol::qualified("rank-test", "bool-pair"))
            .field(Symbol::new("left"), RankBuilder::bool())
            .field(Symbol::new("right"), RankBuilder::bool())
            .build()
            .unwrap(),
    )
}

//! Nearest-neighbor retrieval over stored embeddings.
//!
//! Holds a keyed [`EmbeddingStore`] of fixed-dimension vectors and ranks them
//! against a query by cosine similarity, returning the top-k as
//! [`RetrievedNeighbor`] records.

use std::collections::{BTreeMap, BTreeSet};

use crate::{Nat, RankCodec, RankError, RankNeighborhood, RankResult, limits::RankLimits};

/// Read-only embedding index used by rank retrieval.
///
/// `EmbeddingStore` is the in-memory implementation. Persistent integrations
/// implement this trait over their Table/Dir-backed index without changing the
/// retrieval algorithm.
pub trait EmbeddingIndex {
    /// Returns the number of indexed embeddings.
    fn len(&self) -> usize;

    /// Returns `true` when no embeddings are indexed.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the fixed vector dimension, or `None` while the index is empty.
    fn dimensions(&self) -> Option<usize>;

    /// Returns the embedding stored under `id`, if present.
    fn embedding(&self, id: &str) -> Option<&[f32]>;

    /// Iterates ids in deterministic order.
    fn ids(&self) -> Box<dyn Iterator<Item = &str> + '_>;
}

/// Keyed collection of fixed-dimension embedding vectors to retrieve against.
///
/// Every stored vector shares the same dimension (fixed by the first insert)
/// and is addressed by a non-empty string id.
///
/// # Examples
///
/// ```
/// use sim_lib_rank::{EmbeddingStore, retrieve};
///
/// let store = EmbeddingStore::with_entries([
///     ("a", vec![1.0, 0.0]),
///     ("b", vec![0.0, 1.0]),
/// ])
/// .unwrap();
///
/// let hits = retrieve(&store, &[1.0, 0.1], 1).unwrap();
/// assert_eq!(hits[0].id, "a");
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingStore {
    dimensions: Option<usize>,
    entries: BTreeMap<String, Vec<f32>>,
}

impl EmbeddingStore {
    /// Creates an empty store with no fixed dimension yet.
    pub fn new() -> Self {
        Self {
            dimensions: None,
            entries: BTreeMap::new(),
        }
    }

    /// Builds a store from an iterator of `(id, embedding)` pairs.
    ///
    /// Inserts each pair in turn; fails if any id is empty or any embedding is
    /// invalid or dimension-mismatched.
    pub fn with_entries<I, S>(entries: I) -> RankResult<Self>
    where
        I: IntoIterator<Item = (S, Vec<f32>)>,
        S: Into<String>,
    {
        let mut store = Self::new();
        for (id, embedding) in entries {
            store.insert(id, embedding)?;
        }
        Ok(store)
    }

    /// Inserts or replaces the embedding for `id`, returning any prior vector.
    ///
    /// Rejects empty ids and embeddings that are empty, non-finite, zero-norm,
    /// or whose length does not match the store's fixed dimension. The first
    /// successful insert fixes the dimension for all later entries.
    pub fn insert<S>(&mut self, id: S, embedding: Vec<f32>) -> RankResult<Option<Vec<f32>>>
    where
        S: Into<String>,
    {
        let id = id.into();
        if id.is_empty() {
            return Err(invalid_retrieve("embedding id must not be empty"));
        }
        validate_embedding(&embedding, "embedding")?;
        match self.dimensions {
            Some(dimensions) if dimensions != embedding.len() => {
                return Err(invalid_retrieve(format!(
                    "embedding dimension {} does not match store dimension {dimensions}",
                    embedding.len()
                )));
            }
            Some(_) => {}
            None => self.dimensions = Some(embedding.len()),
        }
        Ok(self.entries.insert(id, embedding))
    }

    /// Returns the number of stored embeddings.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when no embeddings are stored.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the fixed vector dimension, or `None` while the store is empty.
    pub fn dimensions(&self) -> Option<usize> {
        self.dimensions
    }

    /// Returns the embedding stored under `id`, if present.
    pub fn embedding(&self, id: &str) -> Option<&[f32]> {
        self.entries.get(id).map(Vec::as_slice)
    }

    /// Iterates over `(id, embedding)` pairs in ascending id order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[f32])> {
        self.entries
            .iter()
            .map(|(id, embedding)| (id.as_str(), embedding.as_slice()))
    }
}

impl EmbeddingIndex for EmbeddingStore {
    fn len(&self) -> usize {
        EmbeddingStore::len(self)
    }

    fn dimensions(&self) -> Option<usize> {
        EmbeddingStore::dimensions(self)
    }

    fn embedding(&self, id: &str) -> Option<&[f32]> {
        EmbeddingStore::embedding(self, id)
    }

    fn ids(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(self.entries.keys().map(String::as_str))
    }
}

impl Default for EmbeddingStore {
    fn default() -> Self {
        Self::new()
    }
}

/// A single retrieval hit: a stored id and its similarity to the query.
#[derive(Clone, Debug, PartialEq)]
pub struct RetrievedNeighbor {
    /// Id of the matched embedding in the [`EmbeddingStore`].
    pub id: String,
    /// Cosine similarity between the query and this embedding.
    pub score: f32,
}

/// Retrieves the top-k embeddings in `store` most similar to `query`.
///
/// Scores every stored embedding by cosine similarity and returns the highest
/// `k`, ordered by descending score with ids breaking ties.
pub fn retrieve(
    store: &impl EmbeddingIndex,
    query: &[f32],
    k: usize,
) -> RankResult<Vec<RetrievedNeighbor>> {
    let mut limits = RankLimits::default();
    retrieve_limited(store, query, k, &mut limits)
}

/// Retrieves the top-k embeddings with an explicit traversal budget.
pub fn retrieve_limited(
    store: &impl EmbeddingIndex,
    query: &[f32],
    k: usize,
    limits: &mut RankLimits,
) -> RankResult<Vec<RetrievedNeighbor>> {
    retrieve_ids_limited(store, query, store.ids(), k, limits)
}

/// Retrieves top-k embeddings restricted to a rank neighborhood of `ordinal`.
///
/// Asks `neighborhood` for the ordinals adjacent to `ordinal` (plus `ordinal`
/// itself), maps each to its string id, and ranks only those embeddings by
/// cosine similarity to `query`.
pub fn retrieve_rank_neighborhood<N>(
    store: &impl EmbeddingIndex,
    query: &[f32],
    neighborhood: &N,
    codec: &dyn RankCodec,
    ordinal: &Nat,
    k: usize,
    limits: &mut RankLimits,
) -> RankResult<Vec<RetrievedNeighbor>>
where
    N: RankNeighborhood + ?Sized,
{
    let mut ordinals = neighborhood.neighbors(codec, ordinal, limits)?;
    ordinals.push(ordinal.clone());
    retrieve_ids_limited(
        store,
        query,
        ordinals.iter().map(|ordinal| ordinal.to_string()),
        k,
        limits,
    )
}

/// Retrieves top-k embeddings drawn from an explicit candidate id set.
///
/// Scores each distinct id present in `store` against `query` by cosine
/// similarity, skipping duplicates and unknown ids, then returns the best `k`
/// by descending score with ids breaking ties.
pub fn retrieve_ids<I, S>(
    store: &impl EmbeddingIndex,
    query: &[f32],
    ids: I,
    k: usize,
) -> RankResult<Vec<RetrievedNeighbor>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut limits = RankLimits::default();
    retrieve_ids_limited(store, query, ids, k, &mut limits)
}

/// Retrieves top-k embeddings with an explicit traversal budget.
pub fn retrieve_ids_limited<I, S>(
    store: &impl EmbeddingIndex,
    query: &[f32],
    ids: I,
    k: usize,
    limits: &mut RankLimits,
) -> RankResult<Vec<RetrievedNeighbor>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    validate_query(store, query)?;
    limits.check_count(k, "rank.retrieve.k")?;
    let mut seen = BTreeSet::new();
    let mut neighbors = Vec::new();
    for id in ids {
        limits.consume(1, "rank.retrieve.candidate")?;
        let id = id.as_ref();
        if !seen.insert(id.to_owned()) {
            continue;
        }
        let Some(embedding) = store.embedding(id) else {
            continue;
        };
        neighbors.push(RetrievedNeighbor {
            id: id.to_owned(),
            score: cosine(query, embedding),
        });
    }
    neighbors.sort_by(compare_neighbors);
    neighbors.truncate(k);
    Ok(neighbors)
}

fn compare_neighbors(left: &RetrievedNeighbor, right: &RetrievedNeighbor) -> std::cmp::Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.id.cmp(&right.id))
}

fn validate_query(store: &impl EmbeddingIndex, query: &[f32]) -> RankResult<()> {
    validate_embedding(query, "query")?;
    if let Some(dimensions) = store.dimensions()
        && dimensions != query.len()
    {
        return Err(invalid_retrieve(format!(
            "query dimension {} does not match store dimension {dimensions}",
            query.len()
        )));
    }
    Ok(())
}

fn validate_embedding(embedding: &[f32], label: &'static str) -> RankResult<()> {
    if embedding.is_empty() {
        return Err(invalid_retrieve(format!("{label} must not be empty")));
    }
    if embedding.iter().any(|value| !value.is_finite()) {
        return Err(invalid_retrieve(format!(
            "{label} contains a non-finite coordinate"
        )));
    }
    if squared_norm(embedding) == 0.0 {
        return Err(invalid_retrieve(format!("{label} norm must be nonzero")));
    }
    Ok(())
}

fn cosine(left: &[f32], right: &[f32]) -> f32 {
    dot(left, right) / (squared_norm(left).sqrt() * squared_norm(right).sqrt())
}

fn dot(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .zip(right.iter())
        .map(|(left, right)| left * right)
        .sum()
}

fn squared_norm(values: &[f32]) -> f32 {
    values.iter().map(|value| value * value).sum()
}

fn invalid_retrieve(message: impl Into<String>) -> RankError {
    RankError::InvalidNode {
        message: message.into(),
    }
}

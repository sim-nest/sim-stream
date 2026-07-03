//! Rank codecs for finite collections: lists, set permutations, and maps.
//!
//! Each codec ranks and unranks a collection shape into a dense ordinal space
//! so collections can be enumerated and searched like any other ranked space.

use std::collections::BTreeSet;

use sim_kernel::Symbol;

use crate::{
    GroupCodec, Nat, RankBuilder, RankCodec, RankError, RankGrade, RankGrammar, RankGroupCodec,
    RankNode, RankPrimitiveCodec, RankResult, RankVersion, order::nat_to_index,
};

/// Rank codec for variable-length lists of a single element grammar.
#[derive(Clone, Debug)]
pub struct RankListCodec {
    id: Symbol,
    codec: RankGroupCodec,
}

/// Rank codec for permutations of a fixed finite set of item symbols.
#[derive(Clone, Debug)]
pub struct RankSetPermutationCodec {
    item_space: Symbol,
    items: Vec<Symbol>,
}

/// Rank codec for finite maps from a key grammar to a value grammar.
#[derive(Clone, Debug)]
pub struct RankFiniteMapCodec {
    id: Symbol,
    grammar: RankGrammar,
    codec: RankPrimitiveCodec,
}

impl RankListCodec {
    /// Builds a list codec over `element` with the given length bounds.
    pub fn new(
        id: Symbol,
        element: RankGrammar,
        min_len: u64,
        max_len: Option<u64>,
    ) -> RankResult<Self> {
        Ok(Self {
            codec: RankGroupCodec::new(rank_list_grammar(id.clone(), element, min_len, max_len)?),
            id,
        })
    }

    /// Returns the list grammar this codec ranks against.
    pub fn grammar(&self) -> &RankGrammar {
        self.codec.grammar()
    }
}

impl RankCodec for RankListCodec {
    fn id(&self) -> Symbol {
        Symbol::qualified("rank-codec/list", self.id.as_qualified_str())
    }

    fn version(&self) -> RankVersion {
        self.codec.version()
    }

    fn count(&self) -> Option<Nat> {
        self.codec.count()
    }

    fn r_ok(&self, r: &Nat) -> bool {
        self.codec.r_ok(r)
    }

    fn rank_node(&self, node: &RankNode) -> RankResult<Nat> {
        self.codec.rank_node(node)
    }

    fn unrank_node(&self, r: &Nat) -> RankResult<RankNode> {
        self.codec.unrank_node(r)
    }
}

impl GroupCodec for RankListCodec {
    fn group_count(&self) -> Option<Nat> {
        self.codec.group_count()
    }

    fn group_of_node(&self, node: &RankNode) -> RankResult<RankGrade> {
        self.codec.group_of_node(node)
    }

    fn group_count_at(&self, key: RankGrade) -> RankResult<Nat> {
        self.codec.group_count_at(key)
    }

    fn rank_in_group(&self, key: RankGrade, node: &RankNode) -> RankResult<Nat> {
        self.codec.rank_in_group(key, node)
    }

    fn unrank_in_group(&self, key: RankGrade, r: &Nat) -> RankResult<RankNode> {
        self.codec.unrank_in_group(key, r)
    }
}

impl RankSetPermutationCodec {
    /// Builds a permutation codec over the distinct `items` in `item_space`.
    ///
    /// Fails if `items` contains a duplicate symbol.
    pub fn new(item_space: Symbol, items: impl IntoIterator<Item = Symbol>) -> RankResult<Self> {
        Ok(Self {
            item_space,
            items: unique_symbols(items)?,
        })
    }

    /// Returns the symbol of the item space being permuted.
    pub fn item_space(&self) -> &Symbol {
        &self.item_space
    }

    /// Returns the ordered item symbols defining the identity permutation.
    pub fn items(&self) -> &[Symbol] {
        &self.items
    }
}

impl RankCodec for RankSetPermutationCodec {
    fn id(&self) -> Symbol {
        Symbol::qualified(
            "rank-codec/set-permutation",
            self.item_space.as_qualified_str(),
        )
    }

    fn version(&self) -> RankVersion {
        RankVersion::v1()
    }

    fn count(&self) -> Option<Nat> {
        Some(factorial_nat(self.items.len()))
    }

    fn r_ok(&self, r: &Nat) -> bool {
        r < &factorial_nat(self.items.len())
    }

    fn rank_node(&self, node: &RankNode) -> RankResult<Nat> {
        let RankNode::List(values) = node else {
            return Err(RankError::NodeGrammarMismatch {
                expected: "list",
                found: node.kind_name(),
            });
        };
        let permutation = self.permutation_indexes(values)?;
        rank_permutation(&permutation)
    }

    fn unrank_node(&self, r: &Nat) -> RankResult<RankNode> {
        let indexes = unrank_permutation(self.items.len(), r)?;
        Ok(RankNode::List(
            indexes
                .into_iter()
                .map(|index| RankNode::Enum {
                    id: self.item_space.clone(),
                    index: Nat::from(index),
                })
                .collect(),
        ))
    }
}

impl RankSetPermutationCodec {
    fn permutation_indexes(&self, values: &[RankNode]) -> RankResult<Vec<usize>> {
        if values.len() != self.items.len() {
            return Err(invalid_node(
                "set permutation length does not match item count",
            ));
        }
        let mut indexes = Vec::with_capacity(values.len());
        let mut seen = BTreeSet::new();
        for value in values {
            let RankNode::Enum { id, index } = value else {
                return Err(RankError::NodeGrammarMismatch {
                    expected: "enum",
                    found: value.kind_name(),
                });
            };
            if id != &self.item_space {
                return Err(invalid_node("set permutation item space does not match"));
            }
            let index = nat_to_index(index, self.items.len(), "set permutation item")?;
            if !seen.insert(index) {
                return Err(invalid_node("set permutation contains duplicate item"));
            }
            indexes.push(index);
        }
        Ok(indexes)
    }
}

impl RankFiniteMapCodec {
    /// Builds a finite-map codec from `key` to `value` bounded by `max_len`.
    ///
    /// Fails unless the resulting map space is finite (has a known count).
    pub fn new(
        id: Symbol,
        key: RankGrammar,
        value: RankGrammar,
        max_len: Option<u64>,
    ) -> RankResult<Self> {
        let grammar = rank_map_grammar(id.clone(), key, value, max_len)?;
        let codec = RankPrimitiveCodec::new(grammar.clone());
        if codec.count().is_none() {
            return Err(RankError::UnsupportedCodec { kind: "finite-map" });
        }
        Ok(Self { id, grammar, codec })
    }

    /// Returns the map grammar this codec ranks against.
    pub fn grammar(&self) -> &RankGrammar {
        &self.grammar
    }
}

impl RankCodec for RankFiniteMapCodec {
    fn id(&self) -> Symbol {
        Symbol::qualified("rank-codec/map", self.id.as_qualified_str())
    }

    fn version(&self) -> RankVersion {
        self.codec.version()
    }

    fn count(&self) -> Option<Nat> {
        self.codec.count()
    }

    fn r_ok(&self, r: &Nat) -> bool {
        self.codec.r_ok(r)
    }

    fn rank_node(&self, node: &RankNode) -> RankResult<Nat> {
        self.codec.rank_node(node)
    }

    fn unrank_node(&self, r: &Nat) -> RankResult<RankNode> {
        self.codec.unrank_node(r)
    }
}

/// Builds the list grammar for `element` with the given length bounds.
pub fn rank_list_grammar(
    id: Symbol,
    element: RankGrammar,
    min_len: u64,
    max_len: Option<u64>,
) -> RankResult<RankGrammar> {
    RankBuilder::list(id, element, min_len, max_len)
}

/// Builds the finite-map grammar from `key` to `value` bounded by `max_len`.
pub fn rank_map_grammar(
    id: Symbol,
    key: RankGrammar,
    value: RankGrammar,
    max_len: Option<u64>,
) -> RankResult<RankGrammar> {
    RankBuilder::map(id, key, value, max_len)
}

fn unique_symbols(items: impl IntoIterator<Item = Symbol>) -> RankResult<Vec<Symbol>> {
    let items = items.into_iter().collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    for item in &items {
        if !seen.insert(item) {
            return Err(invalid_node(
                "finite set permutation contains duplicate item",
            ));
        }
    }
    Ok(items)
}

fn rank_permutation(permutation: &[usize]) -> RankResult<Nat> {
    let mut available = (0..permutation.len()).collect::<Vec<_>>();
    let mut rank = Nat::zero();
    for (position, value) in permutation.iter().enumerate() {
        let Some(index) = available.iter().position(|candidate| candidate == value) else {
            return Err(invalid_node("permutation contains duplicate item"));
        };
        available.remove(index);
        let factor = factorial_nat(permutation.len() - position - 1);
        rank = rank.checked_add(&Nat::from(index).checked_mul(&factor));
    }
    Ok(rank)
}

fn unrank_permutation(len: usize, ordinal: &Nat) -> RankResult<Vec<usize>> {
    let count = factorial_nat(len);
    if ordinal >= &count {
        return Err(RankError::OrdinalOutOfRange {
            ordinal: ordinal.to_string(),
            count: count.to_string(),
        });
    }
    let mut remaining = ordinal.clone();
    let mut available = (0..len).collect::<Vec<_>>();
    let mut permutation = Vec::with_capacity(len);
    for position in 0..len {
        let factor = factorial_nat(len - position - 1);
        let (index, next_remaining) = remaining.div_mod(&factor)?;
        let index = nat_to_index(&index, available.len(), "set permutation rank digit")?;
        permutation.push(available.remove(index));
        remaining = next_remaining;
    }
    Ok(permutation)
}

fn factorial_nat(len: usize) -> Nat {
    let mut value = Nat::one();
    for next in 2..=len {
        value = value.checked_mul(&Nat::from(next));
    }
    value
}

fn invalid_node(message: &'static str) -> RankError {
    RankError::InvalidNode {
        message: message.to_owned(),
    }
}

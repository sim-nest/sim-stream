//! Meta rank space over a catalogue of grammars.
//!
//! Ranks a graded catalogue of `RankGrammar` descriptions to ordinals, where
//! each ordinal materializes into a concrete codec or space. This makes the set
//! of small ranked spaces itself a ranked, enumerable space.

use std::sync::Arc;

use sim_kernel::Symbol;

use crate::{
    Nat, RankBuilder, RankCodec, RankError, RankGrammar, RankNode, RankPrimitiveCodec, RankResult,
    RankSpace, RankVersion, order::nat_to_index,
};

/// Bound on which graded grammar entries the meta space includes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankMetaSpec {
    max_grade: u64,
}

impl RankMetaSpec {
    /// Builds a spec admitting catalogue entries up to and including
    /// `max_grade`.
    pub fn new(max_grade: u64) -> Self {
        Self { max_grade }
    }

    /// Returns the maximum grade of grammar entries included in the space.
    pub fn max_grade(&self) -> u64 {
        self.max_grade
    }
}

/// Finite rank codec over a graded catalogue of grammar descriptions.
///
/// Each ordinal names one `RankGrammar` entry; `materialize_*` turns an entry
/// into a usable codec or space.
#[derive(Clone, Debug)]
pub struct RankMetaCodec {
    spec: RankMetaSpec,
    enum_id: Symbol,
    grammar: RankGrammar,
    entries: Vec<RankMetaEntry>,
}

#[derive(Clone, Debug)]
struct RankMetaEntry {
    item: Symbol,
    grammar: RankGrammar,
}

impl RankMetaCodec {
    /// Builds the codec by enumerating all catalogue entries permitted by
    /// `spec` and forming the enclosing enumeration grammar.
    pub fn new(spec: RankMetaSpec) -> RankResult<Self> {
        let entries = build_entries(spec.max_grade)?;
        let enum_id = rank_meta_enum_symbol(spec.max_grade);
        let grammar = RankBuilder::enumeration(
            enum_id.clone(),
            entries.iter().map(|entry| entry.item.clone()),
        )?;
        Ok(Self {
            spec,
            enum_id,
            grammar,
            entries,
        })
    }

    /// Returns the bounding spec for this codec.
    pub fn spec(&self) -> &RankMetaSpec {
        &self.spec
    }

    /// Returns the enclosing enumeration grammar whose members are the
    /// catalogue entries.
    pub fn meta_grammar(&self) -> &RankGrammar {
        &self.grammar
    }

    /// Returns the grammar described by the catalogue entry for `node`.
    pub fn grammar_for_node(&self, node: &RankNode) -> RankResult<&RankGrammar> {
        let ordinal = self.rank_node(node)?;
        self.grammar_for_ordinal(&ordinal)
    }

    /// Returns the grammar described by the catalogue entry at `ordinal`.
    pub fn grammar_for_ordinal(&self, ordinal: &Nat) -> RankResult<&RankGrammar> {
        let index = nat_to_index(ordinal, self.entries.len(), "rank meta ordinal")?;
        Ok(&self.entries[index].grammar)
    }

    /// Materializes the catalogue entry for `node` into a concrete primitive
    /// codec.
    pub fn materialize_node(&self, node: &RankNode) -> RankResult<RankPrimitiveCodec> {
        Ok(RankPrimitiveCodec::new(
            self.grammar_for_node(node)?.clone(),
        ))
    }

    /// Materializes the catalogue entry at `ordinal` into a concrete primitive
    /// codec.
    pub fn materialize_ordinal(&self, ordinal: &Nat) -> RankResult<RankPrimitiveCodec> {
        Ok(RankPrimitiveCodec::new(
            self.grammar_for_ordinal(ordinal)?.clone(),
        ))
    }

    /// Materializes the catalogue entry at `ordinal` into a full rank space
    /// under the given identifier.
    pub fn materialize_space(&self, ordinal: &Nat, id: Symbol) -> RankResult<RankSpace> {
        let grammar = self.grammar_for_ordinal(ordinal)?.clone();
        Ok(RankSpace::with_codec(
            id,
            grammar.clone(),
            Arc::new(RankPrimitiveCodec::new(grammar)),
        ))
    }
}

impl RankCodec for RankMetaCodec {
    fn id(&self) -> Symbol {
        Symbol::qualified("rank-meta", "codec")
    }

    fn version(&self) -> RankVersion {
        RankVersion::v1()
    }

    fn count(&self) -> Option<Nat> {
        Some(Nat::from(self.entries.len()))
    }

    fn r_ok(&self, r: &Nat) -> bool {
        r < &Nat::from(self.entries.len())
    }

    fn rank_node(&self, node: &RankNode) -> RankResult<Nat> {
        let RankNode::Enum { id, index } = node else {
            return Err(RankError::NodeGrammarMismatch {
                expected: "enum",
                found: node.kind_name(),
            });
        };
        if id != &self.enum_id {
            return Err(RankError::NodeGrammarMismatch {
                expected: "enum",
                found: "enum",
            });
        }
        nat_to_index(index, self.entries.len(), "rank meta enum index")?;
        Ok(index.clone())
    }

    fn unrank_node(&self, r: &Nat) -> RankResult<RankNode> {
        nat_to_index(r, self.entries.len(), "rank meta ordinal")?;
        Ok(RankNode::Enum {
            id: self.enum_id.clone(),
            index: r.clone(),
        })
    }
}

/// Builds the meta rank space whose ordinals enumerate the graded grammar
/// catalogue permitted by `spec`.
pub fn rank_meta_space(spec: RankMetaSpec) -> RankResult<RankSpace> {
    let codec = RankMetaCodec::new(spec)?;
    let grammar = codec.meta_grammar().clone();
    Ok(RankSpace::with_codec(
        Symbol::qualified("rank", "meta-space"),
        grammar,
        Arc::new(codec),
    ))
}

/// Returns the enumeration symbol identifying the meta grammar for `max_grade`.
pub fn rank_meta_enum_symbol(max_grade: u64) -> Symbol {
    Symbol::qualified("rank-meta", format!("grammar-g{max_grade}"))
}

fn build_entries(max_grade: u64) -> RankResult<Vec<RankMetaEntry>> {
    let mut entries = Vec::new();
    push_entry(&mut entries, max_grade, 0, "unit", RankBuilder::unit());
    push_entry(&mut entries, max_grade, 1, "bool", RankBuilder::bool());
    push_entry(&mut entries, max_grade, 1, "enum-2", enum_grammar(2)?);
    push_entry(
        &mut entries,
        max_grade,
        2,
        "bool-pair",
        RankBuilder::product(meta_symbol("bool-pair"))
            .field(Symbol::new("left"), RankBuilder::bool())
            .field(Symbol::new("right"), RankBuilder::bool())
            .build()?,
    );
    push_entry(
        &mut entries,
        max_grade,
        2,
        "maybe-bool",
        RankBuilder::sum(meta_symbol("maybe-bool"))
            .alt(Symbol::new("none"), RankBuilder::unit())
            .alt(Symbol::new("some"), RankBuilder::bool())
            .build()?,
    );
    push_entry(
        &mut entries,
        max_grade,
        2,
        "bool-list-2",
        RankBuilder::list(meta_symbol("bool-list-2"), RankBuilder::bool(), 0, Some(2))?,
    );
    push_entry(
        &mut entries,
        max_grade,
        3,
        "enum-3-set-2",
        RankBuilder::set(meta_symbol("enum-3-set-2"), enum_grammar(3)?, Some(2))?,
    );
    push_entry(
        &mut entries,
        max_grade,
        3,
        "bool-enum-3-map",
        RankBuilder::map(
            meta_symbol("bool-enum-3-map"),
            RankBuilder::bool(),
            enum_grammar(3)?,
            Some(2),
        )?,
    );
    push_entry(
        &mut entries,
        max_grade,
        4,
        "nested-bool-list",
        RankBuilder::list(
            meta_symbol("nested-bool-list"),
            RankBuilder::list(
                meta_symbol("nested-bool-list-element"),
                RankBuilder::bool(),
                0,
                Some(2),
            )?,
            0,
            Some(2),
        )?,
    );
    push_entry(
        &mut entries,
        max_grade,
        4,
        "enum-product-sum",
        RankBuilder::sum(meta_symbol("enum-product-sum"))
            .alt(Symbol::new("enum"), enum_grammar(2)?)
            .alt(
                Symbol::new("pair"),
                RankBuilder::product(meta_symbol("enum-product-sum-pair"))
                    .field(Symbol::new("flag"), RankBuilder::bool())
                    .field(Symbol::new("color"), enum_grammar(3)?)
                    .build()?,
            )
            .build()?,
    );
    Ok(entries)
}

fn push_entry(
    entries: &mut Vec<RankMetaEntry>,
    max_grade: u64,
    grade: u64,
    name: &str,
    grammar: RankGrammar,
) {
    if grade <= max_grade {
        entries.push(RankMetaEntry {
            item: meta_symbol(name),
            grammar,
        });
    }
}

fn enum_grammar(count: u64) -> RankResult<RankGrammar> {
    RankBuilder::enumeration(
        meta_symbol(&format!("enum-{count}")),
        (0..count).map(|index| Symbol::new(format!("item-{index}"))),
    )
}

fn meta_symbol(name: &str) -> Symbol {
    Symbol::qualified("rank-meta", name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_space_round_trips_descriptions() {
        let codec = RankMetaCodec::new(RankMetaSpec::new(4)).unwrap();
        let count = codec.count().unwrap();
        for ordinal in 0..count.to_decimal_string().parse::<u64>().unwrap() {
            let ordinal = Nat::from(ordinal);
            let node = codec.unrank_node(&ordinal).unwrap();
            assert_eq!(codec.rank_node(&node).unwrap(), ordinal);
        }
    }

    #[test]
    fn meta_space_enumerates_grammars_that_materialize_into_usable_spaces() {
        let codec = RankMetaCodec::new(RankMetaSpec::new(4)).unwrap();
        let count = codec.count().unwrap();
        assert!(count > Nat::from(5_u64));

        for ordinal in 0..count.to_decimal_string().parse::<u64>().unwrap() {
            let ordinal = Nat::from(ordinal);
            let materialized = codec.materialize_ordinal(&ordinal).unwrap();
            let materialized_count = materialized.count().unwrap();
            assert!(materialized_count > Nat::zero());
            let node = materialized.unrank_node(&Nat::zero()).unwrap();
            assert_eq!(materialized.rank_node(&node).unwrap(), Nat::zero());
        }
    }

    #[test]
    fn materialized_meta_space_is_a_rank_space() {
        let space = rank_meta_space(RankMetaSpec::new(2)).unwrap();
        let node = space.codec().unrank_node(&Nat::from(1_u64)).unwrap();
        assert_eq!(space.codec().rank_node(&node).unwrap(), Nat::from(1_u64));

        let meta_codec = RankMetaCodec::new(RankMetaSpec::new(2)).unwrap();
        let materialized = meta_codec
            .materialize_space(
                &Nat::from(1_u64),
                Symbol::qualified("rank-test", "materialized"),
            )
            .unwrap();
        let value = materialized.codec().unrank_node(&Nat::zero()).unwrap();
        assert_eq!(materialized.codec().rank_node(&value).unwrap(), Nat::zero());
    }
}

//! Builders that compose `RankGrammar` shapes for rankable spaces.
//!
//! Provides the fluent entry points -- primitive shapes, references, and the
//! sum/product builders -- used to describe the structure of a `RankSpace`
//! before its nodes are ranked to ordinals.

use std::collections::BTreeSet;

use sim_kernel::Symbol;

use crate::{
    error::{RankError, RankResult},
    grammar::{RankAlt, RankField, RankGrammar},
};

/// Factory of `RankGrammar` shapes for building rankable spaces.
pub struct RankBuilder;

impl RankBuilder {
    /// Returns the empty grammar, a space with no inhabitants.
    pub fn empty() -> RankGrammar {
        RankGrammar::Empty
    }

    /// Returns the unit grammar, a space with exactly one node.
    pub fn unit() -> RankGrammar {
        RankGrammar::Unit
    }

    /// Returns the natural-number grammar, ranking ordinals to themselves.
    pub fn nat() -> RankGrammar {
        RankGrammar::Nat
    }

    /// Returns the integer grammar over the full signed range.
    pub fn int() -> RankGrammar {
        RankGrammar::Int
    }

    /// Returns the boolean grammar with two nodes.
    pub fn bool() -> RankGrammar {
        RankGrammar::Bool
    }

    /// Builds a finite enumeration grammar over `items`, keyed by `id`.
    ///
    /// Rejects an empty item list and duplicate item symbols.
    pub fn enumeration(
        id: Symbol,
        items: impl IntoIterator<Item = Symbol>,
    ) -> RankResult<RankGrammar> {
        let items = items.into_iter().collect::<Vec<_>>();
        reject_empty("enum", &id, items.is_empty())?;
        reject_duplicate_symbols("enum", &id, items.iter())?;
        Ok(RankGrammar::Enum { id, items })
    }

    /// Builds a reference grammar pointing at another named space `id`.
    pub fn reference(id: Symbol) -> RankGrammar {
        RankGrammar::Ref { id }
    }

    /// Builds a recursive self-reference grammar to the space named `id`.
    pub fn recursive_ref(id: Symbol) -> RankGrammar {
        RankGrammar::RecursiveRef { id }
    }

    /// Starts a sum (tagged-union) grammar builder keyed by `id`.
    pub fn sum(id: Symbol) -> RankSumBuilder {
        RankSumBuilder {
            id,
            alts: Vec::new(),
        }
    }

    /// Starts a product (record) grammar builder keyed by `id`.
    pub fn product(id: Symbol) -> RankProductBuilder {
        RankProductBuilder {
            id,
            fields: Vec::new(),
        }
    }

    /// Builds a list grammar over `element` with length bounds.
    ///
    /// Rejects bounds where `min_len` exceeds `max_len`.
    pub fn list(
        id: Symbol,
        element: RankGrammar,
        min_len: u64,
        max_len: Option<u64>,
    ) -> RankResult<RankGrammar> {
        validate_len_bounds(&id, min_len, max_len)?;
        Ok(RankGrammar::List {
            id,
            element: Box::new(element),
            min_len,
            max_len,
        })
    }

    /// Builds a set grammar over distinct `element` nodes with an optional cap.
    pub fn set(id: Symbol, element: RankGrammar, max_len: Option<u64>) -> RankResult<RankGrammar> {
        Ok(RankGrammar::Set {
            id,
            element: Box::new(element),
            max_len,
        })
    }

    /// Builds a map grammar from `key` nodes to `value` nodes with an optional cap.
    pub fn map(
        id: Symbol,
        key: RankGrammar,
        value: RankGrammar,
        max_len: Option<u64>,
    ) -> RankResult<RankGrammar> {
        Ok(RankGrammar::Map {
            id,
            key: Box::new(key),
            value: Box::new(value),
            max_len,
        })
    }

    /// Builds a guarded grammar wrapping `inner` with a `predicate` filter.
    pub fn guard(id: Symbol, inner: RankGrammar, predicate: Symbol) -> RankGrammar {
        RankGrammar::Guard {
            id,
            inner: Box::new(inner),
            predicate,
        }
    }

    /// Validates and returns `grammar` as a recursive space named `id`.
    pub fn recursive_space(id: Symbol, grammar: RankGrammar) -> RankResult<RankGrammar> {
        RankGrammar::validate_recursive_space(&id, &grammar)?;
        Ok(grammar)
    }
}

/// Accumulating builder for a sum (tagged-union) grammar.
pub struct RankSumBuilder {
    id: Symbol,
    alts: Vec<RankAlt>,
}

impl RankSumBuilder {
    /// Appends an alternative `grammar` tagged `id` with zero grade cost.
    pub fn alt(self, id: Symbol, grammar: RankGrammar) -> Self {
        self.alt_with_cost(id, 0, grammar)
    }

    /// Appends an alternative `grammar` tagged `id` with an explicit `grade_cost`.
    pub fn alt_with_cost(mut self, id: Symbol, grade_cost: u64, grammar: RankGrammar) -> Self {
        self.alts.push(RankAlt::new(id, grammar, grade_cost));
        self
    }

    /// Finalizes the sum grammar, rejecting empty or duplicate alternatives.
    pub fn build(self) -> RankResult<RankGrammar> {
        reject_empty("sum", &self.id, self.alts.is_empty())?;
        reject_duplicate_symbols("sum", &self.id, self.alts.iter().map(|alt| &alt.id))?;
        Ok(RankGrammar::Sum {
            id: self.id,
            alts: self.alts,
        })
    }

    /// Finalizes the sum grammar as a recursive space named by its id.
    pub fn build_recursive(self) -> RankResult<RankGrammar> {
        let id = self.id.clone();
        RankBuilder::recursive_space(id, self.build()?)
    }
}

/// Accumulating builder for a product (record) grammar.
pub struct RankProductBuilder {
    id: Symbol,
    fields: Vec<RankField>,
}

impl RankProductBuilder {
    /// Appends a field `grammar` named `id` with zero grade cost.
    pub fn field(self, id: Symbol, grammar: RankGrammar) -> Self {
        self.field_with_cost(id, 0, grammar)
    }

    /// Appends a field `grammar` named `id` with an explicit `grade_cost`.
    pub fn field_with_cost(mut self, id: Symbol, grade_cost: u64, grammar: RankGrammar) -> Self {
        self.fields.push(RankField::new(id, grammar, grade_cost));
        self
    }

    /// Finalizes the product grammar, rejecting duplicate field names.
    pub fn build(self) -> RankResult<RankGrammar> {
        reject_duplicate_symbols(
            "product",
            &self.id,
            self.fields.iter().map(|field| &field.id),
        )?;
        Ok(RankGrammar::Product {
            id: self.id,
            fields: self.fields,
        })
    }
}

fn validate_len_bounds(id: &Symbol, min_len: u64, max_len: Option<u64>) -> RankResult<()> {
    if let Some(max_len) = max_len
        && min_len > max_len
    {
        return Err(RankError::InvalidLengthBounds {
            id: id.clone(),
            min_len,
            max_len,
        });
    }
    Ok(())
}

fn reject_empty(kind: &'static str, id: &Symbol, empty: bool) -> RankResult<()> {
    if empty {
        return Err(RankError::EmptyGrammar {
            kind,
            id: id.clone(),
        });
    }
    Ok(())
}

fn reject_duplicate_symbols<'a>(
    kind: &'static str,
    id: &Symbol,
    symbols: impl IntoIterator<Item = &'a Symbol>,
) -> RankResult<()> {
    let mut seen = BTreeSet::new();
    for symbol in symbols {
        if !seen.insert(symbol) {
            return Err(RankError::DuplicateGrammarSymbol {
                kind,
                id: id.clone(),
                symbol: symbol.clone(),
            });
        }
    }
    Ok(())
}

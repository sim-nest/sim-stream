//! Grammar of rankable spaces.
//!
//! Defines [`RankGrammar`], the recursive description of a structured space
//! whose values can be enumerated and ranked to natural-number ordinals, plus
//! the alternative and field shapes ([`RankAlt`], [`RankField`]) that carry
//! per-branch grade costs.

use sim_kernel::{ContentId, Datum, NumberLiteral, Result as KernelResult, Symbol};

use crate::{
    error::{RankError, RankResult},
    nat::bigint_number_domain,
};

/// Recursive description of a rankable space.
///
/// Each variant names a way to build the set of values a space contains; the
/// grade machinery uses this tree to count values per grade and to rank a value
/// to its natural-number ordinal.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RankGrammar {
    /// The empty space, containing no values.
    Empty,
    /// The unit space, containing exactly one value.
    Unit,
    /// The space of natural numbers (unbounded non-negative integers).
    Nat,
    /// The space of integers (signed, unbounded).
    Int,
    /// The two-element boolean space.
    Bool,
    /// A finite enumeration of named items.
    Enum {
        /// Identifier of this enumeration space.
        id: Symbol,
        /// The ordered enumeration items; ordinal is the item index.
        items: Vec<Symbol>,
    },
    /// A reference to another named space, ranked through its own ordinals.
    Ref {
        /// Identifier of the referenced space.
        id: Symbol,
    },
    /// A self-reference to the enclosing recursive space.
    RecursiveRef {
        /// Identifier of the recursive space being referenced.
        id: Symbol,
    },
    /// A tagged sum (choice) over alternatives.
    Sum {
        /// Identifier of this sum space.
        id: Symbol,
        /// The alternatives, each tagged by position and carrying a grade cost.
        alts: Vec<RankAlt>,
    },
    /// A product (tuple/record) of ordered fields.
    Product {
        /// Identifier of this product space.
        id: Symbol,
        /// The product fields, each carrying a grade cost.
        fields: Vec<RankField>,
    },
    /// An ordered list of elements drawn from a common element space.
    List {
        /// Identifier of this list space.
        id: Symbol,
        /// Grammar of each list element.
        element: Box<RankGrammar>,
        /// Minimum list length.
        min_len: u64,
        /// Optional maximum list length; `None` is unbounded.
        max_len: Option<u64>,
    },
    /// An unordered set of elements drawn from a common element space.
    Set {
        /// Identifier of this set space.
        id: Symbol,
        /// Grammar of each set element.
        element: Box<RankGrammar>,
        /// Optional maximum set size; `None` is unbounded.
        max_len: Option<u64>,
    },
    /// A map from keys to values, each drawn from its own space.
    Map {
        /// Identifier of this map space.
        id: Symbol,
        /// Grammar of map keys.
        key: Box<RankGrammar>,
        /// Grammar of map values.
        value: Box<RankGrammar>,
        /// Optional maximum entry count; `None` is unbounded.
        max_len: Option<u64>,
    },
    /// An inner space narrowed by a named predicate.
    Guard {
        /// Identifier of this guard space.
        id: Symbol,
        /// The space being guarded.
        inner: Box<RankGrammar>,
        /// Identifier of the predicate that admits values.
        predicate: Symbol,
    },
}

/// One alternative of a [`RankGrammar::Sum`] space.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RankAlt {
    /// Identifier of this alternative.
    pub id: Symbol,
    /// Grammar of the value carried by this alternative.
    pub grammar: Box<RankGrammar>,
    /// Grade cost added when this alternative is chosen.
    pub grade_cost: u64,
}

impl RankAlt {
    /// Builds an alternative from an id, inner grammar, and grade cost.
    pub fn new(id: Symbol, grammar: RankGrammar, grade_cost: u64) -> Self {
        Self {
            id,
            grammar: Box::new(grammar),
            grade_cost,
        }
    }
}

/// One field of a [`RankGrammar::Product`] space.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RankField {
    /// Identifier of this field.
    pub id: Symbol,
    /// Grammar of the value held by this field.
    pub grammar: Box<RankGrammar>,
    /// Grade cost added by this field.
    pub grade_cost: u64,
}

impl RankField {
    /// Builds a field from an id, inner grammar, and grade cost.
    pub fn new(id: Symbol, grammar: RankGrammar, grade_cost: u64) -> Self {
        Self {
            id,
            grammar: Box::new(grammar),
            grade_cost,
        }
    }
}

impl RankGrammar {
    /// Renders this grammar as a structured summary datum for inspection.
    pub fn summary_datum(&self) -> Datum {
        match self {
            Self::Empty => node("grammar-empty", Vec::new()),
            Self::Unit => node("grammar-unit", Vec::new()),
            Self::Nat => node("grammar-nat", Vec::new()),
            Self::Int => node("grammar-int", Vec::new()),
            Self::Bool => node("grammar-bool", Vec::new()),
            Self::Enum { id, items } => node(
                "grammar-enum",
                vec![
                    field("id", Datum::Symbol(id.clone())),
                    field("items", symbol_list(items)),
                ],
            ),
            Self::Ref { id } => node("grammar-ref", vec![field("id", Datum::Symbol(id.clone()))]),
            Self::RecursiveRef { id } => node(
                "grammar-recursive-ref",
                vec![field("id", Datum::Symbol(id.clone()))],
            ),
            Self::Sum { id, alts } => node(
                "grammar-sum",
                vec![
                    field("id", Datum::Symbol(id.clone())),
                    field(
                        "alts",
                        Datum::List(alts.iter().map(RankAlt::summary_datum).collect()),
                    ),
                ],
            ),
            Self::Product { id, fields } => node(
                "grammar-product",
                vec![
                    field("id", Datum::Symbol(id.clone())),
                    field(
                        "fields",
                        Datum::List(fields.iter().map(RankField::summary_datum).collect()),
                    ),
                ],
            ),
            Self::List {
                id,
                element,
                min_len,
                max_len,
            } => node(
                "grammar-list",
                vec![
                    field("id", Datum::Symbol(id.clone())),
                    field("element", element.summary_datum()),
                    field("min-len", u64_datum(*min_len)),
                    field("max-len", optional_u64_datum(*max_len)),
                ],
            ),
            Self::Set {
                id,
                element,
                max_len,
            } => node(
                "grammar-set",
                vec![
                    field("id", Datum::Symbol(id.clone())),
                    field("element", element.summary_datum()),
                    field("max-len", optional_u64_datum(*max_len)),
                ],
            ),
            Self::Map {
                id,
                key,
                value,
                max_len,
            } => node(
                "grammar-map",
                vec![
                    field("id", Datum::Symbol(id.clone())),
                    field("key", key.summary_datum()),
                    field("value", value.summary_datum()),
                    field("max-len", optional_u64_datum(*max_len)),
                ],
            ),
            Self::Guard {
                id,
                inner,
                predicate,
            } => node(
                "grammar-guard",
                vec![
                    field("id", Datum::Symbol(id.clone())),
                    field("inner", inner.summary_datum()),
                    field("predicate", Datum::Symbol(predicate.clone())),
                ],
            ),
        }
    }

    /// Returns the content id of this grammar's summary datum.
    pub fn summary_content_id(&self) -> KernelResult<ContentId> {
        self.summary_datum().content_id()
    }

    /// Returns the short kind tag (e.g. `"sum"`, `"list"`) for this grammar.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Unit => "unit",
            Self::Nat => "nat",
            Self::Int => "int",
            Self::Bool => "bool",
            Self::Enum { .. } => "enum",
            Self::Ref { .. } => "ref",
            Self::RecursiveRef { .. } => "recursive-ref",
            Self::Sum { .. } => "sum",
            Self::Product { .. } => "product",
            Self::List { .. } => "list",
            Self::Set { .. } => "set",
            Self::Map { .. } => "map",
            Self::Guard { .. } => "guard",
        }
    }

    /// Checks that recursion under `id` is productive and fully resolved.
    ///
    /// Fails when a recursive reference is unguarded (would loop without
    /// consuming grade) or names a space that is not the enclosing one.
    pub fn validate_recursive_space(id: &Symbol, grammar: &Self) -> RankResult<()> {
        validate_recursive(id, grammar, false)
    }
}

impl RankAlt {
    fn summary_datum(&self) -> Datum {
        node(
            "grammar-alt",
            vec![
                field("id", Datum::Symbol(self.id.clone())),
                field("grammar", self.grammar.summary_datum()),
                field("grade-cost", u64_datum(self.grade_cost)),
            ],
        )
    }
}

impl RankField {
    fn summary_datum(&self) -> Datum {
        node(
            "grammar-field",
            vec![
                field("id", Datum::Symbol(self.id.clone())),
                field("grammar", self.grammar.summary_datum()),
                field("grade-cost", u64_datum(self.grade_cost)),
            ],
        )
    }
}

fn validate_recursive(id: &Symbol, grammar: &RankGrammar, guarded: bool) -> RankResult<()> {
    match grammar {
        RankGrammar::Empty
        | RankGrammar::Unit
        | RankGrammar::Nat
        | RankGrammar::Int
        | RankGrammar::Bool
        | RankGrammar::Enum { .. }
        | RankGrammar::Ref { .. } => Ok(()),
        RankGrammar::RecursiveRef { id: found } if found == id && guarded => Ok(()),
        RankGrammar::RecursiveRef { id: found } if found == id => {
            Err(RankError::UnproductiveRecursion { id: found.clone() })
        }
        RankGrammar::RecursiveRef { id: found } => {
            Err(RankError::UnresolvedRecursiveRef { id: found.clone() })
        }
        RankGrammar::Sum { alts, .. } => {
            for alt in alts {
                validate_recursive(id, &alt.grammar, guarded || alt.grade_cost > 0)?;
            }
            Ok(())
        }
        RankGrammar::Product { fields, .. } => {
            for field in fields {
                validate_recursive(id, &field.grammar, guarded || field.grade_cost > 0)?;
            }
            Ok(())
        }
        RankGrammar::List { element, .. } | RankGrammar::Set { element, .. } => {
            validate_recursive(id, element, true)
        }
        RankGrammar::Map { key, value, .. } => {
            validate_recursive(id, key, true)?;
            validate_recursive(id, value, true)
        }
        RankGrammar::Guard { inner, .. } => validate_recursive(id, inner, guarded),
    }
}

fn node(name: &str, fields: Vec<(Symbol, Datum)>) -> Datum {
    Datum::Node {
        tag: Symbol::qualified("rank", name),
        fields,
    }
}

fn field(name: &str, value: Datum) -> (Symbol, Datum) {
    (Symbol::new(name), value)
}

fn symbol_list(items: &[Symbol]) -> Datum {
    Datum::List(items.iter().cloned().map(Datum::Symbol).collect())
}

fn optional_u64_datum(value: Option<u64>) -> Datum {
    value.map_or(Datum::Nil, u64_datum)
}

fn u64_datum(value: u64) -> Datum {
    Datum::Number(NumberLiteral {
        domain: bigint_number_domain(),
        canonical: value.to_string(),
    })
}

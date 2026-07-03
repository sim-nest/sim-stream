//! Support for deriving rank grammars from Rust types.
//!
//! Defines the [`RankDescribe`] trait that maps a Rust type to its
//! [`RankGrammar`], the [`RankDescribeContext`] threaded through nested
//! descriptions, and standard-library and helper implementations used by
//! derived code.

use std::collections::BTreeSet;
use std::marker::PhantomData;

use sim_kernel::Symbol;

use crate::{
    RankBuilder, RankGrammar,
    error::{RankError, RankResult},
    nat::Nat,
    node::RankNode,
};

/// A Rust type that can describe its own rankable space.
pub trait RankDescribe {
    /// Returns the symbol naming this type's rank space.
    fn rank_type_symbol() -> Symbol;

    /// Builds the grammar for this type within the given context.
    fn rank_grammar(cx: &RankDescribeContext) -> RankResult<RankGrammar>;
}

/// Context threaded through a [`RankDescribe`] traversal.
///
/// Tracks the child spaces available for reference and the current path used
/// for diagnostics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RankDescribeContext {
    child_spaces: BTreeSet<Symbol>,
    path: Vec<Symbol>,
}

impl RankDescribeContext {
    /// Creates an empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `id` as an available child space and returns the context.
    pub fn with_child_space(mut self, id: Symbol) -> Self {
        self.child_spaces.insert(id);
        self
    }

    /// Reports whether `id` names a registered child space.
    pub fn has_child_space(&self, id: &Symbol) -> bool {
        self.child_spaces.contains(id)
    }

    /// Returns a child context descended into the field named `id`.
    pub fn child(&self, id: Symbol) -> Self {
        let mut next = self.clone();
        next.path.push(id);
        next
    }

    /// Returns the current descent path.
    pub fn path(&self) -> &[Symbol] {
        &self.path
    }

    /// Renders the current path as a dotted string (`"$"` at the root).
    pub fn path_string(&self) -> String {
        if self.path.is_empty() {
            "$".to_owned()
        } else {
            self.path
                .iter()
                .map(|symbol| symbol.name.as_ref())
                .collect::<Vec<_>>()
                .join(".")
        }
    }

    /// Returns a reference grammar to `id`, or an error if it is not registered.
    pub fn require_child_space(&self, id: Symbol) -> RankResult<RankGrammar> {
        if self.has_child_space(&id) {
            Ok(RankBuilder::reference(id))
        } else {
            Err(RankError::MissingChildSpace {
                path: self.path_string(),
                id,
            })
        }
    }
}

/// Builder for an enumeration rank space and its variant nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankEnumDescriptor {
    id: Symbol,
    variants: Vec<Symbol>,
}

impl RankEnumDescriptor {
    /// Creates an empty descriptor for the enumeration named `id`.
    pub fn new(id: Symbol) -> Self {
        Self {
            id,
            variants: Vec::new(),
        }
    }

    /// Appends a variant and returns the descriptor.
    pub fn variant(mut self, id: Symbol) -> Self {
        self.variants.push(id);
        self
    }

    /// Builds the enumeration grammar from the accumulated variants.
    pub fn build(&self) -> RankResult<RankGrammar> {
        RankBuilder::enumeration(self.id.clone(), self.variants.clone())
    }

    /// Builds the node for variant `id`, ranked by its position.
    pub fn variant_node(&self, id: &Symbol) -> RankResult<RankNode> {
        let index = self
            .variants
            .iter()
            .position(|variant| variant == id)
            .ok_or_else(|| RankError::InvalidNode {
                message: format!("rank enum {} has no variant {id}", self.id),
            })?;
        Ok(RankNode::Enum {
            id: self.id.clone(),
            index: Nat::from(index),
        })
    }

    /// Returns the enumeration identifier.
    pub fn id(&self) -> &Symbol {
        &self.id
    }

    /// Returns the ordered variant symbols.
    pub fn variants(&self) -> &[Symbol] {
        &self.variants
    }
}

/// Marker type describing a child space referenced by type `T`.
pub struct RankChild<T>(PhantomData<T>);

impl<T: RankDescribe> RankChild<T> {
    /// Returns a reference grammar to `T`'s space, requiring it be registered.
    pub fn grammar(cx: &RankDescribeContext) -> RankResult<RankGrammar> {
        cx.require_child_space(T::rank_type_symbol())
    }
}

impl<T: RankDescribe> RankDescribe for RankChild<T> {
    fn rank_type_symbol() -> Symbol {
        derived_symbol("child", &[T::rank_type_symbol()])
    }

    fn rank_grammar(cx: &RankDescribeContext) -> RankResult<RankGrammar> {
        Self::grammar(cx)
    }
}

/// Marker type describing a recursive self-reference to type `T`'s space.
pub struct RankRecursive<T>(PhantomData<T>);

impl<T: RankDescribe> RankDescribe for RankRecursive<T> {
    fn rank_type_symbol() -> Symbol {
        derived_symbol("recursive", &[T::rank_type_symbol()])
    }

    fn rank_grammar(_cx: &RankDescribeContext) -> RankResult<RankGrammar> {
        Ok(RankBuilder::recursive_ref(T::rank_type_symbol()))
    }
}

impl RankDescribe for () {
    fn rank_type_symbol() -> Symbol {
        Symbol::qualified("rank", "unit")
    }

    fn rank_grammar(_cx: &RankDescribeContext) -> RankResult<RankGrammar> {
        Ok(RankBuilder::unit())
    }
}

impl RankDescribe for bool {
    fn rank_type_symbol() -> Symbol {
        Symbol::qualified("rank", "bool")
    }

    fn rank_grammar(_cx: &RankDescribeContext) -> RankResult<RankGrammar> {
        Ok(RankBuilder::bool())
    }
}

impl RankDescribe for Nat {
    fn rank_type_symbol() -> Symbol {
        Symbol::qualified("rank", "nat")
    }

    fn rank_grammar(_cx: &RankDescribeContext) -> RankResult<RankGrammar> {
        Ok(RankBuilder::nat())
    }
}

impl<T: RankDescribe> RankDescribe for Box<T> {
    fn rank_type_symbol() -> Symbol {
        derived_symbol("box", &[T::rank_type_symbol()])
    }

    fn rank_grammar(cx: &RankDescribeContext) -> RankResult<RankGrammar> {
        T::rank_grammar(cx)
    }
}

impl<T: RankDescribe> RankDescribe for Option<T> {
    fn rank_type_symbol() -> Symbol {
        derived_symbol("option", &[T::rank_type_symbol()])
    }

    fn rank_grammar(cx: &RankDescribeContext) -> RankResult<RankGrammar> {
        RankBuilder::sum(Self::rank_type_symbol())
            .alt(Symbol::new("none"), RankBuilder::unit())
            .alt(
                Symbol::new("some"),
                T::rank_grammar(&cx.child(Symbol::new("some")))?,
            )
            .build()
    }
}

impl<T: RankDescribe> RankDescribe for Vec<T> {
    fn rank_type_symbol() -> Symbol {
        derived_symbol("vec", &[T::rank_type_symbol()])
    }

    fn rank_grammar(cx: &RankDescribeContext) -> RankResult<RankGrammar> {
        RankBuilder::list(
            Self::rank_type_symbol(),
            T::rank_grammar(&cx.child(Symbol::new("item")))?,
            0,
            None,
        )
    }
}

impl<A: RankDescribe, B: RankDescribe> RankDescribe for (A, B) {
    fn rank_type_symbol() -> Symbol {
        derived_symbol("tuple2", &[A::rank_type_symbol(), B::rank_type_symbol()])
    }

    fn rank_grammar(cx: &RankDescribeContext) -> RankResult<RankGrammar> {
        RankBuilder::product(Self::rank_type_symbol())
            .field(
                Symbol::new("0"),
                A::rank_grammar(&cx.child(Symbol::new("0")))?,
            )
            .field(
                Symbol::new("1"),
                B::rank_grammar(&cx.child(Symbol::new("1")))?,
            )
            .build()
    }
}

impl<A: RankDescribe, B: RankDescribe, C: RankDescribe> RankDescribe for (A, B, C) {
    fn rank_type_symbol() -> Symbol {
        derived_symbol(
            "tuple3",
            &[
                A::rank_type_symbol(),
                B::rank_type_symbol(),
                C::rank_type_symbol(),
            ],
        )
    }

    fn rank_grammar(cx: &RankDescribeContext) -> RankResult<RankGrammar> {
        RankBuilder::product(Self::rank_type_symbol())
            .field(
                Symbol::new("0"),
                A::rank_grammar(&cx.child(Symbol::new("0")))?,
            )
            .field(
                Symbol::new("1"),
                B::rank_grammar(&cx.child(Symbol::new("1")))?,
            )
            .field(
                Symbol::new("2"),
                C::rank_grammar(&cx.child(Symbol::new("2")))?,
            )
            .build()
    }
}

/// Builds a derived rank-space symbol from a kind tag and component symbols.
pub fn derived_symbol(kind: &str, parts: &[Symbol]) -> Symbol {
    let mut name = kind.to_owned();
    for part in parts {
        name.push('-');
        name.push_str(&symbol_fragment(part));
    }
    Symbol::qualified("rank-derived", name)
}

fn symbol_fragment(symbol: &Symbol) -> String {
    symbol.as_qualified_str().replace('/', ".")
}

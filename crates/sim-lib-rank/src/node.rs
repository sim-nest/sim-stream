//! The `RankNode` value tree inhabiting a rank space.
//!
//! A `RankNode` is one structured value drawn from a space's grammar; rank and
//! unrank convert between these nodes and their natural-number ordinals.

use num_bigint::BigInt;
use sim_kernel::Symbol;

use crate::nat::Nat;

/// One structured value inhabiting a rank space, ranked to an ordinal.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RankNode {
    /// The single inhabitant of the unit space.
    Unit,
    /// A natural-number node.
    Nat(Nat),
    /// A signed-integer node.
    Int(BigInt),
    /// A boolean node.
    Bool(bool),
    /// An enumeration member: enum `id` at the given `index`.
    Enum {
        /// Enumeration the member belongs to.
        id: Symbol,
        /// Position of the member within the enumeration.
        index: Nat,
    },
    /// A reference into another `space` at `ordinal`.
    Ref {
        /// Referenced space.
        space: Symbol,
        /// Ordinal of the referenced node within that space.
        ordinal: Nat,
    },
    /// A sum (tagged-union) node selecting alternative `tag` carrying `value`.
    Sum {
        /// Index of the selected alternative.
        tag: u32,
        /// Payload node of the selected alternative.
        value: Box<RankNode>,
    },
    /// A product (record) node holding its fields in order.
    Product(Vec<RankNode>),
    /// An ordered list of element nodes.
    List(Vec<RankNode>),
    /// A set of distinct element nodes.
    Set(Vec<RankNode>),
    /// A map of key-value node pairs.
    Map(Vec<(RankNode, RankNode)>),
}

impl RankNode {
    /// Builds a sum node tagged `tag` carrying `value`.
    pub fn sum(tag: u32, value: Self) -> Self {
        Self::Sum {
            tag,
            value: Box::new(value),
        }
    }

    /// Returns the short kind name for this node variant.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Nat(_) => "nat",
            Self::Int(_) => "int",
            Self::Bool(_) => "bool",
            Self::Enum { .. } => "enum",
            Self::Ref { .. } => "ref",
            Self::Sum { .. } => "sum",
            Self::Product(_) => "product",
            Self::List(_) => "list",
            Self::Set(_) => "set",
            Self::Map(_) => "map",
        }
    }
}

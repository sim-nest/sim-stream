//! Recursive data examples for RANK.
//!
//! This module keeps the R6.14 examples as ordinary rank codecs: recursive
//! trees and lists use grade-grouped coordinates, while finite set
//! permutations and finite maps use finite canonical coordinates.

use sim_kernel::Symbol;

use crate::{
    GroupCodec, Nat, RankBuilder, RankCodec, RankError, RankExactOrder, RankGrade, RankGrammar,
    RankGroupCodec, RankNode, RankResult, RankVersion, order::nat_to_index,
    order_builtin::grade_first_order,
};

pub use crate::tree_collection::{
    RankFiniteMapCodec, RankListCodec, RankSetPermutationCodec, rank_list_grammar, rank_map_grammar,
};

const TREE_EMPTY_TAG: u32 = 0;
const TREE_NODE_TAG: u32 = 1;

/// Rank codec for binary trees whose payloads reference a payload space.
///
/// Wraps a grade-grouped `RankGroupCodec` so recursive trees rank and unrank
/// by size grade, with each branch payload drawn from `payload_space`.
#[derive(Clone, Debug)]
pub struct RankTreeCodec {
    payload_space: Symbol,
    codec: RankGroupCodec,
}

#[derive(Clone, Debug)]
struct TreeOrderRecord {
    ordinal: Nat,
    grade: RankGrade,
    metrics: TreeMetrics,
    payload_key: Vec<Nat>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TreeMetrics {
    imbalance: u64,
    height: u64,
    node_count: u64,
}

impl RankTreeCodec {
    /// Builds a tree codec whose branch payloads reference `payload_space`.
    pub fn new(payload_space: Symbol) -> RankResult<Self> {
        Ok(Self {
            codec: RankGroupCodec::new(rank_tree_grammar(payload_space.clone())?),
            payload_space,
        })
    }

    /// Returns the symbol of the payload space referenced by branch nodes.
    pub fn payload_space(&self) -> &Symbol {
        &self.payload_space
    }

    /// Returns the recursive tree grammar this codec ranks against.
    pub fn grammar(&self) -> &RankGrammar {
        self.codec.grammar()
    }

    /// Returns the canonical empty-tree node.
    pub fn empty_node() -> RankNode {
        rank_tree_empty_node()
    }

    /// Builds a branch node from a left subtree, payload ordinal, and right subtree.
    pub fn node(&self, left: RankNode, payload_ordinal: Nat, right: RankNode) -> RankNode {
        rank_tree_branch_node(self.payload_space.clone(), left, payload_ordinal, right)
    }
}

impl RankCodec for RankTreeCodec {
    fn id(&self) -> Symbol {
        Symbol::qualified("rank-codec/tree", self.payload_space.as_qualified_str())
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

impl GroupCodec for RankTreeCodec {
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

/// Returns the recursive-reference symbol naming the tree space for `payload_space`.
pub fn rank_tree_symbol(payload_space: &Symbol) -> Symbol {
    Symbol::qualified("rank-tree", payload_space.as_qualified_str())
}

/// Builds the recursive sum/product grammar for trees over `payload_space`.
///
/// The grammar is an empty alternative plus a unit-cost branch carrying a left
/// subtree, a payload reference, and a right subtree.
pub fn rank_tree_grammar(payload_space: Symbol) -> RankResult<RankGrammar> {
    let tree = rank_tree_symbol(&payload_space);
    let branch = RankBuilder::product(Symbol::new("node"))
        .field(
            Symbol::new("left"),
            RankBuilder::recursive_ref(tree.clone()),
        )
        .field(
            Symbol::new("payload"),
            RankBuilder::reference(payload_space),
        )
        .field(
            Symbol::new("right"),
            RankBuilder::recursive_ref(tree.clone()),
        )
        .build()?;
    RankBuilder::sum(tree)
        .alt(Symbol::new("empty"), RankBuilder::unit())
        .alt_with_cost(Symbol::new("node"), 1, branch)
        .build_recursive()
}

/// Builds the canonical empty-tree node (the empty sum alternative).
pub fn rank_tree_empty_node() -> RankNode {
    RankNode::sum(TREE_EMPTY_TAG, RankNode::Unit)
}

/// Builds a tree branch node from its subtrees and a payload ordinal.
///
/// The payload becomes a reference into `payload_space` at `payload_ordinal`.
pub fn rank_tree_branch_node(
    payload_space: Symbol,
    left: RankNode,
    payload_ordinal: Nat,
    right: RankNode,
) -> RankNode {
    RankNode::sum(
        TREE_NODE_TAG,
        RankNode::Product(vec![
            left,
            RankNode::Ref {
                space: payload_space,
                ordinal: payload_ordinal,
            },
            right,
        ]),
    )
}

/// Builds an exact order over trees up to `max_grade`, smallest grade first.
pub fn rank_tree_grade_first_order(
    codec: &RankTreeCodec,
    max_grade: RankGrade,
) -> RankResult<RankExactOrder> {
    grade_first_order(
        Symbol::qualified("rank-tree-order", "grade-first"),
        codec,
        max_grade,
    )
}

/// Builds an exact order over trees ranked by balance then size.
///
/// Within each grade, trees are ordered by ascending imbalance, height, node
/// count, and finally ordinal, so more balanced trees rank earlier.
pub fn rank_tree_balanced_order(
    codec: &RankTreeCodec,
    max_grade: RankGrade,
) -> RankResult<RankExactOrder> {
    let mut records = tree_prefix_records(codec, max_grade, None)?;
    records.sort_by_key(|record| {
        (
            record.grade,
            record.metrics.imbalance,
            record.metrics.height,
            record.metrics.node_count,
            record.ordinal.clone(),
        )
    });
    order_from_records(Symbol::qualified("rank-tree-order", "balanced"), records)
}

/// Builds an exact order over trees ranked by their payload sequence.
///
/// Trees are keyed by the in-order sequence of payload positions taken from
/// `payload_order`, then by grade, imbalance, and ordinal as tie-breakers.
pub fn rank_tree_payload_order(
    codec: &RankTreeCodec,
    max_grade: RankGrade,
    payload_order: &RankExactOrder,
) -> RankResult<RankExactOrder> {
    let mut records = tree_prefix_records(codec, max_grade, Some(payload_order))?;
    records.sort_by_key(|record| {
        (
            record.payload_key.clone(),
            record.grade,
            record.metrics.imbalance,
            record.ordinal.clone(),
        )
    });
    order_from_records(Symbol::qualified("rank-tree-order", "payload"), records)
}

fn tree_prefix_records(
    codec: &RankTreeCodec,
    max_grade: RankGrade,
    payload_order: Option<&RankExactOrder>,
) -> RankResult<Vec<TreeOrderRecord>> {
    let mut records = Vec::new();
    let mut offset = Nat::zero();
    for grade in 0..=max_grade {
        let count = codec.group_count_at(grade)?;
        let count_index = nat_to_index(&count, usize::MAX, "rank tree grade count")?;
        for index in 0..count_index {
            let ordinal = offset.checked_add(&Nat::from(index));
            let node = codec.unrank_node(&ordinal)?;
            let payload_key = match payload_order {
                Some(order) => tree_payload_key(codec.payload_space(), &node, order)?,
                None => Vec::new(),
            };
            records.push(TreeOrderRecord {
                ordinal,
                grade,
                metrics: tree_metrics(&node)?,
                payload_key,
            });
        }
        offset = offset.checked_add(&count);
    }
    Ok(records)
}

fn order_from_records(id: Symbol, records: Vec<TreeOrderRecord>) -> RankResult<RankExactOrder> {
    RankExactOrder::new(
        id,
        records.into_iter().map(|record| record.ordinal).collect(),
    )
}

fn tree_metrics(node: &RankNode) -> RankResult<TreeMetrics> {
    match node {
        RankNode::Sum { tag, value } if *tag == TREE_EMPTY_TAG => {
            if value.as_ref() == &RankNode::Unit {
                Ok(TreeMetrics {
                    imbalance: 0,
                    height: 0,
                    node_count: 0,
                })
            } else {
                Err(invalid_node("tree empty node must carry unit"))
            }
        }
        RankNode::Sum { tag, value } if *tag == TREE_NODE_TAG => {
            let RankNode::Product(values) = value.as_ref() else {
                return Err(invalid_node("tree branch node must carry product"));
            };
            let [left, payload, right] = values.as_slice() else {
                return Err(invalid_node("tree branch product must have three fields"));
            };
            if !matches!(payload, RankNode::Ref { .. }) {
                return Err(invalid_node("tree branch payload must be a ref"));
            }
            let left = tree_metrics(left)?;
            let right = tree_metrics(right)?;
            Ok(TreeMetrics {
                imbalance: left
                    .imbalance
                    .saturating_add(right.imbalance)
                    .saturating_add(left.height.abs_diff(right.height)),
                height: 1 + left.height.max(right.height),
                node_count: 1 + left.node_count + right.node_count,
            })
        }
        RankNode::Sum { .. } => Err(invalid_node("tree sum tag is outside tree grammar")),
        _ => Err(RankError::NodeGrammarMismatch {
            expected: "sum",
            found: node.kind_name(),
        }),
    }
}

fn tree_payload_key(
    payload_space: &Symbol,
    node: &RankNode,
    payload_order: &RankExactOrder,
) -> RankResult<Vec<Nat>> {
    let mut key = Vec::new();
    collect_tree_payload_key(payload_space, node, payload_order, &mut key)?;
    Ok(key)
}

fn collect_tree_payload_key(
    payload_space: &Symbol,
    node: &RankNode,
    payload_order: &RankExactOrder,
    key: &mut Vec<Nat>,
) -> RankResult<()> {
    match node {
        RankNode::Sum { tag, value } if *tag == TREE_EMPTY_TAG => {
            if value.as_ref() == &RankNode::Unit {
                Ok(())
            } else {
                Err(invalid_node("tree empty node must carry unit"))
            }
        }
        RankNode::Sum { tag, value } if *tag == TREE_NODE_TAG => {
            let RankNode::Product(values) = value.as_ref() else {
                return Err(invalid_node("tree branch node must carry product"));
            };
            let [left, payload, right] = values.as_slice() else {
                return Err(invalid_node("tree branch product must have three fields"));
            };
            collect_tree_payload_key(payload_space, left, payload_order, key)?;
            let RankNode::Ref { space, ordinal } = payload else {
                return Err(invalid_node("tree branch payload must be a ref"));
            };
            if space != payload_space {
                return Err(invalid_node("tree branch payload space does not match"));
            }
            key.push(payload_order.position_of(ordinal)?);
            collect_tree_payload_key(payload_space, right, payload_order, key)
        }
        RankNode::Sum { .. } => Err(invalid_node("tree sum tag is outside tree grammar")),
        _ => Err(RankError::NodeGrammarMismatch {
            expected: "sum",
            found: node.kind_name(),
        }),
    }
}

fn invalid_node(message: &'static str) -> RankError {
    RankError::InvalidNode {
        message: message.to_owned(),
    }
}

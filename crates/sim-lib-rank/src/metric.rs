//! Neighborhoods and distances over a ranked space.
//!
//! A [`RankNeighborhood`] defines which ordinals are adjacent in a ranked
//! space, how far apart two ordinals are, and how to mutate or recombine them
//! -- the local-move structure that search, retrieval, and evolution build on.

use num_bigint::BigInt;
use sim_kernel::Symbol;

use crate::{Nat, RankCodec, RankNode, RankResult, RankVersion, limits::RankLimits};

/// Adjacency, distance, and recombination over the ordinals of a ranked space.
///
/// Implementors define the local move structure -- neighbors, distance, and
/// genetic operators -- that search and retrieval traverse. All operations are
/// fuel-bounded through [`RankLimits`].
pub trait RankNeighborhood: Send + Sync + std::fmt::Debug {
    /// Returns the stable symbol identifying this neighborhood.
    fn id(&self) -> Symbol;
    /// Returns the neighborhood's version.
    fn version(&self) -> RankVersion;
    /// Returns the ordinals adjacent to `ordinal` under this neighborhood.
    fn neighbors(
        &self,
        codec: &dyn RankCodec,
        ordinal: &Nat,
        limits: &mut RankLimits,
    ) -> RankResult<Vec<Nat>>;
    /// Returns the distance between ordinals `a` and `b`, or `None` if undefined.
    fn distance(
        &self,
        codec: &dyn RankCodec,
        a: &Nat,
        b: &Nat,
        limits: &mut RankLimits,
    ) -> RankResult<Option<Nat>>;
    /// Returns a seed-chosen neighbor of `ordinal`, or `ordinal` if it has none.
    fn mutate(
        &self,
        codec: &dyn RankCodec,
        ordinal: &Nat,
        seed: u64,
        limits: &mut RankLimits,
    ) -> RankResult<Nat>;
    /// Recombines ordinals `a` and `b` into a new ordinal, seeded by `seed`.
    fn crossover(
        &self,
        codec: &dyn RankCodec,
        a: &Nat,
        b: &Nat,
        seed: u64,
        limits: &mut RankLimits,
    ) -> RankResult<Nat>;
}

/// Structural neighborhood that edits the decoded node to find neighbors.
///
/// Generic over any codec: it unranks an ordinal, applies small structural
/// edits to the [`RankNode`], and re-ranks the valid results, so distance and
/// recombination follow the node shape rather than any domain specifics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenericNodeNeighborhood {
    id: Symbol,
}

impl GenericNodeNeighborhood {
    /// Builds a generic node neighborhood identified by `id`.
    pub fn new(id: Symbol) -> Self {
        Self { id }
    }

    fn valid_candidate(
        codec: &dyn RankCodec,
        origin: &Nat,
        candidate: RankNode,
        limits: &mut RankLimits,
    ) -> RankResult<Option<Nat>> {
        if limits.remaining_fuel() == 0 {
            return Ok(None);
        }
        limits.consume(1, "rank.neighbor.candidate")?;
        let Ok(rank) = codec.rank_node(&candidate) else {
            return Ok(None);
        };
        if &rank == origin || !codec.r_ok(&rank) || codec.unrank_node(&rank).is_err() {
            return Ok(None);
        }
        limits.check_nat(&rank, "rank.neighbor.candidate")?;
        Ok(Some(rank))
    }
}

impl Default for GenericNodeNeighborhood {
    fn default() -> Self {
        Self::new(Symbol::qualified("rank/metric", "generic-node"))
    }
}

impl RankNeighborhood for GenericNodeNeighborhood {
    fn id(&self) -> Symbol {
        self.id.clone()
    }

    fn version(&self) -> RankVersion {
        RankVersion::v1()
    }

    fn neighbors(
        &self,
        codec: &dyn RankCodec,
        ordinal: &Nat,
        limits: &mut RankLimits,
    ) -> RankResult<Vec<Nat>> {
        limits.consume(1, "rank.neighbors")?;
        limits.check_nat(ordinal, "rank.neighbors")?;
        let node = codec.unrank_node(ordinal)?;
        let mut candidates = Vec::new();
        node_neighbors(&node, &mut candidates);

        let mut ordinals = Vec::new();
        for candidate in candidates {
            if let Some(rank) = Self::valid_candidate(codec, ordinal, candidate, limits)?
                && !ordinals.contains(&rank)
            {
                ordinals.push(rank);
            }
        }
        ordinals.sort();
        Ok(ordinals)
    }

    fn distance(
        &self,
        codec: &dyn RankCodec,
        a: &Nat,
        b: &Nat,
        limits: &mut RankLimits,
    ) -> RankResult<Option<Nat>> {
        limits.consume(1, "rank.distance")?;
        let left = codec.unrank_node(a)?;
        let right = codec.unrank_node(b)?;
        let distance = if a == b {
            Nat::zero()
        } else {
            node_distance(&left, &right, limits)?.max(Nat::one())
        };
        Ok(Some(distance))
    }

    fn mutate(
        &self,
        codec: &dyn RankCodec,
        ordinal: &Nat,
        seed: u64,
        limits: &mut RankLimits,
    ) -> RankResult<Nat> {
        let neighbors = self.neighbors(codec, ordinal, limits)?;
        if neighbors.is_empty() {
            return Ok(ordinal.clone());
        }
        Ok(neighbors[pick_index(seed, neighbors.len())].clone())
    }

    fn crossover(
        &self,
        codec: &dyn RankCodec,
        a: &Nat,
        b: &Nat,
        seed: u64,
        limits: &mut RankLimits,
    ) -> RankResult<Nat> {
        limits.consume(1, "rank.crossover")?;
        let left = codec.unrank_node(a)?;
        let right = codec.unrank_node(b)?;
        for candidate in crossover_candidates(&left, &right, seed) {
            if let Some(rank) = Self::valid_candidate(codec, a, candidate, limits)? {
                return Ok(rank);
            }
        }
        self.mutate(codec, a, seed, limits)
    }
}

fn node_neighbors(node: &RankNode, out: &mut Vec<RankNode>) {
    match node {
        RankNode::Unit => {}
        RankNode::Nat(value) => nat_neighbors(value, out),
        RankNode::Int(value) => {
            out.push(RankNode::Int(value - BigInt::from(1)));
            out.push(RankNode::Int(value + BigInt::from(1)));
        }
        RankNode::Bool(value) => out.push(RankNode::Bool(!value)),
        RankNode::Enum { id, index } => {
            for next in adjacent_nats(index) {
                out.push(RankNode::Enum {
                    id: id.clone(),
                    index: next,
                });
            }
        }
        RankNode::Ref { space, ordinal } => {
            for next in adjacent_nats(ordinal) {
                out.push(RankNode::Ref {
                    space: space.clone(),
                    ordinal: next,
                });
            }
        }
        RankNode::Sum { tag, value } => {
            for child in child_neighbors(value) {
                out.push(RankNode::sum(*tag, child));
            }
            if let Some(prior) = tag.checked_sub(1) {
                out.push(RankNode::sum(prior, RankNode::Unit));
            }
            out.push(RankNode::sum(tag.saturating_add(1), RankNode::Unit));
        }
        RankNode::Product(values) => edit_vec(values, RankNode::Product, out),
        RankNode::List(values) => edit_collection(values, RankNode::List, out),
        RankNode::Set(values) => edit_collection(values, RankNode::Set, out),
        RankNode::Map(entries) => edit_map(entries, out),
    }
}

fn nat_neighbors(value: &Nat, out: &mut Vec<RankNode>) {
    for next in adjacent_nats(value) {
        out.push(RankNode::Nat(next));
    }
}

fn adjacent_nats(value: &Nat) -> Vec<Nat> {
    let mut values = Vec::new();
    if !value.is_zero() {
        values.push(value.checked_sub(&Nat::one()).expect("checked nonzero nat"));
    }
    values.push(value.checked_add(&Nat::one()));
    values
}

fn child_neighbors(node: &RankNode) -> Vec<RankNode> {
    let mut children = Vec::new();
    node_neighbors(node, &mut children);
    children
}

fn edit_vec(
    values: &[RankNode],
    wrap: impl Fn(Vec<RankNode>) -> RankNode,
    out: &mut Vec<RankNode>,
) {
    for (index, value) in values.iter().enumerate() {
        for child in child_neighbors(value) {
            let mut next = values.to_vec();
            next[index] = child;
            out.push(wrap(next));
        }
    }
}

fn edit_collection(
    values: &[RankNode],
    wrap: impl Fn(Vec<RankNode>) -> RankNode + Copy,
    out: &mut Vec<RankNode>,
) {
    edit_vec(values, wrap, out);
    for index in 0..values.len() {
        let mut next = values.to_vec();
        next.remove(index);
        out.push(wrap(next));
    }
    if let Some(last) = values.last() {
        let mut next = values.to_vec();
        next.push(last.clone());
        out.push(wrap(next));
    } else {
        out.push(wrap(vec![RankNode::Unit]));
        out.push(wrap(vec![RankNode::Bool(false)]));
        out.push(wrap(vec![RankNode::Nat(Nat::zero())]));
    }
    for index in 0..values.len().saturating_sub(1) {
        let mut next = values.to_vec();
        next.swap(index, index + 1);
        out.push(wrap(next));
    }
}

fn edit_map(entries: &[(RankNode, RankNode)], out: &mut Vec<RankNode>) {
    for (index, (key, value)) in entries.iter().enumerate() {
        for child in child_neighbors(key) {
            let mut next = entries.to_vec();
            next[index].0 = child;
            out.push(RankNode::Map(next));
        }
        for child in child_neighbors(value) {
            let mut next = entries.to_vec();
            next[index].1 = child;
            out.push(RankNode::Map(next));
        }
    }
    for index in 0..entries.len() {
        let mut next = entries.to_vec();
        next.remove(index);
        out.push(RankNode::Map(next));
    }
}

fn node_distance(a: &RankNode, b: &RankNode, limits: &mut RankLimits) -> RankResult<Nat> {
    limits.consume(1, "rank.distance.node")?;
    Ok(match (a, b) {
        (RankNode::Unit, RankNode::Unit) => Nat::zero(),
        (RankNode::Nat(left), RankNode::Nat(right)) => abs_nat(left, right)?,
        (RankNode::Int(left), RankNode::Int(right)) => {
            Nat::from_biguint((left - right).magnitude().clone())
        }
        (RankNode::Bool(left), RankNode::Bool(right)) => bool_distance(left == right),
        (
            RankNode::Enum {
                id: left_id,
                index: left,
            },
            RankNode::Enum {
                id: right_id,
                index: right,
            },
        ) if left_id == right_id => abs_nat(left, right)?,
        (
            RankNode::Ref {
                space: left_space,
                ordinal: left,
            },
            RankNode::Ref {
                space: right_space,
                ordinal: right,
            },
        ) if left_space == right_space => abs_nat(left, right)?,
        (
            RankNode::Sum {
                tag: left_tag,
                value: left,
            },
            RankNode::Sum {
                tag: right_tag,
                value: right,
            },
        ) => bool_distance(left_tag == right_tag).checked_add(&node_distance(left, right, limits)?),
        (RankNode::Product(left), RankNode::Product(right))
        | (RankNode::List(left), RankNode::List(right))
        | (RankNode::Set(left), RankNode::Set(right)) => sequence_distance(left, right, limits)?,
        (RankNode::Map(left), RankNode::Map(right)) => map_distance(left, right, limits)?,
        _ => Nat::one(),
    })
}

fn abs_nat(a: &Nat, b: &Nat) -> RankResult<Nat> {
    if a >= b {
        a.checked_sub(b)
    } else {
        b.checked_sub(a)
    }
}

fn bool_distance(equal: bool) -> Nat {
    if equal { Nat::zero() } else { Nat::one() }
}

fn sequence_distance(
    left: &[RankNode],
    right: &[RankNode],
    limits: &mut RankLimits,
) -> RankResult<Nat> {
    let mut total = Nat::from(left.len().abs_diff(right.len()));
    for (left, right) in left.iter().zip(right) {
        total = total.checked_add(&node_distance(left, right, limits)?);
    }
    Ok(total)
}

fn map_distance(
    left: &[(RankNode, RankNode)],
    right: &[(RankNode, RankNode)],
    limits: &mut RankLimits,
) -> RankResult<Nat> {
    let mut total = Nat::from(left.len().abs_diff(right.len()));
    for ((left_key, left_value), (right_key, right_value)) in left.iter().zip(right) {
        total = total
            .checked_add(&node_distance(left_key, right_key, limits)?)
            .checked_add(&node_distance(left_value, right_value, limits)?);
    }
    Ok(total)
}

fn crossover_candidates(a: &RankNode, b: &RankNode, seed: u64) -> Vec<RankNode> {
    let left_paths = typed_paths(a);
    let right_paths = typed_paths(b);
    let mut pairs = Vec::new();
    for (left_path, left_kind) in &left_paths {
        for (right_path, right_kind) in &right_paths {
            if left_kind == right_kind {
                pairs.push((left_path.clone(), right_path.clone()));
            }
        }
    }
    rotate_left(&mut pairs, seed);
    pairs
        .into_iter()
        .filter_map(|(left_path, right_path)| {
            replace_path(a, &left_path, get_path(b, &right_path)?.clone())
        })
        .collect()
}

fn typed_paths(node: &RankNode) -> Vec<(Vec<usize>, &'static str)> {
    let mut paths = Vec::new();
    collect_paths(node, Vec::new(), &mut paths);
    paths
}

fn collect_paths(node: &RankNode, path: Vec<usize>, out: &mut Vec<(Vec<usize>, &'static str)>) {
    out.push((path.clone(), node.kind_name()));
    for (index, child) in children(node).iter().enumerate() {
        let mut child_path = path.clone();
        child_path.push(index);
        collect_paths(child, child_path, out);
    }
}

fn children(node: &RankNode) -> Vec<&RankNode> {
    match node {
        RankNode::Sum { value, .. } => vec![value],
        RankNode::Product(values) | RankNode::List(values) | RankNode::Set(values) => {
            values.iter().collect()
        }
        RankNode::Map(entries) => entries
            .iter()
            .flat_map(|(key, value)| [key, value])
            .collect(),
        _ => Vec::new(),
    }
}

fn get_path<'a>(node: &'a RankNode, path: &[usize]) -> Option<&'a RankNode> {
    let Some((&head, tail)) = path.split_first() else {
        return Some(node);
    };
    get_path(children(node).get(head).copied()?, tail)
}

fn replace_path(node: &RankNode, path: &[usize], replacement: RankNode) -> Option<RankNode> {
    let Some((&head, tail)) = path.split_first() else {
        return Some(replacement);
    };
    match node {
        RankNode::Sum { tag, value } if head == 0 => {
            Some(RankNode::sum(*tag, replace_path(value, tail, replacement)?))
        }
        RankNode::Product(values) => {
            replace_in_vec(values, head, tail, replacement, RankNode::Product)
        }
        RankNode::List(values) => replace_in_vec(values, head, tail, replacement, RankNode::List),
        RankNode::Set(values) => replace_in_vec(values, head, tail, replacement, RankNode::Set),
        RankNode::Map(entries) => replace_in_map(entries, head, tail, replacement),
        _ => None,
    }
}

fn replace_in_vec(
    values: &[RankNode],
    index: usize,
    tail: &[usize],
    replacement: RankNode,
    wrap: impl Fn(Vec<RankNode>) -> RankNode,
) -> Option<RankNode> {
    let mut next = values.to_vec();
    *next.get_mut(index)? = replace_path(next.get(index)?, tail, replacement)?;
    Some(wrap(next))
}

fn replace_in_map(
    entries: &[(RankNode, RankNode)],
    index: usize,
    tail: &[usize],
    replacement: RankNode,
) -> Option<RankNode> {
    let mut next = entries.to_vec();
    let entry = next.get_mut(index / 2)?;
    if index.is_multiple_of(2) {
        entry.0 = replace_path(&entry.0, tail, replacement)?;
    } else {
        entry.1 = replace_path(&entry.1, tail, replacement)?;
    }
    Some(RankNode::Map(next))
}

fn rotate_left<T>(values: &mut [T], seed: u64) {
    if values.is_empty() {
        return;
    }
    values.rotate_left(pick_index(seed, values.len()));
}

fn pick_index(seed: u64, len: usize) -> usize {
    let mixed = seed
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .rotate_left(17)
        .wrapping_mul(0xbf58_476d_1ce4_e5b9);
    (mixed as usize) % len
}

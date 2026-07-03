//! Search neighborhood over the bounded expression rank space.
//!
//! Provides neighbors, distance, mutation, and crossover for expression
//! ordinals by mutating their expression trees and re-ranking the results.

use sim_kernel::{Expr, Symbol};

use crate::{
    Nat, RankCodec, RankNeighborhood, RankResult, RankVersion,
    expr::{
        RankExprSpec, atom_exprs, expr_from_rank_node, expr_to_rank_node, push_expr_unique,
        rank_expr_grade,
    },
    limits::RankLimits,
};

/// Search neighborhood over the bounded expression rank space.
///
/// Implements [`RankNeighborhood`] to expand, measure, mutate, and recombine
/// expression ordinals within the bounds of its [`RankExprSpec`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankExprNeighborhood {
    spec: RankExprSpec,
}

impl RankExprNeighborhood {
    /// Builds an expression neighborhood bounded by `spec`.
    pub fn new(spec: RankExprSpec) -> Self {
        Self { spec }
    }
}

impl RankNeighborhood for RankExprNeighborhood {
    fn id(&self) -> Symbol {
        Symbol::qualified("rank-metric", "expr")
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
        limits.consume(1, "rank.expr.neighbors")?;
        let expr = expr_from_rank_node(&self.spec, &codec.unrank_node(ordinal)?)?;
        let mut out = Vec::new();
        for candidate in expr_neighbors(&self.spec, &expr) {
            if limits.remaining_fuel() == 0 {
                break;
            }
            limits.consume(1, "rank.expr.neighbor.candidate")?;
            if let Ok(node) = expr_to_rank_node(&self.spec, &candidate)
                && let Ok(rank) = codec.rank_node(&node)
                && &rank != ordinal
                && codec.r_ok(&rank)
                && !out.contains(&rank)
            {
                out.push(rank);
            }
        }
        out.sort();
        Ok(out)
    }

    fn distance(
        &self,
        codec: &dyn RankCodec,
        a: &Nat,
        b: &Nat,
        limits: &mut RankLimits,
    ) -> RankResult<Option<Nat>> {
        limits.consume(1, "rank.expr.distance")?;
        if a == b {
            return Ok(Some(Nat::zero()));
        }
        let left = expr_from_rank_node(&self.spec, &codec.unrank_node(a)?)?;
        let right = expr_from_rank_node(&self.spec, &codec.unrank_node(b)?)?;
        let left_grade = rank_expr_grade(&self.spec, &left)?;
        let right_grade = rank_expr_grade(&self.spec, &right)?;
        let delta = left_grade.nodes.abs_diff(right_grade.nodes)
            + left_grade.cost.abs_diff(right_grade.cost)
            + usize::from(left != right);
        Ok(Some(Nat::from(delta.max(1))))
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
        Ok(neighbors[(seed as usize) % neighbors.len()].clone())
    }

    fn crossover(
        &self,
        codec: &dyn RankCodec,
        a: &Nat,
        b: &Nat,
        seed: u64,
        limits: &mut RankLimits,
    ) -> RankResult<Nat> {
        limits.consume(1, "rank.expr.crossover")?;
        let left = expr_from_rank_node(&self.spec, &codec.unrank_node(a)?)?;
        let right = expr_from_rank_node(&self.spec, &codec.unrank_node(b)?)?;
        for candidate in expr_crossovers(&left, &right, seed) {
            if let Ok(node) = expr_to_rank_node(&self.spec, &candidate)
                && let Ok(rank) = codec.rank_node(&node)
                && &rank != a
            {
                return Ok(rank);
            }
        }
        self.mutate(codec, a, seed, limits)
    }
}

fn expr_neighbors(spec: &RankExprSpec, expr: &Expr) -> Vec<Expr> {
    let mut out = atom_exprs(spec);
    if spec.max_list_len > 0 {
        push_expr_unique(&mut out, Expr::List(vec![expr.clone()]));
    }
    if spec.max_call_args > 0 {
        for operator in &spec.symbols {
            push_expr_unique(
                &mut out,
                Expr::Call {
                    operator: Box::new(Expr::Symbol(operator.clone())),
                    args: vec![expr.clone()],
                },
            );
        }
    }
    match expr {
        Expr::List(items) => list_neighbors(spec, &mut out, items),
        Expr::Call { operator, args } => call_neighbors(spec, &mut out, operator, args),
        _ => {}
    }
    out
}

fn list_neighbors(spec: &RankExprSpec, out: &mut Vec<Expr>, items: &[Expr]) {
    if items.len() < spec.max_list_len {
        for atom in atom_exprs(spec) {
            let mut next = items.to_vec();
            next.push(atom);
            push_expr_unique(out, Expr::List(next));
        }
    }
    for index in 0..items.len() {
        let mut next = items.to_vec();
        next.remove(index);
        push_expr_unique(out, Expr::List(next));
    }
}

fn call_neighbors(spec: &RankExprSpec, out: &mut Vec<Expr>, operator: &Expr, args: &[Expr]) {
    for symbol in &spec.symbols {
        push_expr_unique(
            out,
            Expr::Call {
                operator: Box::new(Expr::Symbol(symbol.clone())),
                args: args.to_vec(),
            },
        );
    }
    if args.len() < spec.max_call_args {
        for atom in atom_exprs(spec) {
            let mut next = args.to_vec();
            next.push(atom);
            push_expr_unique(
                out,
                Expr::Call {
                    operator: Box::new(operator.clone()),
                    args: next,
                },
            );
        }
    }
    for index in 0..args.len() {
        let mut next = args.to_vec();
        next.remove(index);
        push_expr_unique(
            out,
            Expr::Call {
                operator: Box::new(operator.clone()),
                args: next,
            },
        );
    }
}

fn expr_crossovers(left: &Expr, right: &Expr, seed: u64) -> Vec<Expr> {
    let mut out = Vec::new();
    if let (
        Expr::Call {
            operator: left_op,
            args: left_args,
        },
        Expr::Call {
            operator: right_op,
            args: right_args,
        },
    ) = (left, right)
    {
        let (operator, args) = if seed.is_multiple_of(2) {
            (left_op.clone(), right_args.clone())
        } else {
            (right_op.clone(), left_args.clone())
        };
        out.push(Expr::Call { operator, args });
    }
    out.push(right.clone());
    out
}

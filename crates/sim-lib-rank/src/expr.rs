//! Restricted expression spaces for RANK.
//!
//! A `RankExprSpec` closes the expression universe with finite symbol and
//! string alphabets, arity bounds, and a maximum recursive depth. The resulting
//! codec is finite and exposes lexicographic and size-first exact orders over
//! the same canonical coordinates.

use std::collections::BTreeSet;

use sim_kernel::{Expr, Symbol};

use crate::{
    Nat, RankCodec, RankError, RankExactOrder, RankNode, RankResult, RankVersion,
    order::nat_to_index,
};

const TAG_NIL: u32 = 0;
const TAG_BOOL: u32 = 1;
const TAG_SYMBOL: u32 = 2;
const TAG_STRING: u32 = 3;
const TAG_LIST: u32 = 4;
const TAG_CALL: u32 = 5;
const MAX_GENERATED_EXPRS: usize = 50_000;

/// Bounds that close the expression universe into a finite rank space.
///
/// Fixes finite symbol and string alphabets together with maximum recursion
/// depth, list length, and call arity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankExprSpec {
    pub(crate) symbols: Vec<Symbol>,
    pub(crate) strings: Vec<String>,
    pub(crate) max_depth: usize,
    pub(crate) max_list_len: usize,
    pub(crate) max_call_args: usize,
}

/// Size grade of an expression: node count and a cost weight.
///
/// Used as the primary key for the size-first exact order; smaller grades rank
/// earlier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RankExprGrade {
    /// Total number of expression nodes.
    pub nodes: usize,
    /// Weighted structural cost of the expression.
    pub cost: usize,
}

/// Finite rank codec over the bounded expression space.
///
/// Generates every expression allowed by its [`RankExprSpec`], orders them
/// lexicographically as the canonical ordinal layout, and precomputes a
/// size-first ordering.
#[derive(Clone, Debug)]
pub struct RankExprCodec {
    spec: RankExprSpec,
    exprs: Vec<Expr>,
    size_first_ordinals: Vec<Nat>,
}

impl RankExprSpec {
    /// Builds a spec from the given alphabets and bounds, failing closed on a
    /// duplicate symbol or string.
    pub fn new(
        symbols: impl IntoIterator<Item = Symbol>,
        strings: impl IntoIterator<Item = String>,
        max_depth: usize,
        max_list_len: usize,
        max_call_args: usize,
    ) -> RankResult<Self> {
        let spec = Self {
            symbols: unique_symbols(symbols)?,
            strings: unique_strings(strings)?,
            max_depth,
            max_list_len,
            max_call_args,
        };
        Ok(spec)
    }
}

impl RankExprCodec {
    /// Builds the codec by generating and ordering every expression admitted by
    /// `spec`, and precomputing the size-first order.
    pub fn new(spec: RankExprSpec) -> RankResult<Self> {
        let mut exprs = generate_exprs(&spec)?;
        exprs.sort_by_key(rank_expr_lex_key);
        let mut ordinals = (0..exprs.len()).collect::<Vec<_>>();
        ordinals.sort_by_key(|index| {
            (
                rank_expr_grade(&spec, &exprs[*index]).expect("generated expression is valid"),
                rank_expr_lex_key(&exprs[*index]),
            )
        });
        let size_first_ordinals = ordinals.into_iter().map(Nat::from).collect();
        Ok(Self {
            spec,
            exprs,
            size_first_ordinals,
        })
    }

    /// Returns the bounding spec for this codec.
    pub fn spec(&self) -> &RankExprSpec {
        &self.spec
    }

    /// Ranks an expression to its ordinal, failing closed when the expression
    /// lies outside the restricted space.
    pub fn rank_expr(&self, expr: &Expr) -> RankResult<Nat> {
        rank_expr_grade(&self.spec, expr)?;
        self.exprs
            .iter()
            .position(|candidate| candidate == expr)
            .map(Nat::from)
            .ok_or_else(|| invalid_node("expression is outside the restricted rank space"))
    }

    /// Unranks an ordinal back to its expression, failing closed when the
    /// ordinal is out of range.
    pub fn unrank_expr(&self, ordinal: &Nat) -> RankResult<Expr> {
        let index = nat_to_index(ordinal, self.exprs.len(), "rank expression ordinal")?;
        Ok(self.exprs[index].clone())
    }

    /// Converts an expression into its canonical `RankNode` coordinate.
    pub fn expr_to_node(&self, expr: &Expr) -> RankResult<RankNode> {
        expr_to_rank_node(&self.spec, expr)
    }

    /// Converts a `RankNode` coordinate back into its expression.
    pub fn expr_from_node(&self, node: &RankNode) -> RankResult<Expr> {
        expr_from_rank_node(&self.spec, node)
    }
}

impl RankCodec for RankExprCodec {
    fn id(&self) -> Symbol {
        Symbol::qualified("rank-codec", "expr")
    }

    fn version(&self) -> RankVersion {
        RankVersion::v1()
    }

    fn count(&self) -> Option<Nat> {
        Some(Nat::from(self.exprs.len()))
    }

    fn r_ok(&self, r: &Nat) -> bool {
        nat_to_index(r, self.exprs.len(), "rank expression ordinal").is_ok()
    }

    fn rank_node(&self, node: &RankNode) -> RankResult<Nat> {
        self.rank_expr(&self.expr_from_node(node)?)
    }

    fn unrank_node(&self, r: &Nat) -> RankResult<RankNode> {
        self.expr_to_node(&self.unrank_expr(r)?)
    }
}

/// Builds the lexicographic exact order, the codec's canonical ordinal layout.
pub fn rank_expr_lex_order(codec: &RankExprCodec) -> RankResult<RankExactOrder> {
    RankExactOrder::new(
        Symbol::qualified("rank-expr-order", "lex"),
        (0..codec.exprs.len()).map(Nat::from).collect(),
    )
}

/// Builds the size-first exact order, placing smaller expressions (by grade)
/// before larger ones, breaking ties lexicographically.
pub fn rank_expr_size_first_order(codec: &RankExprCodec) -> RankResult<RankExactOrder> {
    RankExactOrder::new(
        Symbol::qualified("rank-expr-order", "size-first"),
        codec.size_first_ordinals.clone(),
    )
}

/// Returns the lexicographic sort key for canonical expression ordering.
pub fn rank_expr_lex_key(expr: &Expr) -> String {
    match expr {
        Expr::Nil => "nil".to_owned(),
        Expr::Bool(false) => "false".to_owned(),
        Expr::Bool(true) => "true".to_owned(),
        Expr::Symbol(symbol) => symbol.to_string(),
        Expr::String(value) => format!("{value:?}"),
        Expr::List(items) => format!("({})", joined_lex(items)),
        Expr::Call { operator, args } if args.is_empty() => {
            format!("({})", rank_expr_lex_key(operator))
        }
        Expr::Call { operator, args } => {
            format!("({} {})", rank_expr_lex_key(operator), joined_lex(args))
        }
        other => format!("{:?}", other.canonical_key()),
    }
}

fn joined_lex(items: &[Expr]) -> String {
    items
        .iter()
        .map(rank_expr_lex_key)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Converts an expression into a `RankNode` coordinate under `spec`, validating
/// that it stays within the restricted space.
pub fn expr_to_rank_node(spec: &RankExprSpec, expr: &Expr) -> RankResult<RankNode> {
    rank_expr_grade(spec, expr)?;
    expr_to_node_unchecked(spec, expr)
}

/// Converts a `RankNode` coordinate back into an expression under `spec`,
/// validating that it stays within the restricted space.
pub fn expr_from_rank_node(spec: &RankExprSpec, node: &RankNode) -> RankResult<Expr> {
    let expr = expr_from_node_unchecked(spec, node)?;
    rank_expr_grade(spec, &expr)?;
    Ok(expr)
}

/// Computes the size grade of an expression, failing closed when it exceeds the
/// depth, arity, or alphabet bounds of `spec`.
pub fn rank_expr_grade(spec: &RankExprSpec, expr: &Expr) -> RankResult<RankExprGrade> {
    grade_inner(spec, expr, spec.max_depth)
}

fn grade_inner(spec: &RankExprSpec, expr: &Expr, depth: usize) -> RankResult<RankExprGrade> {
    match expr {
        Expr::Nil => Ok(RankExprGrade { nodes: 1, cost: 1 }),
        Expr::Bool(value) => Ok(RankExprGrade {
            nodes: 1,
            cost: 2 + usize::from(*value),
        }),
        Expr::Symbol(symbol) => Ok(RankExprGrade {
            nodes: 1,
            cost: 4 + symbol_index(spec, symbol)?,
        }),
        Expr::String(value) => Ok(RankExprGrade {
            nodes: 1,
            cost: 8 + string_index(spec, value)?,
        }),
        Expr::List(items) => {
            if depth == 0 {
                return Err(invalid_node("expression exceeds max depth"));
            }
            if items.len() > spec.max_list_len {
                return Err(invalid_node("list arity exceeds rank expression bound"));
            }
            fold_grade(10 + items.len(), items.iter(), depth - 1, spec)
        }
        Expr::Call { operator, args } => {
            if depth == 0 {
                return Err(invalid_node("expression exceeds max depth"));
            }
            if args.len() > spec.max_call_args {
                return Err(invalid_node("call arity exceeds rank expression bound"));
            }
            let Expr::Symbol(operator) = operator.as_ref() else {
                return Err(invalid_node("call operator must be in the symbol alphabet"));
            };
            let base = 20 + symbol_index(spec, operator)? + args.len();
            let mut grade = fold_grade(base + 1, args.iter(), depth - 1, spec)?;
            grade.nodes += 1;
            Ok(grade)
        }
        _ => Err(invalid_node(
            "expression variant is outside the restricted rank expression space",
        )),
    }
}

fn fold_grade<'a>(
    base_cost: usize,
    items: impl IntoIterator<Item = &'a Expr>,
    depth: usize,
    spec: &RankExprSpec,
) -> RankResult<RankExprGrade> {
    let mut grade = RankExprGrade {
        nodes: 1,
        cost: base_cost,
    };
    for item in items {
        let child = grade_inner(spec, item, depth)?;
        grade.nodes += child.nodes;
        grade.cost += child.cost;
    }
    Ok(grade)
}

fn generate_exprs(spec: &RankExprSpec) -> RankResult<Vec<Expr>> {
    let mut levels = Vec::<Vec<Expr>>::new();
    for depth in 0..=spec.max_depth {
        let mut current = atom_exprs(spec);
        if depth > 0 {
            let children = &levels[depth - 1];
            for len in 0..=spec.max_list_len {
                for values in expr_tuples(children, len) {
                    push_unique(&mut current, Expr::List(values))?;
                }
            }
            for operator in &spec.symbols {
                for len in 0..=spec.max_call_args {
                    for args in expr_tuples(children, len) {
                        push_unique(
                            &mut current,
                            Expr::Call {
                                operator: Box::new(Expr::Symbol(operator.clone())),
                                args,
                            },
                        )?;
                    }
                }
            }
        }
        current.sort_by_key(rank_expr_lex_key);
        levels.push(current);
    }
    levels
        .pop()
        .ok_or_else(|| invalid_node("rank expression generation produced no levels"))
}

fn expr_tuples(items: &[Expr], len: usize) -> Vec<Vec<Expr>> {
    if len == 0 {
        return vec![Vec::new()];
    }
    let tails = expr_tuples(items, len - 1);
    let mut out = Vec::new();
    for item in items {
        for tail in &tails {
            let mut tuple = Vec::with_capacity(len);
            tuple.push(item.clone());
            tuple.extend(tail.iter().cloned());
            out.push(tuple);
        }
    }
    out
}

pub(crate) fn atom_exprs(spec: &RankExprSpec) -> Vec<Expr> {
    let mut exprs = vec![Expr::Nil, Expr::Bool(false), Expr::Bool(true)];
    exprs.extend(spec.symbols.iter().cloned().map(Expr::Symbol));
    exprs.extend(spec.strings.iter().cloned().map(Expr::String));
    exprs
}

fn expr_to_node_unchecked(spec: &RankExprSpec, expr: &Expr) -> RankResult<RankNode> {
    Ok(match expr {
        Expr::Nil => RankNode::sum(TAG_NIL, RankNode::Unit),
        Expr::Bool(value) => RankNode::sum(TAG_BOOL, RankNode::Bool(*value)),
        Expr::Symbol(symbol) => RankNode::sum(
            TAG_SYMBOL,
            RankNode::Enum {
                id: symbol_alphabet_id(),
                index: Nat::from(symbol_index(spec, symbol)?),
            },
        ),
        Expr::String(value) => RankNode::sum(
            TAG_STRING,
            RankNode::Enum {
                id: string_alphabet_id(),
                index: Nat::from(string_index(spec, value)?),
            },
        ),
        Expr::List(items) => RankNode::sum(
            TAG_LIST,
            RankNode::List(
                items
                    .iter()
                    .map(|item| expr_to_node_unchecked(spec, item))
                    .collect::<RankResult<Vec<_>>>()?,
            ),
        ),
        Expr::Call { operator, args } => {
            let Expr::Symbol(operator) = operator.as_ref() else {
                return Err(invalid_node("call operator must be in the symbol alphabet"));
            };
            RankNode::sum(
                TAG_CALL,
                RankNode::Product(vec![
                    RankNode::Enum {
                        id: symbol_alphabet_id(),
                        index: Nat::from(symbol_index(spec, operator)?),
                    },
                    RankNode::List(
                        args.iter()
                            .map(|arg| expr_to_node_unchecked(spec, arg))
                            .collect::<RankResult<Vec<_>>>()?,
                    ),
                ]),
            )
        }
        _ => {
            return Err(invalid_node(
                "expression variant is outside the restricted rank expression space",
            ));
        }
    })
}

fn expr_from_node_unchecked(spec: &RankExprSpec, node: &RankNode) -> RankResult<Expr> {
    let RankNode::Sum { tag, value } = node else {
        return Err(RankError::NodeGrammarMismatch {
            expected: "sum",
            found: node.kind_name(),
        });
    };
    match (*tag, value.as_ref()) {
        (TAG_NIL, RankNode::Unit) => Ok(Expr::Nil),
        (TAG_BOOL, RankNode::Bool(value)) => Ok(Expr::Bool(*value)),
        (TAG_SYMBOL, RankNode::Enum { id, index }) if id == &symbol_alphabet_id() => {
            let index = nat_to_index(index, spec.symbols.len(), "rank expression symbol index")?;
            Ok(Expr::Symbol(spec.symbols[index].clone()))
        }
        (TAG_STRING, RankNode::Enum { id, index }) if id == &string_alphabet_id() => {
            let index = nat_to_index(index, spec.strings.len(), "rank expression string index")?;
            Ok(Expr::String(spec.strings[index].clone()))
        }
        (TAG_LIST, RankNode::List(items)) => Ok(Expr::List(
            items
                .iter()
                .map(|item| expr_from_node_unchecked(spec, item))
                .collect::<RankResult<Vec<_>>>()?,
        )),
        (TAG_CALL, RankNode::Product(fields)) if fields.len() == 2 => {
            let RankNode::Enum { id, index } = &fields[0] else {
                return Err(invalid_node("call operator node must be an enum"));
            };
            if id != &symbol_alphabet_id() {
                return Err(invalid_node("call operator enum has wrong alphabet"));
            }
            let op = nat_to_index(index, spec.symbols.len(), "rank expression operator index")?;
            let RankNode::List(args) = &fields[1] else {
                return Err(invalid_node("call args node must be a list"));
            };
            Ok(Expr::Call {
                operator: Box::new(Expr::Symbol(spec.symbols[op].clone())),
                args: args
                    .iter()
                    .map(|arg| expr_from_node_unchecked(spec, arg))
                    .collect::<RankResult<Vec<_>>>()?,
            })
        }
        (TAG_CALL, RankNode::Product(_)) => Err(invalid_node("call node arity must be two")),
        (_, found) => Err(RankError::NodeGrammarMismatch {
            expected: "rank expression",
            found: found.kind_name(),
        }),
    }
}

fn unique_symbols(items: impl IntoIterator<Item = Symbol>) -> RankResult<Vec<Symbol>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for item in items {
        if !seen.insert(item.clone()) {
            return Err(invalid_node(format!(
                "duplicate rank expression symbol {item}"
            )));
        }
        out.push(item);
    }
    Ok(out)
}

fn unique_strings(items: impl IntoIterator<Item = String>) -> RankResult<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for item in items {
        if !seen.insert(item.clone()) {
            return Err(invalid_node(format!(
                "duplicate rank expression string {item:?}"
            )));
        }
        out.push(item);
    }
    Ok(out)
}

fn push_unique(out: &mut Vec<Expr>, expr: Expr) -> RankResult<()> {
    push_expr_unique(out, expr);
    if out.len() > MAX_GENERATED_EXPRS {
        return Err(invalid_node(
            "restricted rank expression space exceeds generation bound",
        ));
    }
    Ok(())
}

pub(crate) fn push_expr_unique(out: &mut Vec<Expr>, expr: Expr) {
    if !out.contains(&expr) {
        out.push(expr);
    }
}

fn symbol_index(spec: &RankExprSpec, symbol: &Symbol) -> RankResult<usize> {
    spec.symbols
        .iter()
        .position(|candidate| candidate == symbol)
        .ok_or_else(|| {
            invalid_node(format!(
                "symbol {symbol} is outside the expression alphabet"
            ))
        })
}

fn string_index(spec: &RankExprSpec, value: &str) -> RankResult<usize> {
    spec.strings
        .iter()
        .position(|candidate| candidate == value)
        .ok_or_else(|| {
            invalid_node(format!(
                "string {value:?} is outside the expression alphabet"
            ))
        })
}

fn symbol_alphabet_id() -> Symbol {
    Symbol::qualified("rank-expr", "symbol")
}

fn string_alphabet_id() -> Symbol {
    Symbol::qualified("rank-expr", "string")
}

fn invalid_node(message: impl Into<String>) -> RankError {
    RankError::InvalidNode {
        message: message.into(),
    }
}

//! Ranking contexts and the default order selected for each.
//!
//! A `RankContext` names the situation a space is being ranked for (browsing,
//! search, fuzzing, generation, and so on); each context maps to a default
//! order symbol used when no explicit order is requested.

use sim_kernel::{Datum, Symbol};

/// Situation a rank space is ordered for, selecting a default order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RankContext {
    /// Canonical ordering by ordinal.
    Canonical,
    /// Human browsing, favoring lexicographic order.
    Browse,
    /// Test enumeration, favoring edge cases first.
    Test,
    /// Search, favoring lower-cost candidates first.
    Search,
    /// Fuzzing, favoring a seeded shuffle.
    Fuzz,
    /// Learning, favoring a learned likelihood order.
    Learn,
    /// Expression search, favoring type-directed order.
    ExprSearch,
    /// Expression simplification, favoring normal forms first.
    ExprSimplify,
    /// Logic querying, favoring answer-cost order.
    LogicQuery,
    /// Music generation, favoring tonal likelihood with low motion.
    MusicGeneration,
    /// Music analysis, favoring stable feature order.
    MusicAnalysis,
    /// Tree balancing, favoring balanced shapes first.
    TreeBalance,
    /// Generative beam search.
    Generate,
    /// A user-defined context named by `symbol`.
    User(Symbol),
}

impl RankContext {
    /// Returns the `rank-context/*` symbol naming this context.
    pub fn symbol(&self) -> Symbol {
        match self {
            Self::Canonical => context_symbol("canonical"),
            Self::Browse => context_symbol("browse"),
            Self::Test => context_symbol("test"),
            Self::Search => context_symbol("search"),
            Self::Fuzz => context_symbol("fuzz"),
            Self::Learn => context_symbol("learn"),
            Self::ExprSearch => context_symbol("expr-search"),
            Self::ExprSimplify => context_symbol("expr-simplify"),
            Self::LogicQuery => context_symbol("logic-query"),
            Self::MusicGeneration => context_symbol("music-generation"),
            Self::MusicAnalysis => context_symbol("music-analysis"),
            Self::TreeBalance => context_symbol("tree-balance"),
            Self::Generate => context_symbol("generate"),
            Self::User(symbol) => symbol.clone(),
        }
    }

    /// Parses a `rank-context/*` symbol back into a context.
    ///
    /// Unrecognized symbols become `RankContext::User`.
    pub fn from_symbol(symbol: Symbol) -> Self {
        match (symbol.namespace.as_deref(), symbol.name.as_ref()) {
            (Some("rank-context"), "canonical") => Self::Canonical,
            (Some("rank-context"), "browse") => Self::Browse,
            (Some("rank-context"), "test") => Self::Test,
            (Some("rank-context"), "search") => Self::Search,
            (Some("rank-context"), "fuzz") => Self::Fuzz,
            (Some("rank-context"), "learn") => Self::Learn,
            (Some("rank-context"), "expr-search") => Self::ExprSearch,
            (Some("rank-context"), "expr-simplify") => Self::ExprSimplify,
            (Some("rank-context"), "logic-query") => Self::LogicQuery,
            (Some("rank-context"), "music-generation") => Self::MusicGeneration,
            (Some("rank-context"), "music-analysis") => Self::MusicAnalysis,
            (Some("rank-context"), "tree-balance") => Self::TreeBalance,
            (Some("rank-context"), "generate") => Self::Generate,
            _ => Self::User(symbol),
        }
    }
}

/// Returns the default order symbol associated with `context`.
pub fn default_order_for_context(context: &RankContext) -> Symbol {
    match context {
        RankContext::Canonical => order_symbol("canonical"),
        RankContext::Browse => order_symbol("lex"),
        RankContext::Test => order_symbol("edge-first-then-grade-first"),
        RankContext::Search => order_symbol("cost-first-then-grade-first"),
        RankContext::Fuzz => order_symbol("seeded-shuffle-grade-first"),
        RankContext::Learn => order_symbol("learned-likelihood"),
        RankContext::ExprSearch => order_symbol("type-directed-then-grade-first"),
        RankContext::ExprSimplify => order_symbol("normal-form-first"),
        RankContext::LogicQuery => order_symbol("answer-cost-first"),
        RankContext::MusicGeneration => order_symbol("tonal-likelihood-low-motion"),
        RankContext::MusicAnalysis => order_symbol("feature-stable"),
        RankContext::TreeBalance => order_symbol("balanced-then-grade-first"),
        RankContext::Generate => order_symbol("beam-generate"),
        RankContext::User(symbol) => order_symbol(format!("user-{}", symbol.name)),
    }
}

/// Builds the claim datum pairing `context` with its default `order`.
pub fn default_context_claim_datum(context: &RankContext, order: &Symbol) -> Datum {
    Datum::Node {
        tag: Symbol::qualified("rank", "default-context"),
        fields: vec![
            (Symbol::new("context"), Datum::Symbol(context.symbol())),
            (Symbol::new("order"), Datum::Symbol(order.clone())),
        ],
    }
}

/// Returns the standard contexts paired with their default orders.
pub fn standard_default_contexts() -> Vec<(RankContext, Symbol)> {
    [
        RankContext::Canonical,
        RankContext::Browse,
        RankContext::Test,
        RankContext::Search,
        RankContext::Fuzz,
        RankContext::Generate,
    ]
    .into_iter()
    .map(|context| {
        let order = default_order_for_context(&context);
        (context, order)
    })
    .collect()
}

/// Builds a `rank-context/<name>` symbol.
pub fn context_symbol(name: impl Into<std::sync::Arc<str>>) -> Symbol {
    Symbol::qualified("rank-context", name)
}

/// Builds a `rank-order/<name>` symbol.
pub fn order_symbol(name: impl Into<std::sync::Arc<str>>) -> Symbol {
    Symbol::qualified("rank-order", name)
}

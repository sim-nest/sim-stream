#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! RANK 6 coordinate, codec, order, search, and optional domain spaces.
pub mod builder;
pub mod cap;
pub mod claims;
pub mod codec;
#[cfg(feature = "rank-codec-fallback")]
pub mod codec_fallback;
mod codec_finite;
pub mod codec_group;
mod codec_group_core;
mod codec_group_list;
mod codec_group_product;
mod codec_integer;
pub mod codec_primitive;
pub mod context;
pub mod cookbook;
mod cookbook_runtime;
pub mod derive_support;
pub mod error;
#[cfg(feature = "rank-expr")]
pub mod expr;
#[cfg(feature = "rank-expr")]
mod expr_neighborhood;
pub mod grade;
pub mod grade_compile;
mod grade_util;
pub mod grammar;
pub mod limits;
pub mod lisp;
mod lisp_class;
#[cfg(feature = "rank-scatter")]
pub mod meta;
pub mod metric;
#[cfg(feature = "rank-music")]
pub mod music;
pub mod nat;
pub mod node;
pub mod ops;
pub mod order;
pub mod order_builtin;
pub mod order_compose;
pub mod order_score;
mod read_construct;
pub mod registry;
pub mod retrieve;
#[cfg(feature = "rank-scatter")]
pub mod scatter;
pub mod search;
pub mod space;
pub mod tree;
mod tree_collection;
pub mod version;

pub use builder::{RankBuilder, RankProductBuilder, RankSumBuilder};
pub use cap::{
    rank_browse_capability, rank_codec_capability, rank_enumerate_capability,
    rank_heavy_capability, rank_learn_capability, rank_neighbor_capability,
    rank_public_capabilities, rank_read_capability,
};
pub use claims::{
    RankSpaceCardMetadata, publish_coordinate_claims, publish_space_card_claims,
    publish_space_claims, rank_space_card,
};
pub use codec::RankCodec;
#[cfg(feature = "rank-codec-fallback")]
pub use codec_fallback::{
    binary_codec_symbol, binary_frame_from_nat, binary_frame_to_nat, rank_codec_fallback_card,
    rank_codec_fallback_symbol, rank_expr_storage_identity, rank_expr_with_fallback,
    rank_value_with_fallback, unrank_expr_storage_identity, unrank_expr_with_fallback,
};
pub use codec_group::{GroupCodec, RankGroupCodec};
pub use codec_primitive::RankPrimitiveCodec;
pub use context::{
    RankContext, default_order_for_context, order_symbol, standard_default_contexts,
};
pub use cookbook::{rank_retrieve_demo, recommendation_ranking_demo, space_coordinate_demo};
pub use derive_support::{
    RankChild, RankDescribe, RankDescribeContext, RankEnumDescriptor, RankRecursive, derived_symbol,
};
pub use error::{RankError, RankResult};
#[cfg(feature = "rank-expr")]
pub use expr::{
    RankExprCodec, RankExprGrade, RankExprSpec, expr_from_rank_node, expr_to_rank_node,
    rank_expr_lex_key, rank_expr_lex_order, rank_expr_size_first_order,
};
#[cfg(feature = "rank-expr")]
pub use expr_neighborhood::RankExprNeighborhood;
pub use grade::{GradeMemoStats, RankGrade, count_at_grade, grade_count_is_finite, grade_of_node};
pub use grade_compile::GradeCompiler;
pub use grammar::{RankAlt, RankField, RankGrammar};
pub use limits::RankLimits;
pub use lisp::{
    RankLib, install_rank_lib, install_rank_space, rank_coordinate_class_symbol,
    rank_enumerate_symbol, rank_fn_symbol, rank_mutate_symbol, rank_node_class_symbol,
    rank_space_class_symbol, unrank_fn_symbol,
};
pub use metric::{GenericNodeNeighborhood, RankNeighborhood};
pub use nat::{
    Nat, bigint_number_domain, binomial, coordinate_for_nat, intern_ordinal, ordinal_content_id,
    ordinal_datum,
};
pub use node::RankNode;
pub use ops::{rank_rank_op_key, rank_unrank_op_key};
pub use order::RankExactOrder;
pub use order_builtin::{
    canonical_order, exact_permutation, grade_first_order, reverse_window_order, round_robin_order,
    seeded_shuffle_order,
};
pub use order_compose::then_order;
pub use order_score::{
    BandFrontierSpec, BeamFrontierSpec, RankFrontier, RankFrontierPayload, RankOrdinalRange,
    RankScore, RankScoreFn, ScoreValue, ScoredOrdinal, band_frontier, beam_frontier,
    novelty_frontier,
};
pub use registry::RankSpaceRegistry;
pub use retrieve::{
    EmbeddingStore, RetrievedNeighbor, retrieve, retrieve_ids, retrieve_rank_neighborhood,
};
pub use search::{
    RankBeamSearchResult, RankSearchResult, RankSearchScore, RankSearchState, beam_search,
    hill_climb,
};
pub use space::{
    RankCoordinateValue, RankNodeValue, RankSpace, coordinate_from_value, rank_coordinate_value,
    rank_node_from_value, rank_node_value,
};
pub use version::RankVersion;

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

#[cfg(test)]
mod test_modules;

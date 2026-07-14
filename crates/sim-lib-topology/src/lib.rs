#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Data-driven topology engine.
//!
//! Topologies are authored as graph data, text wiring, diagrams, or `.simtopo`
//! packages, then validated and compiled into deterministic runtime plans.
//!
//! ```rust
//! use std::sync::Arc;
//!
//! use sim_kernel::{Cx, DefaultFactory, Expr, NoopEvalPolicy, Symbol};
//! use sim_lib_topology::{compile_graph, parse_package, topology_card_expr};
//!
//! let package = parse_package(
//!     r#"
//! graph:
//! topology doc-flow
//! node in verb=in
//! node step verb=wire
//! node out verb=out
//! wire in -> step
//! wire step -> out
//!
//! tests:
//! smoke input="ping" expect="ping"
//! capabilities:
//! topology/run
//! "#,
//! )
//! .unwrap();
//!
//! assert_eq!(package.name(), &Symbol::new("doc-flow"));
//! assert_eq!(package.tests.len(), 1);
//!
//! let mut cx = Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory));
//! let plan = compile_graph(&mut cx, &package.graph).unwrap();
//! assert_eq!(plan.nodes.len(), 3);
//!
//! let card = topology_card_expr(&Symbol::qualified("topology", "package-format"));
//! assert!(matches!(card, Expr::Map(_)));
//! ```

pub mod adapter;
pub mod browse;
pub mod capability;
mod citizen;
pub mod compile;
pub mod cookbook;
pub mod diagram;
pub mod error;
pub mod instrument;
pub mod model;
pub mod package;
pub mod parse;
pub mod patch;
pub mod place;
mod record;
pub mod reflect;
pub mod registry;
pub mod replay;
pub mod run;
mod run_cells;
mod run_nonlinear;
mod run_predicate;
pub mod site;
pub mod text;
pub mod validate;
pub mod verb;

pub use adapter::{TopologyAdapter, TopologyAdapterRegistry};
pub use browse::{
    TopologyExampleSpec, TopologyFunctionSpec, TopologyVerbSpec, topology_browse_symbols,
    topology_card_expr, topology_example_specs, topology_function_specs, topology_verb_specs,
};
pub use capability::{
    topology_file_capability, topology_reflect_capability, topology_run_capability,
    topology_write_capability,
};
pub use citizen::{
    TopologyEdgeDescriptor, TopologyNodeDescriptor, TopologyPackageDescriptor,
    topology_edge_class_symbol, topology_node_class_symbol, topology_package_class_symbol,
};
pub use compile::{CompiledGraph, compile_graph};
pub use cookbook::{embodied_intelligence_trace_demo, tiny_graph_demo};
pub use instrument::{
    InstrumentTopologyAdapter, InstrumentTopologyCord, InstrumentTopologyJack,
    InstrumentTopologyModule, InstrumentTopologySpec,
};
pub use model::{
    Budget, BudgetExhausted, Cell, Edge, EdgeId, Graph, GraphTest, Node, NodeId, Port, PortMode,
    PortRef, Scheduler, SchedulerMode,
};
pub use package::{TopologyPackage, load_package_file, parse_package};
pub use parse::{graph_from_value, parse_graph};
pub use patch::{
    PatchOp, TopologyPatch, apply_topology_patch, apply_topology_patch_ops, patched_connection,
};
pub use place::{
    DomainBridge, PlacedNode, PlacementNodeProfile, PlacementRefusal, PlacementRefusalReason,
    PlacementReport, PortLatency, SiteId, SiteMap, SiteProfile, place, place_graph,
};
pub use reflect::{
    ReflectedCell, ReflectedNode, TopologyEdgeVisit, TopologyHistory, TopologyRecordedReply,
    TopologyRunReport, TopologyVisit, topology_explain, topology_reflect, topology_reflect_graph,
    topology_reflect_run,
};
pub use registry::{
    SharedTopologyRegistry, TopologyEntry, TopologyLib, TopologyRegistry, install_topology_lib,
    manifest_name as topology_manifest_name, topology_def, topology_exports, topology_get,
    topology_list, topology_load_file, topology_reload, topology_remove,
};
pub use replay::{TopologyCounterfactual, counterfactual_replay, replay_report};
pub use site::{TopologyConnection, connection_from_graph};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

#[cfg(test)]
mod tests;

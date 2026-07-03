//! Topology replay and counterfactual execution.

use sim_kernel::{Cx, Error, Expr, Result, Symbol};

use crate::{EdgeId, compile_graph, reflect::TopologyRunReport, run::run_graph};

/// Counterfactual change applied to a reflected topology run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopologyCounterfactual {
    /// Replace one node target expression.
    ReplaceTarget {
        /// Node id.
        node: Symbol,
        /// Replacement target expression.
        target: Expr,
    },
    /// Disable one edge by id.
    DisableEdge {
        /// Edge id.
        edge: EdgeId,
    },
    /// Force a branch predicate result on one node.
    ForcePredicate {
        /// Branch node id.
        node: Symbol,
        /// Forced predicate result.
        result: bool,
    },
}

/// Replays a reflected run deterministically from its recorded report.
pub fn replay_report(report: &TopologyRunReport) -> Result<Expr> {
    if report.events.is_empty() {
        return Err(Error::Eval(
            "topology replay requires a reflected run report with events".to_owned(),
        ));
    }
    Ok(report.output.clone())
}

/// Replays a reflected run after applying one counterfactual graph change.
pub fn counterfactual_replay(
    cx: &mut Cx,
    report: &TopologyRunReport,
    change: TopologyCounterfactual,
) -> Result<Expr> {
    let mut graph = report.source_graph().clone();
    apply_counterfactual(&mut graph, change)?;
    let plan = compile_graph(cx, &graph)?;
    run_graph(cx, &graph, &plan, report.input.clone())
}

fn apply_counterfactual(graph: &mut crate::Graph, change: TopologyCounterfactual) -> Result<()> {
    match change {
        TopologyCounterfactual::ReplaceTarget { node, target } => {
            let node = graph_node_mut(graph, &node)?;
            node.target = Some(target);
            Ok(())
        }
        TopologyCounterfactual::DisableEdge { edge } => {
            let before = graph.edges.len();
            graph.edges.retain(|candidate| candidate.id != edge);
            if graph.edges.len() == before {
                return Err(Error::Eval(format!(
                    "topology counterfactual: unknown edge {}",
                    edge.0
                )));
            }
            Ok(())
        }
        TopologyCounterfactual::ForcePredicate { node, result } => {
            let node = graph_node_mut(graph, &node)?;
            upsert_option(&mut node.options, "when", Expr::Bool(result));
            Ok(())
        }
    }
}

fn graph_node_mut<'a>(graph: &'a mut crate::Graph, id: &Symbol) -> Result<&'a mut crate::Node> {
    graph
        .nodes
        .iter_mut()
        .find(|node| node.id.as_symbol() == id)
        .ok_or_else(|| Error::Eval(format!("topology counterfactual: unknown node {id}")))
}

fn upsert_option(options: &mut Vec<(Symbol, Expr)>, key: &str, value: Expr) {
    if let Some((_, existing)) = options
        .iter_mut()
        .find(|(name, _)| name.namespace.is_none() && name.name.as_ref() == key)
    {
        *existing = value;
    } else {
        options.push((Symbol::new(key), value));
    }
}

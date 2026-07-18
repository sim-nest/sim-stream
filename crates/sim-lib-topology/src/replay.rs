//! Topology replay and counterfactual execution.

use std::collections::BTreeMap;

use sim_kernel::{Cx, Error, Expr, Result, Symbol};

use crate::{
    EdgeId, compile_graph,
    reflect::{TopologyEdgeVisit, TopologyRecordedReply, TopologyRunReport, TopologyVisit},
    run::{TopologyEvent, TopologyEventKind, run_graph},
};

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
    validate_replay_evidence(report)?;
    Ok(report.output.clone())
}

/// Validates the event evidence stored in a reflected run report.
pub fn validate_replay_evidence(report: &TopologyRunReport) -> Result<()> {
    if report.events.is_empty() {
        return replay_error("requires a reflected run report with events");
    }
    validate_event_shapes(report)?;
    validate_output_events(report)?;
    validate_visit_counts(report)?;
    validate_recorded_replies(report)?;
    Ok(())
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

fn validate_event_shapes(report: &TopologyRunReport) -> Result<()> {
    for (event_index, event) in report.events.iter().enumerate() {
        if event.node_index >= report.source_graph().nodes.len() {
            return replay_error(format!(
                "event {event_index} references unknown node {}",
                event.node_index
            ));
        }
        match event.kind {
            TopologyEventKind::Enqueued | TopologyEventKind::PortEmitted => {
                require_port_expr(event_index, event)?;
            }
            TopologyEventKind::EdgeRouted => {
                require_port_expr(event_index, event)?;
                let edge_index = event.edge_index.ok_or_else(|| {
                    Error::Eval(format!(
                        "topology replay evidence mismatch: event {event_index} missing edge"
                    ))
                })?;
                let edge = report.source_graph().edges.get(edge_index).ok_or_else(|| {
                    Error::Eval(format!(
                        "topology replay evidence mismatch: event {event_index} references unknown edge {edge_index}"
                    ))
                })?;
                let node = &report.source_graph().nodes[event.node_index];
                if edge.from.node != node.id {
                    return replay_error(format!(
                        "event {event_index} routes edge {} from node {}, not {}",
                        edge.id.0,
                        edge.from.node.as_symbol(),
                        node.id.as_symbol()
                    ));
                }
            }
            TopologyEventKind::OutputEmitted => {
                if event.expr.is_none() {
                    return replay_error(format!("event {event_index} missing output expr"));
                }
            }
            TopologyEventKind::NodeStarted => {}
        }
    }
    Ok(())
}

fn validate_output_events(report: &TopologyRunReport) -> Result<()> {
    let output = output_expr_from_events(&report.events);
    if output != report.output {
        return replay_error("output does not match output events");
    }
    Ok(())
}

fn validate_visit_counts(report: &TopologyRunReport) -> Result<()> {
    let mut node_visits = vec![0u32; report.source_graph().nodes.len()];
    let mut edge_visits = BTreeMap::<EdgeId, u32>::new();
    for event in &report.events {
        match event.kind {
            TopologyEventKind::NodeStarted => {
                node_visits[event.node_index] = node_visits[event.node_index].saturating_add(1);
            }
            TopologyEventKind::EdgeRouted => {
                let Some(edge_index) = event.edge_index else {
                    continue;
                };
                let Some(edge) = report.source_graph().edges.get(edge_index) else {
                    continue;
                };
                *edge_visits.entry(edge.id).or_default() += 1;
            }
            TopologyEventKind::Enqueued
            | TopologyEventKind::PortEmitted
            | TopologyEventKind::OutputEmitted => {}
        }
    }

    let expected_node_visits = report
        .source_graph()
        .nodes
        .iter()
        .enumerate()
        .map(|(node_index, node)| TopologyVisit {
            node: node.id.as_symbol().clone(),
            visits: node_visits[node_index],
        })
        .collect::<Vec<_>>();
    if expected_node_visits != report.node_visits {
        return replay_error("node visits do not match node start events");
    }

    let expected_edge_visits = report
        .source_graph()
        .edges
        .iter()
        .map(|edge| TopologyEdgeVisit {
            edge: edge.id,
            visits: edge_visits.get(&edge.id).copied().unwrap_or(0),
        })
        .collect::<Vec<_>>();
    if expected_edge_visits != report.edge_visits {
        return replay_error("edge visits do not match edge route events");
    }

    for visit in &report.node_visits {
        if !report
            .source_graph()
            .nodes
            .iter()
            .any(|node| node.id.as_symbol() == &visit.node)
        {
            return replay_error(format!("node visit references unknown node {}", visit.node));
        }
    }
    for visit in &report.edge_visits {
        if !report
            .source_graph()
            .edges
            .iter()
            .any(|edge| edge.id == visit.edge)
        {
            return replay_error(format!(
                "edge visit references unknown edge {}",
                visit.edge.0
            ));
        }
    }
    Ok(())
}

fn validate_recorded_replies(report: &TopologyRunReport) -> Result<()> {
    let expected = recorded_replies(report);
    if expected != report.recorded_replies {
        return replay_error("recorded replies do not match port events");
    }
    Ok(())
}

fn require_port_expr(event_index: usize, event: &TopologyEvent) -> Result<()> {
    if event.port.is_none() {
        return replay_error(format!("event {event_index} missing port"));
    }
    if event.expr.is_none() {
        return replay_error(format!("event {event_index} missing expr"));
    }
    Ok(())
}

fn output_expr_from_events(events: &[TopologyEvent]) -> Expr {
    let outputs = events
        .iter()
        .filter(|event| event.kind == TopologyEventKind::OutputEmitted)
        .filter_map(|event| event.expr.clone())
        .collect::<Vec<_>>();
    match outputs.as_slice() {
        [] => Expr::Nil,
        [single] => single.clone(),
        many => Expr::List(many.to_vec()),
    }
}

fn recorded_replies(report: &TopologyRunReport) -> Vec<TopologyRecordedReply> {
    report
        .events
        .iter()
        .filter(|event| event.kind == TopologyEventKind::PortEmitted)
        .filter_map(|event| {
            let node = report.source_graph().nodes.get(event.node_index)?;
            (node.verb.name.as_ref() == "call").then(|| TopologyRecordedReply {
                node: node.id.as_symbol().clone(),
                output: event.expr.clone().unwrap_or(Expr::Nil),
            })
        })
        .collect()
}

fn replay_error(message: impl Into<String>) -> Result<()> {
    Err(Error::Eval(format!(
        "topology replay evidence mismatch: {}",
        message.into()
    )))
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

//! Topology graph and run reflection.

use std::collections::VecDeque;

use sim_kernel::{Cx, Expr, Result, Symbol};

use crate::{
    CompiledGraph, Graph,
    capability::{
        require_graph_capabilities, topology_reflect_capability, topology_run_capability,
    },
    run::{TopologyEvent, TopologyEventKind, TopologyRun},
    text::graph_to_expr,
};

/// One node visit count in a reflected topology run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyVisit {
    /// The visited node.
    pub node: Symbol,
    /// Number of times the node was visited.
    pub visits: u32,
}

/// One edge route count in a reflected topology run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyEdgeVisit {
    /// The routed edge.
    pub edge: crate::EdgeId,
    /// Number of times the edge was routed.
    pub visits: u32,
}

/// Reflected node metadata with target redaction applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReflectedNode {
    /// The node id.
    pub id: Symbol,
    /// The node verb.
    pub verb: Symbol,
    /// The node target, present unless redacted.
    pub target: Option<Expr>,
    /// Whether the target was redacted.
    pub redacted: bool,
}

/// Reflected cell state with privacy redaction applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReflectedCell {
    /// The cell name.
    pub name: Symbol,
    /// The cell value, redacted if the cell is private.
    pub value: Expr,
    /// Whether the cell is marked private.
    pub private: bool,
    /// Whether the value was redacted.
    pub redacted: bool,
}

/// Recorded target output used by replay-oriented reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyRecordedReply {
    /// The call node that produced the output.
    pub node: Symbol,
    /// The recorded output value.
    pub output: Expr,
}

/// Compact reflection of one topology run.
#[derive(Clone, Debug)]
pub struct TopologyRunReport {
    /// Identifier of the run this report reflects.
    pub run_id: u64,
    /// The graph that was run.
    pub graph: Symbol,
    /// The run input.
    pub input: Expr,
    /// The run output.
    pub output: Expr,
    /// The recorded run events in order.
    pub events: Vec<TopologyEvent>,
    /// Per-node visit counts.
    pub node_visits: Vec<TopologyVisit>,
    /// Per-edge route counts.
    pub edge_visits: Vec<TopologyEdgeVisit>,
    /// Reflected node metadata.
    pub nodes: Vec<ReflectedNode>,
    /// Reflected final cell state.
    pub cells: Vec<ReflectedCell>,
    /// Recorded call-node outputs.
    pub recorded_replies: Vec<TopologyRecordedReply>,
    /// Whether any field in the report was redacted.
    pub redacted: bool,
    pub(crate) source_graph: Graph,
}

impl TopologyRunReport {
    /// Converts the run report into a stable expression map.
    pub fn as_expr(&self) -> Expr {
        Expr::Map(vec![
            entry("kind", Expr::Symbol(Symbol::new("topology-run"))),
            entry("run-id", number_expr(self.run_id)),
            entry("graph", Expr::Symbol(self.graph.clone())),
            entry("input", self.input.clone()),
            entry("output", self.output.clone()),
            entry("redacted", Expr::Bool(self.redacted)),
            entry("nodes", Expr::List(node_exprs(&self.nodes))),
            entry("cells", Expr::List(cell_exprs(&self.cells))),
            entry(
                "node-visits",
                Expr::List(node_visit_exprs(&self.node_visits)),
            ),
            entry(
                "edge-visits",
                Expr::List(edge_visit_exprs(&self.edge_visits)),
            ),
            entry("events", Expr::List(event_exprs(self))),
            entry(
                "recorded-replies",
                Expr::List(reply_exprs(&self.recorded_replies)),
            ),
        ])
    }

    /// Converts this report into a compact explanation expression.
    pub fn explanation_expr(&self) -> Expr {
        Expr::Map(vec![
            entry("kind", Expr::Symbol(Symbol::new("topology-explanation"))),
            entry("run-id", number_expr(self.run_id)),
            entry("graph", Expr::Symbol(self.graph.clone())),
            entry("summary", Expr::String(summary(self))),
            entry(
                "node-visits",
                Expr::List(node_visit_exprs(&self.node_visits)),
            ),
            entry("edge-choices", Expr::List(edge_choice_exprs(self))),
            entry("outputs", Expr::List(output_exprs(&self.events))),
        ])
    }

    /// Returns the unredacted graph retained for replay inside this crate.
    pub(crate) fn source_graph(&self) -> &Graph {
        &self.source_graph
    }
}

/// Bounded in-memory topology run history.
#[derive(Clone, Debug)]
pub struct TopologyHistory {
    capacity: usize,
    next_id: u64,
    entries: VecDeque<TopologyRunReport>,
}

impl TopologyHistory {
    /// Creates a history with the requested maximum number of retained runs.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            next_id: 1,
            entries: VecDeque::new(),
        }
    }

    /// Records a run and returns its assigned run id.
    pub fn record(&mut self, mut report: TopologyRunReport) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        report.run_id = id;
        if self.capacity > 0 {
            self.entries.push_back(report);
            while self.entries.len() > self.capacity {
                self.entries.pop_front();
            }
        }
        id
    }

    /// Returns a recorded run by id.
    pub fn get(&self, run_id: u64) -> Option<&TopologyRunReport> {
        self.entries.iter().find(|report| report.run_id == run_id)
    }

    /// Returns retained run ids in oldest-to-newest order.
    pub fn run_ids(&self) -> Vec<u64> {
        self.entries.iter().map(|report| report.run_id).collect()
    }
}

/// Reflects a graph as canonical topology data.
pub fn topology_reflect_graph(cx: &Cx, graph: &Graph) -> Expr {
    if cx.capabilities().contains(&topology_reflect_capability()) {
        return graph_to_expr(graph);
    }
    let mut reflected = graph.clone();
    for node in &mut reflected.nodes {
        if node.target.is_some() {
            node.target = Some(redacted_expr());
        }
    }
    for cell in &mut reflected.cells {
        if cell.private {
            cell.initial = redacted_expr();
        }
    }
    graph_to_expr(&reflected)
}

/// Runs a compiled topology and returns a reflected run report.
pub fn topology_reflect(
    cx: &mut Cx,
    graph: &Graph,
    plan: &CompiledGraph,
    input: Expr,
) -> Result<TopologyRunReport> {
    cx.require(&topology_run_capability())?;
    require_graph_capabilities(cx, graph)?;
    let input_for_report = input.clone();
    let mut run = TopologyRun::new(graph, plan, input)?;
    run.run(cx)?;
    let output = run.output_expr();
    let reveal = cx.capabilities().contains(&topology_reflect_capability());

    Ok(TopologyRunReport {
        run_id: 0,
        graph: graph.name.clone(),
        input: input_for_report,
        output,
        events: run.events().to_vec(),
        node_visits: reflect_node_visits(graph, &run.budget.node_visits),
        edge_visits: reflect_edge_visits(graph, &run.budget.edge_visits),
        nodes: reflect_nodes(graph, reveal),
        cells: reflect_cells(graph, &run, reveal)?,
        recorded_replies: recorded_replies(graph, run.events()),
        redacted: !reveal && has_sensitive_reflection(graph),
        source_graph: graph.clone(),
    })
}

/// Compiles, runs, and reflects a topology graph.
pub fn topology_reflect_run(cx: &mut Cx, graph: &Graph, input: Expr) -> Result<TopologyRunReport> {
    let plan = crate::compile_graph(cx, graph)?;
    topology_reflect(cx, graph, &plan, input)
}

/// Returns a run explanation expression.
pub fn topology_explain(report: &TopologyRunReport) -> Expr {
    report.explanation_expr()
}

fn reflect_node_visits(graph: &Graph, visits: &[u32]) -> Vec<TopologyVisit> {
    graph
        .nodes
        .iter()
        .zip(visits.iter())
        .map(|(node, visits)| TopologyVisit {
            node: node.id.as_symbol().clone(),
            visits: *visits,
        })
        .collect()
}

fn reflect_edge_visits(graph: &Graph, visits: &[u32]) -> Vec<TopologyEdgeVisit> {
    graph
        .edges
        .iter()
        .zip(visits.iter())
        .map(|(edge, visits)| TopologyEdgeVisit {
            edge: edge.id,
            visits: *visits,
        })
        .collect()
}

fn reflect_nodes(graph: &Graph, reveal: bool) -> Vec<ReflectedNode> {
    graph
        .nodes
        .iter()
        .map(|node| {
            let redacted = node.target.is_some() && !reveal;
            ReflectedNode {
                id: node.id.as_symbol().clone(),
                verb: node.verb.clone(),
                target: node.target.as_ref().map(|target| {
                    if redacted {
                        redacted_expr()
                    } else {
                        target.clone()
                    }
                }),
                redacted,
            }
        })
        .collect()
}

fn reflect_cells(graph: &Graph, run: &TopologyRun<'_>, reveal: bool) -> Result<Vec<ReflectedCell>> {
    graph
        .cells
        .iter()
        .map(|cell| {
            let redacted = cell.private && !reveal;
            Ok(ReflectedCell {
                name: cell.name.clone(),
                value: if redacted {
                    redacted_expr()
                } else {
                    run.cells().read(&cell.name)?
                },
                private: cell.private,
                redacted,
            })
        })
        .collect()
}

fn recorded_replies(graph: &Graph, events: &[TopologyEvent]) -> Vec<TopologyRecordedReply> {
    events
        .iter()
        .filter(|event| event.kind == TopologyEventKind::PortEmitted)
        .filter_map(|event| {
            let node = graph.nodes.get(event.node_index)?;
            (node.verb.name.as_ref() == "call").then(|| TopologyRecordedReply {
                node: node.id.as_symbol().clone(),
                output: event.expr.clone().unwrap_or(Expr::Nil),
            })
        })
        .collect()
}

fn has_sensitive_reflection(graph: &Graph) -> bool {
    graph.nodes.iter().any(|node| node.target.is_some())
        || graph.cells.iter().any(|cell| cell.private)
}

fn event_exprs(report: &TopologyRunReport) -> Vec<Expr> {
    report
        .events
        .iter()
        .map(|event| {
            let mut entries = vec![
                entry("kind", Expr::Symbol(event_kind_symbol(&event.kind))),
                entry("node", event_node_expr(report, event.node_index)),
            ];
            if let Some(port) = &event.port {
                entries.push(entry("port", Expr::Symbol(port.clone())));
            }
            if let Some(edge_index) = event.edge_index {
                entries.push(entry("edge", event_edge_expr(report, edge_index)));
            }
            if let Some(expr) = &event.expr {
                entries.push(entry("expr", expr.clone()));
            }
            Expr::Map(entries)
        })
        .collect()
}

fn node_exprs(nodes: &[ReflectedNode]) -> Vec<Expr> {
    nodes
        .iter()
        .map(|node| {
            let mut entries = vec![
                entry("id", Expr::Symbol(node.id.clone())),
                entry("verb", Expr::Symbol(node.verb.clone())),
                entry("redacted", Expr::Bool(node.redacted)),
            ];
            if let Some(target) = &node.target {
                entries.push(entry("target", target.clone()));
            }
            Expr::Map(entries)
        })
        .collect()
}

fn cell_exprs(cells: &[ReflectedCell]) -> Vec<Expr> {
    cells
        .iter()
        .map(|cell| {
            Expr::Map(vec![
                entry("name", Expr::Symbol(cell.name.clone())),
                entry("value", cell.value.clone()),
                entry("private", Expr::Bool(cell.private)),
                entry("redacted", Expr::Bool(cell.redacted)),
            ])
        })
        .collect()
}

fn node_visit_exprs(visits: &[TopologyVisit]) -> Vec<Expr> {
    visits
        .iter()
        .map(|visit| {
            Expr::Map(vec![
                entry("node", Expr::Symbol(visit.node.clone())),
                entry("visits", number_expr(u64::from(visit.visits))),
            ])
        })
        .collect()
}

fn edge_visit_exprs(visits: &[TopologyEdgeVisit]) -> Vec<Expr> {
    visits
        .iter()
        .map(|visit| {
            Expr::Map(vec![
                entry("edge", number_expr(u64::from(visit.edge.0))),
                entry("visits", number_expr(u64::from(visit.visits))),
            ])
        })
        .collect()
}

fn edge_choice_exprs(report: &TopologyRunReport) -> Vec<Expr> {
    report
        .events
        .iter()
        .filter(|event| event.kind == TopologyEventKind::EdgeRouted)
        .map(|event| {
            Expr::Map(vec![
                entry("from", event_node_expr(report, event.node_index)),
                entry(
                    "edge",
                    event
                        .edge_index
                        .map_or(Expr::Nil, |index| event_edge_expr(report, index)),
                ),
                entry(
                    "port",
                    event.port.clone().map(Expr::Symbol).unwrap_or(Expr::Nil),
                ),
            ])
        })
        .collect()
}

fn output_exprs(events: &[TopologyEvent]) -> Vec<Expr> {
    events
        .iter()
        .filter(|event| event.kind == TopologyEventKind::OutputEmitted)
        .filter_map(|event| event.expr.clone())
        .collect()
}

fn reply_exprs(replies: &[TopologyRecordedReply]) -> Vec<Expr> {
    replies
        .iter()
        .map(|reply| {
            Expr::Map(vec![
                entry("node", Expr::Symbol(reply.node.clone())),
                entry("output", reply.output.clone()),
            ])
        })
        .collect()
}

fn summary(report: &TopologyRunReport) -> String {
    format!(
        "run {} visited {} nodes, routed {} edges, emitted {} output(s)",
        report.graph,
        report
            .node_visits
            .iter()
            .filter(|visit| visit.visits > 0)
            .count(),
        report
            .edge_visits
            .iter()
            .filter(|visit| visit.visits > 0)
            .count(),
        output_exprs(&report.events).len()
    )
}

fn event_node_expr(report: &TopologyRunReport, node_index: usize) -> Expr {
    report
        .source_graph
        .nodes
        .get(node_index)
        .map(|node| Expr::Symbol(node.id.as_symbol().clone()))
        .unwrap_or(Expr::Nil)
}

fn event_edge_expr(report: &TopologyRunReport, edge_index: usize) -> Expr {
    report
        .source_graph
        .edges
        .get(edge_index)
        .map(|edge| number_expr(u64::from(edge.id.0)))
        .unwrap_or(Expr::Nil)
}

fn event_kind_symbol(kind: &TopologyEventKind) -> Symbol {
    Symbol::new(match kind {
        TopologyEventKind::Enqueued => "enqueued",
        TopologyEventKind::NodeStarted => "node-started",
        TopologyEventKind::PortEmitted => "port-emitted",
        TopologyEventKind::EdgeRouted => "edge-routed",
        TopologyEventKind::OutputEmitted => "output-emitted",
    })
}

fn redacted_expr() -> Expr {
    Expr::Symbol(Symbol::qualified("topology", "redacted"))
}

use sim_value::build::{entry, uint as number_expr};

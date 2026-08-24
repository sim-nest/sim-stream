//! Topology run-state and sequential core scheduler.

use std::collections::VecDeque;

use sim_kernel::{Cx, Error, Expr, Result, Symbol};

use crate::{
    Budget, CompiledGraph, Edge, Graph, Node,
    adapter::{call_target_expr, resolve_target},
    capability::{require_graph_capabilities, topology_run_capability},
    run_contract::{check_expr_shape, input_port, output_port},
    verb::{CoreRunState, VerbAction, run_core_node},
};

pub use crate::{
    run_cells::TopologyCells, run_nonlinear::TopologyNonlinearState,
    run_predicate::predicate_accepts,
};

/// Packet emitted from one node output port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyPacket {
    /// Source node index in the compiled graph.
    pub node_index: usize,
    /// Source output port.
    pub port: Symbol,
    /// Packet payload.
    pub expr: Expr,
}

/// Scheduled delivery to one node input port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkItem {
    /// Destination node index in the compiled graph.
    pub node_index: usize,
    /// Destination input port.
    pub port: Symbol,
    /// Delivered payload.
    pub expr: Expr,
}

/// Basic event kind emitted by the sequential topology scheduler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopologyEventKind {
    /// A work item was queued.
    Enqueued,
    /// A node began executing.
    NodeStarted,
    /// A node emitted an output port packet.
    PortEmitted,
    /// An edge routed a packet.
    EdgeRouted,
    /// A public output was produced.
    OutputEmitted,
}

/// Compact execution event for inspection and later replay support.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyEvent {
    /// Event kind.
    pub kind: TopologyEventKind,
    /// Node index associated with the event.
    pub node_index: usize,
    /// Optional port associated with the event.
    pub port: Option<Symbol>,
    /// Optional edge index associated with the event.
    pub edge_index: Option<usize>,
    /// Optional event payload.
    pub expr: Option<Expr>,
}

impl TopologyEvent {
    fn node(kind: TopologyEventKind, node_index: usize) -> Self {
        Self {
            kind,
            node_index,
            port: None,
            edge_index: None,
            expr: None,
        }
    }

    fn node_expr(kind: TopologyEventKind, node_index: usize, expr: Expr) -> Self {
        Self {
            kind,
            node_index,
            port: None,
            edge_index: None,
            expr: Some(expr),
        }
    }

    fn port(kind: TopologyEventKind, node_index: usize, port: Symbol, expr: Expr) -> Self {
        Self {
            kind,
            node_index,
            port: Some(port),
            edge_index: None,
            expr: Some(expr),
        }
    }

    fn edge(node_index: usize, port: Symbol, edge_index: usize, expr: Expr) -> Self {
        Self {
            kind: TopologyEventKind::EdgeRouted,
            node_index,
            port: Some(port),
            edge_index: Some(edge_index),
            expr: Some(expr),
        }
    }
}

/// Structured budget exhaustion details for a topology run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyBudgetError {
    /// Exhausted budget resource.
    pub resource: Symbol,
    /// Configured resource limit.
    pub limit: u32,
    /// Observed resource count.
    pub actual: u32,
}

impl TopologyBudgetError {
    /// Converts the budget error into a stable expression value.
    pub fn as_expr(&self) -> Expr {
        Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("kind")),
                Expr::Symbol(Symbol::new("topology-budget-exhausted")),
            ),
            (
                Expr::Symbol(Symbol::new("resource")),
                Expr::Symbol(self.resource.clone()),
            ),
            (
                Expr::Symbol(Symbol::new("limit")),
                Expr::String(self.limit.to_string()),
            ),
            (
                Expr::Symbol(Symbol::new("actual")),
                Expr::String(self.actual.to_string()),
            ),
        ])
    }

    fn into_error(self) -> Error {
        let value = self.as_expr();
        Error::Eval(format!(
            "topology budget exhausted: resource={} limit={} actual={} value={value:?}",
            self.resource, self.limit, self.actual,
        ))
    }
}

/// Runtime budget counters for one topology run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BudgetLedger {
    pub(crate) limits: Budget,
    /// Scheduler steps consumed.
    pub steps: u32,
    /// Per-node visit counts.
    pub node_visits: Vec<u32>,
    /// Per-edge route counts.
    pub edge_visits: Vec<u32>,
    /// Public outputs emitted.
    pub outputs: u32,
    /// Nested eval-fabric calls.
    pub child_runs: u32,
}

impl BudgetLedger {
    /// Creates a zeroed budget ledger for a compiled graph.
    pub fn new(limits: Budget, node_count: usize, edge_count: usize) -> Self {
        Self {
            limits,
            steps: 0,
            node_visits: vec![0; node_count],
            edge_visits: vec![0; edge_count],
            outputs: 0,
            child_runs: 0,
        }
    }

    /// Records one scheduler step.
    pub fn record_step(&mut self) -> Result<()> {
        self.steps = self.steps.saturating_add(1);
        self.require_at_most(self.steps, self.limits.max_steps, "max-steps")
    }

    /// Records one node visit.
    pub fn record_node_visit(&mut self, node_index: usize) -> Result<()> {
        self.node_visits[node_index] = self.node_visits[node_index].saturating_add(1);
        self.require_at_most(
            self.node_visits[node_index],
            self.limits.max_node_visits,
            "max-node-visits",
        )
    }

    /// Records one edge traversal.
    pub fn record_edge_visit(&mut self, edge_index: usize, edge_limit: Option<u32>) -> Result<()> {
        self.edge_visits[edge_index] = self.edge_visits[edge_index].saturating_add(1);
        let limit = edge_limit
            .unwrap_or(self.limits.max_edge_visits)
            .min(self.limits.max_edge_visits);
        let resource = if edge_limit.is_some() {
            "edge-max-visits"
        } else {
            "max-edge-visits"
        };
        self.require_at_most(self.edge_visits[edge_index], limit, resource)
    }

    /// Records one public output.
    pub fn record_output(&mut self) -> Result<()> {
        self.outputs = self.outputs.saturating_add(1);
        self.require_at_most(self.outputs, self.limits.max_outputs, "max-outputs")
    }

    /// Records one nested eval-fabric call.
    pub fn record_child_run(&mut self) -> Result<()> {
        self.child_runs = self.child_runs.saturating_add(1);
        self.require_at_most(
            self.child_runs,
            self.limits.max_child_runs,
            "max-child-runs",
        )
    }

    /// Checks one buffered merge node against the run budget.
    pub fn check_merge_buffer(&self, buffered: usize) -> Result<()> {
        let actual = u32::try_from(buffered).unwrap_or(u32::MAX);
        self.require_at_most(actual, self.limits.max_steps, "merge-buffer")
    }

    fn require_at_most(&self, actual: u32, limit: u32, resource: &str) -> Result<()> {
        if actual <= limit {
            Ok(())
        } else {
            Err(TopologyBudgetError {
                resource: Symbol::new(resource),
                limit,
                actual,
            }
            .into_error())
        }
    }
}

pub use crate::run_continuation::{
    TopologyBindingDescriptor, TopologyBindings, TopologyContinuation, TopologyProgress,
};
use crate::run_continuation::{topology_fingerprint, validate_budget_policy};

/// Live state for a sequential topology run.
pub struct TopologyRun<'a> {
    graph: &'a Graph,
    plan: &'a CompiledGraph,
    queue: VecDeque<WorkItem>,
    outputs: Vec<Expr>,
    cells: TopologyCells,
    nonlinear: TopologyNonlinearState,
    /// Budget counters for this run.
    pub budget: BudgetLedger,
    events: Vec<TopologyEvent>,
    bindings: TopologyBindings,
    bindings_validated: bool,
}

impl<'a> TopologyRun<'a> {
    /// Creates a run with one boundary input expression.
    pub fn new(graph: &'a Graph, plan: &'a CompiledGraph, input: Expr) -> Result<Self> {
        validate_budget_policy(&graph.budget)?;
        let mut run = Self {
            graph,
            plan,
            queue: VecDeque::new(),
            outputs: Vec::new(),
            cells: TopologyCells::new(graph)?,
            nonlinear: TopologyNonlinearState::new(plan.nodes.len()),
            budget: BudgetLedger::new(graph.budget.clone(), plan.nodes.len(), plan.edges.len()),
            events: Vec::new(),
            bindings: TopologyBindings::new(),
            bindings_validated: false,
        };
        for input_node in &plan.input_nodes {
            run.enqueue(WorkItem {
                node_index: *input_node,
                port: Symbol::new("in"),
                expr: input.clone(),
            });
        }
        Ok(run)
    }

    /// Runs until the queue is exhausted.
    pub fn run(&mut self, cx: &mut Cx) -> Result<()> {
        loop {
            match self.step(cx)? {
                TopologyProgress::Advanced | TopologyProgress::Output(_) => {}
                TopologyProgress::Exhausted | TopologyProgress::Complete => return Ok(()),
            }
        }
    }

    /// Advances exactly one scheduler work item.
    pub fn step(&mut self, cx: &mut Cx) -> Result<TopologyProgress> {
        self.validate_bindings()?;
        let Some(item) = self.queue.pop_front() else {
            return Ok(if self.outputs.is_empty() {
                TopologyProgress::Exhausted
            } else {
                TopologyProgress::Complete
            });
        };
        let output_start = self.outputs.len();
        self.budget.record_step()?;
        self.budget.record_node_visit(item.node_index)?;
        self.check_graph_input(cx, &item)?;
        self.check_node_input(cx, &item)?;
        self.events.push(TopologyEvent::node(
            TopologyEventKind::NodeStarted,
            item.node_index,
        ));

        let node = self.node(item.node_index)?;
        let bound = self.bindings.get(&node.id).map(|(_, value)| value);
        let actions = run_core_node(
            cx,
            self.graph,
            self.plan,
            CoreRunState {
                budget: &mut self.budget,
                cells: &mut self.cells,
                nonlinear: &mut self.nonlinear,
                bound_target: bound,
            },
            &item,
        )?;
        for action in actions {
            match action {
                VerbAction::Emit(packet) => {
                    self.check_node_output(cx, &packet)?;
                    self.events.push(TopologyEvent::port(
                        TopologyEventKind::PortEmitted,
                        packet.node_index,
                        packet.port.clone(),
                        packet.expr.clone(),
                    ));
                    self.route_packet(cx, packet)?;
                }
                VerbAction::Complete { node_index, expr } => {
                    self.push_output(cx, node_index, expr)?
                }
            }
        }
        let emitted = self.outputs[output_start..].to_vec();
        if emitted.is_empty() {
            Ok(TopologyProgress::Advanced)
        } else {
            Ok(TopologyProgress::Output(emitted))
        }
    }

    /// Installs live bindings. Required descriptor identities are checked before execution.
    pub fn set_bindings(&mut self, bindings: TopologyBindings) {
        self.bindings = bindings;
        self.bindings_validated = false;
    }

    /// Seals all scheduler state needed to resume at the next work item.
    pub fn continuation(&self) -> TopologyContinuation {
        TopologyContinuation {
            fingerprint: topology_fingerprint(self.graph, self.plan),
            queue: self.queue.iter().cloned().collect(),
            outputs: self.outputs.clone(),
            cells: self.cells.values().clone(),
            nonlinear: self.nonlinear.to_expr(),
            budget: self.budget.clone(),
            events: self.events.clone(),
            bindings: self.bindings.descriptors(),
        }
    }

    /// Restores a sealed continuation and validates it before any target can run.
    pub fn resume(
        graph: &'a Graph,
        plan: &'a CompiledGraph,
        continuation: TopologyContinuation,
        bindings: TopologyBindings,
    ) -> Result<Self> {
        if continuation.fingerprint != topology_fingerprint(graph, plan) {
            return Err(Error::Eval(
                "topology continuation: graph fingerprint mismatch".into(),
            ));
        }
        if continuation
            .queue
            .iter()
            .any(|item| item.node_index >= plan.nodes.len())
        {
            return Err(Error::Eval(
                "topology continuation: malformed queue entry".into(),
            ));
        }
        if continuation.budget.node_visits.len() != plan.nodes.len()
            || continuation.budget.edge_visits.len() != plan.edges.len()
            || continuation.budget.steps > graph.budget.max_steps
            || continuation.budget.outputs > graph.budget.max_outputs
            || continuation
                .budget
                .node_visits
                .iter()
                .any(|v| *v > graph.budget.max_node_visits)
            || continuation
                .budget
                .edge_visits
                .iter()
                .any(|v| *v > graph.budget.max_edge_visits)
        {
            return Err(Error::Eval(
                "topology continuation: widened or malformed counters".into(),
            ));
        }
        for (node, expected) in &continuation.bindings {
            let Some(source) = graph.nodes.iter().find(|source| &source.id == node) else {
                return Err(Error::Eval(
                    "topology continuation: unknown node binding".into(),
                ));
            };
            if expected.role != source.role || expected.options != source.options {
                return Err(Error::Eval(format!(
                    "topology continuation: binding static context mismatch {}",
                    node.as_symbol()
                )));
            }
            let Some((actual, _)) = bindings.get(node) else {
                return Err(Error::Eval(format!(
                    "topology continuation: missing live binding {}",
                    node.as_symbol()
                )));
            };
            if actual != expected {
                return Err(Error::Eval(format!(
                    "topology continuation: binding identity mismatch {}",
                    node.as_symbol()
                )));
            }
        }
        if bindings
            .entries
            .keys()
            .any(|node| !graph.nodes.iter().any(|n| &n.id == node))
            || continuation
                .bindings
                .keys()
                .any(|node| !graph.nodes.iter().any(|n| &n.id == node))
        {
            return Err(Error::Eval(
                "topology continuation: unknown node binding".into(),
            ));
        }
        let cells = TopologyCells::restore(graph, continuation.cells)?;
        let nonlinear =
            TopologyNonlinearState::from_expr(&continuation.nonlinear, plan.nodes.len())
                .map_err(|e| Error::Eval(format!("topology continuation: {e}")))?;
        Ok(Self {
            graph,
            plan,
            queue: continuation.queue.into(),
            outputs: continuation.outputs,
            cells,
            nonlinear,
            budget: continuation.budget,
            events: continuation.events,
            bindings,
            bindings_validated: true,
        })
    }

    fn validate_bindings(&mut self) -> Result<()> {
        if self.bindings_validated {
            return Ok(());
        }
        for node in self.bindings.entries.keys() {
            if !self
                .graph
                .nodes
                .iter()
                .any(|candidate| &candidate.id == node)
            {
                return Err(Error::Eval(format!(
                    "topology binding: unknown node {}",
                    node.as_symbol()
                )));
            }
        }
        for (id, (descriptor, _)) in &self.bindings.entries {
            let node = self
                .graph
                .nodes
                .iter()
                .find(|node| &node.id == id)
                .expect("binding node checked");
            if descriptor.role != node.role || descriptor.options != node.options {
                return Err(Error::Eval(format!(
                    "topology binding: static context mismatch {}",
                    id.as_symbol()
                )));
            }
        }
        self.bindings_validated = true;
        Ok(())
    }

    /// Returns public outputs in emission order.
    pub fn outputs(&self) -> &[Expr] {
        &self.outputs
    }

    /// Returns emitted execution events.
    pub fn events(&self) -> &[TopologyEvent] {
        &self.events
    }

    /// Returns run-local cell state.
    pub fn cells(&self) -> &TopologyCells {
        &self.cells
    }

    /// Converts outputs to the graph return expression.
    pub fn output_expr(&self) -> Expr {
        match self.outputs.as_slice() {
            [] => Expr::Nil,
            [single] => single.clone(),
            many => Expr::List(many.to_vec()),
        }
    }

    fn enqueue(&mut self, item: WorkItem) {
        self.events.push(TopologyEvent::port(
            TopologyEventKind::Enqueued,
            item.node_index,
            item.port.clone(),
            item.expr.clone(),
        ));
        self.queue.push_back(item);
    }

    fn push_output(&mut self, cx: &mut Cx, node_index: usize, expr: Expr) -> Result<()> {
        check_expr_shape(cx, "graph output", self.graph.output.as_ref(), &expr)?;
        self.budget.record_output()?;
        self.events.push(TopologyEvent::node_expr(
            TopologyEventKind::OutputEmitted,
            node_index,
            expr.clone(),
        ));
        self.outputs.push(expr);
        Ok(())
    }

    fn route_packet(&mut self, cx: &mut Cx, packet: TopologyPacket) -> Result<()> {
        let edge_indices = self.plan.outgoing_edges[packet.node_index].clone();
        for edge_index in edge_indices {
            let compiled = &self.plan.edges[edge_index];
            if compiled.from.port != packet.port {
                continue;
            }
            let edge = &self.graph.edges[compiled.source_index];
            if !edge_allows(cx, edge, &packet.expr)? {
                continue;
            }
            self.budget.record_edge_visit(edge_index, edge.max_visits)?;
            let routed = route_edge_expr(cx, edge, packet.expr.clone())?;
            self.check_edge_value(cx, compiled.to_node, &compiled.to.port, &routed)?;
            self.events.push(TopologyEvent::edge(
                packet.node_index,
                packet.port.clone(),
                edge_index,
                routed.clone(),
            ));
            self.enqueue(WorkItem {
                node_index: compiled.to_node,
                port: compiled.to.port.clone(),
                expr: routed,
            });
        }
        Ok(())
    }

    fn check_graph_input(&self, cx: &mut Cx, item: &WorkItem) -> Result<()> {
        if item.port == Symbol::new("in") && self.plan.input_nodes.contains(&item.node_index) {
            check_expr_shape(cx, "graph input", self.graph.input.as_ref(), &item.expr)?;
        }
        Ok(())
    }

    fn check_node_input(&self, cx: &mut Cx, item: &WorkItem) -> Result<()> {
        let node = self.node(item.node_index)?;
        check_expr_shape(
            cx,
            format!("node {} input", node.id.as_symbol()),
            node.input.as_ref(),
            &item.expr,
        )?;
        if let Some(port) = input_port(node, &item.port) {
            check_expr_shape(
                cx,
                format!("node {} input port {}", node.id.as_symbol(), port.name),
                port.shape.as_ref(),
                &item.expr,
            )?;
        }
        Ok(())
    }

    fn check_node_output(&self, cx: &mut Cx, packet: &TopologyPacket) -> Result<()> {
        let node = self.node(packet.node_index)?;
        check_expr_shape(
            cx,
            format!("node {} output", node.id.as_symbol()),
            node.output.as_ref(),
            &packet.expr,
        )?;
        if let Some(port) = output_port(node, &packet.port) {
            check_expr_shape(
                cx,
                format!("node {} output port {}", node.id.as_symbol(), port.name),
                port.shape.as_ref(),
                &packet.expr,
            )?;
        }
        Ok(())
    }

    fn check_edge_value(
        &self,
        cx: &mut Cx,
        node_index: usize,
        port_name: &Symbol,
        routed: &Expr,
    ) -> Result<()> {
        let node = self.node(node_index)?;
        if let Some(port) = input_port(node, port_name) {
            check_expr_shape(
                cx,
                format!(
                    "edge into node {} input port {}",
                    node.id.as_symbol(),
                    port.name
                ),
                port.shape.as_ref(),
                routed,
            )?;
        }
        Ok(())
    }

    fn node(&self, node_index: usize) -> Result<&'a Node> {
        self.graph
            .nodes
            .get(node_index)
            .ok_or_else(|| Error::Eval(format!("topology run: unknown node index {node_index}")))
    }
}

/// Runs a compiled graph with one input expression.
pub fn run_graph(cx: &mut Cx, graph: &Graph, plan: &CompiledGraph, input: Expr) -> Result<Expr> {
    run_graph_with_bindings(cx, graph, plan, input, TopologyBindings::new())
}

/// Runs a compiled graph with one input expression and explicit live node bindings.
pub fn run_graph_with_bindings(
    cx: &mut Cx,
    graph: &Graph,
    plan: &CompiledGraph,
    input: Expr,
    bindings: TopologyBindings,
) -> Result<Expr> {
    cx.require(&topology_run_capability())?;
    require_graph_capabilities(cx, graph)?;
    let mut run = TopologyRun::new(graph, plan, input)?;
    run.set_bindings(bindings);
    run.run(cx)?;
    Ok(run.output_expr())
}

fn edge_allows(cx: &mut Cx, edge: &Edge, input: &Expr) -> Result<bool> {
    match &edge.when {
        Some(predicate) => predicate_accepts(cx, predicate, input),
        None => Ok(true),
    }
}

fn route_edge_expr(cx: &mut Cx, edge: &crate::Edge, input: Expr) -> Result<Expr> {
    let transformed = match &edge.transform {
        Some(transform) => {
            let target = resolve_target(cx, transform)?;
            call_target_expr(cx, target, input)?
        }
        None => input,
    };

    if let Some(name) = &edge.as_name {
        Ok(Expr::Map(vec![(Expr::Symbol(name.clone()), transformed)]))
    } else {
        Ok(transformed)
    }
}

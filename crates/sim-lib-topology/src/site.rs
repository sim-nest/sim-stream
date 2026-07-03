//! `TopologySite` eval-fabric surface exposing a compiled graph as an endpoint.
//!
//! Connects topology graphs to the kernel `EvalFabric` contract so a topology
//! can serve as a live, location-transparent eval target.

use std::{any::Any, time::Duration};

use sim_kernel::{
    CapabilityName, ClassRef, Consistency, Cx, EvalFabric, EvalMode, EvalReply, EvalRequest, Expr,
    Object, ObjectCompat, Result, Symbol, Value,
};

use crate::{
    CompiledGraph, Graph, capability::topology_run_capability, compile_graph, run::run_graph,
};

/// Local eval fabric backed by a compiled topology graph.
#[sim_citizen_derive::non_citizen(
    reason = "live topology eval-fabric handle; reconstruct from topology/Package descriptor data",
    kind = "handle",
    descriptor = "topology/Package"
)]
#[derive(Clone, Debug)]
pub struct TopologyConnection {
    source: Graph,
    graph: CompiledGraph,
}

impl TopologyConnection {
    /// Creates a topology connection for an already compiled graph.
    pub fn new(source: Graph, graph: CompiledGraph) -> Self {
        Self { source, graph }
    }

    /// Returns the source graph data owned by this connection.
    pub fn source_graph(&self) -> &Graph {
        &self.source
    }

    /// Returns the compiled graph plan owned by this connection.
    pub fn graph(&self) -> &CompiledGraph {
        &self.graph
    }

    /// Returns the kind label used by topology adapter discovery.
    pub fn site_kind(&self) -> &'static str {
        "topology"
    }

    /// Runs one local eval request through this topology.
    pub fn request(
        &self,
        cx: &mut Cx,
        expr: Expr,
        timeout: Option<Duration>,
        required_capabilities: Vec<CapabilityName>,
    ) -> Result<Value> {
        let reply = self.realize(
            cx,
            EvalRequest {
                expr,
                result_shape: None,
                required_capabilities,
                deadline: timeout,
                consistency: Consistency::LocalFirst,
                mode: EvalMode::Eval,
                answer_limit: None,
                stream_buffer: None,
                stream: false,
                trace: false,
            },
        )?;
        Ok(reply.value)
    }
}

impl Object for TopologyConnection {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok(format!("#<topology-connection {}>", self.source.name))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ObjectCompat for TopologyConnection {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        cx.factory().nil()
    }

    fn as_eval_fabric(&self) -> Option<&dyn EvalFabric> {
        Some(self)
    }
}

impl EvalFabric for TopologyConnection {
    fn realize(&self, cx: &mut Cx, request: EvalRequest) -> Result<EvalReply> {
        answer_request(cx, self, request)
    }
}

/// Builds a local eval fabric whose site runs the provided topology graph.
pub fn connection_from_graph(cx: &mut Cx, graph: &Graph) -> Result<TopologyConnection> {
    let compiled = compile_graph(cx, graph)?;
    Ok(TopologyConnection::new(graph.clone(), compiled))
}

fn answer_request(
    cx: &mut Cx,
    connection: &TopologyConnection,
    request: EvalRequest,
) -> Result<EvalReply> {
    cx.require(&topology_run_capability())?;
    cx.require_all(&request.required_capabilities)?;
    let output = run_graph(
        cx,
        connection.source_graph(),
        connection.graph(),
        request.expr,
    )?;
    let reply = EvalReply {
        value: cx.factory().expr(output)?,
        diagnostics: cx.take_diagnostics(),
        trace: request
            .trace
            .then(|| cx.factory().symbol(Symbol::new("topology")).ok())
            .flatten(),
    };
    Ok(reply)
}

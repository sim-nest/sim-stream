//! `TopologySite` eval-fabric surface exposing a compiled graph as an endpoint.
//!
//! Connects topology graphs to the kernel `EvalFabric` contract so a topology
//! can serve as a live, location-transparent eval target.

use std::{any::Any, sync::Arc, time::Duration};

use sim_kernel::{
    Args, CORE_FUNCTION_CLASS_ID, Callable, CapabilityName, ClassRef, Consistency, Cx, Error,
    EvalFabric, EvalMode, EvalReply, EvalRequest, Expr, Object, ObjectCompat, Result, Symbol,
    Value,
};

use crate::{
    CompiledGraph, Graph, capability::topology_run_capability, compile_graph, parse_graph,
    run::run_graph, run_contract::check_value_shape,
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

#[derive(Clone, Debug)]
pub(crate) struct TopologySiteFactory {
    symbol: Symbol,
}

impl TopologySiteFactory {
    pub(crate) fn new(symbol: Symbol) -> Self {
        Self { symbol }
    }
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

impl Object for TopologySiteFactory {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok(format!("#<topology-site {}>", self.symbol))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ObjectCompat for TopologySiteFactory {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        cx.factory().class_stub(
            CORE_FUNCTION_CLASS_ID,
            Symbol::qualified("core", "Function"),
        )
    }

    fn as_callable(&self) -> Option<&dyn Callable> {
        Some(self)
    }
}

impl Callable for TopologySiteFactory {
    fn call(&self, cx: &mut Cx, args: Args) -> Result<Value> {
        let values = args.values();
        if values.len() != 1 {
            return Err(Error::Eval(format!(
                "{} expects 1 graph argument, got {}",
                self.symbol,
                values.len()
            )));
        }
        let graph = parse_graph(cx, values[0].clone())?;
        let connection = connection_from_graph(cx, &graph)?;
        cx.factory().opaque(Arc::new(connection))
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
    validate_request_controls(&request)?;
    let result_shape = request.result_shape.clone();
    let trace = request.trace;
    let output = run_graph(
        cx,
        connection.source_graph(),
        connection.graph(),
        request.expr,
    )?;
    let value = cx.factory().expr(output)?;
    check_value_shape(cx, "request result", result_shape.as_ref(), value.clone())?;
    let reply = EvalReply {
        value,
        diagnostics: cx.take_diagnostics(),
        trace: trace
            .then(|| cx.factory().symbol(Symbol::new("topology")).ok())
            .flatten(),
    };
    Ok(reply)
}

fn validate_request_controls(request: &EvalRequest) -> Result<()> {
    if request.mode != EvalMode::Eval {
        return Err(sim_kernel::Error::Eval(format!(
            "topology request: unsupported eval mode {}",
            request.mode.as_symbol()
        )));
    }
    if request.deadline.is_some() {
        return Err(sim_kernel::Error::Eval(
            "topology request: deadline is unsupported".to_owned(),
        ));
    }
    if matches!(request.answer_limit, Some(0)) {
        return Err(sim_kernel::Error::Eval(
            "topology request: answer_limit must be greater than zero".to_owned(),
        ));
    }
    if request.stream || request.stream_buffer.is_some() {
        return Err(sim_kernel::Error::Eval(
            "topology request: streaming replies are unsupported".to_owned(),
        ));
    }
    Ok(())
}

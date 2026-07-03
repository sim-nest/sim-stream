use std::sync::Arc;

use sim_kernel::{
    CapabilityName, Consistency, Cx, DefaultFactory, EagerPolicy, Error, EvalFabric, EvalMode,
    EvalRequest, Expr, Symbol, eval_fabric_capability,
};

use crate::{Edge, Graph, Node, PortRef, connection_from_graph, topology_run_capability};

#[test]
fn topology_connection_can_be_used_as_eval_fabric() {
    let mut cx = runtime_cx();
    let connection = connection_from_graph(&mut cx, &identity_graph()).expect("connection");
    assert_eq!(connection.site_kind(), "topology");

    let output = connection
        .request(
            &mut cx,
            Expr::String("request-ok".to_owned()),
            None,
            Vec::new(),
        )
        .expect("topology request")
        .object()
        .as_expr(&mut cx)
        .expect("expr output");
    assert_eq!(output, Expr::String("request-ok".to_owned()));
}

#[test]
fn topology_connection_honors_request_capabilities_and_trace() {
    let mut cx = runtime_cx();
    cx.grant(CapabilityName::new("client-cap"));
    let connection = connection_from_graph(&mut cx, &identity_graph()).expect("connection");
    let input = Expr::List(vec![Expr::Symbol(Symbol::new("payload"))]);
    let reply = connection
        .realize(
            &mut cx,
            EvalRequest {
                expr: input.clone(),
                result_shape: None,
                required_capabilities: vec![CapabilityName::new("client-cap")],
                deadline: None,
                consistency: Consistency::LocalFirst,
                mode: EvalMode::Eval,
                answer_limit: None,
                stream_buffer: None,
                stream: false,
                trace: true,
            },
        )
        .expect("topology reply");

    assert_eq!(reply.value.object().as_expr(&mut cx).unwrap(), input);
    assert!(reply.trace.is_some());
}

#[test]
fn topology_connection_requires_client_capabilities() {
    let mut cx = runtime_cx();
    let connection = connection_from_graph(&mut cx, &identity_graph()).expect("connection");
    let error = match connection.realize(
        &mut cx,
        EvalRequest {
            expr: Expr::String("blocked".to_owned()),
            result_shape: None,
            required_capabilities: vec![CapabilityName::new("client-cap")],
            deadline: None,
            consistency: Consistency::LocalFirst,
            mode: EvalMode::Eval,
            answer_limit: None,
            stream_buffer: None,
            stream: false,
            trace: false,
        },
    ) {
        Ok(_) => panic!("client capability should be required"),
        Err(error) => error,
    };

    let Error::CapabilityDenied { capability } = error else {
        panic!("unexpected client-cap error: {error}");
    };
    assert_eq!(capability, CapabilityName::new("client-cap"));
}

#[test]
fn topology_connection_requires_topology_run_capability() {
    let mut cx = Cx::new(Arc::new(EagerPolicy), Arc::new(DefaultFactory));
    let connection = connection_from_graph(&mut cx, &identity_graph()).expect("connection");

    let error = connection
        .request(
            &mut cx,
            Expr::String("blocked".to_owned()),
            None,
            Vec::new(),
        )
        .expect_err("topology run should require capability");
    let Error::CapabilityDenied { capability } = error else {
        panic!("unexpected topology-run error: {error}");
    };
    assert_eq!(capability, topology_run_capability());
}

fn runtime_cx() -> Cx {
    let mut cx = Cx::new(Arc::new(EagerPolicy), Arc::new(DefaultFactory));
    cx.grant(eval_fabric_capability());
    cx.grant(topology_run_capability());
    cx
}

fn identity_graph() -> Graph {
    let mut graph = Graph::minimal("identity-topology");
    graph.nodes = vec![Node::named("in", "in"), Node::named("out", "out")];
    graph.edges = vec![Edge::new(0, PortRef::output("in"), PortRef::input("out"))];
    graph
}

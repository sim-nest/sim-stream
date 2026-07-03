use sim_kernel::{Expr, Result, Symbol};

use crate::{
    ClockDomain, LatencyClass, PlacedFragment, RateContract, StreamDirection, StreamEndpoint,
    StreamEndpointKind, StreamMedia, stream_edge,
};

struct TestEndpoint;

impl StreamEndpoint for TestEndpoint {
    fn endpoint_id(&self) -> Symbol {
        Symbol::qualified("test", "endpoint")
    }

    fn endpoint_kind(&self) -> StreamEndpointKind {
        StreamEndpointKind::EvalSite
    }

    fn clock_domain(&self) -> ClockDomain {
        ClockDomain::Control
    }

    fn latency_class(&self) -> LatencyClass {
        LatencyClass::Interactive
    }
}

#[test]
fn placed_fragment_carries_stream_edges() -> Result<()> {
    let output = stream_edge(
        "out",
        StreamMedia::Data,
        StreamDirection::Source,
        RateContract::control(),
    );
    let envelope = output.result_envelope(0, Expr::String("ok".to_owned()))?;
    let fragment = PlacedFragment::new(
        Symbol::qualified("test", "fragment"),
        Expr::String("node".to_owned()),
    )
    .with_output_edge(output.with_envelopes(vec![envelope.clone()]));

    assert_eq!(fragment.output_envelopes(), vec![envelope]);
    Ok(())
}

#[test]
fn endpoint_rejects_input_edge_clock_mismatch() {
    let input = stream_edge(
        "in",
        StreamMedia::Data,
        StreamDirection::Sink,
        RateContract::sample_exact(None),
    );
    let err = TestEndpoint.accept_input_edges(&[input]).unwrap_err();

    assert!(err.to_string().contains("does not match endpoint"));
}

use sim_kernel::{Error, Expr, Symbol};

use super::test_cx;
use crate::{
    InstrumentTopologyAdapter, InstrumentTopologyCord, InstrumentTopologyJack,
    InstrumentTopologyModule, InstrumentTopologySpec, PortRef, validate::validate_graph,
};

#[test]
fn instrument_adapter_maps_modules_cords_and_normalled_defaults() {
    let adapter = InstrumentTopologyAdapter;
    let spec = identity_spec()
        .with_module(
            InstrumentTopologyModule::new("gain", Symbol::new("gain"))
                .with_input(InstrumentTopologyJack::value("in", true))
                .with_input(
                    InstrumentTopologyJack::value("bias", true)
                        .with_normalled_default(Expr::String("unity".to_owned())),
                )
                .with_output(InstrumentTopologyJack::value("out", true))
                .with_setting(Symbol::new("gain"), Expr::String("0.5".to_owned()))
                .with_raw(Symbol::new("raw-gain"), Expr::String("64".to_owned())),
        )
        .with_cord(InstrumentTopologyCord::new(
            PortRef::output("in"),
            PortRef::input("gain"),
        ))
        .with_cord(InstrumentTopologyCord::new(
            PortRef::output("gain"),
            PortRef::input("out"),
        ));
    let graph = adapter.graph_from_spec(&spec);

    validate_graph(&mut test_cx(), &graph).expect("normalled graph is valid");
    let gain = graph
        .nodes
        .iter()
        .find(|node| node.id.as_symbol() == &Symbol::new("gain"))
        .expect("gain node");
    assert!(
        gain.inputs
            .iter()
            .any(|port| port.name == Symbol::new("bias") && !port.required)
    );
    assert!(
        gain.options
            .iter()
            .any(|(key, _)| key.name.as_ref() == "settings")
    );
    assert!(
        gain.options
            .iter()
            .any(|(key, _)| key.name.as_ref() == "normalled-defaults")
    );
}

#[test]
fn instrument_adapter_rejects_invalid_cord_endpoint_node() {
    let adapter = InstrumentTopologyAdapter;
    let graph = adapter.graph_from_spec(&identity_spec().with_cord(InstrumentTopologyCord::new(
        PortRef::output("missing"),
        PortRef::input("out"),
    )));

    assert_validate_error(&graph, &["unknown output endpoint node missing"]);
}

#[test]
fn instrument_adapter_rejects_missing_port() {
    let adapter = InstrumentTopologyAdapter;
    let graph = adapter.graph_from_spec(
        &identity_spec()
            .with_module(
                InstrumentTopologyModule::new("amp", Symbol::new("amp"))
                    .with_input(InstrumentTopologyJack::value("audio", true))
                    .with_output(InstrumentTopologyJack::value("out", true)),
            )
            .with_cord(InstrumentTopologyCord::new(
                PortRef::output("in"),
                PortRef::new("amp", Symbol::new("missing")),
            )),
    );

    assert_validate_error(&graph, &["unknown input endpoint port amp:missing"]);
}

#[test]
fn instrument_adapter_rejects_unbounded_cycle() {
    let adapter = InstrumentTopologyAdapter;
    let graph = adapter.graph_from_spec(
        &identity_spec()
            .with_module(module("a"))
            .with_module(module("b"))
            .with_cord(InstrumentTopologyCord::new(
                PortRef::output("in"),
                PortRef::input("a"),
            ))
            .with_cord(InstrumentTopologyCord::new(
                PortRef::output("a"),
                PortRef::input("b"),
            ))
            .with_cord(InstrumentTopologyCord::new(
                PortRef::output("b"),
                PortRef::input("a"),
            ))
            .with_cord(InstrumentTopologyCord::new(
                PortRef::output("b"),
                PortRef::input("out"),
            )),
    );

    assert_validate_error(&graph, &["unbounded cycle", "a -> b -> a"]);
}

fn identity_spec() -> InstrumentTopologySpec {
    InstrumentTopologySpec::new(Symbol::new("instrument-test"))
        .with_module(
            InstrumentTopologyModule::new("in", Symbol::new("in"))
                .with_output(InstrumentTopologyJack::value("out", true)),
        )
        .with_module(
            InstrumentTopologyModule::new("out", Symbol::new("out"))
                .with_input(InstrumentTopologyJack::value("in", true)),
        )
}

fn module(name: &str) -> InstrumentTopologyModule {
    InstrumentTopologyModule::new(name, Symbol::new("module"))
        .with_input(InstrumentTopologyJack::value("in", true))
        .with_output(InstrumentTopologyJack::value("out", true))
}

fn assert_validate_error(graph: &crate::Graph, fragments: &[&str]) {
    let error = validate_graph(&mut test_cx(), graph).expect_err("validation should fail");
    let Error::Eval(message) = error else {
        panic!("unexpected validation error type: {error}");
    };
    for fragment in fragments {
        assert!(
            message.contains(fragment),
            "validation error {message:?} did not contain {fragment:?}"
        );
    }
}

use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use sim_kernel::{Cx, DefaultFactory, EagerPolicy, Error, Expr, Symbol};

use crate::{
    TopologyRegistry, install_topology_lib, parse_graph, text::graph_from_text,
    text::graph_to_expr, topology_def, topology_file_capability, topology_get, topology_list,
    topology_load_file, topology_reload, topology_remove, topology_run_capability,
    topology_write_capability,
};

#[test]
fn registry_def_get_list_remove_require_write_capability() {
    let mut cx = test_cx();
    let mut registry = TopologyRegistry::new();
    let graph = identity_graph("registry-flow");
    let name = Symbol::new("registry-flow");

    let denied = topology_def(&mut cx, &mut registry, name.clone(), graph.clone())
        .expect_err("write capability required");
    assert_capability(denied, topology_write_capability());

    cx.grant(topology_write_capability());
    let entry = topology_def(&mut cx, &mut registry, name.clone(), graph).expect("defined graph");
    assert_eq!(entry.name, name);
    assert_eq!(topology_list(&registry), vec![Symbol::new("registry-flow")]);
    assert_eq!(
        topology_get(&registry, &Symbol::new("registry-flow"))
            .expect("registered graph")
            .graph
            .nodes
            .len(),
        2
    );

    let removed = topology_remove(&mut cx, &mut registry, &Symbol::new("registry-flow"))
        .expect("removed graph")
        .expect("entry existed");
    assert_eq!(removed.name, Symbol::new("registry-flow"));
    assert!(topology_get(&registry, &Symbol::new("registry-flow")).is_none());
}

#[test]
fn registry_package_load_requires_file_and_write_capabilities() {
    let path = write_package("capabilities", package_source("capability-flow", 8));
    let mut cx = test_cx();
    let mut registry = TopologyRegistry::new();

    let denied =
        topology_load_file(&mut cx, &mut registry, &path).expect_err("file capability required");
    assert_capability(denied, topology_file_capability());

    cx.grant(topology_file_capability());
    let denied =
        topology_load_file(&mut cx, &mut registry, &path).expect_err("write capability required");
    assert_capability(denied, topology_write_capability());

    cx.grant(topology_write_capability());
    let entry = topology_load_file(&mut cx, &mut registry, &path).expect("loaded package");
    assert_eq!(entry.name, Symbol::new("capability-flow"));

    let _ = fs::remove_file(path);
}

#[test]
fn registry_package_can_be_loaded_named_run_and_reloaded() {
    let path = write_package("reload", package_source("reload-flow", 8));
    let mut cx = test_cx();
    cx.grant(topology_file_capability());
    cx.grant(topology_write_capability());
    cx.grant(topology_run_capability());
    let mut registry = TopologyRegistry::new();

    let entry = topology_load_file(&mut cx, &mut registry, &path).expect("loaded package");
    assert_eq!(entry.name, Symbol::new("reload-flow"));
    let output = entry
        .run(&mut cx, Expr::String("payload".to_owned()))
        .expect("ran package graph");
    assert_eq!(output, Expr::String("payload".to_owned()));

    fs::write(&path, package_source("reload-flow", 17)).expect("rewrite package");
    let reloaded =
        topology_reload(&mut cx, &mut registry, &Symbol::new("reload-flow")).expect("reloaded");

    assert_eq!(reloaded.graph.budget.max_steps, 17);
    assert_eq!(
        topology_get(&registry, &Symbol::new("reload-flow"))
            .expect("registry entry")
            .graph
            .budget
            .max_steps,
        17
    );

    let _ = fs::remove_file(path);
}

#[test]
fn registry_installs_topology_function_surface() {
    let mut cx = test_cx();
    cx.grant(topology_write_capability());
    install_topology_lib(&mut cx).expect("installed topology lib");

    let graph = identity_graph("function-flow");
    let def = cx
        .resolve_function(&Symbol::qualified("topology", "def"))
        .expect("topology/def");
    let value = cx
        .call_exprs(
            def,
            vec![
                Expr::Symbol(Symbol::new("function-flow")),
                graph_to_expr(&graph),
            ],
        )
        .expect("called topology/def");
    let reparsed = parse_graph(&mut cx, value).expect("def returned graph data");
    assert_eq!(reparsed.name, Symbol::new("function-flow"));

    let list = cx
        .resolve_function(&Symbol::qualified("topology", "list"))
        .expect("topology/list");
    let value = cx
        .call_exprs(list, Vec::new())
        .expect("called topology/list");
    assert_eq!(
        value.object().as_expr(&mut cx).expect("symbol list"),
        Expr::List(vec![Expr::Symbol(Symbol::new("function-flow"))])
    );
}

fn test_cx() -> Cx {
    Cx::new(Arc::new(EagerPolicy), Arc::new(DefaultFactory))
}

fn identity_graph(name: &str) -> crate::Graph {
    graph_from_text(&format!(
        r#"
topology {name}
node in verb=in
node out verb=out
wire in -> out
"#
    ))
    .expect("identity graph")
}

fn package_source(name: &str, max_steps: u32) -> String {
    format!(
        r#"
graph:
topology {name}
node in verb=in
node out verb=out
wire in -> out
budget max-steps={max_steps}

metadata:
revision={max_steps}

tests:
smoke input="payload" expect="payload"
"#
    )
}

fn write_package(label: &str, source: String) -> PathBuf {
    let path = temp_path(label);
    fs::write(&path, source).expect("write package");
    path
}

fn temp_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "sim-topology-registry-{label}-{}-{nanos}.simtopo",
        std::process::id()
    ))
}

fn assert_capability(error: Error, expected: sim_kernel::CapabilityName) {
    let Error::CapabilityDenied { capability } = error else {
        panic!("unexpected error: {error}");
    };
    assert_eq!(capability, expected);
}

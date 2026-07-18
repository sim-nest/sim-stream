use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use sim_kernel::{Args, Cx, DefaultFactory, EagerPolicy, Error, Expr, Symbol};

use crate::{
    Cell, Edge, Graph, Node, PortRef, TopologyPackageSource, TopologyRegistry,
    install_topology_lib, parse_graph, text::graph_from_text, text::graph_to_expr, topology_def,
    topology_file_capability, topology_get, topology_list, topology_load_file,
    topology_load_source, topology_reflect_capability, topology_reload, topology_remove,
    topology_run_capability, topology_write_capability,
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
fn registry_def_rejects_invalid_graph_before_insertion() {
    let mut cx = test_cx();
    cx.grant(topology_write_capability());
    let mut registry = TopologyRegistry::new();
    let graph = invalid_endpoint_graph("invalid-def-flow");

    let error = topology_def(
        &mut cx,
        &mut registry,
        Symbol::new("invalid-def-flow"),
        graph,
    )
    .expect_err("invalid graph should fail");

    assert!(error.to_string().contains("unknown input endpoint node"));
    assert!(topology_get(&registry, &Symbol::new("invalid-def-flow")).is_none());
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
fn registry_package_load_rejects_invalid_graph_before_insertion() {
    let path = write_package("invalid-load", invalid_package_source("invalid-load-flow"));
    let mut cx = test_cx();
    cx.grant(topology_file_capability());
    cx.grant(topology_write_capability());
    let mut registry = TopologyRegistry::new();

    let error =
        topology_load_file(&mut cx, &mut registry, &path).expect_err("invalid package graph");

    assert!(error.to_string().contains("unknown input endpoint node"));
    assert!(topology_list(&registry).is_empty());

    let _ = fs::remove_file(path);
}

#[test]
fn registry_load_source_and_reload_use_table_descriptor_without_file_capability() {
    let mut cx = test_cx();
    cx.grant(topology_write_capability());
    let mut registry = TopologyRegistry::new();
    let initial = cx
        .factory()
        .string(package_source("table-flow", 8))
        .expect("package value");
    let table = cx
        .new_table(vec![(Symbol::new("pkg"), initial)])
        .expect("table source");

    let entry = topology_load_source(
        &mut cx,
        &mut registry,
        TopologyPackageSource::table_entry(table.clone(), Symbol::new("pkg")),
    )
    .expect("loaded table source");

    assert_eq!(entry.name, Symbol::new("table-flow"));
    assert_eq!(entry.graph.budget.max_steps, 8);

    let updated = cx
        .factory()
        .string(package_source("table-flow", 17))
        .expect("updated package value");
    table
        .object()
        .as_table_impl()
        .expect("table implementation")
        .set(&mut cx, Symbol::new("pkg"), updated)
        .expect("updated source table");

    let reloaded =
        topology_reload(&mut cx, &mut registry, &Symbol::new("table-flow")).expect("reloaded");

    assert_eq!(reloaded.graph.budget.max_steps, 17);
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

#[test]
fn registry_runtime_get_redacts_without_reflect_capability() {
    let mut cx = test_cx();
    cx.grant(topology_write_capability());
    install_topology_lib(&mut cx).expect("installed topology lib");
    let graph = sensitive_registry_graph();

    cx.call_function(
        &Symbol::qualified("topology", "def"),
        Args::new(vec![
            cx.factory()
                .symbol(Symbol::new("sensitive-flow"))
                .expect("name value"),
            cx.factory()
                .expr(graph_to_expr(&graph))
                .expect("graph value"),
        ]),
    )
    .expect("defined graph");

    let redacted = cx
        .call_function(
            &Symbol::qualified("topology", "get"),
            Args::new(vec![
                cx.factory()
                    .symbol(Symbol::new("sensitive-flow"))
                    .expect("name value"),
            ]),
        )
        .expect("got graph")
        .object()
        .as_expr(&mut cx)
        .expect("graph expr");

    assert!(expr_contains_symbol(
        &redacted,
        &Symbol::qualified("topology", "redacted")
    ));
    assert!(!expr_contains_symbol(
        &redacted,
        &Symbol::qualified("test", "secret")
    ));
    assert!(!expr_contains_string(&redacted, "raw-secret"));

    cx.grant(topology_reflect_capability());
    let revealed = cx
        .call_function(
            &Symbol::qualified("topology", "get"),
            Args::new(vec![
                cx.factory()
                    .symbol(Symbol::new("sensitive-flow"))
                    .expect("name value"),
            ]),
        )
        .expect("got graph")
        .object()
        .as_expr(&mut cx)
        .expect("graph expr");

    assert!(expr_contains_symbol(
        &revealed,
        &Symbol::qualified("test", "secret")
    ));
    assert!(expr_contains_string(&revealed, "raw-secret"));
}

#[test]
fn registry_runtime_load_source_accepts_table_value() {
    let mut cx = test_cx();
    cx.grant(topology_write_capability());
    install_topology_lib(&mut cx).expect("installed topology lib");
    let package = cx
        .factory()
        .string(package_source("runtime-table-flow", 11))
        .expect("package value");
    let table = cx
        .new_table(vec![(Symbol::new("pkg"), package)])
        .expect("table source");
    let key = cx
        .factory()
        .symbol(Symbol::new("pkg"))
        .expect("key symbol value");

    let value = cx
        .call_function(
            &Symbol::qualified("topology", "load-source"),
            Args::new(vec![table, key]),
        )
        .expect("loaded table source");
    let reparsed = parse_graph(&mut cx, value).expect("graph returned");

    assert_eq!(reparsed.name, Symbol::new("runtime-table-flow"));
    assert_eq!(reparsed.budget.max_steps, 11);
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

fn invalid_endpoint_graph(name: &str) -> Graph {
    let mut graph = Graph::minimal(name);
    graph.nodes = vec![Node::named("in", "in"), Node::named("out", "out")];
    graph.edges = vec![Edge::new(
        0,
        PortRef::output("in"),
        PortRef::input("missing"),
    )];
    graph
}

fn sensitive_registry_graph() -> Graph {
    let mut graph = Graph::minimal("sensitive-flow");
    let mut call = Node::named("call", "call");
    call.target = Some(Expr::Symbol(Symbol::qualified("test", "secret")));
    let mut cell = Cell::new(
        Symbol::new("secret-log"),
        Expr::String("raw-secret".to_owned()),
    );
    cell.private = true;
    graph.nodes = vec![
        Node::named("in", "in"),
        call,
        Node::named("save", "cell"),
        Node::named("out", "out"),
    ];
    graph.edges = vec![
        Edge::new(0, PortRef::output("in"), PortRef::input("call")),
        Edge::new(1, PortRef::output("call"), PortRef::input("save")),
        Edge::new(2, PortRef::output("save"), PortRef::input("out")),
    ];
    graph.cells = vec![cell];
    graph
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

fn invalid_package_source(name: &str) -> String {
    format!(
        r#"
graph:
topology {name}
node in verb=in
node out verb=out
wire in -> missing
"#
    )
}

fn expr_contains_symbol(expr: &Expr, expected: &Symbol) -> bool {
    match expr {
        Expr::Symbol(symbol) => symbol == expected,
        Expr::List(items) | Expr::Vector(items) | Expr::Set(items) => items
            .iter()
            .any(|item| expr_contains_symbol(item, expected)),
        Expr::Map(entries) => entries.iter().any(|(key, value)| {
            expr_contains_symbol(key, expected) || expr_contains_symbol(value, expected)
        }),
        Expr::Call { operator, args } => {
            expr_contains_symbol(operator, expected)
                || args.iter().any(|arg| expr_contains_symbol(arg, expected))
        }
        Expr::Infix {
            left,
            operator,
            right,
        } => {
            expr_contains_symbol(left, expected)
                || operator == expected
                || expr_contains_symbol(right, expected)
        }
        Expr::Prefix { operator, arg } | Expr::Postfix { operator, arg } => {
            operator == expected || expr_contains_symbol(arg, expected)
        }
        Expr::Block(items) => items
            .iter()
            .any(|item| expr_contains_symbol(item, expected)),
        Expr::Quote { expr, .. } | Expr::Annotated { expr, .. } => {
            expr_contains_symbol(expr, expected)
        }
        Expr::Extension { payload, .. } => expr_contains_symbol(payload, expected),
        _ => false,
    }
}

fn expr_contains_string(expr: &Expr, expected: &str) -> bool {
    match expr {
        Expr::String(text) => text == expected,
        Expr::List(items) | Expr::Vector(items) | Expr::Set(items) => items
            .iter()
            .any(|item| expr_contains_string(item, expected)),
        Expr::Map(entries) => entries.iter().any(|(key, value)| {
            expr_contains_string(key, expected) || expr_contains_string(value, expected)
        }),
        Expr::Call { operator, args } => {
            expr_contains_string(operator, expected)
                || args.iter().any(|arg| expr_contains_string(arg, expected))
        }
        Expr::Infix { left, right, .. } => {
            expr_contains_string(left, expected) || expr_contains_string(right, expected)
        }
        Expr::Prefix { arg, .. } | Expr::Postfix { arg, .. } => expr_contains_string(arg, expected),
        Expr::Block(items) => items
            .iter()
            .any(|item| expr_contains_string(item, expected)),
        Expr::Quote { expr, .. } | Expr::Annotated { expr, .. } => {
            expr_contains_string(expr, expected)
        }
        Expr::Extension { payload, .. } => expr_contains_string(payload, expected),
        _ => false,
    }
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

use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use sim_kernel::{CapabilityName, Expr, NumberLiteral, Symbol};

use crate::{load_package_file, parse_package};

#[test]
fn package_parse_sections_attach_graph_tests_metadata_and_capabilities() {
    let package = parse_package(
        r#"
graph:
topology packaged-flow
node in verb=in
node out verb=out
wire in -> out
budget max-steps=9

metadata:
owner="ops team"

tests:
smoke input="hello" expect="hello"

capabilities:
agent/run
topology-extra
vendor:read
"#,
    )
    .expect("parsed package");

    assert_eq!(package.name(), &Symbol::new("packaged-flow"));
    assert_eq!(package.graph.budget.max_steps, 9);
    assert_eq!(package.metadata[0].0, Symbol::new("owner"));
    assert_eq!(package.metadata[0].1, Expr::String("ops team".to_owned()));
    assert_eq!(package.tests[0].name, Symbol::new("smoke"));
    assert_eq!(package.tests[0].input, Expr::String("hello".to_owned()));
    assert_eq!(package.tests[0].expect, Expr::String("hello".to_owned()));
    assert_eq!(
        package.capabilities,
        vec![
            CapabilityName::new("agent/run"),
            CapabilityName::new("topology-extra"),
            CapabilityName::new("vendor:read")
        ]
    );
}

#[test]
fn package_load_file_reads_simtopo_source() {
    let path = write_temp_package(
        "load-file",
        r#"
graph:
topology load-file-flow
node in verb=in
node out verb=out
wire in -> out

metadata:
revision=1
"#,
    );

    let package = load_package_file(&path).expect("loaded package");

    assert_eq!(package.name(), &Symbol::new("load-file-flow"));
    assert_eq!(
        package.metadata[0].1,
        Expr::Number(NumberLiteral {
            domain: Symbol::new("i64"),
            canonical: "1".to_owned(),
        })
    );

    let _ = fs::remove_file(path);
}

#[test]
fn package_rejects_unknown_section() {
    let error = parse_package(
        r#"
graph:
topology bad

unknown:
value
"#,
    )
    .expect_err("unknown section should fail");

    assert!(
        error
            .to_string()
            .contains("unknown package section unknown")
    );
}

#[test]
fn package_rejects_invalid_capability_name() {
    let error = parse_package(
        r#"
graph:
topology bad-capability
node in verb=in
node out verb=out
wire in -> out

capabilities:
:bad
"#,
    )
    .expect_err("invalid capability should fail");

    let text = error.to_string();
    assert!(text.contains("topology package parse error"));
    assert!(text.contains("capability parse error"));
}

fn write_temp_package(label: &str, source: &str) -> PathBuf {
    let path = temp_path(label);
    fs::write(&path, source).expect("write temp package");
    path
}

fn temp_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "sim-topology-package-{label}-{}-{nanos}.simtopo",
        std::process::id()
    ))
}

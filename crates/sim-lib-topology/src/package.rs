//! Topology package parsing and loading.

use std::{
    fs,
    path::{Path, PathBuf},
};

use sim_kernel::{CapabilityName, Cx, Error, Expr, Result, Symbol, Value};

use crate::{
    Graph, GraphTest,
    capability::{capability_name_from_text, capability_symbol},
    text::graph_from_text,
};

/// Reloadable source descriptor for topology package text.
#[derive(Clone, Debug)]
pub enum TopologyPackageSource {
    /// Compatibility host-file source for `topology/load-file`.
    HostFile {
        /// Path read by the compatibility host-file loader.
        path: PathBuf,
    },
    /// Source text read from a runtime table entry.
    TableEntry {
        /// Table or Dir value that supplies package text.
        table: Value,
        /// Key looked up inside the table.
        key: Symbol,
    },
}

impl TopologyPackageSource {
    /// Creates a host-file package source descriptor.
    pub fn host_file(path: impl Into<PathBuf>) -> Self {
        Self::HostFile { path: path.into() }
    }

    /// Creates a table-backed package source descriptor.
    pub fn table_entry(table: Value, key: Symbol) -> Self {
        Self::TableEntry { table, key }
    }

    pub(crate) fn read_to_string(&self, cx: &mut Cx) -> Result<String> {
        match self {
            Self::HostFile { path } => read_host_file(path),
            Self::TableEntry { table, key } => read_table_entry(cx, table, key),
        }
    }

    pub(crate) fn requires_topology_file(&self) -> bool {
        matches!(self, Self::HostFile { .. })
    }
}

/// Parsed `.simtopo` package.
#[derive(Clone, Debug)]
pub struct TopologyPackage {
    /// Package graph.
    pub graph: Graph,
    /// Package test declarations mirrored from the graph.
    pub tests: Vec<GraphTest>,
    /// Package metadata mirrored from the graph.
    pub metadata: Vec<(Symbol, Expr)>,
    /// Capabilities required by the graph.
    pub capabilities: Vec<CapabilityName>,
}

impl TopologyPackage {
    /// Returns the graph name used as the package artifact name.
    pub fn name(&self) -> &Symbol {
        &self.graph.name
    }
}

/// Parses a `.simtopo` package.
///
/// The package format is intentionally section based. `graph:` contains the
/// topology text DSL, `tests:` contains `test` lines or shorthand test bodies,
/// `metadata:` contains `meta` lines or `key=value` shorthand, and
/// `capabilities:` contains one capability name per non-empty line.
pub fn parse_package(source: &str) -> Result<TopologyPackage> {
    let sections = PackageSections::parse(source)?;
    let mut graph_text = String::new();

    append_lines(&mut graph_text, &sections.graph);
    for line in &sections.metadata {
        let trimmed = line.trim();
        if trimmed.starts_with("meta ") {
            graph_text.push_str(trimmed);
        } else {
            graph_text.push_str("meta ");
            graph_text.push_str(trimmed);
        }
        graph_text.push('\n');
    }
    for line in &sections.tests {
        let trimmed = line.trim();
        if trimmed.starts_with("test ") {
            graph_text.push_str(trimmed);
        } else {
            graph_text.push_str("test ");
            graph_text.push_str(trimmed);
        }
        graph_text.push('\n');
    }

    let capabilities = sections
        .capabilities
        .iter()
        .map(|line| parse_capability_line(line.line_no, &line.text))
        .collect::<Result<Vec<_>>>()?;

    let mut graph = graph_from_text(&graph_text)?;
    graph.capabilities = capabilities.iter().map(capability_symbol).collect();

    Ok(TopologyPackage {
        tests: graph.tests.clone(),
        metadata: graph.metadata.clone(),
        capabilities,
        graph,
    })
}

/// Reads and parses a `.simtopo` package from disk.
pub fn load_package_file(path: impl Into<PathBuf>) -> Result<TopologyPackage> {
    let path = path.into();
    let source = read_host_file(&path)?;
    parse_package(&source)
}

/// Reads and parses a topology package from a reloadable source descriptor.
pub fn load_package_source(cx: &mut Cx, source: &TopologyPackageSource) -> Result<TopologyPackage> {
    parse_package(&source.read_to_string(cx)?)
}

fn read_host_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).map_err(|err| {
        Error::HostError(format!(
            "failed to read topology package {}: {err}",
            path.display()
        ))
    })?;
    String::from_utf8(bytes).map_err(|err| {
        Error::HostError(format!(
            "failed to decode topology package {} as utf-8: {err}",
            path.display()
        ))
    })
}

fn read_table_entry(cx: &mut Cx, table: &Value, key: &Symbol) -> Result<String> {
    let table = table.object().as_table_impl().ok_or(Error::TypeMismatch {
        expected: "table",
        found: "non-table",
    })?;
    let value = table.get(cx, key.clone())?;
    match value.object().as_expr(cx)? {
        Expr::String(source) => Ok(source),
        Expr::Bytes(bytes) => String::from_utf8(bytes).map_err(|err| {
            Error::Eval(format!(
                "topology package table entry {key} is not utf-8: {err}"
            ))
        }),
        Expr::Nil => Err(Error::Eval(format!(
            "topology package table entry {key} is missing"
        ))),
        other => Err(Error::TypeMismatch {
            expected: "string or bytes package source",
            found: expr_type(&other),
        }),
    }
}

#[derive(Clone, Copy)]
enum Section {
    Graph,
    Tests,
    Metadata,
    Capabilities,
}

#[derive(Default)]
struct PackageSections {
    graph: Vec<String>,
    tests: Vec<String>,
    metadata: Vec<String>,
    capabilities: Vec<PackageLine>,
}

impl PackageSections {
    fn parse(source: &str) -> Result<Self> {
        let mut sections = Self::default();
        let mut current = None;

        for (index, line) in source.lines().enumerate() {
            let line_no = index + 1;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(section) = parse_section_header(trimmed, line_no)? {
                current = Some(section);
                continue;
            }
            let Some(section) = current else {
                return Err(package_error(
                    line_no,
                    1,
                    "package content must appear inside a named section",
                ));
            };
            match section {
                Section::Graph => sections.graph.push(line.to_owned()),
                Section::Tests => sections.tests.push(trimmed.to_owned()),
                Section::Metadata => sections.metadata.push(trimmed.to_owned()),
                Section::Capabilities => sections.capabilities.push(PackageLine {
                    line_no,
                    text: trimmed.to_owned(),
                }),
            }
        }

        if sections.graph.is_empty() {
            return Err(Error::Eval(
                "topology package parse error: missing graph section".to_owned(),
            ));
        }

        Ok(sections)
    }
}

struct PackageLine {
    line_no: usize,
    text: String,
}

fn parse_section_header(trimmed: &str, line: usize) -> Result<Option<Section>> {
    let Some(name) = trimmed.strip_suffix(':') else {
        return Ok(None);
    };
    match name {
        "graph" => Ok(Some(Section::Graph)),
        "tests" => Ok(Some(Section::Tests)),
        "metadata" => Ok(Some(Section::Metadata)),
        "capabilities" => Ok(Some(Section::Capabilities)),
        _ => Err(package_error(
            line,
            1,
            format!("unknown package section {name}"),
        )),
    }
}

fn append_lines(output: &mut String, lines: &[String]) {
    for line in lines {
        output.push_str(line);
        output.push('\n');
    }
}

fn parse_capability_line(line_no: usize, text: &str) -> Result<CapabilityName> {
    capability_name_from_text(text).map_err(|err| package_error(line_no, 1, err.to_string()))
}

fn expr_type(expr: &Expr) -> &'static str {
    match expr {
        Expr::Nil => "nil",
        Expr::Bool(_) => "bool",
        Expr::Number(_) => "number",
        Expr::Symbol(_) => "symbol",
        Expr::Local(_) => "local",
        Expr::String(_) => "string",
        Expr::Bytes(_) => "bytes",
        Expr::List(_) => "list",
        Expr::Vector(_) => "vector",
        Expr::Map(_) => "map",
        Expr::Set(_) => "set",
        Expr::Call { .. } => "call",
        Expr::Infix { .. } => "infix",
        Expr::Prefix { .. } => "prefix",
        Expr::Postfix { .. } => "postfix",
        Expr::Block(_) => "block",
        Expr::Quote { .. } => "quote",
        Expr::Annotated { .. } => "annotated",
        Expr::Extension { .. } => "extension",
    }
}

fn package_error(line: usize, column: usize, message: impl Into<String>) -> Error {
    Error::Eval(format!(
        "topology package parse error at line {line}, column {column}: {}",
        message.into()
    ))
}

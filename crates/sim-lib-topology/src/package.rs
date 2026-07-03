//! Topology package parsing and loading.

use std::{fs, path::Path};

use sim_kernel::{Error, Expr, Result, Symbol};

use crate::{Graph, GraphTest, text::graph_from_text};

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
    pub capabilities: Vec<Symbol>,
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
/// `capabilities:` contains one capability symbol per non-empty line.
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

    let mut graph = graph_from_text(&graph_text)?;
    graph.capabilities = sections
        .capabilities
        .iter()
        .map(|line| symbol_from_text(line.trim()))
        .collect();

    Ok(TopologyPackage {
        tests: graph.tests.clone(),
        metadata: graph.metadata.clone(),
        capabilities: graph.capabilities.clone(),
        graph,
    })
}

/// Reads and parses a `.simtopo` package from disk.
pub fn load_package_file(path: impl AsRef<Path>) -> Result<TopologyPackage> {
    let path = path.as_ref();
    let source = fs::read_to_string(path).map_err(|err| {
        Error::HostError(format!(
            "failed to read topology package {}: {err}",
            path.display()
        ))
    })?;
    parse_package(&source)
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
    capabilities: Vec<String>,
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
                Section::Capabilities => sections.capabilities.push(trimmed.to_owned()),
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

fn symbol_from_text(text: &str) -> Symbol {
    if let Some((namespace, name)) = text.split_once('/')
        && !namespace.is_empty()
        && !name.is_empty()
    {
        return Symbol::qualified(namespace.to_owned(), name.to_owned());
    }
    if let Some((namespace, name)) = text.split_once(':')
        && !namespace.is_empty()
        && !name.is_empty()
    {
        return Symbol::qualified(namespace.to_owned(), name.to_owned());
    }
    Symbol::new(text.to_owned())
}

fn package_error(line: usize, column: usize, message: impl Into<String>) -> Error {
    Error::Eval(format!(
        "topology package parse error at line {line}, column {column}: {}",
        message.into()
    ))
}

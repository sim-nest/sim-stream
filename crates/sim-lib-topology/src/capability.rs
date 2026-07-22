//! Capability helpers for topology loading and execution.

use sim_kernel::{CapabilityName, Cx, Error, Result, Symbol};

use crate::Graph;

/// Capability required to execute a compiled topology graph.
pub fn topology_run_capability() -> CapabilityName {
    CapabilityName::new("topology-run")
}

/// Capability required to read topology packages from files.
pub fn topology_file_capability() -> CapabilityName {
    CapabilityName::new("topology-file")
}

/// Capability required to mutate the topology registry.
pub fn topology_write_capability() -> CapabilityName {
    CapabilityName::new("topology-write")
}

/// Capability required to inspect unredacted topology run details.
pub fn topology_reflect_capability() -> CapabilityName {
    CapabilityName::new("topology-reflect")
}

pub(crate) fn capability_name_from_text(text: &str) -> Result<CapabilityName> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(invalid_capability("empty capability name"));
    }
    if trimmed.starts_with(['/', ':', '.', '-']) || trimmed.ends_with(['/', ':', '.', '-']) {
        return Err(invalid_capability(format!(
            "capability name has an invalid boundary: {trimmed}"
        )));
    }
    if trimmed.contains("//") || trimmed.contains("::") || trimmed.contains("..") {
        return Err(invalid_capability(format!(
            "capability name has an empty segment: {trimmed}"
        )));
    }
    if trimmed.chars().any(|ch| !is_capability_char(ch)) {
        return Err(invalid_capability(format!(
            "capability name contains an unsupported character: {trimmed}"
        )));
    }
    Ok(CapabilityName::new(trimmed.to_owned()))
}

pub(crate) fn capability_name_from_symbol(symbol: &Symbol) -> Result<CapabilityName> {
    capability_name_from_text(&symbol.as_qualified_str())
}

pub(crate) fn capability_symbol(name: &CapabilityName) -> Symbol {
    symbol_from_text(name.as_str())
}

pub(crate) fn graph_capability_names(graph: &Graph) -> Result<Vec<CapabilityName>> {
    graph
        .capabilities
        .iter()
        .map(capability_name_from_symbol)
        .collect()
}

pub(crate) fn require_graph_capabilities(cx: &Cx, graph: &Graph) -> Result<()> {
    let capabilities = graph_capability_names(graph)?;
    cx.require_all(&capabilities)
}

fn invalid_capability(message: impl Into<String>) -> Error {
    Error::Eval(format!(
        "topology capability parse error: {}",
        message.into()
    ))
}

fn is_capability_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':')
}

fn symbol_from_text(text: &str) -> Symbol {
    if let Some((namespace, name)) = text.split_once('/')
        && !namespace.is_empty()
        && !name.is_empty()
    {
        return Symbol::qualified(namespace.to_owned(), name.to_owned());
    }
    Symbol::new(text.to_owned())
}

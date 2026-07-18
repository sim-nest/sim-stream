//! Topology registry and file reload support.

use std::{collections::BTreeMap, path::Path};

use sim_kernel::{CapabilityName, Cx, Error, Expr, Result, Symbol};

use crate::{
    Graph, TopologyConnection,
    capability::{graph_capability_names, topology_file_capability, topology_write_capability},
    compile_graph,
    package::{TopologyPackage, TopologyPackageSource, load_package_source},
    run::run_graph,
    site::connection_from_graph,
};

mod runtime;

pub use runtime::{
    SharedTopologyRegistry, TopologyLib, install_topology_lib, manifest_name, topology_exports,
    topology_site_symbol,
};

/// Registered topology artifact.
#[derive(Clone, Debug)]
pub struct TopologyEntry {
    /// Registry name.
    pub name: Symbol,
    /// Registered graph data.
    pub graph: Graph,
    /// Reloadable package source descriptor when loaded from an external source.
    pub source: Option<TopologyPackageSource>,
    /// Package metadata captured at load time.
    pub metadata: Vec<(Symbol, Expr)>,
    /// Package capabilities captured as typed authority names at load time.
    pub capabilities: Vec<CapabilityName>,
}

impl TopologyEntry {
    fn from_graph(name: Symbol, graph: Graph) -> Result<Self> {
        Ok(Self {
            name,
            metadata: graph.metadata.clone(),
            capabilities: graph_capability_names(&graph)?,
            graph,
            source: None,
        })
    }

    fn from_package(package: TopologyPackage, source: TopologyPackageSource) -> Self {
        Self {
            name: package.name().clone(),
            graph: package.graph,
            source: Some(source),
            metadata: package.metadata,
            capabilities: package.capabilities,
        }
    }

    /// Builds a server connection backed by this topology graph.
    pub fn connection(&self, cx: &mut Cx) -> Result<TopologyConnection> {
        connection_from_graph(cx, &self.graph)
    }

    /// Compiles and runs this topology graph with one input expression.
    pub fn run(&self, cx: &mut Cx, input: Expr) -> Result<Expr> {
        let plan = compile_graph(cx, &self.graph)?;
        run_graph(cx, &self.graph, &plan, input)
    }
}

/// Per-context registry for named topology artifacts.
#[derive(Clone, Debug, Default)]
pub struct TopologyRegistry {
    entries: BTreeMap<Symbol, TopologyEntry>,
}

impl TopologyRegistry {
    /// Creates an empty topology registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Defines or replaces a named topology graph.
    pub fn def(&mut self, cx: &mut Cx, name: Symbol, graph: Graph) -> Result<TopologyEntry> {
        cx.require(&topology_write_capability())?;
        compile_graph(cx, &graph)?;
        let entry = TopologyEntry::from_graph(name.clone(), graph)?;
        self.entries.insert(name, entry.clone());
        Ok(entry)
    }

    /// Returns a registered topology entry by name.
    pub fn get(&self, name: &Symbol) -> Option<&TopologyEntry> {
        self.entries.get(name)
    }

    /// Returns registered topology names in deterministic order.
    pub fn list(&self) -> Vec<Symbol> {
        self.entries.keys().cloned().collect()
    }

    /// Removes a named topology entry.
    pub fn remove(&mut self, cx: &mut Cx, name: &Symbol) -> Result<Option<TopologyEntry>> {
        cx.require(&topology_write_capability())?;
        Ok(self.entries.remove(name))
    }

    /// Loads a topology package from disk and registers it by graph name.
    pub fn load_file(&mut self, cx: &mut Cx, path: impl AsRef<Path>) -> Result<TopologyEntry> {
        self.load_source(
            cx,
            TopologyPackageSource::host_file(path.as_ref().to_path_buf()),
        )
    }

    /// Loads a topology package from a source descriptor and registers it by graph name.
    pub fn load_source(
        &mut self,
        cx: &mut Cx,
        source: TopologyPackageSource,
    ) -> Result<TopologyEntry> {
        if source.requires_topology_file() {
            cx.require(&topology_file_capability())?;
        }
        cx.require(&topology_write_capability())?;
        let package = load_package_source(cx, &source)?;
        compile_graph(cx, &package.graph)?;
        let entry = TopologyEntry::from_package(package, source);
        self.entries.insert(entry.name.clone(), entry.clone());
        Ok(entry)
    }

    /// Reloads a named topology entry from its original package path.
    pub fn reload(&mut self, cx: &mut Cx, name: &Symbol) -> Result<TopologyEntry> {
        cx.require(&topology_write_capability())?;
        let source = self
            .entries
            .get(name)
            .and_then(|entry| entry.source.clone())
            .ok_or_else(|| Error::Eval(format!("topology {name} has no reloadable source")))?;
        if source.requires_topology_file() {
            cx.require(&topology_file_capability())?;
        }
        let package = load_package_source(cx, &source)?;
        if package.name() != name {
            return Err(Error::Eval(format!(
                "topology reload name mismatch: expected {name}, found {}",
                package.name()
            )));
        }
        compile_graph(cx, &package.graph)?;
        let entry = TopologyEntry::from_package(package, source);
        self.entries.insert(name.clone(), entry.clone());
        Ok(entry)
    }
}

/// Implements `topology/def`.
pub fn topology_def(
    cx: &mut Cx,
    registry: &mut TopologyRegistry,
    name: Symbol,
    graph: Graph,
) -> Result<TopologyEntry> {
    registry.def(cx, name, graph)
}

/// Implements `topology/get`.
pub fn topology_get<'a>(
    registry: &'a TopologyRegistry,
    name: &Symbol,
) -> Option<&'a TopologyEntry> {
    registry.get(name)
}

/// Implements `topology/list`.
pub fn topology_list(registry: &TopologyRegistry) -> Vec<Symbol> {
    registry.list()
}

/// Implements `topology/remove`.
pub fn topology_remove(
    cx: &mut Cx,
    registry: &mut TopologyRegistry,
    name: &Symbol,
) -> Result<Option<TopologyEntry>> {
    registry.remove(cx, name)
}

/// Implements `topology/load-file`.
pub fn topology_load_file(
    cx: &mut Cx,
    registry: &mut TopologyRegistry,
    path: impl AsRef<Path>,
) -> Result<TopologyEntry> {
    registry.load_file(cx, path)
}

/// Implements `topology/load-source`.
pub fn topology_load_source(
    cx: &mut Cx,
    registry: &mut TopologyRegistry,
    source: TopologyPackageSource,
) -> Result<TopologyEntry> {
    registry.load_source(cx, source)
}

/// Implements `topology/reload`.
pub fn topology_reload(
    cx: &mut Cx,
    registry: &mut TopologyRegistry,
    name: &Symbol,
) -> Result<TopologyEntry> {
    registry.reload(cx, name)
}

//! Runtime function surface for topology registries.

use std::sync::{Arc, Mutex};

use sim_kernel::{
    AbiVersion, Cx, Export, Lib, LibManifest, LibTarget, Linker, Result, Symbol, Version,
};

mod calls;
mod reports;

use crate::{
    browse::{topology_browse_symbols, topology_browse_value},
    site::TopologySiteFactory,
};
use calls::topology_functions;

/// Shared registry handle used by the installed topology function surface.
pub type SharedTopologyRegistry = Arc<Mutex<super::TopologyRegistry>>;

/// Host-registered topology library that installs registry functions.
pub struct TopologyLib {
    registry: SharedTopologyRegistry,
}

impl TopologyLib {
    /// Creates a topology lib with a fresh per-context registry.
    pub fn new() -> Self {
        Self {
            registry: Arc::new(Mutex::new(super::TopologyRegistry::new())),
        }
    }

    /// Creates a topology lib backed by an explicit registry handle.
    pub fn with_registry(registry: SharedTopologyRegistry) -> Self {
        Self { registry }
    }

    /// Returns the shared registry handle owned by this lib.
    pub fn registry(&self) -> SharedTopologyRegistry {
        self.registry.clone()
    }
}

impl Default for TopologyLib {
    fn default() -> Self {
        Self::new()
    }
}

impl Lib for TopologyLib {
    fn manifest(&self) -> LibManifest {
        LibManifest {
            id: manifest_name(),
            version: Version(env!("CARGO_PKG_VERSION").to_owned()),
            abi: AbiVersion { major: 0, minor: 1 },
            target: LibTarget::HostRegistered,
            requires: Vec::new(),
            capabilities: Vec::new(),
            exports: topology_exports(),
        }
    }

    fn load(&self, cx: &mut sim_kernel::LoadCx, linker: &mut Linker<'_>) -> Result<()> {
        for function in topology_functions(self.registry.clone()) {
            linker.function_value(
                function.symbol().clone(),
                cx.factory().opaque(Arc::new(function))?,
            )?;
        }
        for symbol in topology_browse_symbols() {
            linker.value(symbol.clone(), topology_browse_value(cx, symbol)?)?;
        }
        let site = cx
            .factory()
            .opaque(Arc::new(TopologySiteFactory::new(topology_site_symbol())))?;
        linker.site_value(topology_site_symbol(), site)?;
        Ok(())
    }
}

/// Installs topology registry functions into a context.
pub fn install_topology_lib(cx: &mut Cx) -> Result<()> {
    sim_lib_core::install_once(cx, &TopologyLib::new()).map(|_| ())
}

/// Export records for the topology registry function surface.
pub fn topology_exports() -> Vec<Export> {
    let mut exports = calls::topology_function_symbols()
        .into_iter()
        .map(|symbol| Export::Function {
            symbol,
            function_id: None,
        })
        .collect::<Vec<_>>();
    exports.extend(
        topology_browse_symbols()
            .into_iter()
            .map(|symbol| Export::Value { symbol }),
    );
    exports.push(Export::Site {
        symbol: topology_site_symbol(),
        runtime_id: None,
    });
    exports
}

/// Manifest id for the topology library.
pub fn manifest_name() -> Symbol {
    Symbol::qualified("sim", "topology")
}

/// Runtime site export symbol for the topology connection factory.
pub fn topology_site_symbol() -> Symbol {
    Symbol::qualified("topology", "site")
}

//! Capability helpers for topology loading and execution.

use sim_kernel::CapabilityName;

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

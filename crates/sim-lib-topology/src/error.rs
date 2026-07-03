//! Topology engine error and validation-diagnostic constructors.

use sim_kernel::{Error, Symbol};

/// Builds a topology validation error naming the graph, context, and message.
pub fn validation_error(
    graph: &Symbol,
    context: impl AsRef<str>,
    message: impl AsRef<str>,
) -> Error {
    Error::Eval(format!(
        "topology validation error in {graph}: {}: {}",
        context.as_ref(),
        message.as_ref()
    ))
}

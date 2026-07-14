//! Deterministic cookbook builders for stream-core recipes.

use sim_kernel::{Expr, Symbol};

use crate::{BufferPolicy, ClockDomain, StreamDirection, StreamMedia, StreamMetadata};

/// Build the modeled metadata descriptor used by the cookbook recipe.
pub fn metadata_descriptor_demo() -> Expr {
    let metadata = StreamMetadata::new(
        Symbol::qualified("stream/demo", "inbound-data"),
        StreamMedia::Data,
        StreamDirection::Source,
        ClockDomain::ServerFrame.symbol(),
        BufferPolicy::bounded(8).expect("cookbook buffer is bounded"),
    );
    metadata.table_expr()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_descriptor_round_trips_through_constructor_args() {
        let Expr::Map(entries) = metadata_descriptor_demo() else {
            panic!("metadata demo is a map")
        };
        assert!(entries.iter().any(|(_, value)| {
            matches!(value, Expr::Symbol(symbol) if symbol.as_qualified_str() == "stream/media/data")
        }));

        let metadata = StreamMetadata::new(
            Symbol::qualified("stream/demo", "inbound-data"),
            StreamMedia::Data,
            StreamDirection::Source,
            ClockDomain::ServerFrame.symbol(),
            BufferPolicy::bounded(8).expect("valid buffer"),
        );
        let rebuilt = StreamMetadata::from_constructor_args(metadata.to_constructor_args())
            .expect("constructor args rebuild metadata");
        assert_eq!(rebuilt.id(), metadata.id());
        assert_eq!(rebuilt.media(), StreamMedia::Data);
    }
}

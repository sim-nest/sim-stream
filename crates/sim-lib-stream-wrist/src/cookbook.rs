//! Deterministic cookbook builders for wrist stream recipes.

use sim_kernel::Expr;
use sim_lib_stream_device::{DeviceSample, ModeledSource};

use crate::ModeledHeartRateSource;

/// Builds the modeled heart-rate event descriptor used by the cookbook.
pub fn worn_heart_rate_descriptor_demo() -> Expr {
    ModeledHeartRateSource.at(0).to_expr()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WornEvent;

    #[test]
    fn worn_heart_rate_descriptor_round_trips() {
        let demo = worn_heart_rate_descriptor_demo();
        let decoded = WornEvent::from_expr(&demo).expect("demo descriptor is valid");
        assert_eq!(decoded.seq(), 0);
    }
}

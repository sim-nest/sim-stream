//! Deterministic cookbook builders for XR stream recipes.

use sim_kernel::Expr;
use sim_lib_stream_device::{DeviceSample, ModeledSource};

use crate::ModeledViturePoseSource;

/// Builds the modeled XR pose descriptor used by the cookbook.
pub fn xr_modeled_descriptor_demo() -> Expr {
    ModeledViturePoseSource.at(0).to_expr()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::XrPoseSample;

    #[test]
    fn xr_modeled_descriptor_round_trips() {
        let demo = xr_modeled_descriptor_demo();
        let decoded = XrPoseSample::from_expr(&demo).expect("demo descriptor is valid");
        assert_eq!(decoded.seq(), 0);
    }
}

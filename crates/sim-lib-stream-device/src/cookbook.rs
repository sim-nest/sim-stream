//! Deterministic cookbook builders for stream-device recipes.

use sim_kernel::Expr;

use crate::{DeviceCaps, DeviceSample};

/// Builds the modeled device capabilities descriptor used by the cookbook.
pub fn device_caps_descriptor_demo() -> Expr {
    DeviceCaps::demo(0).to_expr()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeviceCaps, DeviceSample};

    #[test]
    fn device_caps_descriptor_round_trips() {
        let demo = device_caps_descriptor_demo();
        let decoded = DeviceCaps::from_expr(&demo).expect("demo descriptor is valid");
        assert_eq!(decoded.seq(), 0);
    }
}

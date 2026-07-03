//! Resource limits bounding rank traversals.
//!
//! Tracks a fuel budget for steps and a maximum bit-width for ordinals so that
//! search, neighborhood, and retrieval work fails closed instead of running
//! away on large or deep spaces.

use crate::{
    error::{RankError, RankResult},
    nat::Nat,
};

/// Fuel and ordinal-size budget for a rank traversal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankLimits {
    fuel: u64,
    max_bits: u64,
}

impl RankLimits {
    /// Default starting fuel budget.
    pub const DEFAULT_FUEL: u64 = 10_000;
    /// Default maximum permitted ordinal bit-width.
    pub const DEFAULT_MAX_BITS: u64 = 1_000_000;

    /// Builds limits with explicit `fuel` and `max_bits` budgets.
    pub fn new(fuel: u64, max_bits: u64) -> Self {
        Self { fuel, max_bits }
    }

    /// Returns the fuel remaining in this budget.
    pub fn remaining_fuel(&self) -> u64 {
        self.fuel
    }

    /// Returns the maximum permitted ordinal bit-width.
    pub fn max_bits(&self) -> u64 {
        self.max_bits
    }

    /// Consumes `needed` fuel, failing if the budget is insufficient.
    ///
    /// `limit` names the operation for the resulting limit-exceeded error.
    pub fn consume(&mut self, needed: u64, limit: &'static str) -> RankResult<()> {
        if needed > self.fuel {
            return Err(RankError::LimitExceeded {
                limit,
                needed,
                remaining: self.fuel,
            });
        }
        self.fuel -= needed;
        Ok(())
    }

    /// Checks that `value`'s bit-width is within `max_bits`.
    ///
    /// `limit` names the operation for the resulting bit-limit error.
    pub fn check_nat(&self, value: &Nat, limit: &'static str) -> RankResult<()> {
        let bits = value.bits();
        if bits > self.max_bits {
            return Err(RankError::BitLimitExceeded {
                limit,
                bits,
                max_bits: self.max_bits,
            });
        }
        Ok(())
    }
}

impl Default for RankLimits {
    fn default() -> Self {
        Self::new(Self::DEFAULT_FUEL, Self::DEFAULT_MAX_BITS)
    }
}

//! Capability names guarding the rank library's operations.
//!
//! Each constructor returns the [`CapabilityName`] a caller must hold to invoke
//! the corresponding rank surface.

use sim_kernel::CapabilityName;

/// Capability `rank.read` for ranking and unranking values.
pub fn rank_read_capability() -> CapabilityName {
    CapabilityName::new("rank.read")
}

/// Capability `rank.enumerate` for enumerating a space's nodes.
pub fn rank_enumerate_capability() -> CapabilityName {
    CapabilityName::new("rank.enumerate")
}

/// Capability `rank.browse` for browsing ranked spaces.
pub fn rank_browse_capability() -> CapabilityName {
    CapabilityName::new("rank.browse")
}

/// Capability `rank.neighbor` for neighborhood and mutation queries.
pub fn rank_neighbor_capability() -> CapabilityName {
    CapabilityName::new("rank.neighbor")
}

/// Capability `rank.heavy` for expensive rank computations.
pub fn rank_heavy_capability() -> CapabilityName {
    CapabilityName::new("rank.heavy")
}

/// Capability `rank.learn` for learning or training over ranked spaces.
pub fn rank_learn_capability() -> CapabilityName {
    CapabilityName::new("rank.learn")
}

/// Capability `rank.codec` for codec-level operations.
pub fn rank_codec_capability() -> CapabilityName {
    CapabilityName::new("rank.codec")
}

/// Returns the full set of public rank capabilities.
pub fn rank_public_capabilities() -> Vec<CapabilityName> {
    vec![
        rank_read_capability(),
        rank_enumerate_capability(),
        rank_browse_capability(),
        rank_neighbor_capability(),
        rank_heavy_capability(),
        rank_learn_capability(),
        rank_codec_capability(),
    ]
}

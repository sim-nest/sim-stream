//! Semantic version type for rank codecs and neighborhoods.
//!
//! A simple major/minor/patch triple that displays and parses as `x.y.z`.

use core::{fmt, str::FromStr};

use crate::error::{RankError, RankResult};

/// Semantic version of a rank codec or neighborhood.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RankVersion {
    /// Major version: incompatible changes.
    pub major: u16,
    /// Minor version: backward-compatible additions.
    pub minor: u16,
    /// Patch version: backward-compatible fixes.
    pub patch: u16,
}

impl RankVersion {
    /// Builds a version from its `major`, `minor`, and `patch` components.
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Returns version `1.0.0`.
    pub const fn v1() -> Self {
        Self::new(1, 0, 0)
    }
}

impl fmt::Display for RankVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for RankVersion {
    type Err = RankError;

    fn from_str(value: &str) -> RankResult<Self> {
        let mut parts = value.split('.');
        let Some(major) = parts.next() else {
            return Err(invalid_version(value));
        };
        let Some(minor) = parts.next() else {
            return Err(invalid_version(value));
        };
        let Some(patch) = parts.next() else {
            return Err(invalid_version(value));
        };
        if parts.next().is_some() {
            return Err(invalid_version(value));
        }

        Ok(Self {
            major: parse_part(major, value)?,
            minor: parse_part(minor, value)?,
            patch: parse_part(patch, value)?,
        })
    }
}

fn parse_part(part: &str, full: &str) -> RankResult<u16> {
    if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_version(full));
    }
    part.parse().map_err(|_| invalid_version(full))
}

fn invalid_version(input: &str) -> RankError {
    RankError::InvalidVersion {
        input: input.to_owned(),
    }
}

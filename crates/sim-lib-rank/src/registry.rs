//! Registry of named rank spaces.
//!
//! Maps each space symbol to a shared [`RankSpace`], rejecting duplicate
//! registrations and supporting lookup by symbol.

use std::{collections::BTreeMap, sync::Arc};

use sim_kernel::Symbol;

use crate::{
    error::{RankError, RankResult},
    space::RankSpace,
};

/// Symbol-keyed registry of shared rank spaces.
#[derive(Clone, Debug, Default)]
pub struct RankSpaceRegistry {
    spaces: BTreeMap<Symbol, Arc<RankSpace>>,
}

impl RankSpaceRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `space` under its symbol, returning the shared handle.
    ///
    /// Fails with a duplicate-space error if a space with that symbol is
    /// already registered.
    pub fn register(&mut self, space: RankSpace) -> RankResult<Arc<RankSpace>> {
        let id = space.symbol().clone();
        if self.spaces.contains_key(&id) {
            return Err(RankError::DuplicateSpace { id });
        }
        let space = Arc::new(space);
        self.spaces.insert(id, space.clone());
        Ok(space)
    }

    /// Returns the space registered under `id`, if any.
    pub fn get(&self, id: &Symbol) -> Option<Arc<RankSpace>> {
        self.spaces.get(id).cloned()
    }

    /// Returns the space registered under `id`, or an unknown-space error.
    pub fn require(&self, id: &Symbol) -> RankResult<Arc<RankSpace>> {
        self.get(id)
            .ok_or_else(|| RankError::UnknownSpace { id: id.clone() })
    }

    /// Returns the number of registered spaces.
    pub fn len(&self) -> usize {
        self.spaces.len()
    }

    /// Returns `true` when no spaces are registered.
    pub fn is_empty(&self) -> bool {
        self.spaces.is_empty()
    }
}

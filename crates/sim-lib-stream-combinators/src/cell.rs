use std::sync::Mutex;

use sim_kernel::{Error, Result};

/// Immutable snapshot of a [`StreamCell`] value at a known version.
///
/// A snapshot pairs the cloned cell value with the version counter that was
/// current when it was read, so callers can later attempt a compare-and-set
/// against that version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellSnapshot<T> {
    /// The cell value as observed at this snapshot.
    pub value: T,
    /// The cell version counter at the time of the read.
    pub version: u64,
}

/// Versioned, thread-safe cell holding the latest value of a stream.
///
/// A `StreamCell` collapses a stream to its most recent value with optimistic
/// concurrency: each successful write bumps a version counter, and writers must
/// present the version they observed to commit. It is the shared "current
/// value" handoff point between a producing stream and its consumers.
///
/// # Examples
///
/// ```
/// use sim_lib_stream_combinators::stream_cell;
///
/// let cell = stream_cell("first".to_owned());
/// let snapshot = cell.get().unwrap();
/// assert_eq!(snapshot.version, 0);
///
/// let next = cell.set("second".to_owned(), snapshot.version).unwrap();
/// assert_eq!(next, 1);
///
/// // A stale version is rejected.
/// assert!(cell.set("third".to_owned(), snapshot.version).is_err());
/// assert_eq!(cell.get().unwrap().value, "second");
/// ```
pub struct StreamCell<T> {
    state: Mutex<CellState<T>>,
}

struct CellState<T> {
    value: T,
    version: u64,
}

/// Builds a [`StreamCell`] holding `value` at version `0`.
pub fn stream_cell<T>(value: T) -> StreamCell<T> {
    StreamCell::new(value)
}

impl<T> StreamCell<T> {
    /// Creates a cell holding `value` at version `0`.
    pub fn new(value: T) -> Self {
        Self {
            state: Mutex::new(CellState { value, version: 0 }),
        }
    }
}

impl<T: Clone> StreamCell<T> {
    /// Reads a versioned snapshot of the current cell value.
    pub fn get(&self) -> Result<CellSnapshot<T>> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream cell"))?;
        Ok(CellSnapshot {
            value: state.value.clone(),
            version: state.version,
        })
    }

    /// Writes `value` only if `expected_version` matches the current version.
    ///
    /// On success the version is bumped and the new version is returned; a
    /// mismatch leaves the cell untouched and returns an error.
    pub fn set(&self, value: T, expected_version: u64) -> Result<u64> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream cell"))?;
        if state.version != expected_version {
            return Err(Error::Eval(format!(
                "stale stream cell version: expected {expected_version}, current {}",
                state.version
            )));
        }
        state.value = value;
        state.version = state.version.saturating_add(1);
        Ok(state.version)
    }
}

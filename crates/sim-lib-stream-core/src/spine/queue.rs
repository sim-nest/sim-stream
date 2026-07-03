//! Internal queues backing a [`StreamValue`], plus the observable counters and
//! push outcomes they expose.
//!
//! A stream value is driven by one of two spines: a `PullSpine` over a fixed
//! buffer of pre-built items, or a `PushSpine` whose bounded queue is fed by an
//! external producer under a [`BufferPolicy`]. Both spines are private to the
//! [`super`] module; the runtime-visible surface they share is [`StreamStats`]
//! (lifetime counters) and [`PushResult`] (the per-push backpressure decision).
//!
//! [`StreamValue`]: super::StreamValue

use std::{
    collections::VecDeque,
    sync::{Condvar, Mutex},
    time::Duration,
};

use sim_kernel::{Error, Result};

use crate::{BackpressureOutcome, BufferOverflowPolicy, BufferPolicy};

use super::StreamItem;

/// Lifetime counters observed on a stream spine.
///
/// Every field accumulates over the life of the stream and is snapshotted by
/// [`StreamValue::stats`]; the two boolean flags record terminal state. The
/// counters are advanced by the queue as packets are pushed, yielded, dropped,
/// or rejected, giving a behavioral record of how backpressure played out.
///
/// [`StreamValue::stats`]: super::StreamValue::stats
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamStats {
    /// Total packets handed to the spine via push, regardless of outcome.
    pub pushed: u64,
    /// Packets admitted to the queue (excludes drops, rejects, and closed
    /// pushes).
    pub accepted: u64,
    /// Packets pulled out of the spine and handed to a consumer.
    pub yielded: u64,
    /// Packets dropped on overflow under the drop-newest policy.
    pub dropped_newest: u64,
    /// Packets evicted from the front on overflow under the drop-oldest policy.
    pub dropped_oldest: u64,
    /// Overflows that occurred while the error policy was in force.
    pub overflow_errors: u64,
    /// Pushes refused (returned as [`PushResult::Rejected`]).
    pub rejected: u64,
    /// Timed waits that elapsed without a packet becoming available.
    pub timeouts: u64,
    /// Timed pulls that returned empty because the wait timed out.
    pub timed_out: u64,
    /// Pulls that had to block waiting for a packet.
    pub blocked: u64,
    /// Whether the stream has been closed to further input.
    pub closed: bool,
    /// Whether the stream was cancelled (closed and drained of buffered items).
    pub cancelled: bool,
}

/// Outcome of pushing a single packet into a push stream.
///
/// The variant records what the spine did with the packet under the active
/// [`BufferPolicy`]. Every variant except [`PushResult::Accepted`] carries the
/// packet back so the caller can route the rejected or evicted item elsewhere.
/// Map a result to its kernel-facing [`BackpressureOutcome`] with
/// [`PushResult::outcome`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PushResult {
    /// The packet was admitted to the queue.
    Accepted,
    /// The queue was full; this newest packet was dropped under the drop-newest
    /// policy.
    DroppedNewest(StreamItem),
    /// The queue was full; the oldest queued packet was evicted to make room,
    /// and that evicted packet is returned here.
    DroppedOldest(StreamItem),
    /// The queue was full and the error policy refused the push; the packet is
    /// returned unaccepted.
    Rejected(StreamItem),
    /// The stream was already closed; the packet is returned unaccepted.
    Closed(StreamItem),
}

impl PushResult {
    /// Maps this push result to the kernel-facing [`BackpressureOutcome`].
    pub fn outcome(&self) -> BackpressureOutcome {
        match self {
            Self::Accepted => BackpressureOutcome::Accepted,
            Self::DroppedNewest(_) => BackpressureOutcome::DroppedNewest,
            Self::DroppedOldest(_) => BackpressureOutcome::DroppedOldest,
            Self::Rejected(_) => BackpressureOutcome::Rejected,
            Self::Closed(_) => BackpressureOutcome::Closed,
        }
    }
}

pub(super) struct PullSpine {
    state: Mutex<PullState>,
}

struct PullState {
    items: VecDeque<StreamItem>,
    closed: bool,
    stats: StreamStats,
}

impl PullSpine {
    pub(super) fn new(items: Vec<StreamItem>) -> Self {
        Self {
            state: Mutex::new(PullState {
                items: items.into(),
                closed: false,
                stats: StreamStats::default(),
            }),
        }
    }

    pub(super) fn next(&self) -> Result<Option<StreamItem>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("pull stream"))?;
        if state.closed {
            return Ok(None);
        }
        let item = state.items.pop_front();
        if item.is_some() {
            state.stats.yielded += 1;
        } else {
            state.closed = true;
        }
        Ok(item)
    }

    pub(super) fn peek(&self) -> Result<Option<StreamItem>> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("pull stream"))?;
        Ok((!state.closed)
            .then(|| state.items.front().cloned())
            .flatten())
    }

    pub(super) fn is_done(&self) -> Result<bool> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("pull stream"))?;
        Ok(state.closed || state.items.is_empty())
    }

    pub(super) fn close(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("pull stream"))?;
        state.closed = true;
        state.stats.closed = true;
        Ok(())
    }

    pub(super) fn cancel(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("pull stream"))?;
        state.items.clear();
        state.closed = true;
        state.stats.closed = true;
        state.stats.cancelled = true;
        Ok(())
    }

    pub(super) fn stats(&self) -> Result<StreamStats> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("pull stream"))?;
        Ok(state.stats.clone())
    }

    pub(super) fn depth(&self) -> Result<usize> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("pull stream"))?;
        Ok(state.items.len())
    }
}

pub(super) struct PushSpine {
    policy: BufferPolicy,
    state: Mutex<PushState>,
    ready: Condvar,
}

struct PushState {
    queue: VecDeque<StreamItem>,
    closed: bool,
    stats: StreamStats,
}

impl PushSpine {
    pub(super) fn new(policy: BufferPolicy) -> Self {
        Self {
            policy,
            state: Mutex::new(PushState {
                queue: VecDeque::new(),
                closed: false,
                stats: StreamStats::default(),
            }),
            ready: Condvar::new(),
        }
    }

    pub(super) fn push(&self, item: StreamItem) -> Result<PushResult> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("push stream"))?;
        state.stats.pushed += 1;
        if state.closed {
            state.stats.closed = true;
            return Ok(PushResult::Closed(item));
        }
        if state.queue.len() < self.policy.capacity() {
            state.queue.push_back(item);
            state.stats.accepted += 1;
            self.ready.notify_one();
            return Ok(PushResult::Accepted);
        }
        match self.policy.overflow() {
            BufferOverflowPolicy::DropNewest => {
                state.stats.dropped_newest += 1;
                Ok(PushResult::DroppedNewest(item))
            }
            BufferOverflowPolicy::DropOldest => {
                let dropped = state.queue.pop_front().ok_or_else(|| {
                    Error::Eval("stream queue overflowed without an oldest item".to_owned())
                })?;
                state.queue.push_back(item);
                state.stats.dropped_oldest += 1;
                self.ready.notify_one();
                Ok(PushResult::DroppedOldest(dropped))
            }
            BufferOverflowPolicy::Error => {
                state.stats.overflow_errors += 1;
                state.stats.rejected += 1;
                Ok(PushResult::Rejected(item))
            }
        }
    }

    pub(super) fn next(&self) -> Result<Option<StreamItem>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("push stream"))?;
        let item = state.queue.pop_front();
        if item.is_some() {
            state.stats.yielded += 1;
        }
        Ok(item)
    }

    pub(super) fn next_timeout(&self, timeout: Duration) -> Result<Option<StreamItem>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("push stream"))?;
        if state.queue.is_empty() && !state.closed {
            let (guard, wait) = self
                .ready
                .wait_timeout(state, timeout)
                .map_err(|_| Error::PoisonedLock("push stream"))?;
            state = guard;
            if wait.timed_out() && state.queue.is_empty() {
                state.stats.timeouts += 1;
                state.stats.timed_out += 1;
                return Ok(None);
            }
        }
        let item = state.queue.pop_front();
        if item.is_some() {
            state.stats.yielded += 1;
        }
        Ok(item)
    }

    pub(super) fn peek(&self) -> Result<Option<StreamItem>> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("push stream"))?;
        Ok(state.queue.front().cloned())
    }

    pub(super) fn is_done(&self) -> Result<bool> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("push stream"))?;
        Ok(state.closed && state.queue.is_empty())
    }

    pub(super) fn close(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("push stream"))?;
        state.closed = true;
        state.stats.closed = true;
        self.ready.notify_all();
        Ok(())
    }

    pub(super) fn cancel(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("push stream"))?;
        state.queue.clear();
        state.closed = true;
        state.stats.closed = true;
        state.stats.cancelled = true;
        self.ready.notify_all();
        Ok(())
    }

    pub(super) fn stats(&self) -> Result<StreamStats> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("push stream"))?;
        Ok(state.stats.clone())
    }

    pub(super) fn depth(&self) -> Result<usize> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("push stream"))?;
        Ok(state.queue.len())
    }
}

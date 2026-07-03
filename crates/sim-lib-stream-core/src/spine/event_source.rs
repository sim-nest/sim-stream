//! Event-source adapter that feeds a stream's packets into the kernel run
//! ledger.
//!
//! [`StreamEventSource`] wraps a [`StreamValue`] and implements the kernel
//! [`EventSource`] contract: each pull drains one packet into a sequenced
//! packet event under a run [`Ref`], and a terminal `done` event is emitted
//! once -- and only once -- the stream reports itself done. The kernel defines
//! the `EventSource` protocol; this adapter supplies the stream-specific
//! behavior of turning spine packets into ledger events.

use std::sync::{Arc, Mutex};

use sim_kernel::{Cx, Error, Event, EventSource, Ref, Result};

use super::StreamValue;

/// Bridges a [`StreamValue`] to the kernel [`EventSource`] contract.
///
/// Holds the stream, the run [`Ref`] that events are attributed to, and the
/// sequencing state. Construct one with [`StreamEventSource::new`] (or
/// [`StreamValue::event_source`]) and pull events through the [`EventSource`]
/// implementation.
///
/// [`StreamValue::event_source`]: super::StreamValue::event_source
pub struct StreamEventSource {
    stream: Arc<StreamValue>,
    run: Ref,
    state: Mutex<EventSourceState>,
}

struct EventSourceState {
    next_seq: u64,
    done_sent: bool,
}

impl StreamEventSource {
    /// Creates an event source over `stream`, attributing emitted events to
    /// `run` and numbering them from `start_seq`.
    pub fn new(stream: Arc<StreamValue>, run: Ref, start_seq: u64) -> Self {
        Self {
            stream,
            run,
            state: Mutex::new(EventSourceState {
                next_seq: start_seq,
                done_sent: false,
            }),
        }
    }
}

impl EventSource for StreamEventSource {
    fn next(&self, cx: &mut Cx) -> Result<Option<Event>> {
        if let Some(item) = self.stream.next_packet()? {
            let mut state = self
                .state
                .lock()
                .map_err(|_| Error::PoisonedLock("stream event source"))?;
            let event = item.chunk_event(cx, self.run.clone(), state.next_seq)?;
            state.next_seq = state.next_seq.saturating_add(1);
            return Ok(Some(event));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream event source"))?;
        if self.stream.is_done()? && !state.done_sent {
            state.done_sent = true;
            let event = Event::done(self.run.clone(), state.next_seq)?;
            state.next_seq = state.next_seq.saturating_add(1);
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    fn close(&self, _cx: &mut Cx) -> Result<()> {
        self.stream.cancel()
    }
}

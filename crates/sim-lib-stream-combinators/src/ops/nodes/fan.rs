use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use sim_kernel::{Error, Result};
use sim_lib_stream_core::{StreamItem, StreamMetadata};

use crate::stream::{Stream, StreamNode};

pub(crate) fn fan_readers(source: Stream) -> (Stream, Stream) {
    let state = Arc::new(Mutex::new(FanState {
        history: VecDeque::new(),
        base: 0,
        cursors: vec![0, 0],
        done: false,
    }));
    (
        Stream::new(FanReader {
            source: source.clone(),
            state: Arc::clone(&state),
            id: 0,
        }),
        Stream::new(FanReader {
            source,
            state,
            id: 1,
        }),
    )
}

struct FanState {
    /// Buffered packets in the retained window, oldest first.
    history: VecDeque<StreamItem>,
    /// Absolute index of `history`'s front packet (count already pruned).
    base: usize,
    /// Absolute read position of each reader, indexed by reader id.
    cursors: Vec<usize>,
    /// Whether the shared source has reached its terminal `done`.
    done: bool,
}

impl FanState {
    /// Drops every buffered packet below the slowest reader's cursor.
    ///
    /// Once both readers have advanced past a packet it can never be replayed,
    /// so the retained window shrinks to the gap between the readers rather than
    /// growing with the whole stream.
    fn prune(&mut self) {
        let low = self.cursors.iter().copied().min().unwrap_or(self.base);
        if low > self.base {
            let drop = low - self.base;
            self.history.drain(0..drop.min(self.history.len()));
            self.base = low;
        }
    }
}

struct FanReader {
    source: Stream,
    state: Arc<Mutex<FanState>>,
    id: usize,
}

impl StreamNode for FanReader {
    fn metadata(&self) -> &StreamMetadata {
        self.source.metadata()
    }

    fn next_packet(&self) -> Result<Option<StreamItem>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("fan stream"))?;
        let cursor = state.cursors[self.id];
        if cursor < state.base + state.history.len() {
            let item = state.history[cursor - state.base].clone();
            state.cursors[self.id] = cursor + 1;
            state.prune();
            return Ok(Some(item));
        }
        if state.done {
            return Ok(None);
        }
        match self.source.next_packet()? {
            Some(item) => {
                state.history.push_back(item.clone());
                state.cursors[self.id] = cursor + 1;
                state.prune();
                Ok(Some(item))
            }
            None => {
                if self.source.is_done()? {
                    state.done = true;
                }
                Ok(None)
            }
        }
    }

    fn is_done(&self) -> Result<bool> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("fan stream"))?;
        let reader_caught_up = state.cursors[self.id] >= state.base + state.history.len();
        if state.done {
            return Ok(reader_caught_up);
        }
        if reader_caught_up && self.source.is_done()? {
            state.done = true;
            return Ok(true);
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_kernel::Symbol;
    use sim_lib_stream_core::{
        BufferPolicy, StreamDiagnostic, StreamDirection, StreamMedia, StreamPacket,
    };

    fn meta() -> StreamMetadata {
        StreamMetadata::new(
            Symbol::qualified("stream/test", "fan"),
            StreamMedia::Diagnostic,
            StreamDirection::Source,
            Symbol::qualified("clock", "test"),
            BufferPolicy::bounded(8).unwrap(),
        )
    }

    fn item(message: &str) -> StreamItem {
        StreamItem::new(StreamPacket::Diagnostic(StreamDiagnostic::new(
            Symbol::qualified("stream/test", "packet"),
            message,
        )))
    }

    fn shared() -> Arc<Mutex<FanState>> {
        Arc::new(Mutex::new(FanState {
            history: VecDeque::new(),
            base: 0,
            cursors: vec![0, 0],
            done: false,
        }))
    }

    #[test]
    fn history_stays_bounded_when_readers_advance_together() {
        let items: Vec<StreamItem> = (0..6).map(|i| item(&format!("p{i}"))).collect();
        let source = Stream::pull(meta(), items);
        let state = shared();
        let left = FanReader {
            source: source.clone(),
            state: Arc::clone(&state),
            id: 0,
        };
        let right = FanReader {
            source,
            state: Arc::clone(&state),
            id: 1,
        };

        for _ in 0..6 {
            left.next_packet().unwrap();
            assert!(state.lock().unwrap().history.len() <= 1);
            right.next_packet().unwrap();
            assert!(state.lock().unwrap().history.is_empty());
        }
    }

    #[test]
    fn history_prunes_below_the_slowest_reader() {
        let items: Vec<StreamItem> = (0..6).map(|i| item(&format!("p{i}"))).collect();
        let source = Stream::pull(meta(), items);
        let state = shared();
        let left = FanReader {
            source: source.clone(),
            state: Arc::clone(&state),
            id: 0,
        };
        let right = FanReader {
            source,
            state: Arc::clone(&state),
            id: 1,
        };

        for _ in 0..4 {
            left.next_packet().unwrap();
        }
        assert_eq!(state.lock().unwrap().history.len(), 4);

        for _ in 0..4 {
            right.next_packet().unwrap();
        }
        assert!(state.lock().unwrap().history.is_empty());
    }
}

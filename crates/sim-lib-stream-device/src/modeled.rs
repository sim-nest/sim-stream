//! Deterministic modeled device sample sources.

use sim_kernel::Symbol;

use crate::{DeviceCaps, DeviceSample};

/// A deterministic sample source driven entirely by an integer index.
///
/// Implementations must not read clocks, random sources, hardware, network, or
/// process state. The same source and index must always produce the same sample.
pub trait ModeledSource {
    /// Sample type emitted by the source.
    type Sample: DeviceSample;

    /// Returns the sample for `index`.
    fn at(&self, index: u64) -> Self::Sample;
}

/// Reports whether a modeled source's sequence numbers are monotone.
pub fn seq_is_monotone<S: ModeledSource>(source: &S, start: u64, count: usize) -> bool {
    let mut previous = None;
    for offset in 0..count {
        let Some(index) = start.checked_add(offset as u64) else {
            return false;
        };
        let seq = source.at(index).seq();
        if previous.is_some_and(|previous| previous > seq) {
            return false;
        }
        previous = Some(seq);
    }
    true
}

/// Deterministic source for the base [`DeviceCaps`] sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeledDeviceCapsSource {
    device: Symbol,
    streams: Vec<Symbol>,
    inputs: Vec<Symbol>,
    outputs: Vec<Symbol>,
    seq_base: u64,
}

impl ModeledDeviceCapsSource {
    /// Builds a modeled capabilities source.
    pub fn new(
        device: Symbol,
        streams: Vec<Symbol>,
        inputs: Vec<Symbol>,
        outputs: Vec<Symbol>,
    ) -> Self {
        Self {
            device,
            streams,
            inputs,
            outputs,
            seq_base: 0,
        }
    }

    /// Sets the sequence number emitted at index zero.
    pub fn with_seq_base(mut self, seq_base: u64) -> Self {
        self.seq_base = seq_base;
        self
    }

    /// Builds the deterministic demo source used by tests and docs.
    pub fn demo() -> Self {
        Self::new(
            Symbol::qualified("device", "modeled-edge"),
            vec![
                Symbol::qualified("device/stream", "battery"),
                Symbol::qualified("device/stream", "motion"),
            ],
            vec![Symbol::qualified("device/input", "button")],
            vec![
                Symbol::qualified("device/output", "screen"),
                Symbol::qualified("device/output", "haptic"),
            ],
        )
    }
}

impl ModeledSource for ModeledDeviceCapsSource {
    type Sample = DeviceCaps;

    fn at(&self, index: u64) -> Self::Sample {
        DeviceCaps::new(
            self.seq_base.saturating_add(index),
            self.device.clone(),
            self.streams.clone(),
            self.inputs.clone(),
            self.outputs.clone(),
        )
    }
}

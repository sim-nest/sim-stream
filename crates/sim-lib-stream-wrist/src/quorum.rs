//! Quorum scoring for paired worn-device sensor samples.

use sim_kernel::Expr;
use sim_lib_stream_device::{DeviceSampleError, DeviceSampleResult};
use sim_value::access;

use crate::{WORN_CONFIDENCE_MAX, WornEvent, WornSensor};

/// Which side of a two-sample quorum is preferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuorumSide {
    /// The first sample passed to the quorum helper.
    A,
    /// The second sample passed to the quorum helper.
    B,
}

/// Heart-rate quorum result for two worn samples.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeartRateQuorum {
    /// Both watches agree within the configured tolerance.
    Agree {
        /// Averaged beats per minute.
        beats_per_minute: u16,
        /// Quorum confidence in ten-thousandths.
        confidence: u16,
    },
    /// The samples diverge enough that the fleet should prefer one source.
    LowConfidence {
        /// The higher-confidence source.
        prefer: QuorumSide,
        /// Beats per minute from the preferred source.
        beats_per_minute: u16,
        /// Absolute disagreement between the samples.
        delta_bpm: u16,
        /// Lowered quorum confidence in ten-thousandths.
        confidence: u16,
    },
}

impl HeartRateQuorum {
    /// Returns the quorum confidence in ten-thousandths.
    pub fn confidence(&self) -> u16 {
        match self {
            Self::Agree { confidence, .. } | Self::LowConfidence { confidence, .. } => *confidence,
        }
    }
}

/// Scores two heart-rate worn samples as one fleet sensor value.
///
/// Samples must both be [`WornSensor::HeartRate`]. Agreement produces an average
/// value at the lower source confidence; disagreement above `max_delta_bpm`
/// lowers confidence and returns the higher-confidence source as preferred.
pub fn heart_rate_quorum(
    a: &WornEvent,
    b: &WornEvent,
    max_delta_bpm: u16,
) -> DeviceSampleResult<HeartRateQuorum> {
    require_sensor(a, WornSensor::HeartRate, "a")?;
    require_sensor(b, WornSensor::HeartRate, "b")?;

    let a_bpm = heart_rate_bpm(a)?;
    let b_bpm = heart_rate_bpm(b)?;
    let delta = a_bpm.abs_diff(b_bpm);
    let confidence = a.confidence().min(b.confidence());

    if delta > max_delta_bpm {
        let prefer = if a.confidence() >= b.confidence() {
            QuorumSide::A
        } else {
            QuorumSide::B
        };
        let beats_per_minute = match prefer {
            QuorumSide::A => a_bpm,
            QuorumSide::B => b_bpm,
        };
        Ok(HeartRateQuorum::LowConfidence {
            prefer,
            beats_per_minute,
            delta_bpm: delta,
            confidence: lowered_confidence(confidence),
        })
    } else {
        Ok(HeartRateQuorum::Agree {
            beats_per_minute: average_u16(a_bpm, b_bpm),
            confidence,
        })
    }
}

fn require_sensor(event: &WornEvent, sensor: WornSensor, label: &str) -> DeviceSampleResult<()> {
    if event.sensor() == sensor {
        Ok(())
    } else {
        Err(DeviceSampleError::new(format!(
            "quorum sample {label} must use sensor {}, found {}",
            sensor.name(),
            event.sensor().name()
        )))
    }
}

fn heart_rate_bpm(event: &WornEvent) -> DeviceSampleResult<u16> {
    let entries = access::map_entries(event.payload(), "heart-rate payload").map_err(to_sample)?;
    let Some(value) = access::entry_field(entries, "beats-per-minute") else {
        return Err(DeviceSampleError::new(
            "heart-rate payload missing beats-per-minute",
        ));
    };
    let Expr::Number(number) = value else {
        return Err(DeviceSampleError::new(
            "heart-rate beats-per-minute must be numeric",
        ));
    };
    number.canonical.parse::<u16>().map_err(|err| {
        DeviceSampleError::new(format!("invalid heart-rate beats-per-minute: {err}"))
    })
}

fn lowered_confidence(confidence: u16) -> u16 {
    (confidence / 2).min(WORN_CONFIDENCE_MAX)
}

fn average_u16(a: u16, b: u16) -> u16 {
    ((u32::from(a) + u32::from(b)) / 2)
        .try_into()
        .expect("average of two u16 values fits u16")
}

fn to_sample(error: sim_kernel::Error) -> DeviceSampleError {
    DeviceSampleError::new(error.to_string())
}

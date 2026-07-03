//! PCM audio packet payloads: the real-time audio profile behind
//! [`PcmPacket`](crate::PcmPacket).
//!
//! A PCM packet holds an interleaved block of samples described by a channel
//! count, a frame count, and a [`PcmSampleFormat`]. Sample length must equal
//! `channels * frames`; the constructors enforce that invariant (and finite
//! f32 samples) so a packet is always internally consistent.

use sim_kernel::{Error, Expr, Result, Symbol};

use crate::buffer::symbol_field;

use super::{list_field, parse_string_expr, parse_string_field};

/// Sample encoding of a [`PcmPacket`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcmSampleFormat {
    /// 16-bit signed integer samples.
    I16,
    /// 32-bit IEEE-754 floating-point samples.
    F32,
}

impl PcmSampleFormat {
    fn symbol(self) -> Symbol {
        Symbol::qualified("pcm", self.name())
    }

    fn name(self) -> &'static str {
        match self {
            Self::I16 => "i16",
            Self::F32 => "f32",
        }
    }
}

/// A block of interleaved PCM audio samples.
///
/// Carries `channels` interleaved channels of `frames` frames each in a single
/// sample format. The total sample count always equals `channels * frames`.
/// Equality compares f32 samples bitwise so packets with `NaN` payloads still
/// round-trip consistently.
#[derive(Clone, Debug)]
pub struct PcmPacket {
    channels: usize,
    frames: usize,
    samples: PcmPacketSamples,
}

impl PartialEq for PcmPacket {
    fn eq(&self, other: &Self) -> bool {
        self.channels == other.channels
            && self.frames == other.frames
            && self.samples == other.samples
    }
}

impl Eq for PcmPacket {}

#[derive(Clone, Debug)]
enum PcmPacketSamples {
    I16(Vec<i16>),
    F32(Vec<f32>),
}

impl PartialEq for PcmPacketSamples {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::I16(left), Self::I16(right)) => left == right,
            (Self::F32(left), Self::F32(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|(left, right)| left.to_bits() == right.to_bits())
            }
            _ => false,
        }
    }
}

impl Eq for PcmPacketSamples {}

impl PcmPacket {
    /// Builds an [`PcmSampleFormat::I16`] packet from interleaved samples.
    ///
    /// Returns an error when `channels` is zero or `samples_i16.len()` does
    /// not equal `channels * frames`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sim_lib_stream_core::{PcmPacket, PcmSampleFormat};
    ///
    /// // Two channels, two frames -> four interleaved samples.
    /// let packet = PcmPacket::i16(2, 2, vec![0, 1, 2, 3]).unwrap();
    /// assert_eq!(packet.channels(), 2);
    /// assert_eq!(packet.frames(), 2);
    /// assert_eq!(packet.sample_format(), PcmSampleFormat::I16);
    /// assert_eq!(packet.samples_i16(), &[0, 1, 2, 3]);
    /// ```
    pub fn i16(channels: usize, frames: usize, samples_i16: Vec<i16>) -> Result<Self> {
        validate_pcm_shape(channels, frames, samples_i16.len())?;
        Ok(Self {
            channels,
            frames,
            samples: PcmPacketSamples::I16(samples_i16),
        })
    }

    /// Builds an [`PcmSampleFormat::F32`] packet from interleaved samples.
    ///
    /// Returns an error when `channels` is zero, the sample length does not
    /// equal `channels * frames`, or any sample is not finite.
    pub fn f32(channels: usize, frames: usize, samples_f32: Vec<f32>) -> Result<Self> {
        validate_pcm_shape(channels, frames, samples_f32.len())?;
        validate_f32_samples(&samples_f32)?;
        Ok(Self {
            channels,
            frames,
            samples: PcmPacketSamples::F32(samples_f32),
        })
    }

    /// Returns the number of interleaved channels.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Returns the number of frames per channel.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Returns the sample format of the stored samples.
    pub fn sample_format(&self) -> PcmSampleFormat {
        match self.samples {
            PcmPacketSamples::I16(_) => PcmSampleFormat::I16,
            PcmPacketSamples::F32(_) => PcmSampleFormat::F32,
        }
    }

    /// Returns the interleaved i16 samples.
    ///
    /// # Panics
    ///
    /// Panics if the packet holds f32 samples; check
    /// [`sample_format`](PcmPacket::sample_format) first.
    pub fn samples_i16(&self) -> &[i16] {
        match &self.samples {
            PcmPacketSamples::I16(samples) => samples,
            PcmPacketSamples::F32(_) => panic!("PCM packet does not contain i16 samples"),
        }
    }

    /// Returns the interleaved f32 samples.
    ///
    /// # Panics
    ///
    /// Panics if the packet holds i16 samples; check
    /// [`sample_format`](PcmPacket::sample_format) first.
    pub fn samples_f32(&self) -> &[f32] {
        match &self.samples {
            PcmPacketSamples::F32(samples) => samples,
            PcmPacketSamples::I16(_) => panic!("PCM packet does not contain f32 samples"),
        }
    }

    /// Encodes the packet as a `stream/packet/pcm` [`Expr`] map.
    pub fn to_expr(&self) -> Expr {
        Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("packet")),
                Expr::Symbol(Symbol::qualified("stream/packet", "pcm")),
            ),
            (
                Expr::Symbol(Symbol::new("channels")),
                Expr::String(self.channels.to_string()),
            ),
            (
                Expr::Symbol(Symbol::new("frames")),
                Expr::String(self.frames.to_string()),
            ),
            (
                Expr::Symbol(Symbol::new("sample-format")),
                Expr::Symbol(self.sample_format().symbol()),
            ),
            (
                Expr::Symbol(Symbol::new("samples")),
                Expr::List(self.sample_exprs()),
            ),
        ])
    }

    pub(super) fn from_entries(entries: &[(Expr, Expr)]) -> Result<Self> {
        let sample_format = symbol_field(entries, "sample-format")?;
        let channels = parse_string_field::<usize>(entries, "channels")?;
        let frames = parse_string_field::<usize>(entries, "frames")?;
        match sample_format.as_qualified_str().as_str() {
            "pcm/i16" => {
                let samples = list_field(entries, "samples")?
                    .iter()
                    .enumerate()
                    .map(|(index, sample)| {
                        parse_string_expr::<i16>(sample, "PCM i16 sample").map_err(|err| {
                            Error::Eval(format!("invalid PCM i16 sample at {index}: {err}"))
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Self::i16(channels, frames, samples)
            }
            "pcm/f32" => {
                let samples = list_field(entries, "samples")?
                    .iter()
                    .enumerate()
                    .map(|(index, sample)| {
                        parse_string_expr::<f32>(sample, "PCM f32 sample").map_err(|err| {
                            Error::Eval(format!("invalid PCM f32 sample at {index}: {err}"))
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Self::f32(channels, frames, samples)
            }
            _ => Err(Error::Eval(format!(
                "unsupported PCM sample format {}",
                sample_format.as_qualified_str()
            ))),
        }
    }

    fn sample_exprs(&self) -> Vec<Expr> {
        match &self.samples {
            PcmPacketSamples::I16(samples) => samples
                .iter()
                .map(|sample| Expr::String(sample.to_string()))
                .collect(),
            PcmPacketSamples::F32(samples) => samples
                .iter()
                .map(|sample| Expr::String(sample.to_string()))
                .collect(),
        }
    }
}

fn validate_pcm_shape(channels: usize, frames: usize, samples: usize) -> Result<()> {
    if channels == 0 {
        return Err(Error::Eval(
            "PCM packet channel count must be greater than zero".to_owned(),
        ));
    }
    let expected = channels
        .checked_mul(frames)
        .ok_or_else(|| Error::Eval("PCM packet sample count overflow".to_owned()))?;
    if samples != expected {
        return Err(Error::Eval(format!(
            "PCM packet sample length {samples} does not match channels {channels} * frames {frames}"
        )));
    }
    Ok(())
}

fn validate_f32_samples(samples: &[f32]) -> Result<()> {
    if let Some(index) = samples.iter().position(|sample| !sample.is_finite()) {
        return Err(Error::Eval(format!(
            "PCM f32 sample at {index} must be finite"
        )));
    }
    Ok(())
}

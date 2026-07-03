use sim_kernel::{Error, Result};
use sim_lib_stream_core::PcmPacket;

use crate::{PcmSampleFormat, PcmSpec};

/// Owned block of PCM audio: a [`PcmSpec`] plus a frame count and the matching
/// interleaved sample data.
///
/// A `PcmBuffer` is the unit of audio carried across a source or sink. Its
/// sample storage always agrees with its spec: an [`PcmSampleFormat::I16`] spec
/// holds `i16` samples and an [`PcmSampleFormat::F32`] spec holds `f32` samples,
/// with exactly `channels * frames` interleaved values. The constructors
/// enforce that invariant, so [`samples_i16`](PcmBuffer::samples_i16) and
/// [`samples_f32`](PcmBuffer::samples_f32) only need to panic on a caller that
/// asks for the wrong format.
///
/// Equality is bit-exact for `f32` samples (NaN bit patterns compare equal to
/// themselves), which keeps the type usable as a deterministic test fixture.
///
/// # Examples
///
/// ```
/// use sim_lib_stream_audio::{PcmBuffer, PcmSpec};
///
/// let spec = PcmSpec::i16(2, 48_000)?;
/// let buffer = PcmBuffer::i16(spec, 2, vec![1, 2, 3, 4])?;
/// assert_eq!(buffer.frames(), 2);
/// assert_eq!(buffer.samples_i16(), &[1, 2, 3, 4]);
/// # Ok::<(), sim_kernel::Error>(())
/// ```
#[derive(Clone, Debug)]
pub struct PcmBuffer {
    spec: PcmSpec,
    frames: usize,
    samples: PcmBufferSamples,
}

impl PartialEq for PcmBuffer {
    fn eq(&self, other: &Self) -> bool {
        self.spec == other.spec && self.frames == other.frames && self.samples == other.samples
    }
}

impl Eq for PcmBuffer {}

#[derive(Clone, Debug)]
enum PcmBufferSamples {
    I16(Vec<i16>),
    F32(Vec<f32>),
}

impl PartialEq for PcmBufferSamples {
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

impl Eq for PcmBufferSamples {}

impl PcmBuffer {
    /// Builds an `i16` buffer from a spec, frame count, and interleaved samples.
    ///
    /// Returns an error when `spec` is not an [`PcmSampleFormat::I16`] spec or
    /// when `samples_i16.len()` is not `spec.channels() * frames`.
    pub fn i16(spec: PcmSpec, frames: usize, samples_i16: Vec<i16>) -> Result<Self> {
        if spec.sample_format() != PcmSampleFormat::I16 {
            return Err(Error::Eval(
                "PCM buffer requires an i16 PCM spec".to_owned(),
            ));
        }
        let expected = expected_samples(spec.channels(), frames)?;
        if samples_i16.len() != expected {
            return Err(Error::Eval(format!(
                "PCM buffer sample length {} does not match channels {} * frames {}",
                samples_i16.len(),
                spec.channels(),
                frames
            )));
        }
        Ok(Self {
            spec,
            frames,
            samples: PcmBufferSamples::I16(samples_i16),
        })
    }

    /// Builds an `f32` buffer from a spec, frame count, and interleaved samples.
    ///
    /// Returns an error when `spec` is not an [`PcmSampleFormat::F32`] spec, when
    /// `samples_f32.len()` is not `spec.channels() * frames`, or when any sample
    /// is not finite.
    pub fn f32(spec: PcmSpec, frames: usize, samples_f32: Vec<f32>) -> Result<Self> {
        if spec.sample_format() != PcmSampleFormat::F32 {
            return Err(Error::Eval(
                "PCM buffer requires an f32 PCM spec".to_owned(),
            ));
        }
        let expected = expected_samples(spec.channels(), frames)?;
        if samples_f32.len() != expected {
            return Err(Error::Eval(format!(
                "PCM buffer sample length {} does not match channels {} * frames {}",
                samples_f32.len(),
                spec.channels(),
                frames
            )));
        }
        validate_f32_samples(&samples_f32)?;
        Ok(Self {
            spec,
            frames,
            samples: PcmBufferSamples::F32(samples_f32),
        })
    }

    /// Builds a buffer from a `sim-lib-stream-core` [`PcmPacket`], adopting
    /// `spec`.
    ///
    /// Returns an error when the packet's channel count or sample format does
    /// not match `spec`.
    pub fn from_packet(spec: PcmSpec, packet: &PcmPacket) -> Result<Self> {
        if packet.channels() != spec.channels() {
            return Err(Error::Eval(format!(
                "PCM packet channel count {} does not match sink spec {}",
                packet.channels(),
                spec.channels()
            )));
        }
        if packet.sample_format() != spec.sample_format() {
            return Err(Error::Eval(format!(
                "PCM packet sample format {:?} does not match sink spec {:?}",
                packet.sample_format(),
                spec.sample_format()
            )));
        }
        match spec.sample_format() {
            PcmSampleFormat::I16 => Self::i16(spec, packet.frames(), packet.samples_i16().to_vec()),
            PcmSampleFormat::F32 => Self::f32(spec, packet.frames(), packet.samples_f32().to_vec()),
        }
    }

    /// Encodes this buffer as a `sim-lib-stream-core` [`PcmPacket`] for stream
    /// transport.
    pub fn to_packet(&self) -> Result<PcmPacket> {
        match &self.samples {
            PcmBufferSamples::I16(samples) => {
                PcmPacket::i16(self.spec.channels(), self.frames, samples.clone())
            }
            PcmBufferSamples::F32(samples) => {
                PcmPacket::f32(self.spec.channels(), self.frames, samples.clone())
            }
        }
    }

    /// Returns the audio format of this buffer.
    pub fn spec(&self) -> PcmSpec {
        self.spec
    }

    /// Returns the number of audio frames (samples per channel) in this buffer.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Returns the interleaved `i16` samples.
    ///
    /// # Panics
    ///
    /// Panics if this buffer holds `f32` samples rather than `i16`.
    pub fn samples_i16(&self) -> &[i16] {
        match &self.samples {
            PcmBufferSamples::I16(samples) => samples,
            PcmBufferSamples::F32(_) => panic!("PCM buffer does not contain i16 samples"),
        }
    }

    /// Returns the interleaved `f32` samples.
    ///
    /// # Panics
    ///
    /// Panics if this buffer holds `i16` samples rather than `f32`.
    pub fn samples_f32(&self) -> &[f32] {
        match &self.samples {
            PcmBufferSamples::F32(samples) => samples,
            PcmBufferSamples::I16(_) => panic!("PCM buffer does not contain f32 samples"),
        }
    }
}

/// Converts one `i16` sample to the normalized `f32` range `[-1.0, 1.0]`.
///
/// Negative values scale by `32768` and non-negative values by [`i16::MAX`], so
/// the full `i16` range maps symmetrically into the float interval.
///
/// # Examples
///
/// ```
/// use sim_lib_stream_audio::i16_sample_to_f32;
///
/// assert_eq!(i16_sample_to_f32(0), 0.0);
/// assert_eq!(i16_sample_to_f32(i16::MAX), 1.0);
/// ```
pub fn i16_sample_to_f32(sample: i16) -> f32 {
    if sample < 0 {
        f32::from(sample) / 32768.0
    } else {
        f32::from(sample) / f32::from(i16::MAX)
    }
}

/// Converts one normalized `f32` sample to `i16`, clamping to `[-1.0, 1.0]`.
///
/// Returns an error when the input is not finite. In-range values are rounded
/// to the nearest integer and saturated at the `i16` bounds.
pub fn f32_sample_to_i16(sample: f32) -> Result<i16> {
    if !sample.is_finite() {
        return Err(Error::Eval("PCM f32 sample must be finite".to_owned()));
    }
    let clamped = sample.clamp(-1.0, 1.0);
    if clamped < 0.0 {
        Ok((clamped * 32768.0).round().max(f32::from(i16::MIN)) as i16)
    } else {
        Ok((clamped * f32::from(i16::MAX))
            .round()
            .min(f32::from(i16::MAX)) as i16)
    }
}

/// Converts a slice of `i16` samples to normalized `f32` samples via
/// [`i16_sample_to_f32`].
pub fn i16_samples_to_f32(samples: &[i16]) -> Vec<f32> {
    samples
        .iter()
        .map(|sample| i16_sample_to_f32(*sample))
        .collect()
}

/// Converts a slice of normalized `f32` samples to `i16` via
/// [`f32_sample_to_i16`].
///
/// Returns an error at the first non-finite sample.
pub fn f32_samples_to_i16(samples: &[f32]) -> Result<Vec<i16>> {
    samples
        .iter()
        .map(|sample| f32_sample_to_i16(*sample))
        .collect()
}

/// Splits interleaved `i16` samples into one `Vec` per channel.
///
/// Returns an error when `channels` is zero or `samples.len()` is not a
/// multiple of `channels`.
pub fn i16_interleaved_to_planar(samples: &[i16], channels: usize) -> Result<Vec<Vec<i16>>> {
    interleaved_to_planar(samples, channels)
}

/// Interleaves per-channel `i16` planes into a single interleaved `Vec`.
///
/// Returns an error when no channel is supplied or the channel planes differ in
/// length.
pub fn i16_planar_to_interleaved(channels: &[Vec<i16>]) -> Result<Vec<i16>> {
    planar_to_interleaved(channels)
}

/// Splits interleaved `f32` samples into one `Vec` per channel.
///
/// Returns an error when any sample is not finite, `channels` is zero, or
/// `samples.len()` is not a multiple of `channels`.
pub fn f32_interleaved_to_planar(samples: &[f32], channels: usize) -> Result<Vec<Vec<f32>>> {
    validate_f32_samples(samples)?;
    interleaved_to_planar(samples, channels)
}

/// Interleaves per-channel `f32` planes into a single interleaved `Vec`.
///
/// Returns an error when no channel is supplied, the channel planes differ in
/// length, or any sample is not finite.
pub fn f32_planar_to_interleaved(channels: &[Vec<f32>]) -> Result<Vec<f32>> {
    let samples = planar_to_interleaved(channels)?;
    validate_f32_samples(&samples)?;
    Ok(samples)
}

fn expected_samples(channels: usize, frames: usize) -> Result<usize> {
    channels
        .checked_mul(frames)
        .ok_or_else(|| Error::Eval("PCM buffer sample count overflow".to_owned()))
}

fn validate_f32_samples(samples: &[f32]) -> Result<()> {
    if let Some(index) = samples.iter().position(|sample| !sample.is_finite()) {
        return Err(Error::Eval(format!(
            "PCM f32 sample at {index} must be finite"
        )));
    }
    Ok(())
}

fn interleaved_to_planar<T>(samples: &[T], channels: usize) -> Result<Vec<Vec<T>>>
where
    T: Copy,
{
    if channels == 0 {
        return Err(Error::Eval(
            "PCM channel count must be greater than zero".to_owned(),
        ));
    }
    if !samples.len().is_multiple_of(channels) {
        return Err(Error::Eval(format!(
            "PCM interleaved sample length {} is not divisible by channels {}",
            samples.len(),
            channels
        )));
    }
    let frames = samples.len() / channels;
    let mut planar = vec![Vec::with_capacity(frames); channels];
    for frame in samples.chunks(channels) {
        for (channel, sample) in frame.iter().enumerate() {
            planar[channel].push(*sample);
        }
    }
    Ok(planar)
}

fn planar_to_interleaved<T>(channels: &[Vec<T>]) -> Result<Vec<T>>
where
    T: Copy,
{
    let Some(first) = channels.first() else {
        return Err(Error::Eval(
            "PCM planar data must contain at least one channel".to_owned(),
        ));
    };
    let frames = first.len();
    if let Some((index, channel)) = channels
        .iter()
        .enumerate()
        .find(|(_, channel)| channel.len() != frames)
    {
        return Err(Error::Eval(format!(
            "PCM planar channel {index} length {} does not match frame count {frames}",
            channel.len()
        )));
    }
    let mut interleaved = Vec::with_capacity(frames * channels.len());
    for frame in 0..frames {
        for channel in channels {
            interleaved.push(channel[frame]);
        }
    }
    Ok(interleaved)
}

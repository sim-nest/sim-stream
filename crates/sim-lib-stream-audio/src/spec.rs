use sim_kernel::{Error, Result};
pub use sim_lib_stream_core::PcmSampleFormat;

/// Audio format description for a PCM stream: channel count, sample rate, and
/// sample encoding.
///
/// A `PcmSpec` is the validated, immutable contract shared by PCM buffers,
/// sources, and sinks. It is constructed through [`PcmSpec::i16`] or
/// [`PcmSpec::f32`], which reject a zero channel count or zero sample rate, so a
/// constructed value is always a usable audio configuration.
///
/// # Examples
///
/// ```
/// use sim_lib_stream_audio::{PcmSampleFormat, PcmSpec};
///
/// let spec = PcmSpec::f32(2, 48_000)?;
/// assert_eq!(spec.channels(), 2);
/// assert_eq!(spec.sample_rate_hz(), 48_000);
/// assert_eq!(spec.sample_format(), PcmSampleFormat::F32);
/// # Ok::<(), sim_kernel::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcmSpec {
    channels: usize,
    sample_rate_hz: u32,
    sample_format: PcmSampleFormat,
}

impl PcmSpec {
    /// Builds an interleaved 16-bit signed integer ([`PcmSampleFormat::I16`])
    /// spec.
    ///
    /// Returns an error when `channels` or `sample_rate_hz` is zero.
    pub fn i16(channels: usize, sample_rate_hz: u32) -> Result<Self> {
        if channels == 0 {
            return Err(Error::Eval(
                "PCM spec channel count must be greater than zero".to_owned(),
            ));
        }
        if sample_rate_hz == 0 {
            return Err(Error::Eval(
                "PCM spec sample rate must be greater than zero".to_owned(),
            ));
        }
        Ok(Self {
            channels,
            sample_rate_hz,
            sample_format: PcmSampleFormat::I16,
        })
    }

    /// Builds an interleaved 32-bit float ([`PcmSampleFormat::F32`]) spec.
    ///
    /// Returns an error when `channels` or `sample_rate_hz` is zero.
    pub fn f32(channels: usize, sample_rate_hz: u32) -> Result<Self> {
        if channels == 0 {
            return Err(Error::Eval(
                "PCM spec channel count must be greater than zero".to_owned(),
            ));
        }
        if sample_rate_hz == 0 {
            return Err(Error::Eval(
                "PCM spec sample rate must be greater than zero".to_owned(),
            ));
        }
        Ok(Self {
            channels,
            sample_rate_hz,
            sample_format: PcmSampleFormat::F32,
        })
    }

    /// Returns the number of interleaved audio channels.
    pub fn channels(self) -> usize {
        self.channels
    }

    /// Returns the sample rate in hertz (frames per second).
    pub fn sample_rate_hz(self) -> u32 {
        self.sample_rate_hz
    }

    /// Returns the per-sample encoding.
    pub fn sample_format(self) -> PcmSampleFormat {
        self.sample_format
    }
}

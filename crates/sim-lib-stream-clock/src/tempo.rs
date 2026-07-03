use sim_kernel::{Error, Result};

use crate::Instant;

/// One constant-tempo span of a MIDI [`TempoMap`], starting at a tick offset.
///
/// A segment holds the tempo (in microseconds per quarter note) that applies
/// from `start_tick` until the next segment begins. The tempo must be
/// non-zero.
///
/// # Examples
///
/// ```
/// use sim_lib_stream_clock::TempoSegment;
///
/// let segment = TempoSegment::new(0, 500_000)?;
/// assert_eq!(segment.start_tick, 0);
/// assert_eq!(segment.us_per_quarter, 500_000);
/// # Ok::<(), sim_kernel::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TempoSegment {
    /// Tick offset at which this tempo takes effect.
    pub start_tick: u64,
    /// Tempo of the segment, in microseconds per quarter note.
    pub us_per_quarter: u32,
}

impl TempoSegment {
    /// Builds a tempo segment beginning at `start_tick`.
    ///
    /// Returns an error when `us_per_quarter` is zero.
    pub fn new(start_tick: u64, us_per_quarter: u32) -> Result<Self> {
        if us_per_quarter == 0 {
            return Err(Error::Eval(
                "tempo segment microseconds-per-quarter must be non-zero".to_owned(),
            ));
        }
        Ok(Self {
            start_tick,
            us_per_quarter,
        })
    }
}

/// Ordered list of [`TempoSegment`]s describing how MIDI tempo changes over a
/// timeline.
///
/// A valid map starts with a segment at tick 0 and has strictly increasing
/// segment ticks, so every tick maps to exactly one tempo. The map drives the
/// tick <-> [`Instant`] conversions for MIDI clocks.
///
/// # Examples
///
/// ```
/// use sim_lib_stream_clock::TempoMap;
///
/// let map = TempoMap::single(500_000)?;
/// assert_eq!(map.segments().len(), 1);
/// assert_eq!(map.segments()[0].start_tick, 0);
/// # Ok::<(), sim_kernel::Error>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TempoMap {
    segments: Vec<TempoSegment>,
}

impl TempoMap {
    /// Builds a tempo map from `segments`.
    ///
    /// Returns an error when `segments` is empty, when the first segment does
    /// not start at tick 0, or when the segment ticks are not strictly
    /// increasing.
    pub fn new(segments: Vec<TempoSegment>) -> Result<Self> {
        let first = segments
            .first()
            .ok_or_else(|| Error::Eval("tempo map must contain at least one segment".to_owned()))?;
        if first.start_tick != 0 {
            return Err(Error::Eval(
                "tempo map must start with a segment at tick 0".to_owned(),
            ));
        }
        for pair in segments.windows(2) {
            if pair[0].start_tick >= pair[1].start_tick {
                return Err(Error::Eval(
                    "tempo map segment ticks must be strictly increasing".to_owned(),
                ));
            }
        }
        Ok(Self { segments })
    }

    /// Builds a single-segment map holding a constant tempo from tick 0.
    ///
    /// Returns an error when `us_per_quarter` is zero.
    pub fn single(us_per_quarter: u32) -> Result<Self> {
        Self::new(vec![TempoSegment::new(0, us_per_quarter)?])
    }

    /// Returns the map's segments in tick order.
    pub fn segments(&self) -> &[TempoSegment] {
        &self.segments
    }

    pub(crate) fn segment_start_instants(&self, tpq: u32) -> Result<Vec<Instant>> {
        if tpq == 0 {
            return Err(Error::Eval("midi clock TPQ must be non-zero".to_owned()));
        }
        let mut starts = Vec::with_capacity(self.segments.len());
        starts.push(Instant::seconds(0));
        let mut current = Instant::seconds(0);
        for pair in self.segments.windows(2) {
            let delta_ticks = pair[1].start_tick - pair[0].start_tick;
            let duration = midi_tick_duration(delta_ticks, tpq, pair[0].us_per_quarter)?;
            current = current.checked_add(duration)?;
            starts.push(current);
        }
        Ok(starts)
    }
}

pub(crate) fn midi_tick_duration(ticks: u64, tpq: u32, us_per_quarter: u32) -> Result<Instant> {
    if tpq == 0 {
        return Err(Error::Eval("midi clock TPQ must be non-zero".to_owned()));
    }
    if us_per_quarter == 0 {
        return Err(Error::Eval(
            "tempo segment microseconds-per-quarter must be non-zero".to_owned(),
        ));
    }
    let numerator = i128::from(ticks)
        .checked_mul(i128::from(us_per_quarter))
        .ok_or_else(|| Error::Eval("midi tick duration overflowed".to_owned()))?;
    let denominator = i128::from(tpq) * 1_000_000;
    Instant::new(numerator, denominator)
}

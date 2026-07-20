//! Deterministic modeled worn-device sources.

use sim_lib_stream_device::ModeledSource;

use crate::WornEvent;

/// Deterministic modeled heart-rate source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeledHeartRateSource;

impl ModeledSource for ModeledHeartRateSource {
    type Sample = WornEvent;

    fn at(&self, index: u64) -> Self::Sample {
        let beats = 58 + u16::try_from(index % 43).expect("bounded heart-rate delta");
        WornEvent::heart_rate(index, beats).expect("modeled heart-rate sample is valid")
    }
}

/// Deterministic modeled motion source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeledMotionSource;

impl ModeledSource for ModeledMotionSource {
    type Sample = WornEvent;

    fn at(&self, index: u64) -> Self::Sample {
        let swing = i32::try_from(index % 21).expect("bounded motion delta") - 10;
        WornEvent::motion(index, swing * 12, 1_000 - swing.abs() * 2, swing * -7)
            .expect("modeled motion sample is valid")
    }
}

/// Deterministic modeled GPS location source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeledLocationSource;

impl ModeledSource for ModeledLocationSource {
    type Sample = WornEvent;

    fn at(&self, index: u64) -> Self::Sample {
        let offset = i32::try_from(index % 200).expect("bounded location delta") - 100;
        WornEvent::gps(
            index,
            59_329_300 + offset * 10,
            18_068_600 - offset * 7,
            450 + u32::try_from(index % 50).expect("bounded accuracy delta"),
        )
        .expect("modeled location sample is valid")
    }
}

/// Deterministic modeled battery source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeledBatterySource;

impl ModeledSource for ModeledBatterySource {
    type Sample = WornEvent;

    fn at(&self, index: u64) -> Self::Sample {
        let percent = 100 - u8::try_from(index % 101).expect("bounded battery percent");
        let charging = index % 20 >= 16;
        WornEvent::battery(index, percent, charging).expect("modeled battery sample is valid")
    }
}

/// Deterministic modeled connection source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeledConnectionSource;

impl ModeledSource for ModeledConnectionSource {
    type Sample = WornEvent;

    fn at(&self, index: u64) -> Self::Sample {
        let connected = !index.is_multiple_of(11);
        let rssi = -45 - i16::try_from(index % 30).expect("bounded RSSI delta");
        WornEvent::connection(index, connected, rssi).expect("modeled connection sample is valid")
    }
}

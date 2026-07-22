//! Strict worn-device sample records.

use sim_kernel::{Expr, Symbol};
use sim_lib_stream_core::StreamPacket;
use sim_lib_stream_device::{
    DeviceSample, DeviceSampleError, DeviceSampleResult, device_sample_record_symbol,
    sample_kind_symbol, sample_packet,
};
use sim_value::{access, build};

/// Maximum accepted confidence value, expressed in ten-thousandths.
pub const WORN_CONFIDENCE_MAX: u16 = 10_000;

const WORN_SENSOR_NAMESPACE: &str = "stream/worn-sensor";
const WORN_PAYLOAD_NAMESPACE: &str = "stream/worn-payload";
const WORN_AUDIO_NAMESPACE: &str = "stream/worn-audio";

const WORN_EVENT_FIELDS: &[&str] = &["kind", "sample", "seq", "sensor", "confidence", "payload"];
const MIC_AUDIO_FIELDS: &[&str] = &["kind", "frame-index", "sample-rate-hz", "channels", "bytes"];

/// The stable set of wearable sensor lanes accepted by [`WornEvent`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WornSensor {
    /// Heart-rate measurements.
    HeartRate,
    /// Wrist motion measurements.
    Motion,
    /// Blood oxygen measurements.
    Spo2,
    /// Skin or device temperature measurements.
    Temperature,
    /// Barometric pressure measurements.
    Barometer,
    /// Altitude estimates.
    Altimeter,
    /// Compass heading measurements.
    Compass,
    /// Step count measurements.
    Steps,
    /// Sleep stage or sleep summary measurements.
    Sleep,
    /// Sport activity state measurements.
    Sport,
    /// Lap boundary measurements.
    Lap,
    /// Route summary measurements.
    Route,
    /// GPS position measurements.
    Gps,
    /// Depth measurements.
    Depth,
    /// Battery state measurements.
    Battery,
    /// Device connection state measurements.
    Connection,
    /// Button input events.
    Button,
    /// Touch input events.
    Touch,
    /// Raw microphone audio frames.
    MicAudio,
    /// Haptic acknowledgment events.
    HapticAck,
}

impl WornSensor {
    /// Returns every stable worn sensor variant in wire-order.
    pub fn all() -> &'static [Self] {
        &ALL_WORN_SENSORS
    }

    /// Returns the stable wire name for this sensor.
    pub fn name(self) -> &'static str {
        match self {
            Self::HeartRate => "heart-rate",
            Self::Motion => "motion",
            Self::Spo2 => "spo2",
            Self::Temperature => "temperature",
            Self::Barometer => "barometer",
            Self::Altimeter => "altimeter",
            Self::Compass => "compass",
            Self::Steps => "steps",
            Self::Sleep => "sleep",
            Self::Sport => "sport",
            Self::Lap => "lap",
            Self::Route => "route",
            Self::Gps => "gps",
            Self::Depth => "depth",
            Self::Battery => "battery",
            Self::Connection => "connection",
            Self::Button => "button",
            Self::Touch => "touch",
            Self::MicAudio => "mic-audio",
            Self::HapticAck => "haptic-ack",
        }
    }

    /// Returns the stable symbol for this sensor.
    pub fn symbol(self) -> Symbol {
        Symbol::qualified(WORN_SENSOR_NAMESPACE, self.name())
    }

    /// Decodes a worn sensor from its stable symbol.
    pub fn from_symbol(symbol: &Symbol) -> DeviceSampleResult<Self> {
        if symbol.namespace.as_deref() != Some(WORN_SENSOR_NAMESPACE) {
            return Err(DeviceSampleError::new(format!(
                "worn sensor must be a {WORN_SENSOR_NAMESPACE}/* symbol, found {symbol}"
            )));
        }
        Self::all()
            .iter()
            .copied()
            .find(|sensor| sensor.name() == symbol.name.as_ref())
            .ok_or_else(|| DeviceSampleError::new(format!("unknown worn sensor {symbol}")))
    }
}

const ALL_WORN_SENSORS: [WornSensor; 20] = [
    WornSensor::HeartRate,
    WornSensor::Motion,
    WornSensor::Spo2,
    WornSensor::Temperature,
    WornSensor::Barometer,
    WornSensor::Altimeter,
    WornSensor::Compass,
    WornSensor::Steps,
    WornSensor::Sleep,
    WornSensor::Sport,
    WornSensor::Lap,
    WornSensor::Route,
    WornSensor::Gps,
    WornSensor::Depth,
    WornSensor::Battery,
    WornSensor::Connection,
    WornSensor::Button,
    WornSensor::Touch,
    WornSensor::MicAudio,
    WornSensor::HapticAck,
];

/// A raw microphone audio frame payload for [`WornSensor::MicAudio`].
///
/// This payload intentionally carries framed bytes and format metadata. It does
/// not carry transcripts, commands, or intent labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MicAudioFrame {
    frame_index: u64,
    sample_rate_hz: u32,
    channels: u16,
    bytes: Vec<u8>,
}

impl MicAudioFrame {
    /// Builds a raw microphone frame.
    ///
    /// Returns an error when the sample rate or channel count is zero.
    pub fn new(
        frame_index: u64,
        sample_rate_hz: u32,
        channels: u16,
        bytes: Vec<u8>,
    ) -> DeviceSampleResult<Self> {
        if sample_rate_hz == 0 {
            return Err(DeviceSampleError::new(
                "mic-audio sample-rate-hz must be greater than zero",
            ));
        }
        if channels == 0 {
            return Err(DeviceSampleError::new(
                "mic-audio channels must be greater than zero",
            ));
        }
        Ok(Self {
            frame_index,
            sample_rate_hz,
            channels,
            bytes,
        })
    }

    /// Returns the frame index within the raw audio stream.
    pub fn frame_index(&self) -> u64 {
        self.frame_index
    }

    /// Returns the audio sample rate in hertz.
    pub fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    /// Returns the number of channels in the raw frame.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Returns the raw framed audio bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Encodes this raw audio frame as an expression payload.
    pub fn to_expr(&self) -> Expr {
        build::map(vec![
            ("kind", Expr::Symbol(mic_audio_raw_frame_symbol())),
            ("frame-index", build::uint(self.frame_index)),
            (
                "sample-rate-hz",
                build::uint(u64::from(self.sample_rate_hz)),
            ),
            ("channels", build::uint(u64::from(self.channels))),
            ("bytes", Expr::Bytes(self.bytes.clone())),
        ])
    }

    /// Decodes a strict raw audio frame payload.
    pub fn from_expr(expr: &Expr) -> DeviceSampleResult<Self> {
        let entries = map_entries(expr, "mic-audio payload map")?;
        expect_only_fields(entries, MIC_AUDIO_FIELDS, "mic-audio payload")?;
        expect_symbol_field(entries, "kind", &mic_audio_raw_frame_symbol())?;
        let bytes = match field(entries, "bytes")? {
            Expr::Bytes(bytes) => bytes.clone(),
            other => {
                return Err(DeviceSampleError::new(format!(
                    "mic-audio bytes must be bytes, found {}",
                    sim_value::kind::expr_kind(other)
                )));
            }
        };
        Self::new(
            u64_field(entries, "frame-index")?,
            u32_field(entries, "sample-rate-hz")?,
            u16_field(entries, "channels")?,
            bytes,
        )
    }
}

/// A strict wearable event sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WornEvent {
    seq: u64,
    sensor: WornSensor,
    confidence: u16,
    payload: Expr,
}

impl WornEvent {
    /// Builds a worn event sample.
    ///
    /// `confidence` is an integer in ten-thousandths, from `0` through
    /// [`WORN_CONFIDENCE_MAX`]. The microphone-audio sensor accepts only the
    /// raw framed audio payload produced by [`MicAudioFrame`].
    pub fn new(
        seq: u64,
        sensor: WornSensor,
        confidence: u16,
        payload: Expr,
    ) -> DeviceSampleResult<Self> {
        if confidence > WORN_CONFIDENCE_MAX {
            return Err(DeviceSampleError::new(format!(
                "worn event confidence must be <= {WORN_CONFIDENCE_MAX}"
            )));
        }
        if sensor == WornSensor::MicAudio {
            MicAudioFrame::from_expr(&payload)?;
        }
        Ok(Self {
            seq,
            sensor,
            confidence,
            payload,
        })
    }

    /// Builds a heart-rate event.
    pub fn heart_rate(seq: u64, beats_per_minute: u16) -> DeviceSampleResult<Self> {
        if beats_per_minute == 0 {
            return Err(DeviceSampleError::new(
                "heart-rate beats-per-minute must be greater than zero",
            ));
        }
        Self::new(
            seq,
            WornSensor::HeartRate,
            9_800,
            build::map(vec![
                ("kind", payload_symbol_expr("heart-rate")),
                ("beats-per-minute", build::uint(u64::from(beats_per_minute))),
            ]),
        )
    }

    /// Builds a motion event from milli-g acceleration components.
    pub fn motion(
        seq: u64,
        x_milli_g: i32,
        y_milli_g: i32,
        z_milli_g: i32,
    ) -> DeviceSampleResult<Self> {
        Self::new(
            seq,
            WornSensor::Motion,
            9_600,
            build::map(vec![
                ("kind", payload_symbol_expr("motion")),
                ("x-milli-g", build::int(i64::from(x_milli_g))),
                ("y-milli-g", build::int(i64::from(y_milli_g))),
                ("z-milli-g", build::int(i64::from(z_milli_g))),
            ]),
        )
    }

    /// Builds a GPS location event.
    pub fn gps(
        seq: u64,
        latitude_micro_degrees: i32,
        longitude_micro_degrees: i32,
        accuracy_cm: u32,
    ) -> DeviceSampleResult<Self> {
        Self::new(
            seq,
            WornSensor::Gps,
            9_500,
            build::map(vec![
                ("kind", payload_symbol_expr("gps")),
                (
                    "latitude-micro-degrees",
                    build::int(i64::from(latitude_micro_degrees)),
                ),
                (
                    "longitude-micro-degrees",
                    build::int(i64::from(longitude_micro_degrees)),
                ),
                ("accuracy-cm", build::uint(u64::from(accuracy_cm))),
            ]),
        )
    }

    /// Builds a battery event.
    pub fn battery(seq: u64, percent: u8, charging: bool) -> DeviceSampleResult<Self> {
        if percent > 100 {
            return Err(DeviceSampleError::new(
                "battery percent must be between 0 and 100",
            ));
        }
        Self::new(
            seq,
            WornSensor::Battery,
            10_000,
            build::map(vec![
                ("kind", payload_symbol_expr("battery")),
                ("percent", build::uint(u64::from(percent))),
                ("charging", Expr::Bool(charging)),
            ]),
        )
    }

    /// Builds a connection event.
    pub fn connection(seq: u64, connected: bool, rssi_dbm: i16) -> DeviceSampleResult<Self> {
        Self::new(
            seq,
            WornSensor::Connection,
            9_900,
            build::map(vec![
                ("kind", payload_symbol_expr("connection")),
                ("connected", Expr::Bool(connected)),
                ("rssi-dbm", build::int(i64::from(rssi_dbm))),
            ]),
        )
    }

    /// Builds a microphone event from a raw audio frame.
    pub fn mic_audio(seq: u64, confidence: u16, frame: MicAudioFrame) -> DeviceSampleResult<Self> {
        Self::new(seq, WornSensor::MicAudio, confidence, frame.to_expr())
    }

    /// Returns the monotone sequence number.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Returns the sensor lane.
    pub fn sensor(&self) -> WornSensor {
        self.sensor
    }

    /// Returns the confidence score in ten-thousandths.
    pub fn confidence(&self) -> u16 {
        self.confidence
    }

    /// Returns the sensor-specific payload expression.
    pub fn payload(&self) -> &Expr {
        &self.payload
    }

    /// Wraps this worn event as a stream data packet.
    pub fn to_stream_packet(&self) -> StreamPacket {
        sample_packet(self)
    }
}

impl DeviceSample for WornEvent {
    fn sample_kind() -> &'static str {
        "worn-event"
    }

    fn seq(&self) -> u64 {
        self.seq
    }

    fn to_expr(&self) -> Expr {
        build::map(vec![
            ("kind", Expr::Symbol(device_sample_record_symbol())),
            ("sample", Expr::Symbol(worn_event_sample_kind_symbol())),
            ("seq", build::uint(self.seq)),
            ("sensor", Expr::Symbol(self.sensor.symbol())),
            ("confidence", build::uint(u64::from(self.confidence))),
            ("payload", self.payload.clone()),
        ])
    }

    fn from_expr(expr: &Expr) -> DeviceSampleResult<Self> {
        let entries = map_entries(expr, "worn event map")?;
        expect_only_fields(entries, WORN_EVENT_FIELDS, "worn event")?;
        expect_symbol_field(entries, "kind", &device_sample_record_symbol())?;
        expect_symbol_field(entries, "sample", &worn_event_sample_kind_symbol())?;
        Self::new(
            u64_field(entries, "seq")?,
            WornSensor::from_symbol(symbol_field(entries, "sensor")?)?,
            u16_field(entries, "confidence")?,
            field(entries, "payload")?.clone(),
        )
    }
}

/// Returns the qualified sample-kind symbol for [`WornEvent`].
pub fn worn_event_sample_kind_symbol() -> Symbol {
    sample_kind_symbol(WornEvent::sample_kind())
}

/// Returns the raw microphone frame payload tag.
pub fn mic_audio_raw_frame_symbol() -> Symbol {
    Symbol::qualified(WORN_AUDIO_NAMESPACE, "raw-frame")
}

pub(crate) fn decode_known_worn_event(expr: &Expr) -> DeviceSampleResult<()> {
    WornEvent::from_expr(expr).map(|_| ())
}

pub(crate) fn worn_constructor_args(expr: &Expr) -> DeviceSampleResult<Vec<Expr>> {
    decode_known_worn_event(expr)?;
    Ok(vec![expr.clone()])
}

fn payload_symbol_expr(name: &'static str) -> Expr {
    Expr::Symbol(Symbol::qualified(WORN_PAYLOAD_NAMESPACE, name))
}

fn map_entries<'a>(
    expr: &'a Expr,
    expected: &'static str,
) -> DeviceSampleResult<&'a [(Expr, Expr)]> {
    access::map_entries(expr, expected).map_err(kernel_error)
}

fn expect_only_fields(
    entries: &[(Expr, Expr)],
    known: &[&str],
    context: &str,
) -> DeviceSampleResult<()> {
    for (key, _) in entries {
        let Expr::Symbol(symbol) = key else {
            return Err(DeviceSampleError::new(format!(
                "{context} fields must use bare symbol keys"
            )));
        };
        if symbol.namespace.is_some() || !known.contains(&symbol.name.as_ref()) {
            return Err(DeviceSampleError::new(format!(
                "{context} has unexpected field {symbol}"
            )));
        }
    }
    Ok(())
}

fn expect_symbol_field(
    entries: &[(Expr, Expr)],
    name: &str,
    expected: &Symbol,
) -> DeviceSampleResult<()> {
    let actual = symbol_field(entries, name)?;
    if actual == expected {
        Ok(())
    } else {
        Err(DeviceSampleError::new(format!(
            "worn event field {name} must be {expected}, found {actual}"
        )))
    }
}

fn symbol_field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> DeviceSampleResult<&'a Symbol> {
    match field(entries, name)? {
        Expr::Symbol(symbol) => Ok(symbol),
        other => Err(DeviceSampleError::new(format!(
            "worn event field {name} must be a symbol, found {}",
            sim_value::kind::expr_kind(other)
        ))),
    }
}

fn field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> DeviceSampleResult<&'a Expr> {
    entries
        .iter()
        .find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == name => {
                Some(value)
            }
            _ => None,
        })
        .ok_or_else(|| DeviceSampleError::new(format!("worn event missing field {name}")))
}

fn u64_field(entries: &[(Expr, Expr)], name: &str) -> DeviceSampleResult<u64> {
    let value = field(entries, name)?;
    let Expr::Number(number) = value else {
        return Err(DeviceSampleError::new(format!(
            "worn event field {name} must be a u64 number, found {}",
            sim_value::kind::expr_kind(value)
        )));
    };
    if !matches!(number.domain.name.as_ref(), "i64" | "u64") {
        return Err(DeviceSampleError::new(format!(
            "worn event field {name} must use an integer domain, found {}",
            number.domain
        )));
    }
    number
        .canonical
        .parse::<u64>()
        .map_err(|err| DeviceSampleError::new(format!("invalid worn event field {name}: {err}")))
}

fn u32_field(entries: &[(Expr, Expr)], name: &str) -> DeviceSampleResult<u32> {
    u64_field(entries, name)?
        .try_into()
        .map_err(|_| DeviceSampleError::new(format!("worn event field {name} is out of range")))
}

fn u16_field(entries: &[(Expr, Expr)], name: &str) -> DeviceSampleResult<u16> {
    u64_field(entries, name)?
        .try_into()
        .map_err(|_| DeviceSampleError::new(format!("worn event field {name} is out of range")))
}

fn kernel_error(error: sim_kernel::Error) -> DeviceSampleError {
    DeviceSampleError::new(error.to_string())
}

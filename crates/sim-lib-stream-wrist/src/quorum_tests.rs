use sim_kernel::Expr;
use sim_value::build;

use crate::{HeartRateQuorum, QuorumSide, WornEvent, WornSensor, heart_rate_quorum};

#[test]
fn divergent_hr_lowers_confidence_and_prefers_higher_confidence() {
    let low = heart_rate(1, 72, 8_000);
    let high = heart_rate(2, 94, 9_600);

    let quorum = heart_rate_quorum(&low, &high, 5).unwrap();

    assert_eq!(
        quorum,
        HeartRateQuorum::LowConfidence {
            prefer: QuorumSide::B,
            beats_per_minute: 94,
            delta_bpm: 22,
            confidence: 4_000,
        }
    );
    assert!(quorum.confidence() < high.confidence());
}

#[test]
fn agreeing_hr_averages_value_at_lower_confidence() {
    let a = heart_rate(1, 72, 9_500);
    let b = heart_rate(2, 76, 9_100);

    assert_eq!(
        heart_rate_quorum(&a, &b, 5).unwrap(),
        HeartRateQuorum::Agree {
            beats_per_minute: 74,
            confidence: 9_100,
        }
    );
}

#[test]
fn quorum_rejects_non_heart_rate_samples() {
    let heart = WornEvent::heart_rate(1, 72).unwrap();
    let battery = WornEvent::battery(2, 89, false).unwrap();

    let error = heart_rate_quorum(&heart, &battery, 5).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("sample b must use sensor heart-rate")
    );
}

fn heart_rate(seq: u64, beats_per_minute: u16, confidence: u16) -> WornEvent {
    WornEvent::new(
        seq,
        WornSensor::HeartRate,
        confidence,
        build::map(vec![
            (
                "kind",
                Expr::Symbol(sim_kernel::Symbol::qualified(
                    "stream/worn-payload",
                    "heart-rate",
                )),
            ),
            ("beats-per-minute", build::uint(u64::from(beats_per_minute))),
        ]),
    )
    .unwrap()
}

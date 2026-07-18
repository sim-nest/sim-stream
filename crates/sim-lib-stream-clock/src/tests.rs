use sim_kernel::{Ref, Symbol};
use sim_lib_stream_core::{ClockDomain, clock_index_symbol, tick_clock_index};

use crate::{
    Clock, ClockChart, ClockIndex, Instant, StreamClockDescriptor, TempoMap, TempoSegment,
};

use sim_kernel::testing::bare_cx as cx;

#[test]
fn frame_indexes_round_trip_for_exact_second_multiples() {
    let clock = Clock::frame(Symbol::qualified("clock", "sample"), 48_000).unwrap();

    let conversion = clock.index_for_instant(Instant::seconds(2)).unwrap();

    assert!(conversion.is_exact());
    assert_eq!(clock.domain(), ClockDomain::Sample);
    assert_eq!(conversion.index().value(), 96_000);
    assert_eq!(
        clock.instant_for_index(conversion.index()).unwrap(),
        Instant::seconds(2)
    );
}

#[test]
fn non_multiple_conversion_emits_diagnostic() {
    let clock = Clock::frame(Symbol::qualified("clock", "sample"), 48_000).unwrap();
    let instant = Instant::new(1, 96_000).unwrap();

    let conversion = clock.index_for_instant(instant).unwrap();

    assert_eq!(conversion.index().value(), 0);
    assert_eq!(conversion.diagnostics().len(), 1);
    assert!(
        conversion.diagnostics()[0]
            .message
            .contains("not an exact frame")
    );
}

#[test]
fn negative_denominator_does_not_bypass_non_negative_instants() {
    let err = Instant::new(1, -2).unwrap_err();

    assert!(format!("{err}").contains("non-negative"));
}

#[test]
fn single_tempo_midi_ticks_match_hand_computation() {
    let tempo_map = TempoMap::single(500_000).unwrap();
    let clock = Clock::midi(Symbol::qualified("clock", "midi"), 480, tempo_map).unwrap();

    let conversion = clock.index_for_instant(Instant::seconds(1)).unwrap();

    assert!(conversion.is_exact());
    assert_eq!(conversion.index().value(), 960);
    assert_eq!(
        clock.instant_for_index(conversion.index()).unwrap(),
        Instant::seconds(1)
    );
}

#[test]
fn tempo_map_requires_segment_at_tick_zero() {
    let err = TempoMap::new(vec![TempoSegment::new(10, 500_000).unwrap()]).unwrap_err();

    assert!(format!("{err}").contains("tick 0"));
}

#[test]
fn increasing_instants_produce_non_decreasing_indexes() {
    let clock = Clock::frame(Symbol::qualified("clock", "sample"), 44_100).unwrap();
    let instants = [
        Instant::new(0, 1).unwrap(),
        Instant::new(1, 88_200).unwrap(),
        Instant::new(1, 44_100).unwrap(),
        Instant::new(1, 1_000).unwrap(),
        Instant::seconds(1),
    ];
    let indexes = instants
        .into_iter()
        .map(|instant| clock.index_for_instant(instant).unwrap().index().value())
        .collect::<Vec<_>>();

    assert!(indexes.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn clock_index_mints_semantic_tick_ref() {
    let mut cx = cx();
    let clock = Clock::frame(Symbol::qualified("clock", "sample"), 48_000).unwrap();

    let tick = clock
        .tick_for_index(&mut cx, ClockIndex::new(24_000))
        .unwrap();

    assert_eq!(tick.clock, Symbol::qualified("clock", "sample"));
    assert_eq!(tick.index, Ref::Symbol(clock_index_symbol(24_000)));
    assert_eq!(
        tick_clock_index(&tick, &Symbol::qualified("clock", "sample"))
            .unwrap()
            .unwrap()
            .value(),
        24_000
    );
}

#[test]
fn citizen_clock_descriptors_round_trip_and_fail_closed() {
    let frame = StreamClockDescriptor::frame(Symbol::qualified("clock", "sample"), 48_000)
        .expect("frame descriptor");
    let rebuilt = frame.clock().expect("frame clock");
    assert_eq!(rebuilt.id(), &Symbol::qualified("clock", "sample"));
    assert_eq!(rebuilt.domain(), ClockDomain::Sample);
    assert_eq!(
        rebuilt.chart(),
        &ClockChart::Frames {
            frames_per_second: 48_000
        }
    );

    let midi = StreamClockDescriptor::midi(
        Symbol::qualified("clock", "midi"),
        480,
        TempoMap::single(500_000).unwrap(),
    )
    .expect("midi descriptor");
    let rebuilt = midi.clock().expect("midi clock");
    assert_eq!(rebuilt.id(), &Symbol::qualified("clock", "midi"));
    assert_eq!(rebuilt.domain(), ClockDomain::MidiTick);
    assert!(matches!(rebuilt.chart(), ClockChart::Midi { tpq: 480, .. }));

    let err = StreamClockDescriptor::frame(Symbol::qualified("clock", "bad"), 0).unwrap_err();
    assert!(format!("{err}").contains("non-zero"));
}

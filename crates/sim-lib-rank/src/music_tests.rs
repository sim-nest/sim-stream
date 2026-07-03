use sim_kernel::Symbol;

use crate::{
    Nat, RankCodec, RankExactOrder, RankLimits, RankNode, RankSearchScore, beam_search,
    music::{
        RankArticulation, RankDuration, RankMelodyCodec, RankMelodyNeighborhood, RankMusicSpec,
        RankProgressionCodec, RankRhythmCodec, rank_melody_low_motion_order,
        rank_music_order_explanation, rank_note_node, rank_progression_cadence_first_order,
        rank_progression_low_motion_order, rank_rhythm_duration_simple_order,
    },
};

fn spec(fixed_total: Option<RankDuration>) -> RankMusicSpec {
    RankMusicSpec::new(2, 1, 3, fixed_total, 2, 2).unwrap()
}

fn quarter_spec() -> RankMusicSpec {
    RankMusicSpec::new(4, 1, 3, None, 2, 2).unwrap()
}

#[test]
fn fixed_total_rhythms_round_trip() {
    let codec = RankRhythmCodec::new(spec(Some(RankDuration::new(1, 1).unwrap()))).unwrap();
    let order = rank_rhythm_duration_simple_order(&codec).unwrap();

    for index in 0..nat_to_usize(&codec.count().unwrap()) {
        let ordinal = Nat::from(index);
        let node = codec.unrank_node(&ordinal).unwrap();
        assert_eq!(codec.rank_node(&node).unwrap(), ordinal);
        assert_fixed_total(&node, RankDuration::new(1, 1).unwrap());
    }

    let first = order.unrank_node(&codec, &Nat::zero()).unwrap();
    assert!(
        rank_music_order_explanation(order.id())
            .unwrap()
            .contains("rational durations")
    );
    assert_fixed_total(&first, RankDuration::new(1, 1).unwrap());
}

#[test]
fn bounded_melodies_round_trip() {
    let codec = RankMelodyCodec::new(quarter_spec()).unwrap();

    for index in 0_usize..24 {
        let ordinal = Nat::from(index);
        let node = codec.unrank_node(&ordinal).unwrap();
        assert_eq!(codec.rank_node(&node).unwrap(), ordinal);
    }

    let low_motion = rank_melody_low_motion_order(&codec).unwrap();
    assert!(
        rank_music_order_explanation(low_motion.id())
            .unwrap()
            .contains("pitch motion")
    );
}

#[test]
fn progressions_enumerate_under_cadence_first_and_low_motion() {
    let codec = RankProgressionCodec::new(quarter_spec()).unwrap();
    let cadence = rank_progression_cadence_first_order(&codec).unwrap();
    let low_motion = rank_progression_low_motion_order(&codec).unwrap();

    for index in 0..nat_to_usize(&codec.count().unwrap()) {
        let ordinal = Nat::from(index);
        let node = codec.unrank_node(&ordinal).unwrap();
        assert_eq!(codec.rank_node(&node).unwrap(), ordinal);
    }

    assert_ne!(
        cadence.canonical_ordinals(),
        low_motion.canonical_ordinals()
    );
    let cadence_first = cadence.unrank_node(&codec, &Nat::zero()).unwrap();
    let low_motion_first = low_motion.unrank_node(&codec, &Nat::zero()).unwrap();

    assert!(ends_with_dominant_tonic(&cadence_first));
    assert!(root_motion(&low_motion_first) <= root_motion(&cadence_first));
    assert!(
        rank_music_order_explanation(cadence.id())
            .unwrap()
            .contains("cadence")
    );
    assert!(
        rank_music_order_explanation(low_motion.id())
            .unwrap()
            .contains("root motion")
    );
}

#[test]
fn beam_search_produces_melodic_variation_near_seed() {
    let spec = quarter_spec();
    let codec = RankMelodyCodec::new(spec.clone()).unwrap();
    let neighborhood = RankMelodyNeighborhood::new(spec);
    let duration = RankDuration::new(1, 4).unwrap();
    let start = RankNode::List(vec![
        rank_note_node(0, duration, RankArticulation::Normal).unwrap(),
    ]);
    let target = RankNode::List(vec![
        rank_note_node(2, duration, RankArticulation::Normal).unwrap(),
    ]);
    let start = codec.rank_node(&start).unwrap();
    let target = codec.rank_node(&target).unwrap();

    let result = beam_search(
        &neighborhood,
        &codec,
        &start,
        4,
        3,
        &mut RankLimits::new(500, 64),
        |ordinal, node| target_score(&target, ordinal, node),
    )
    .unwrap();

    assert_eq!(result.best.ordinal, target);
    assert!(result.best.score > -1);
}

fn target_score(
    target: &Nat,
    ordinal: &Nat,
    node: &RankNode,
) -> crate::RankResult<RankSearchScore> {
    if ordinal == target {
        return Ok(100);
    }
    Ok(-(first_pitch(node).unwrap_or(99).abs_diff(2) as RankSearchScore))
}

fn assert_fixed_total(node: &RankNode, expected: RankDuration) {
    let RankNode::List(items) = node else {
        panic!("rhythm must be a list");
    };
    let total = items
        .iter()
        .map(duration_from_node)
        .reduce(|left, right| {
            RankDuration::new(
                left.numerator * right.denominator + right.numerator * left.denominator,
                left.denominator * right.denominator,
            )
            .unwrap()
        })
        .unwrap();
    assert_eq!(total, expected);
}

fn duration_from_node(node: &RankNode) -> RankDuration {
    let RankNode::Product(values) = node else {
        panic!("duration must be product");
    };
    let [RankNode::Nat(numerator), RankNode::Nat(denominator)] = values.as_slice() else {
        panic!("duration must contain numerator and denominator");
    };
    RankDuration::new(nat_to_u64(numerator), nat_to_u64(denominator)).unwrap()
}

fn ends_with_dominant_tonic(node: &RankNode) -> bool {
    progression_roots(node).ends_with(&[7, 0])
}

fn root_motion(node: &RankNode) -> u64 {
    progression_roots(node)
        .windows(2)
        .map(|window| {
            let distance = window[0].abs_diff(window[1]);
            u64::from(distance.min(12 - distance))
        })
        .sum()
}

fn progression_roots(node: &RankNode) -> Vec<u8> {
    let RankNode::List(chords) = node else {
        return Vec::new();
    };
    chords
        .iter()
        .filter_map(|chord| match chord {
            RankNode::Set(pitches) => pitches.first().and_then(pitch_from_node),
            _ => None,
        })
        .collect()
}

fn first_pitch(node: &RankNode) -> Option<u8> {
    let RankNode::List(items) = node else {
        return None;
    };
    items.iter().find_map(|item| match item {
        RankNode::Sum { tag: 1, value } => {
            let RankNode::Product(values) = value.as_ref() else {
                return None;
            };
            values.first().and_then(pitch_from_node)
        }
        _ => None,
    })
}

fn pitch_from_node(node: &RankNode) -> Option<u8> {
    let RankNode::Enum { id, index } = node else {
        return None;
    };
    if id != &Symbol::qualified("rank-music", "pitch-class") {
        return None;
    }
    Some(nat_to_u64(index) as u8)
}

fn nat_to_usize(value: &Nat) -> usize {
    value.to_decimal_string().parse().unwrap()
}

fn nat_to_u64(value: &Nat) -> u64 {
    value.to_decimal_string().parse().unwrap()
}

#[test]
fn exact_order_addresses_are_unchanged() {
    let codec = RankProgressionCodec::new(quarter_spec()).unwrap();
    let order = rank_progression_cadence_first_order(&codec).unwrap();
    assert_order_preserves_addresses(&codec, &order);
}

fn assert_order_preserves_addresses(codec: &dyn RankCodec, order: &RankExactOrder) {
    for position in 0..nat_to_usize(&order.count()).min(8) {
        let node = order.unrank_node(codec, &Nat::from(position)).unwrap();
        let canonical = codec.rank_node(&node).unwrap();
        assert_eq!(order.position_of(&canonical).unwrap(), Nat::from(position));
    }
}

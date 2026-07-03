use std::cmp::Ordering;
use std::collections::BTreeSet;

use sim_kernel::Symbol;

use crate::{Nat, RankCodec, RankError, RankNode, RankResult, limits::RankLimits};

use super::{
    NOTE_TAG, REST_TAG, RankArticulation, RankDuration, RankMusicSpec, rank_duration_node,
    rank_note_node, rank_pitch_class_node, rank_rest_node,
};

const MAX_MUSIC_NODES: usize = 50_000;

pub(super) fn generate_rhythms(spec: &RankMusicSpec) -> RankResult<Vec<RankNode>> {
    let durations = duration_values(spec)?;
    let mut rhythms = Vec::new();
    for len in 1..=spec.max_rhythm_len {
        generate_duration_lists(
            &durations,
            len,
            &mut Vec::new(),
            &mut rhythms,
            spec.fixed_total,
        )?;
    }
    ensure_bounded(rhythms.len())?;
    Ok(rhythms)
}

pub(super) fn generate_melodies(spec: &RankMusicSpec) -> RankResult<Vec<RankNode>> {
    let atoms = melody_atoms(spec)?;
    let mut melodies = Vec::new();
    for len in 1..=spec.max_melody_len {
        generate_node_lists(&atoms, len, &mut Vec::new(), &mut melodies)?;
    }
    ensure_bounded(melodies.len())?;
    Ok(melodies)
}

pub(super) fn generate_progressions(spec: &RankMusicSpec) -> RankResult<Vec<RankNode>> {
    let chords = progression_chords()?;
    let mut progressions = Vec::new();
    for len in 1..=spec.max_progression_len {
        generate_node_lists(&chords, len, &mut Vec::new(), &mut progressions)?;
    }
    ensure_bounded(progressions.len())?;
    Ok(progressions)
}

pub(super) fn valid_ordinals(
    codec: &dyn RankCodec,
    origin: &Nat,
    candidates: Vec<RankNode>,
    limits: &mut RankLimits,
) -> RankResult<Vec<Nat>> {
    let mut ordinals = Vec::new();
    for candidate in candidates {
        limits.consume(1, "rank.music.neighbor.candidate")?;
        let Ok(rank) = codec.rank_node(&candidate) else {
            continue;
        };
        if &rank != origin && codec.r_ok(&rank) && !ordinals.contains(&rank) {
            ordinals.push(rank);
        }
    }
    ordinals.sort();
    Ok(ordinals)
}

pub(super) fn melody_neighbor_nodes(
    spec: &RankMusicSpec,
    node: &RankNode,
) -> RankResult<Vec<RankNode>> {
    let RankNode::List(items) = node else {
        return Err(RankError::NodeGrammarMismatch {
            expected: "list",
            found: node.kind_name(),
        });
    };
    let durations = duration_values(spec)?;
    let mut out = Vec::new();
    for (index, item) in items.iter().enumerate() {
        for edited in edit_melody_atom(item, &durations)? {
            let mut next = items.clone();
            next[index] = edited;
            out.push(RankNode::List(next));
        }
    }
    if items.len() < spec.max_melody_len {
        let mut next = items.clone();
        next.push(rank_rest_node(durations[0]));
        out.push(RankNode::List(next));
    }
    if items.len() > 1 {
        let mut next = items.clone();
        next.pop();
        out.push(RankNode::List(next));
    }
    Ok(out)
}

pub(super) fn melody_crossover_nodes(
    left: &RankNode,
    right: &RankNode,
    seed: u64,
) -> RankResult<Vec<RankNode>> {
    let (RankNode::List(left), RankNode::List(right)) = (left, right) else {
        return Ok(Vec::new());
    };
    if left.is_empty() || right.is_empty() {
        return Ok(Vec::new());
    }
    let left_cut = (seed as usize) % left.len();
    let right_cut = (seed as usize) % right.len();
    let mut items = left[..=left_cut].to_vec();
    items.extend_from_slice(&right[right_cut..]);
    Ok(vec![RankNode::List(items)])
}

pub(super) fn rhythm_simplicity(node: &RankNode) -> (usize, u64, u64) {
    let RankNode::List(items) = node else {
        return (usize::MAX, u64::MAX, u64::MAX);
    };
    let durations = items
        .iter()
        .filter_map(|item| duration_from_node(item).ok())
        .collect::<Vec<_>>();
    let denominator_sum = durations.iter().map(|duration| duration.denominator).sum();
    let numerator_sum = durations.iter().map(|duration| duration.numerator).sum();
    (items.len(), denominator_sum, numerator_sum)
}

pub(super) fn melody_motion(node: &RankNode) -> u64 {
    adjacent_pitch_motion(melody_pitches(node))
}

pub(super) fn melody_span(node: &RankNode) -> u64 {
    let pitches = melody_pitches(node);
    match (pitches.iter().min(), pitches.iter().max()) {
        (Some(min), Some(max)) => u64::from(max - min),
        _ => 0,
    }
}

pub(super) fn progression_motion(node: &RankNode) -> u64 {
    adjacent_pitch_motion(progression_roots(node))
}

pub(super) fn cadence_score(node: &RankNode) -> u64 {
    let roots = progression_roots(node);
    match roots.as_slice() {
        [.., 7, 0] => 0,
        [.., 0] => 1,
        _ => 2,
    }
}

pub(super) fn melody_node_distance(left: &RankNode, right: &RankNode) -> u64 {
    let left = melody_pitches(left);
    let right = melody_pitches(right);
    let shared = left
        .iter()
        .zip(&right)
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum::<u64>();
    shared + left.len().abs_diff(right.len()) as u64
}

pub(super) fn ordinals(count: usize) -> Vec<Nat> {
    (0..count).map(Nat::from).collect()
}

pub(super) fn pitch_class_symbol() -> Symbol {
    Symbol::qualified("rank-music", "pitch-class")
}

pub(super) fn articulation_symbol() -> Symbol {
    Symbol::qualified("rank-music", "articulation")
}

pub(super) fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    a
}

pub(super) fn invalid_node(message: &'static str) -> RankError {
    RankError::InvalidNode {
        message: message.to_owned(),
    }
}

fn generate_duration_lists(
    durations: &[RankDuration],
    remaining: usize,
    current: &mut Vec<RankDuration>,
    out: &mut Vec<RankNode>,
    fixed_total: Option<RankDuration>,
) -> RankResult<()> {
    if remaining == 0 {
        if fixed_total.is_none_or(|total| duration_sum(current) == total) {
            out.push(RankNode::List(
                current.iter().copied().map(rank_duration_node).collect(),
            ));
        }
        return Ok(());
    }
    for duration in durations {
        current.push(*duration);
        generate_duration_lists(durations, remaining - 1, current, out, fixed_total)?;
        current.pop();
    }
    Ok(())
}

fn generate_node_lists(
    items: &[RankNode],
    remaining: usize,
    current: &mut Vec<RankNode>,
    out: &mut Vec<RankNode>,
) -> RankResult<()> {
    if remaining == 0 {
        out.push(RankNode::List(current.clone()));
        return Ok(());
    }
    for item in items {
        current.push(item.clone());
        generate_node_lists(items, remaining - 1, current, out)?;
        current.pop();
    }
    Ok(())
}

fn melody_atoms(spec: &RankMusicSpec) -> RankResult<Vec<RankNode>> {
    let durations = duration_values(spec)?;
    let mut atoms = Vec::new();
    for duration in &durations {
        atoms.push(rank_rest_node(*duration));
        for pitch in 0..12 {
            for articulation in [
                RankArticulation::Staccato,
                RankArticulation::Normal,
                RankArticulation::Legato,
            ] {
                atoms.push(rank_note_node(pitch, *duration, articulation)?);
            }
        }
    }
    Ok(atoms)
}

fn duration_values(spec: &RankMusicSpec) -> RankResult<Vec<RankDuration>> {
    let mut durations = BTreeSet::new();
    for numerator in 1..=spec.max_duration_numerator {
        for denominator in 1..=spec.max_denominator {
            durations.insert(RankDuration::new(numerator, denominator)?);
        }
    }
    let mut durations = durations.into_iter().collect::<Vec<_>>();
    durations.sort_by(compare_duration);
    Ok(durations)
}

fn progression_chords() -> RankResult<Vec<RankNode>> {
    [[0, 4, 7], [5, 9, 0], [7, 11, 2], [9, 0, 4]]
        .into_iter()
        .map(|pitches| {
            pitches
                .into_iter()
                .map(rank_pitch_class_node)
                .collect::<RankResult<Vec<_>>>()
                .map(RankNode::Set)
        })
        .collect()
}

fn edit_melody_atom(atom: &RankNode, durations: &[RankDuration]) -> RankResult<Vec<RankNode>> {
    match atom {
        RankNode::Sum { tag, value } if *tag == REST_TAG => {
            let duration = duration_from_node(value)?;
            Ok(duration_neighbors(duration, durations)
                .into_iter()
                .map(rank_rest_node)
                .collect())
        }
        RankNode::Sum { tag, value } if *tag == NOTE_TAG => {
            let (pitch, duration, articulation) = note_parts(value)?;
            let mut out = Vec::new();
            out.push(rank_note_node((pitch + 1) % 12, duration, articulation)?);
            out.push(rank_note_node((pitch + 11) % 12, duration, articulation)?);
            for next in duration_neighbors(duration, durations) {
                out.push(rank_note_node(pitch, next, articulation)?);
            }
            let next_articulation = RankArticulation::from_index((articulation.index() + 1) % 3)?;
            out.push(rank_note_node(pitch, duration, next_articulation)?);
            Ok(out)
        }
        _ => Err(invalid_node("melody atom is outside music grammar")),
    }
}

fn duration_neighbors(duration: RankDuration, durations: &[RankDuration]) -> Vec<RankDuration> {
    let Some(index) = durations
        .iter()
        .position(|candidate| candidate == &duration)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if index > 0 {
        out.push(durations[index - 1]);
    }
    if index + 1 < durations.len() {
        out.push(durations[index + 1]);
    }
    out
}

fn melody_pitches(node: &RankNode) -> Vec<u8> {
    let RankNode::List(items) = node else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| match item {
            RankNode::Sum { tag, value } if *tag == NOTE_TAG => note_parts(value).ok(),
            _ => None,
        })
        .map(|(pitch, _, _)| pitch)
        .collect()
}

fn progression_roots(node: &RankNode) -> Vec<u8> {
    let RankNode::List(chords) = node else {
        return Vec::new();
    };
    chords
        .iter()
        .filter_map(|chord| match chord {
            RankNode::Set(pitches) => pitches
                .first()
                .and_then(|pitch| pitch_from_node(pitch).ok()),
            _ => None,
        })
        .collect()
}

fn adjacent_pitch_motion(pitches: Vec<u8>) -> u64 {
    pitches
        .windows(2)
        .map(|window| {
            let distance = window[0].abs_diff(window[1]);
            u64::from(distance.min(12 - distance))
        })
        .sum()
}

fn duration_from_node(node: &RankNode) -> RankResult<RankDuration> {
    let RankNode::Product(values) = node else {
        return Err(RankError::NodeGrammarMismatch {
            expected: "product",
            found: node.kind_name(),
        });
    };
    let [RankNode::Nat(numerator), RankNode::Nat(denominator)] = values.as_slice() else {
        return Err(invalid_node(
            "music duration must be product of two naturals",
        ));
    };
    RankDuration::new(nat_to_u64(numerator)?, nat_to_u64(denominator)?)
}

fn note_parts(node: &RankNode) -> RankResult<(u8, RankDuration, RankArticulation)> {
    let RankNode::Product(values) = node else {
        return Err(invalid_node("music note must carry product"));
    };
    let [pitch, duration, articulation] = values.as_slice() else {
        return Err(invalid_node("music note product must have three fields"));
    };
    let articulation = match articulation {
        RankNode::Enum { id, index } if id == &articulation_symbol() => {
            RankArticulation::from_index(nat_to_u64(index)?)
        }
        _ => Err(invalid_node("music note articulation is invalid")),
    }?;
    Ok((
        pitch_from_node(pitch)?,
        duration_from_node(duration)?,
        articulation,
    ))
}

fn pitch_from_node(node: &RankNode) -> RankResult<u8> {
    let RankNode::Enum { id, index } = node else {
        return Err(RankError::NodeGrammarMismatch {
            expected: "enum",
            found: node.kind_name(),
        });
    };
    if id != &pitch_class_symbol() {
        return Err(invalid_node("music pitch class symbol does not match"));
    }
    let pitch = nat_to_u64(index)?;
    u8::try_from(pitch).map_err(|_| invalid_node("music pitch class does not fit in u8"))
}

fn duration_sum(durations: &[RankDuration]) -> RankDuration {
    durations
        .iter()
        .copied()
        .reduce(|left, right| {
            RankDuration::new(
                left.numerator * right.denominator + right.numerator * left.denominator,
                left.denominator * right.denominator,
            )
            .expect("positive duration sum")
        })
        .unwrap_or(RankDuration {
            numerator: 0,
            denominator: 1,
        })
}

fn compare_duration(left: &RankDuration, right: &RankDuration) -> Ordering {
    (left.numerator * right.denominator)
        .cmp(&(right.numerator * left.denominator))
        .then_with(|| left.denominator.cmp(&right.denominator))
}

fn nat_to_u64(value: &Nat) -> RankResult<u64> {
    value
        .to_decimal_string()
        .parse()
        .map_err(|_| invalid_node("music natural does not fit in u64"))
}

fn ensure_bounded(count: usize) -> RankResult<()> {
    if count > MAX_MUSIC_NODES {
        return Err(invalid_node("music space generation bound exceeded"));
    }
    Ok(())
}

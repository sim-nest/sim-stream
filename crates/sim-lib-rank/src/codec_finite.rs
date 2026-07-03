use num_bigint::BigUint;

use crate::{
    error::{RankError, RankResult},
    nat::Nat,
};

pub(super) fn mixed_rank(ranks: &[Nat], counts: &[Nat]) -> RankResult<Nat> {
    let mut total = Nat::zero();
    let mut multiplier = Nat::one();
    for (rank, count) in ranks.iter().zip(counts) {
        require_below(rank, count)?;
        total = total.checked_add(&rank.checked_mul(&multiplier));
        multiplier = multiplier.checked_mul(count);
    }
    Ok(total)
}

pub(super) fn mixed_unrank(r: &Nat, counts: &[Nat]) -> RankResult<Vec<Nat>> {
    let total = counts
        .iter()
        .fold(Nat::one(), |total, count| total.checked_mul(count));
    require_below(r, &total)?;
    let mut remaining = r.clone();
    let mut ranks = Vec::with_capacity(counts.len());
    for count in counts {
        let (next, rank) = remaining.div_mod(count)?;
        ranks.push(rank);
        remaining = next;
    }
    if !remaining.is_zero() {
        return Err(out_of_range(r, &total));
    }
    Ok(ranks)
}

pub(super) fn list_offset(base: &Nat, min_len: u64, len: u64, max_len: u64) -> RankResult<Nat> {
    if len > max_len {
        return Err(invalid_node("list length is outside grammar bounds"));
    }
    let mut offset = Nat::zero();
    for current_len in min_len..len {
        offset = offset.checked_add(&pow_nat(base, current_len));
    }
    Ok(offset)
}

pub(super) fn find_len_group(
    base: &Nat,
    min_len: u64,
    max_len: u64,
    r: &Nat,
) -> RankResult<(u64, Nat)> {
    let mut remaining = r.clone();
    for len in min_len..=max_len {
        let count = pow_nat(base, len);
        if remaining < count {
            return Ok((len, remaining));
        }
        remaining = remaining.checked_sub(&count)?;
    }
    Err(out_of_range(r, &list_total(base, min_len, max_len)))
}

pub(super) fn set_offset(element_count: u64, max_len: u64, len: u64) -> Nat {
    let mut offset = Nat::zero();
    for current_len in 0..len.min(max_len.saturating_add(1)) {
        offset = offset.checked_add(&binomial_u64(element_count, current_len));
    }
    offset
}

pub(super) fn find_set_len_group(
    element_count: u64,
    max_len: u64,
    r: &Nat,
) -> RankResult<(u64, Nat)> {
    let mut remaining = r.clone();
    for len in 0..=max_len {
        let count = binomial_u64(element_count, len);
        if remaining < count {
            return Ok((len, remaining));
        }
        remaining = remaining.checked_sub(&count)?;
    }
    Err(out_of_range(r, &set_total(element_count, max_len)))
}

pub(super) fn map_offset(key_count: u64, value_count: &Nat, max_len: u64, len: u64) -> Nat {
    let mut offset = Nat::zero();
    for current_len in 0..len.min(max_len.saturating_add(1)) {
        offset = offset.checked_add(
            &binomial_u64(key_count, current_len).checked_mul(&pow_nat(value_count, current_len)),
        );
    }
    offset
}

pub(super) fn find_map_len_group(
    key_count: u64,
    value_count: &Nat,
    max_len: u64,
    r: &Nat,
) -> RankResult<(u64, Nat)> {
    let mut remaining = r.clone();
    for len in 0..=max_len {
        let count = binomial_u64(key_count, len).checked_mul(&pow_nat(value_count, len));
        if remaining < count {
            return Ok((len, remaining));
        }
        remaining = remaining.checked_sub(&count)?;
    }
    Err(out_of_range(r, &map_total(key_count, value_count, max_len)))
}

pub(super) fn combination_rank(element_count: u64, ranks: &[u64]) -> RankResult<Nat> {
    for (index, rank) in ranks.iter().enumerate() {
        if *rank >= element_count {
            return Err(invalid_node(
                "combination element rank is outside element count",
            ));
        }
        if index > 0 && ranks[index - 1] >= *rank {
            return Err(invalid_node(
                "combination element ranks must be unique and sorted",
            ));
        }
    }
    let mut total = Nat::zero();
    for (index, rank) in ranks.iter().enumerate() {
        total = total.checked_add(&binomial_u64(
            *rank,
            u64::try_from(index + 1).map_err(|_| invalid_node("combination length overflow"))?,
        ));
    }
    Ok(total)
}

pub(super) fn combination_unrank(element_count: u64, len: u64, r: &Nat) -> RankResult<Vec<u64>> {
    require_below(r, &binomial_u64(element_count, len))?;
    let mut remaining = r.clone();
    let mut max = element_count;
    let mut result = Vec::with_capacity(
        usize::try_from(len).map_err(|_| invalid_node("combination length overflow"))?,
    );
    for choose in (1..=len).rev() {
        let mut candidate = max
            .checked_sub(1)
            .ok_or_else(|| invalid_node("combination length exceeds element count"))?;
        loop {
            let count = binomial_u64(candidate, choose);
            if count <= remaining {
                result.push(candidate);
                remaining = remaining.checked_sub(&count)?;
                max = candidate;
                break;
            }
            candidate = candidate
                .checked_sub(1)
                .ok_or_else(|| invalid_node("combination unrank exhausted candidates"))?;
        }
    }
    result.reverse();
    Ok(result)
}

pub(super) fn pow_nat(base: &Nat, exp: u64) -> Nat {
    let mut result = Nat::one();
    for _ in 0..exp {
        result = result.checked_mul(base);
    }
    result
}

pub(super) fn binomial_u64(n: u64, k: u64) -> Nat {
    if k > n {
        return Nat::zero();
    }
    let k = k.min(n - k);
    let mut result = BigUint::from(1_u32);
    for i in 1..=k {
        result *= BigUint::from(n - k + i);
        result /= BigUint::from(i);
    }
    Nat::from_biguint(result)
}

fn list_total(base: &Nat, min_len: u64, max_len: u64) -> Nat {
    let mut total = Nat::zero();
    for len in min_len..=max_len {
        total = total.checked_add(&pow_nat(base, len));
    }
    total
}

fn set_total(element_count: u64, max_len: u64) -> Nat {
    let mut total = Nat::zero();
    for len in 0..=max_len {
        total = total.checked_add(&binomial_u64(element_count, len));
    }
    total
}

fn map_total(key_count: u64, value_count: &Nat, max_len: u64) -> Nat {
    let mut total = Nat::zero();
    for len in 0..=max_len {
        total = total
            .checked_add(&binomial_u64(key_count, len).checked_mul(&pow_nat(value_count, len)));
    }
    total
}

fn require_below(value: &Nat, count: &Nat) -> RankResult<()> {
    if value < count {
        Ok(())
    } else {
        Err(out_of_range(value, count))
    }
}

fn out_of_range(ordinal: &Nat, count: &Nat) -> RankError {
    RankError::OrdinalOutOfRange {
        ordinal: ordinal.to_string(),
        count: count.to_string(),
    }
}

fn invalid_node(message: &'static str) -> RankError {
    RankError::InvalidNode {
        message: message.into(),
    }
}

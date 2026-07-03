use num_bigint::{BigInt, BigUint, Sign};

use crate::{RankResult, nat::Nat};

pub(super) fn rank_int(value: &BigInt) -> Nat {
    let magnitude = value.magnitude();
    match value.sign() {
        Sign::Minus => Nat::from_biguint((magnitude * 2_u32) - BigUint::from(1_u32)),
        Sign::NoSign | Sign::Plus => Nat::from_biguint(magnitude * 2_u32),
    }
}

pub(super) fn unrank_int(r: &Nat) -> RankResult<BigInt> {
    let (quotient, remainder) = r.div_mod(&Nat::from(2_u64))?;
    if remainder.is_zero() {
        Ok(BigInt::from_biguint(
            Sign::Plus,
            quotient.as_biguint().clone(),
        ))
    } else {
        Ok(BigInt::from_biguint(
            Sign::Minus,
            quotient.checked_add(&Nat::one()).as_biguint().clone(),
        ))
    }
}

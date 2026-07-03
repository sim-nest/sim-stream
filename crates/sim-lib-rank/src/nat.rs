//! The `Nat` ordinal type and helpers for ranked positions.
//!
//! `Nat` is the unbounded natural-number ordinal that rank/unrank operations
//! map nodes to and from; this module also provides the bigint number domain,
//! ordinal interning into the datum store, coordinate construction, and the
//! `binomial` helper used by combinatorial ranking.

use core::{fmt, str::FromStr};

use num_bigint::BigUint;
use sim_kernel::{
    ContentId, Coordinate, Cx, Datum, DatumStore, NumberLiteral, Ref, Result as KernelResult,
    Symbol,
};

use crate::{
    error::{RankError, RankResult},
    limits::RankLimits,
};

/// Unbounded natural-number ordinal indexing a position in a rank space.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Nat(BigUint);

impl Nat {
    /// Returns the ordinal zero.
    pub fn zero() -> Self {
        Self(BigUint::from(0_u8))
    }

    /// Returns the ordinal one.
    pub fn one() -> Self {
        Self(BigUint::from(1_u8))
    }

    /// Wraps a `BigUint` as an ordinal.
    pub fn from_biguint(value: BigUint) -> Self {
        Self(value)
    }

    /// Borrows the underlying `BigUint`.
    pub fn as_biguint(&self) -> &BigUint {
        &self.0
    }

    /// Reports whether this ordinal is zero.
    pub fn is_zero(&self) -> bool {
        self.0 == BigUint::from(0_u8)
    }

    /// Returns the number of bits needed to represent this ordinal.
    pub fn bits(&self) -> u64 {
        self.0.bits()
    }

    /// Renders this ordinal as a base-10 string.
    pub fn to_decimal_string(&self) -> String {
        self.0.to_string()
    }

    /// Converts this ordinal to a bigint-domain number literal.
    pub fn to_number_literal(&self) -> NumberLiteral {
        NumberLiteral {
            domain: bigint_number_domain(),
            canonical: self.to_decimal_string(),
        }
    }

    /// Parses an ordinal from a bigint-domain number literal.
    ///
    /// Fails if the literal's domain is not the bigint domain.
    pub fn from_number_literal(value: &NumberLiteral) -> RankResult<Self> {
        let expected = bigint_number_domain();
        if value.domain != expected {
            return Err(RankError::InvalidNumberDomain {
                expected,
                found: value.domain.clone(),
            });
        }
        value.canonical.parse()
    }

    /// Returns the sum of two ordinals.
    pub fn checked_add(&self, rhs: &Self) -> Self {
        Self(&self.0 + &rhs.0)
    }

    /// Returns the difference, erroring if it would be negative.
    pub fn checked_sub(&self, rhs: &Self) -> RankResult<Self> {
        if self.0 < rhs.0 {
            return Err(RankError::NegativeOrdinal {
                value: format!("{} - {}", self, rhs),
            });
        }
        Ok(Self(&self.0 - &rhs.0))
    }

    /// Returns the product of two ordinals.
    pub fn checked_mul(&self, rhs: &Self) -> Self {
        Self(&self.0 * &rhs.0)
    }

    /// Returns the quotient and remainder, erroring on divide by zero.
    pub fn div_mod(&self, rhs: &Self) -> RankResult<(Self, Self)> {
        if rhs.is_zero() {
            return Err(RankError::DivideByZero);
        }
        Ok((Self(&self.0 / &rhs.0), Self(&self.0 % &rhs.0)))
    }

    /// Raises this ordinal to `exp`, charging fuel and checking bit limits.
    pub fn pow_u32(&self, exp: u32, limits: &mut RankLimits) -> RankResult<Self> {
        limits.consume(u64::from(exp), "rank.pow")?;
        let value = Self(self.0.pow(exp));
        limits.check_nat(&value, "rank.pow")?;
        Ok(value)
    }
}

impl fmt::Debug for Nat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Nat({})", self.0)
    }
}

impl fmt::Display for Nat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for Nat {
    fn from(value: u64) -> Self {
        Self(BigUint::from(value))
    }
}

impl From<usize> for Nat {
    fn from(value: usize) -> Self {
        Self(BigUint::from(value))
    }
}

impl TryFrom<i64> for Nat {
    type Error = RankError;

    fn try_from(value: i64) -> RankResult<Self> {
        let unsigned = u64::try_from(value).map_err(|_| RankError::NegativeOrdinal {
            value: value.to_string(),
        })?;
        Ok(Self::from(unsigned))
    }
}

impl FromStr for Nat {
    type Err = RankError;

    fn from_str(value: &str) -> RankResult<Self> {
        if value.starts_with('-') {
            return Err(RankError::NegativeOrdinal {
                value: value.to_owned(),
            });
        }
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(RankError::InvalidDecimal {
                input: value.to_owned(),
            });
        }
        let parsed = BigUint::parse_bytes(value.as_bytes(), 10).ok_or_else(|| {
            RankError::InvalidDecimal {
                input: value.to_owned(),
            }
        })?;
        Ok(Self(parsed))
    }
}

/// Returns the `numbers/bigint` domain symbol used for ordinals.
pub fn bigint_number_domain() -> Symbol {
    Symbol::qualified("numbers", "bigint")
}

/// Builds the datum encoding an ordinal as a bigint number.
pub fn ordinal_datum(value: &Nat) -> Datum {
    Datum::Number(value.to_number_literal())
}

/// Returns the content id of an ordinal's datum encoding.
pub fn ordinal_content_id(value: &Nat) -> KernelResult<ContentId> {
    ordinal_datum(value).content_id()
}

/// Interns an ordinal's datum into the store and returns its content id.
pub fn intern_ordinal(cx: &mut Cx, value: &Nat) -> KernelResult<ContentId> {
    cx.datum_store_mut().intern(ordinal_datum(value))
}

/// Builds a coordinate reference into `space` at ordinal `value`.
pub fn coordinate_for_nat(cx: &mut Cx, space: Symbol, value: &Nat) -> KernelResult<Ref> {
    Ok(Ref::Coord(Coordinate {
        space,
        ordinal: intern_ordinal(cx, value)?,
    }))
}

/// Computes the binomial coefficient `n choose k`, charging fuel per step.
///
/// Returns zero when `k` exceeds `n`; used by combinatorial ranking.
pub fn binomial(n: &Nat, k: &Nat, limits: &mut RankLimits) -> RankResult<Nat> {
    if k.0 > n.0 {
        return Ok(Nat::zero());
    }

    let n_minus_k = Nat(&n.0 - &k.0);
    let k = if n_minus_k.0 < k.0 {
        n_minus_k.0
    } else {
        k.0.clone()
    };
    let one = BigUint::from(1_u8);
    let mut i = one.clone();
    let start = &n.0 - &k;
    let mut result = BigUint::from(1_u8);

    while i <= k {
        limits.consume(1, "rank.binomial")?;
        result *= &start + &i;
        result /= &i;
        let current = Nat(result.clone());
        limits.check_nat(&current, "rank.binomial")?;
        i += &one;
    }

    Ok(Nat(result))
}

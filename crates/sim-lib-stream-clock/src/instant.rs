use sim_kernel::{Error, Result};

/// Exact, non-negative point in time held as a reduced rational number of
/// seconds.
///
/// Clock conversions need a time base that is free of floating-point rounding,
/// so an `Instant` stores a numerator and denominator (both in seconds) and
/// keeps them in lowest terms with a positive denominator. Stream instants are
/// always non-negative; constructing a negative value fails closed.
///
/// # Examples
///
/// ```
/// use sim_lib_stream_clock::Instant;
///
/// let half = Instant::new(1, 2)?;
/// let one = Instant::seconds(1);
/// let sum = one.checked_add(half)?;
/// assert_eq!(sum.numerator(), 3);
/// assert_eq!(sum.denominator(), 2);
/// # Ok::<(), sim_kernel::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant {
    numerator: i128,
    denominator: i128,
}

impl Instant {
    /// Builds an instant of `numerator / denominator` seconds, reduced to
    /// lowest terms with a positive denominator.
    ///
    /// Returns an error when `denominator` is zero, when the reduced value
    /// would be negative, or when normalizing the sign overflows `i128`.
    pub fn new(numerator: i128, denominator: i128) -> Result<Self> {
        if denominator == 0 {
            return Err(Error::Eval(
                "instant denominator must be non-zero".to_owned(),
            ));
        }
        let mut numerator = numerator;
        let mut denominator = denominator;
        if denominator < 0 {
            numerator = numerator
                .checked_neg()
                .ok_or_else(|| Error::Eval("instant numerator overflowed".to_owned()))?;
            denominator = denominator
                .checked_neg()
                .ok_or_else(|| Error::Eval("instant denominator overflowed".to_owned()))?;
        }
        if numerator < 0 {
            return Err(Error::Eval(
                "stream instants must be non-negative".to_owned(),
            ));
        }
        let divisor = gcd(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    /// Returns the instant for a whole number of `seconds`.
    pub fn seconds(seconds: u64) -> Self {
        Self {
            numerator: i128::from(seconds),
            denominator: 1,
        }
    }

    /// Returns the numerator of the reduced seconds fraction.
    pub fn numerator(self) -> i128 {
        self.numerator
    }

    /// Returns the (always positive) denominator of the reduced seconds
    /// fraction.
    pub fn denominator(self) -> i128 {
        self.denominator
    }

    /// Returns the sum of `self` and `other`, reduced to lowest terms.
    ///
    /// Returns an error when the cross-multiplied addition overflows `i128`.
    pub fn checked_add(self, other: Self) -> Result<Self> {
        let numerator = self
            .numerator
            .checked_mul(other.denominator)
            .and_then(|left| {
                other
                    .numerator
                    .checked_mul(self.denominator)
                    .and_then(|right| left.checked_add(right))
            })
            .ok_or_else(|| Error::Eval("instant addition overflowed".to_owned()))?;
        let denominator = self
            .denominator
            .checked_mul(other.denominator)
            .ok_or_else(|| Error::Eval("instant denominator overflowed".to_owned()))?;
        Self::new(numerator, denominator)
    }

    /// Returns the difference `self - other`, reduced to lowest terms.
    ///
    /// Returns an error when the subtraction overflows `i128` or when the
    /// result would be negative (stream instants are non-negative).
    pub fn checked_sub(self, other: Self) -> Result<Self> {
        let numerator = self
            .numerator
            .checked_mul(other.denominator)
            .and_then(|left| {
                other
                    .numerator
                    .checked_mul(self.denominator)
                    .and_then(|right| left.checked_sub(right))
            })
            .ok_or_else(|| Error::Eval("instant subtraction overflowed".to_owned()))?;
        let denominator = self
            .denominator
            .checked_mul(other.denominator)
            .ok_or_else(|| Error::Eval("instant denominator overflowed".to_owned()))?;
        Self::new(numerator, denominator)
    }
}

fn gcd(mut left: i128, mut right: i128) -> i128 {
    left = left.abs();
    right = right.abs();
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}

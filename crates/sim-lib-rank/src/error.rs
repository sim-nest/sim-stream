//! Error types for the rank/unrank codec surface.
//!
//! Defines [`RankError`], the failure taxonomy raised while ranking nodes to
//! natural ordinals and unranking ordinals back to nodes, plus the
//! [`RankResult`] alias used throughout the crate.

use core::fmt;

use sim_kernel::{Error as KernelError, Symbol};

/// Result alias for fallible rank/unrank operations, carrying [`RankError`].
pub type RankResult<T> = core::result::Result<T, RankError>;

/// Failure raised while ranking nodes to ordinals or unranking ordinals to nodes.
///
/// Each variant names a distinct point where a value, grammar, ordinal, or
/// resource limit violates the codec contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RankError {
    /// An ordinal parsed or computed as a negative value, which the natural
    /// ordinal space cannot represent.
    NegativeOrdinal {
        /// Textual form of the offending value.
        value: String,
    },
    /// A decimal string could not be parsed as a natural ordinal.
    InvalidDecimal {
        /// The input text that failed to parse.
        input: String,
    },
    /// An ordinal value carried a number domain other than the one expected.
    InvalidNumberDomain {
        /// Number-domain symbol the codec required.
        expected: Symbol,
        /// Number-domain symbol actually found on the value.
        found: Symbol,
    },
    /// A natural-number division requested a zero divisor.
    DivideByZero,
    /// A grammar of the named kind has no inhabitants and cannot be ranked.
    EmptyGrammar {
        /// Grammar kind label (e.g. enum, sum).
        kind: &'static str,
        /// Identifier of the empty grammar.
        id: Symbol,
    },
    /// A grammar declared the same symbol more than once.
    DuplicateGrammarSymbol {
        /// Grammar kind label.
        kind: &'static str,
        /// Identifier of the offending grammar.
        id: Symbol,
        /// The symbol that appeared more than once.
        symbol: Symbol,
    },
    /// A collection grammar declared a minimum length greater than its maximum.
    InvalidLengthBounds {
        /// Identifier of the collection grammar.
        id: Symbol,
        /// Declared minimum length.
        min_len: u64,
        /// Declared maximum length.
        max_len: u64,
    },
    /// A field path referenced a child coordinate space that is not registered.
    MissingChildSpace {
        /// Field path that required the missing space.
        path: String,
        /// Identifier of the missing child space.
        id: Symbol,
    },
    /// A recursive grammar can recurse without increasing grade, making its
    /// graded counts non-terminating.
    UnproductiveRecursion {
        /// Identifier of the unproductive recursive grammar.
        id: Symbol,
    },
    /// A recursive reference was used without a resolved target grammar.
    UnresolvedRecursiveRef {
        /// Identifier of the unresolved recursive reference.
        id: Symbol,
    },
    /// A grade value exceeded the `u64` range used for grade arithmetic.
    GradeOverflow {
        /// Textual form of the overflowing grade.
        value: String,
    },
    /// A node's shape did not match the grammar branch ranking it.
    NodeGrammarMismatch {
        /// Grammar kind the codec expected.
        expected: &'static str,
        /// Node kind actually supplied.
        found: &'static str,
    },
    /// A node was structurally invalid for ranking, with a contextual message.
    InvalidNode {
        /// Human-readable description of the invalidity.
        message: String,
    },
    /// Grade-based ranking is not defined for the named grammar kind.
    UnsupportedGrade {
        /// Grammar kind that lacks grade support.
        kind: &'static str,
    },
    /// Ranking or counting is not defined for the named grammar kind (for
    /// example an unbounded or recursive construct in a finite codec).
    UnsupportedCodec {
        /// Grammar kind that lacks codec support.
        kind: &'static str,
    },
    /// A coordinate space identifier was registered more than once.
    DuplicateSpace {
        /// Identifier of the duplicated space.
        id: Symbol,
    },
    /// A coordinate space identifier was referenced but never registered.
    UnknownSpace {
        /// Identifier of the unknown space.
        id: Symbol,
    },
    /// An ordinal fell outside the inhabited range of its codec.
    OrdinalOutOfRange {
        /// The out-of-range ordinal.
        ordinal: String,
        /// The codec's inhabitant count.
        count: String,
    },
    /// A counted resource (fuel) limit was exhausted during ranking.
    LimitExceeded {
        /// Name of the limit that was hit.
        limit: &'static str,
        /// Amount of fuel that was required.
        needed: u64,
        /// Amount of fuel that remained.
        remaining: u64,
    },
    /// A natural ordinal exceeded the configured bit-width limit.
    BitLimitExceeded {
        /// Name of the bit limit that was hit.
        limit: &'static str,
        /// Bit length of the offending natural.
        bits: u64,
        /// Maximum bit length permitted.
        max_bits: u64,
    },
    /// A codec version string could not be parsed.
    InvalidVersion {
        /// The input text that failed to parse.
        input: String,
    },
    /// An order does not expose exact ordinal positions for its elements.
    PositionUnavailable {
        /// Identifier of the order lacking positions.
        id: Symbol,
    },
}

impl fmt::Display for RankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NegativeOrdinal { value } => {
                write!(f, "rank ordinal must be non-negative, found {value}")
            }
            Self::InvalidDecimal { input } => {
                write!(f, "invalid rank natural decimal {input:?}")
            }
            Self::InvalidNumberDomain { expected, found } => {
                write!(
                    f,
                    "invalid rank ordinal number domain: expected {expected}, found {found}"
                )
            }
            Self::DivideByZero => f.write_str("rank natural division by zero"),
            Self::EmptyGrammar { kind, id } => {
                write!(f, "rank {kind} grammar {id} must not be empty")
            }
            Self::DuplicateGrammarSymbol { kind, id, symbol } => {
                write!(
                    f,
                    "rank {kind} grammar {id} contains duplicate symbol {symbol}"
                )
            }
            Self::InvalidLengthBounds {
                id,
                min_len,
                max_len,
            } => write!(
                f,
                "rank collection grammar {id} has invalid length bounds {min_len}..{max_len}"
            ),
            Self::MissingChildSpace { path, id } => {
                write!(
                    f,
                    "rank field path {path} requires missing child space {id}"
                )
            }
            Self::UnproductiveRecursion { id } => {
                write!(
                    f,
                    "rank recursive grammar {id} can recurse without grade cost"
                )
            }
            Self::UnresolvedRecursiveRef { id } => {
                write!(f, "rank recursive reference {id} is not resolved")
            }
            Self::GradeOverflow { value } => {
                write!(f, "rank grade value {value} does not fit in u64")
            }
            Self::NodeGrammarMismatch { expected, found } => {
                write!(
                    f,
                    "rank node does not match grammar: expected {expected}, found {found}"
                )
            }
            Self::InvalidNode { message } => write!(f, "invalid rank node: {message}"),
            Self::UnsupportedGrade { kind } => {
                write!(f, "rank grade operation is unsupported for {kind}")
            }
            Self::UnsupportedCodec { kind } => {
                write!(f, "rank codec operation is unsupported for {kind}")
            }
            Self::DuplicateSpace { id } => {
                write!(f, "rank space {id} is already registered")
            }
            Self::UnknownSpace { id } => {
                write!(f, "rank space {id} is not registered")
            }
            Self::OrdinalOutOfRange { ordinal, count } => {
                write!(f, "rank ordinal {ordinal} is outside codec count {count}")
            }
            Self::LimitExceeded {
                limit,
                needed,
                remaining,
            } => write!(
                f,
                "rank limit {limit} exceeded: needed {needed} fuel with {remaining} remaining"
            ),
            Self::BitLimitExceeded {
                limit,
                bits,
                max_bits,
            } => write!(
                f,
                "rank bit limit {limit} exceeded: natural has {bits} bits, max is {max_bits}"
            ),
            Self::InvalidVersion { input } => {
                write!(f, "invalid rank version {input:?}")
            }
            Self::PositionUnavailable { id } => {
                write!(f, "rank order {id} does not provide exact positions")
            }
        }
    }
}

impl std::error::Error for RankError {}

impl From<RankError> for KernelError {
    fn from(value: RankError) -> Self {
        Self::Eval(value.to_string())
    }
}

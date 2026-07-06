//! Buffer policy values and small expr field-extraction helpers.
//!
//! This module supplies the stream fabric's buffering contract:
//! [`BufferPolicy`] (a bounded capacity plus an overflow rule),
//! [`BufferOverflowPolicy`] (what to do when a full buffer receives a packet),
//! and [`BackpressureOutcome`] (the result a producer observes when it offers a
//! packet). Each carries a stable `stream/*` symbol so the policy round-trips
//! through the runtime's symbol and [`Expr`] surfaces. The crate-private
//! `field` helper reads a bare-symbol entry out of an [`Expr::Map`] and is
//! reused by sibling modules that decode stream values; the typed
//! `string_field`/`symbol_field` readers are thin wrappers over the shared
//! `sim_value::access` slice readers.

use sim_kernel::{Error, Expr, Result, Symbol};
use sim_value::access;
pub(crate) use sim_value::kind::expr_kind;

/// Result a producer observes when it offers a packet to a buffered stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackpressureOutcome {
    /// The packet was buffered.
    Accepted,
    /// The buffer was full and this newest packet was dropped.
    DroppedNewest,
    /// The buffer was full and the oldest buffered packet was dropped to admit
    /// this one.
    DroppedOldest,
    /// The producer is blocked until capacity frees up.
    Blocked,
    /// The offer timed out before capacity freed up.
    TimedOut,
    /// The offer was rejected by policy.
    Rejected,
    /// The stream is closed and accepts no further packets.
    Closed,
}

impl BackpressureOutcome {
    /// Returns the stable wire label for this outcome.
    pub fn wire_label(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::DroppedNewest => "dropped-newest",
            Self::DroppedOldest => "dropped-oldest",
            Self::Blocked => "blocked",
            Self::TimedOut => "timed-out",
            Self::Rejected => "rejected",
            Self::Closed => "closed",
        }
    }

    /// Returns the `stream/backpressure/<label>` symbol for this outcome.
    pub fn symbol(self) -> Symbol {
        Symbol::qualified("stream/backpressure", self.wire_label())
    }

    /// Parses an outcome from its bare or `stream/backpressure`-qualified
    /// symbol.
    ///
    /// Returns an error for an unrecognized outcome symbol.
    pub fn from_symbol(symbol: &Symbol) -> Result<Self> {
        match symbol.as_qualified_str().as_str() {
            "accepted" | "stream/backpressure/accepted" => Ok(Self::Accepted),
            "dropped-newest" | "stream/backpressure/dropped-newest" => Ok(Self::DroppedNewest),
            "dropped-oldest" | "stream/backpressure/dropped-oldest" => Ok(Self::DroppedOldest),
            "blocked" | "stream/backpressure/blocked" => Ok(Self::Blocked),
            "timed-out" | "stream/backpressure/timed-out" => Ok(Self::TimedOut),
            "rejected" | "stream/backpressure/rejected" => Ok(Self::Rejected),
            "closed" | "stream/backpressure/closed" => Ok(Self::Closed),
            other => Err(Error::Eval(format!(
                "unknown stream backpressure outcome {other}"
            ))),
        }
    }
}

/// Rule applied when a full buffer receives another packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferOverflowPolicy {
    /// Drop the incoming (newest) packet.
    DropNewest,
    /// Drop the oldest buffered packet to make room.
    DropOldest,
    /// Treat the overflow as an error.
    Error,
}

impl BufferOverflowPolicy {
    /// Returns the `stream/overflow/<rule>` symbol for this policy.
    pub fn symbol(self) -> Symbol {
        match self {
            Self::DropNewest => Symbol::qualified("stream/overflow", "drop-newest"),
            Self::DropOldest => Symbol::qualified("stream/overflow", "drop-oldest"),
            Self::Error => Symbol::qualified("stream/overflow", "error"),
        }
    }

    /// Parses an overflow policy from its `stream/overflow`-qualified symbol.
    ///
    /// Returns an error for an unrecognized overflow symbol.
    pub fn from_symbol(symbol: &Symbol) -> Result<Self> {
        match symbol.as_qualified_str().as_str() {
            "stream/overflow/drop-newest" => Ok(Self::DropNewest),
            "stream/overflow/drop-oldest" => Ok(Self::DropOldest),
            "stream/overflow/error" => Ok(Self::Error),
            other => Err(Error::Eval(format!(
                "unknown stream buffer overflow policy {other}"
            ))),
        }
    }
}

/// Buffering contract for a stream: a bounded capacity plus an overflow rule.
///
/// A policy is always bounded with a capacity of at least one; the constructors
/// reject a zero capacity. The overflow rule decides what happens when the
/// buffer is full.
///
/// # Examples
///
/// ```
/// use sim_lib_stream_core::{BufferOverflowPolicy, BufferPolicy};
///
/// let policy = BufferPolicy::bounded(8).expect("capacity is nonzero");
/// assert_eq!(policy.capacity(), 8);
/// assert_eq!(policy.overflow(), BufferOverflowPolicy::DropNewest);
/// assert!(BufferPolicy::bounded(0).is_err());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferPolicy {
    capacity: usize,
    overflow: BufferOverflowPolicy,
}

impl BufferPolicy {
    /// Builds a bounded policy of `capacity` with the default
    /// [`BufferOverflowPolicy::DropNewest`] overflow rule.
    ///
    /// Returns an error if `capacity` is zero.
    pub fn bounded(capacity: usize) -> Result<Self> {
        Self::bounded_with_overflow(capacity, BufferOverflowPolicy::DropNewest)
    }

    /// Builds a bounded policy of `capacity` with an explicit overflow rule.
    ///
    /// Returns an error if `capacity` is zero.
    pub fn bounded_with_overflow(capacity: usize, overflow: BufferOverflowPolicy) -> Result<Self> {
        if capacity == 0 {
            return Err(Error::Eval(
                "stream buffer capacity must be greater than zero".to_owned(),
            ));
        }
        Ok(Self { capacity, overflow })
    }

    /// Returns the buffer capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the overflow rule.
    pub fn overflow(&self) -> BufferOverflowPolicy {
        self.overflow
    }

    /// Returns the `stream/buffer/bounded-<capacity>` symbol for this policy.
    pub fn symbol(&self) -> Symbol {
        Symbol::qualified("stream/buffer", format!("bounded-{}", self.capacity))
    }

    /// Encodes this policy as an [`Expr::Map`] with `capacity` and `overflow`
    /// fields.
    pub fn to_expr(&self) -> Expr {
        Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("capacity")),
                Expr::String(self.capacity.to_string()),
            ),
            (
                Expr::Symbol(Symbol::new("overflow")),
                Expr::Symbol(self.overflow.symbol()),
            ),
        ])
    }

    /// Decodes a policy from an [`Expr::Map`] produced by
    /// [`to_expr`](Self::to_expr).
    ///
    /// Returns an error if the expression is not a map, a field is missing or
    /// the wrong type, or the capacity fails to parse.
    pub fn from_expr(expr: &Expr) -> Result<Self> {
        let Expr::Map(entries) = expr else {
            return Err(Error::TypeMismatch {
                expected: "stream buffer policy map",
                found: expr_kind(expr),
            });
        };
        let capacity = string_field(entries, "capacity")?
            .parse::<usize>()
            .map_err(|err| Error::Eval(format!("invalid stream buffer capacity: {err}")))?;
        let overflow = BufferOverflowPolicy::from_symbol(symbol_field(entries, "overflow")?)?;
        Self::bounded_with_overflow(capacity, overflow)
    }
}

pub(crate) fn string_field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> Result<&'a str> {
    access::entry_required_str(entries, name, "string field")
}

pub(crate) fn symbol_field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> Result<&'a Symbol> {
    access::entry_required_sym(entries, name, "symbol field")
}

pub(crate) fn field<'a>(entries: &'a [(Expr, Expr)], name: &str) -> Result<&'a Expr> {
    entries
        .iter()
        .find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == name => {
                Some(value)
            }
            _ => None,
        })
        .ok_or_else(|| Error::Eval(format!("stream value missing {name} field")))
}

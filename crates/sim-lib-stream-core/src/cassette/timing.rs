use sim_kernel::{Expr, Result, Symbol};

use crate::{
    StreamEnvelope, StreamMetadata, StreamPacket,
    buffer::{expr_kind, field, symbol_field},
};

use super::{
    StreamCassetteTiming, bool_field, ensure_fields, optional_u64, optional_u64_expr, parse_usize,
};

impl StreamCassetteTiming {
    /// Serializes the timing summary to an [`Expr`] map keyed by field symbol.
    pub fn to_expr(&self) -> Expr {
        Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("clock")),
                Expr::Symbol(self.clock.clone()),
            ),
            (
                Expr::Symbol(Symbol::new("packet-count")),
                Expr::String(self.packet_count.to_string()),
            ),
            (
                Expr::Symbol(Symbol::new("first-sequence")),
                optional_u64_expr(self.first_sequence),
            ),
            (
                Expr::Symbol(Symbol::new("last-sequence")),
                optional_u64_expr(self.last_sequence),
            ),
            (Expr::Symbol(Symbol::new("finite")), Expr::Bool(self.finite)),
        ])
    }

    /// Deserializes a timing summary from an [`Expr`] map produced by
    /// [`to_expr`](StreamCassetteTiming::to_expr).
    ///
    /// Validates the field set and fails closed on missing or unexpected
    /// fields or type mismatches.
    pub fn from_expr(expr: &Expr) -> Result<Self> {
        let Expr::Map(entries) = expr else {
            return Err(sim_kernel::Error::TypeMismatch {
                expected: "stream cassette timing map",
                found: expr_kind(expr),
            });
        };
        ensure_fields(
            entries,
            &[
                "clock",
                "packet-count",
                "first-sequence",
                "last-sequence",
                "finite",
            ],
        )?;
        Ok(Self {
            clock: symbol_field(entries, "clock")?.clone(),
            packet_count: parse_usize(entries, "packet-count")?,
            first_sequence: optional_u64(field(entries, "first-sequence")?)?,
            last_sequence: optional_u64(field(entries, "last-sequence")?)?,
            finite: bool_field(entries, "finite")?,
        })
    }
}

pub(super) fn timing_from_envelopes(
    metadata: &StreamMetadata,
    envelopes: &[StreamEnvelope],
) -> StreamCassetteTiming {
    StreamCassetteTiming {
        clock: metadata.clock().clone(),
        packet_count: envelopes.len(),
        first_sequence: envelopes.first().map(StreamEnvelope::sequence),
        last_sequence: envelopes.last().map(StreamEnvelope::sequence),
        finite: true,
    }
}

pub(super) fn diagnostics_from_envelopes(envelopes: &[StreamEnvelope]) -> Vec<Symbol> {
    let mut diagnostics = Vec::new();
    for envelope in envelopes {
        for diagnostic in envelope.diagnostics() {
            push_unique(&mut diagnostics, diagnostic.clone());
        }
        if let StreamPacket::Diagnostic(packet) = envelope.packet() {
            push_unique(&mut diagnostics, packet.kind().clone());
        }
    }
    diagnostics
}

fn push_unique(symbols: &mut Vec<Symbol>, symbol: Symbol) {
    if !symbols.contains(&symbol) {
        symbols.push(symbol);
    }
}

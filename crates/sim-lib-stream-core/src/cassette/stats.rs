use sim_kernel::{Error, Expr, Result, Symbol};
use sim_value::access;

use crate::StreamStats;
use crate::buffer::{expr_kind, string_field};

pub(super) fn stream_stats_expr(stats: &StreamStats) -> Expr {
    Expr::Map(vec![
        stat("pushed", stats.pushed),
        stat("accepted", stats.accepted),
        stat("yielded", stats.yielded),
        stat("dropped-newest", stats.dropped_newest),
        stat("dropped-oldest", stats.dropped_oldest),
        stat("overflow-errors", stats.overflow_errors),
        stat("rejected", stats.rejected),
        stat("timeouts", stats.timeouts),
        stat("timed-out", stats.timed_out),
        stat("blocked", stats.blocked),
        (
            Expr::Symbol(Symbol::new("closed")),
            Expr::Bool(stats.closed),
        ),
        (
            Expr::Symbol(Symbol::new("cancelled")),
            Expr::Bool(stats.cancelled),
        ),
    ])
}

pub(super) fn stream_stats_from_expr(expr: &Expr) -> Result<StreamStats> {
    let Expr::Map(entries) = expr else {
        return Err(Error::TypeMismatch {
            expected: "stream stats map",
            found: expr_kind(expr),
        });
    };
    Ok(StreamStats {
        pushed: parse_u64(entries, "pushed")?,
        accepted: parse_u64(entries, "accepted")?,
        yielded: parse_u64(entries, "yielded")?,
        dropped_newest: parse_u64(entries, "dropped-newest")?,
        dropped_oldest: parse_u64(entries, "dropped-oldest")?,
        overflow_errors: parse_u64(entries, "overflow-errors")?,
        rejected: parse_u64(entries, "rejected")?,
        timeouts: parse_u64(entries, "timeouts")?,
        timed_out: parse_u64(entries, "timed-out")?,
        blocked: parse_u64(entries, "blocked")?,
        closed: bool_field(entries, "closed")?,
        cancelled: bool_field(entries, "cancelled")?,
    })
}

fn stat(name: &str, value: u64) -> (Expr, Expr) {
    (
        Expr::Symbol(Symbol::new(name)),
        Expr::String(value.to_string()),
    )
}

fn parse_u64(entries: &[(Expr, Expr)], name: &str) -> Result<u64> {
    string_field(entries, name)?
        .parse::<u64>()
        .map_err(|err| Error::Eval(format!("invalid stream cassette {name}: {err}")))
}

fn bool_field(entries: &[(Expr, Expr)], name: &str) -> Result<bool> {
    access::entry_required_bool(entries, name, "bool field")
}

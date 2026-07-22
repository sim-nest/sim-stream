//! Deterministic cookbook builders for stream-clock recipes.

use sim_kernel::{Expr, NumberLiteral, Symbol};

use crate::{Clock, ClockChart, Instant, TempoMap, TempoSegment};

/// Build the modeled tempo chart descriptor used by the cookbook recipe.
pub fn tempo_chart_demo() -> Expr {
    let tempo = TempoMap::new(vec![
        TempoSegment::new(0, 500_000).expect("valid initial tempo"),
        TempoSegment::new(1_920, 400_000).expect("valid second tempo"),
    ])
    .expect("valid tempo map");
    let clock = Clock::midi(
        Symbol::qualified("stream/clock", "cookbook-tempo"),
        480,
        tempo,
    )
    .expect("valid cookbook MIDI clock");
    let beat = clock
        .index_for_instant(Instant::seconds(1))
        .expect("one second maps onto the MIDI clock");
    Expr::Map(vec![
        (field("kind"), sym("stream-clock", "tempo-chart")),
        (field("clock"), Expr::Symbol(clock.id().clone())),
        (field("domain"), Expr::Symbol(clock.domain().symbol())),
        (
            field("tpq"),
            match clock.chart() {
                ClockChart::Midi { tpq, .. } => number(*tpq),
                ClockChart::Frames { .. } => number(0),
            },
        ),
        (field("index-at-one-second"), number(beat.index().value())),
        (
            field("segments"),
            Expr::Vector(match clock.chart() {
                ClockChart::Midi { tempo_map, .. } => tempo_map
                    .segments()
                    .iter()
                    .map(|segment| {
                        Expr::Map(vec![
                            (field("start-tick"), number(segment.start_tick)),
                            (field("us-per-quarter"), number(segment.us_per_quarter)),
                        ])
                    })
                    .collect(),
                ClockChart::Frames { .. } => Vec::new(),
            }),
        ),
    ])
}

fn field(name: &str) -> Expr {
    Expr::Symbol(Symbol::qualified("stream-clock", name))
}

fn sym(namespace: &str, name: &str) -> Expr {
    Expr::Symbol(Symbol::qualified(namespace, name))
}

fn number(value: impl ToString) -> Expr {
    Expr::Number(NumberLiteral {
        domain: Symbol::qualified("numbers", "i64"),
        canonical: value.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempo_chart_demo_contains_valid_segments() {
        let Expr::Map(entries) = tempo_chart_demo() else {
            panic!("tempo chart demo is a map")
        };
        let Some((_, Expr::Vector(segments))) = entries.iter().find(
            |(key, _)| matches!(key, Expr::Symbol(symbol) if symbol.name.as_ref() == "segments"),
        ) else {
            panic!("tempo chart has segments")
        };
        assert_eq!(segments.len(), 2);
    }
}

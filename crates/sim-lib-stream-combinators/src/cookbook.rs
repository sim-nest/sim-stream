//! Deterministic cookbook builders for stream-combinator recipes.

use sim_kernel::{Expr, NumberLiteral, Symbol};

use crate::stream_window_data_kind;

/// Build the modeled pipeline-stage descriptor used by the cookbook recipe.
pub fn pipeline_stages_demo() -> Expr {
    Expr::Map(vec![
        (field("kind"), sym("stream-combinators", "pipeline")),
        (
            field("stages"),
            Expr::Vector(vec![
                sym("stream/stage", "map"),
                sym("stream/stage", "filter-data-kind"),
                Expr::Symbol(stream_window_data_kind()),
                sym("stream/stage", "record"),
                sym("stream/stage", "replay"),
            ]),
        ),
        (field("window-size"), number(4)),
        (field("modeled-source"), sym("stream/source", "memory-data")),
    ])
}

fn field(name: &str) -> Expr {
    Expr::Symbol(Symbol::qualified("stream-combinators", name))
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
    fn pipeline_demo_includes_window_data_kind() {
        let Expr::Map(entries) = pipeline_stages_demo() else {
            panic!("pipeline stages demo is a map")
        };
        assert!(entries.iter().any(|(_, value)| {
            matches!(value, Expr::Vector(stages) if stages.iter().any(|stage| matches!(stage, Expr::Symbol(symbol) if *symbol == stream_window_data_kind())))
        }));
    }
}

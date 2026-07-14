//! Deterministic cookbook builders for stream-prelude recipes.

use sim_kernel::{Expr, NumberLiteral, Symbol};

/// Build the modeled memory pipe descriptor used by the cookbook recipe.
pub fn memory_pipe_demo() -> Expr {
    Expr::Map(vec![
        (field("kind"), sym("stream-prelude", "memory-pipe")),
        (field("source"), sym("stream/source", "memory-midi")),
        (field("sink"), sym("stream/sink", "memory-midi")),
        (
            field("stages"),
            Expr::Vector(vec![
                sym("stream/stage", "identity"),
                sym("stream/stage", "filter-data-kind"),
                sym("stream/stage", "window-by-count"),
            ]),
        ),
        (field("capacity"), number(8)),
        (field("capability"), sym("capability", "stream.open")),
    ])
}

fn field(name: &str) -> Expr {
    Expr::Symbol(Symbol::qualified("stream-prelude", name))
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
    fn memory_pipe_demo_names_source_sink_and_stages() {
        let Expr::Map(entries) = memory_pipe_demo() else {
            panic!("memory pipe demo is a map")
        };
        assert!(entries.iter().any(|(_, value)| {
            matches!(value, Expr::Symbol(symbol) if symbol.as_qualified_str() == "stream/source/memory-midi")
        }));
        assert!(
            entries
                .iter()
                .any(|(_, value)| matches!(value, Expr::Vector(stages) if stages.len() == 3))
        );
    }
}

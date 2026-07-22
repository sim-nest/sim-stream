//! Deterministic cookbook builders for topology recipes.

use sim_kernel::{Expr, NumberLiteral, Symbol};

/// Build the tiny data-authored graph descriptor used by the cookbook recipe.
pub fn tiny_graph_demo() -> Expr {
    list(vec![
        Expr::Symbol(Symbol::qualified("topology", "graph")),
        kw("name"),
        sym_plain("doc-flow"),
        kw("nodes"),
        list(vec![
            list(vec![sym_plain("in"), kw("verb"), sym_plain("in")]),
            list(vec![sym_plain("out"), kw("verb"), sym_plain("out")]),
        ]),
        kw("edges"),
        list(vec![list(vec![
            sym_plain("in"),
            sym_plain("->"),
            sym_plain("out"),
        ])]),
    ])
}

/// Build the deterministic embodied-intelligence trace used by the recipe.
pub fn embodied_intelligence_trace_demo() -> Expr {
    list(vec![
        sym_plain("embodied-intelligence-trace"),
        list(vec![
            sym_plain("id"),
            sym_plain("a30-030-embodied-intelligence"),
        ]),
        list(vec![
            sym_plain("metadata"),
            list(vec![
                sym_plain("fixture"),
                sym_plain("synthetic-mobile-base"),
            ]),
            list(vec![sym_plain("synthetic-data"), sym_plain("yes")]),
            list(vec![sym_plain("deterministic"), sym_plain("yes")]),
            list(vec![sym_plain("actuation"), sym_plain("simulated-only")]),
        ]),
        list(vec![
            sym_plain("body-model"),
            list(vec![sym_plain("plant"), sym_plain("two-wheel-base")]),
            list(vec![
                sym_plain("pose"),
                sym_plain("x10"),
                sym_plain("y4"),
                sym_plain("heading90"),
            ]),
            list(vec![sym_plain("battery-percent"), number(72)]),
            list(vec![sym_plain("payload"), sym_plain("locked")]),
        ]),
        list(vec![
            sym_plain("asymmetric-loop"),
            list(vec![
                sym_plain("slow-strategy"),
                list(vec![sym_plain("runner"), sym_plain("fake-world-model")]),
                list(vec![sym_plain("period-ms"), number(1000)]),
                list(vec![
                    sym_plain("plan"),
                    sym_plain("inspect-zone-b-then-dock"),
                ]),
            ]),
            list(vec![
                sym_plain("fast-control"),
                list(vec![
                    sym_plain("law"),
                    sym_plain("deterministic-p-controller"),
                ]),
                list(vec![sym_plain("period-ms"), number(20)]),
                list(vec![sym_plain("target"), sym_plain("waypoint-b")]),
                list(vec![sym_plain("linear-command"), number(12)]),
                list(vec![sym_plain("angular-command"), number(-2)]),
            ]),
        ]),
        list(vec![
            sym_plain("constraint-envelope"),
            list(vec![
                sym_plain("constraint"),
                sym_plain("battery-min-pass"),
                sym_plain("yes"),
            ]),
            list(vec![
                sym_plain("constraint"),
                sym_plain("keepout-zone-pass"),
                sym_plain("yes"),
            ]),
            list(vec![
                sym_plain("constraint"),
                sym_plain("payload-lock-pass"),
                sym_plain("yes"),
            ]),
            list(vec![sym_plain("binary-and"), sym_plain("go")]),
        ]),
        list(vec![
            sym_plain("topology-graph"),
            graph_node("perception"),
            graph_node("strategy"),
            graph_node("control"),
            graph_node("actuator-gate"),
            edge("perception", "strategy"),
            edge("strategy", "control"),
            edge("control", "actuator-gate"),
        ]),
        list(vec![
            sym_plain("cascade-failure"),
            list(vec![sym_plain("inject"), sym_plain("actuator-gate")]),
            list(vec![
                sym_plain("propagates-to"),
                sym_plain("motor-left"),
                sym_plain("motor-right"),
            ]),
            list(vec![sym_plain("result"), sym_plain("no-go")]),
            list(vec![sym_plain("safe-state"), sym_plain("coast-and-brake")]),
        ]),
        list(vec![
            sym_plain("decision"),
            list(vec![sym_plain("motion"), sym_plain("go-until-gate")]),
            list(vec![sym_plain("actuation"), sym_plain("simulated-only")]),
        ]),
        list(vec![
            sym_plain("effect-ledger"),
            effect("run-slow-fake-strategy", "inspect-zone-b"),
            effect("run-fast-control", "p-controller"),
            effect("evaluate-constraint-envelope", "go"),
            effect("inject-topology-failure", "actuator-gate"),
            effect("propagate-safe-state", "coast-and-brake"),
        ]),
    ])
}

fn graph_node(id: &str) -> Expr {
    list(vec![
        sym_plain("node"),
        sym_plain(id),
        sym_plain("status"),
        sym_plain("ok"),
    ])
}

fn edge(from: &str, to: &str) -> Expr {
    list(vec![sym_plain("edge"), sym_plain(from), sym_plain(to)])
}

fn effect(action: &str, result: &str) -> Expr {
    list(vec![
        sym_plain("effect"),
        sym_plain(action),
        sym_plain(result),
    ])
}

fn sym_plain(name: &str) -> Expr {
    Expr::Symbol(Symbol::new(name))
}

fn kw(name: &str) -> Expr {
    Expr::Symbol(Symbol::new(format!(":{name}")))
}

fn number(value: impl ToString) -> Expr {
    Expr::Number(NumberLiteral {
        domain: Symbol::qualified("numbers", "i64"),
        canonical: value.to_string(),
    })
}

fn list(items: Vec<Expr>) -> Expr {
    Expr::List(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_graph_expr;

    #[test]
    fn tiny_graph_demo_parses_as_topology_graph() {
        parse_graph_expr(&tiny_graph_demo()).expect("tiny graph demo parses");
    }

    #[test]
    fn embodied_trace_names_simulated_actuation() {
        let rendered = format!("{:?}", embodied_intelligence_trace_demo());
        assert!(rendered.contains("simulated-only"));
        assert!(rendered.contains("actuator-gate"));
    }
}

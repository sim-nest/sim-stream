//! Deterministic cookbook builders for rank recipes.

use sim_kernel::{Expr, NumberLiteral, Symbol};

use crate::{EmbeddingStore, RankGrammar, RankSpace, retrieve_ids};

/// Build the modeled space/coordinate descriptor used by the cookbook recipe.
pub fn space_coordinate_demo() -> Expr {
    let space = RankSpace::group(
        Symbol::qualified("rank/cookbook", "unit-node"),
        RankGrammar::Unit,
    );
    Expr::Map(vec![
        (field("kind"), sym("rank", "space-coordinate")),
        (field("space"), Expr::Symbol(space.symbol().clone())),
        (field("codec"), sym("rank/codec", "unit")),
        (field("ordinal"), number(0)),
    ])
}

/// Build the modeled rank-backed retrieve descriptor used by the cookbook.
pub fn rank_retrieve_demo() -> Expr {
    let store = EmbeddingStore::with_entries(vec![
        ("essay-c".to_owned(), vec![72.0, 76.0, 35.0]),
        ("guide-a".to_owned(), vec![60.0, 90.0, 25.0]),
        ("clip-b".to_owned(), vec![95.0, 35.0, 10.0]),
    ])
    .expect("valid cookbook embedding store");
    let ranked = retrieve_ids(
        &store,
        &[70.0, 80.0, 30.0],
        ["essay-c", "guide-a", "clip-b"],
        3,
    )
    .expect("ranked cookbook retrieve");
    Expr::Map(vec![
        (field("kind"), sym("rank", "retrieve")),
        (field("metric"), sym("rank/metric", "euclidean")),
        (
            field("ordered"),
            Expr::Vector(
                ranked
                    .into_iter()
                    .map(|neighbor| Expr::String(neighbor.id))
                    .collect(),
            ),
        ),
    ])
}

/// Build the deterministic recommendation trace used by the 30-agent recipe.
pub fn recommendation_ranking_demo() -> Expr {
    list(vec![
        sym_plain("recommendation-ranking"),
        list(vec![sym_plain("id"), sym_plain("a30-018-recommendation")]),
        list(vec![
            sym_plain("space"),
            sym_plain("content-preference"),
            list(vec![sym_plain("axis"), sym_plain("novelty")]),
            list(vec![sym_plain("axis"), sym_plain("depth")]),
            list(vec![sym_plain("axis"), sym_plain("duration")]),
        ]),
        list(vec![
            sym_plain("preference"),
            list(vec![sym_plain("user"), sym_plain("reader-1")]),
            list(vec![
                sym_plain("coordinate"),
                sym_plain("novelty"),
                number(70),
            ]),
            list(vec![
                sym_plain("coordinate"),
                sym_plain("depth"),
                number(80),
            ]),
            list(vec![
                sym_plain("coordinate"),
                sym_plain("duration"),
                number(30),
            ]),
        ]),
        list(vec![
            sym_plain("items"),
            item("guide-a", 60, 90, 25),
            item("clip-b", 95, 35, 10),
            item("essay-c", 72, 76, 35),
        ]),
        list(vec![
            sym_plain("ranking"),
            list(vec![sym_plain("method"), sym_plain("weighted-distance")]),
            list(vec![sym_plain("tie-break"), sym_plain("id-ascending")]),
            list(vec![
                sym_plain("ordered"),
                sym_plain("essay-c"),
                sym_plain("guide-a"),
                sym_plain("clip-b"),
            ]),
        ]),
        list(vec![
            sym_plain("explanations"),
            pick("essay-c", 9, "closest-balanced-match"),
            pick("guide-a", 15, "depth-match"),
            pick("clip-b", 61, "novelty-heavy"),
        ]),
        list(vec![
            sym_plain("answer"),
            sym_plain("recommend-essay-c-then-guide-a"),
        ]),
    ])
}

fn item(id: &str, novelty: i64, depth: i64, duration: i64) -> Expr {
    list(vec![
        sym_plain("item"),
        sym_plain(id),
        list(vec![
            sym_plain("coordinate"),
            sym_plain("novelty"),
            number(novelty),
        ]),
        list(vec![
            sym_plain("coordinate"),
            sym_plain("depth"),
            number(depth),
        ]),
        list(vec![
            sym_plain("coordinate"),
            sym_plain("duration"),
            number(duration),
        ]),
    ])
}

fn pick(id: &str, distance: i64, reason: &str) -> Expr {
    list(vec![
        sym_plain("pick"),
        sym_plain(id),
        sym_plain("distance"),
        number(distance),
        sym_plain("reason"),
        sym_plain(reason),
    ])
}

fn field(name: &str) -> Expr {
    Expr::Symbol(Symbol::qualified("rank", name))
}

fn sym(namespace: &str, name: &str) -> Expr {
    Expr::Symbol(Symbol::qualified(namespace, name))
}

fn sym_plain(name: &str) -> Expr {
    Expr::Symbol(Symbol::new(name))
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

    #[test]
    fn retrieve_demo_orders_closest_items_first() {
        let Expr::Map(entries) = rank_retrieve_demo() else {
            panic!("retrieve demo is a map")
        };
        let Some((_, Expr::Vector(items))) = entries.iter().find(
            |(key, _)| matches!(key, Expr::Symbol(symbol) if symbol.name.as_ref() == "ordered"),
        ) else {
            panic!("retrieve demo has ordered items")
        };
        assert_eq!(items.first(), Some(&Expr::String("essay-c".to_owned())));
    }

    #[test]
    fn recommendation_demo_keeps_expected_root_symbol() {
        let Expr::List(items) = recommendation_ranking_demo() else {
            panic!("recommendation demo is a list")
        };
        assert!(
            matches!(&items[0], Expr::Symbol(symbol) if symbol.name.as_ref() == "recommendation-ranking")
        );
    }
}

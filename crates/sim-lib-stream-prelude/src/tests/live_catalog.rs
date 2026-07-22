use sim_kernel::{Expr, Symbol};

use crate::{stream_control_capability, stream_open_capability, stream_stats_capability};

use super::support::{cx, eval_lisp, midi_source_form, table_value, value_expr};

#[test]
fn cancel_older_than_uses_catalog_time_not_wall_clock() {
    let mut cx = cx(&[
        stream_open_capability(),
        stream_control_capability(),
        stream_stats_capability(),
    ]);
    let old = eval_lisp(&mut cx, &midi_source_form("stream/catalog-old")).unwrap();
    cx.env_mut().define(Symbol::new("old"), old);
    let now = eval_lisp(&mut cx, "(stream/advance-catalog-time! 5)").unwrap();
    assert_eq!(value_expr(&mut cx, now), Expr::String("5".to_owned()));
    let fresh = eval_lisp(&mut cx, &midi_source_form("stream/catalog-fresh")).unwrap();
    cx.env_mut().define(Symbol::new("fresh"), fresh);

    let exact = eval_lisp(&mut cx, "(stream/cancel-older-than! 5)").unwrap();
    assert_eq!(value_expr(&mut cx, exact), Expr::List(Vec::new()));
    let cancelled = eval_lisp(&mut cx, "(stream/cancel-older-than! 4)").unwrap();
    assert_eq!(
        value_expr(&mut cx, cancelled),
        Expr::List(vec![Expr::Symbol(Symbol::new("stream/catalog-old"))])
    );

    let old_stats = eval_lisp(&mut cx, "(stream/stats old)").unwrap();
    assert_eq!(
        table_value(&value_expr(&mut cx, old_stats), "cancelled"),
        Some(&Expr::Bool(true))
    );
    let fresh_stats = eval_lisp(&mut cx, "(stream/stats fresh)").unwrap();
    assert_eq!(
        table_value(&value_expr(&mut cx, fresh_stats), "cancelled"),
        Some(&Expr::Bool(false))
    );
}

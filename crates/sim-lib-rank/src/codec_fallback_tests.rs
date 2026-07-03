use sim_kernel::{Error, Expr, Symbol};

use crate::{
    binary_frame_to_nat, rank_codec_capability, rank_codec_fallback_card, rank_expr_with_fallback,
    rank_value_with_fallback, unrank_expr_storage_identity, unrank_expr_with_fallback,
};

use sim_kernel::testing::eager_cx as cx;

#[test]
fn simple_values_rank_and_unrank_through_fallback() {
    let mut cx = cx();
    let value = cx.factory().string("rank fallback".to_owned()).unwrap();

    assert_denied(rank_value_with_fallback(&mut cx, &value).unwrap_err());

    cx.grant(rank_codec_capability());
    let ordinal = rank_value_with_fallback(&mut cx, &value).unwrap();
    let decoded = unrank_expr_with_fallback(&mut cx, &ordinal).unwrap();

    assert_eq!(decoded, Expr::String("rank fallback".to_owned()));
}

#[test]
fn expression_fallback_uses_canonical_binary_storage_identity() {
    let mut cx = cx();
    cx.grant(rank_codec_capability());
    let expr = Expr::Map(vec![
        (Expr::Symbol(Symbol::new("b")), Expr::Bool(false)),
        (Expr::Symbol(Symbol::new("a")), Expr::Bool(true)),
    ]);
    let same_expr_different_order = Expr::Map(vec![
        (Expr::Symbol(Symbol::new("a")), Expr::Bool(true)),
        (Expr::Symbol(Symbol::new("b")), Expr::Bool(false)),
    ]);

    let left = rank_expr_with_fallback(&mut cx, &expr).unwrap();
    let right = rank_expr_with_fallback(&mut cx, &same_expr_different_order).unwrap();

    assert_eq!(left, right);
    assert_eq!(unrank_expr_with_fallback(&mut cx, &left).unwrap(), expr);
}

#[test]
fn invalid_byte_frames_return_structured_errors() {
    let invalid = binary_frame_to_nat(b"BAD!");
    let err = unrank_expr_storage_identity(&invalid).unwrap_err();

    assert!(matches!(
        err,
        Error::CodecError { message, .. } if message.contains("magic mismatch")
    ));
}

#[test]
fn fallback_card_labels_storage_identity_not_semantic_order() {
    let mut cx = cx();
    let card = rank_codec_fallback_card(&mut cx)
        .unwrap()
        .object()
        .as_expr(&mut cx)
        .unwrap();

    assert_eq!(
        table_string(&card, "identity"),
        Some("storage identity".to_owned())
    );
    assert_eq!(
        table_string(&card, "order-semantics"),
        Some("not semantic order".to_owned())
    );
    assert_list_contains_symbol(
        table_value(&card, "requires").unwrap(),
        Symbol::qualified("capability", "rank.codec"),
    );
}

fn assert_denied(err: Error) {
    assert!(matches!(
        err,
        Error::CapabilityDenied { capability } if capability == rank_codec_capability()
    ));
}

fn table_string(expr: &Expr, key: &str) -> Option<String> {
    let Expr::String(value) = table_value(expr, key)? else {
        return None;
    };
    Some(value.clone())
}

fn table_value<'a>(expr: &'a Expr, key: &str) -> Option<&'a Expr> {
    let Expr::Map(entries) = expr else {
        return None;
    };
    entries.iter().find_map(|(entry_key, value)| {
        if entry_key == &Expr::Symbol(Symbol::new(key)) {
            Some(value)
        } else {
            None
        }
    })
}

fn assert_list_contains_symbol(expr: &Expr, symbol: Symbol) {
    let Expr::List(items) = expr else {
        panic!("expected list, found {expr:?}");
    };
    assert!(
        items.contains(&Expr::Symbol(symbol.clone())),
        "missing {symbol} in {items:?}"
    );
}

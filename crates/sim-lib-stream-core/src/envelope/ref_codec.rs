use sim_kernel::{ContentId, Coordinate, Error, Expr, HandleId, Ref, Result, Symbol};

use crate::buffer::{expr_kind, field, string_field, symbol_field};

pub(super) fn ref_expr(reference: &Ref) -> Expr {
    match reference {
        Ref::Symbol(symbol) => Expr::Symbol(symbol.clone()),
        Ref::Content(content) => content_ref_expr(content),
        Ref::Handle(handle) => Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("ref")),
                Expr::Symbol(Symbol::qualified("stream/ref", "handle")),
            ),
            (
                Expr::Symbol(Symbol::new("id")),
                Expr::String(handle.0.to_string()),
            ),
        ]),
        Ref::Coord(coordinate) => Expr::Map(vec![
            (
                Expr::Symbol(Symbol::new("ref")),
                Expr::Symbol(Symbol::qualified("stream/ref", "coord")),
            ),
            (
                Expr::Symbol(Symbol::new("space")),
                Expr::Symbol(coordinate.space.clone()),
            ),
            (
                Expr::Symbol(Symbol::new("ordinal")),
                content_ref_expr(&coordinate.ordinal),
            ),
        ]),
    }
}

pub(super) fn ref_from_expr(expr: &Expr) -> Result<Ref> {
    match expr {
        Expr::Symbol(symbol) => Ok(Ref::Symbol(symbol.clone())),
        Expr::Map(entries) => {
            let tag = symbol_field(entries, "ref")?;
            match tag.as_qualified_str().as_str() {
                "stream/ref/content" => Ok(Ref::Content(content_from_entries(entries)?)),
                "stream/ref/handle" => {
                    let id = string_field(entries, "id")?
                        .parse::<u128>()
                        .map_err(|err| Error::Eval(format!("invalid stream handle ref: {err}")))?;
                    Ok(Ref::Handle(HandleId(id)))
                }
                "stream/ref/coord" => {
                    ensure_fields(entries, &["ref", "space", "ordinal"])?;
                    let ordinal = match ref_from_expr(field(entries, "ordinal")?)? {
                        Ref::Content(content) => content,
                        _ => {
                            return Err(Error::Eval(
                                "stream coordinate ordinal must be a content ref".to_owned(),
                            ));
                        }
                    };
                    Ok(Ref::Coord(Coordinate {
                        space: symbol_field(entries, "space")?.clone(),
                        ordinal,
                    }))
                }
                other => Err(Error::Eval(format!("unknown stream ref tag {other}"))),
            }
        }
        other => Err(Error::TypeMismatch {
            expected: "stream ref expression",
            found: expr_kind(other),
        }),
    }
}

fn content_ref_expr(content: &ContentId) -> Expr {
    Expr::Map(vec![
        (
            Expr::Symbol(Symbol::new("ref")),
            Expr::Symbol(Symbol::qualified("stream/ref", "content")),
        ),
        (
            Expr::Symbol(Symbol::new("algorithm")),
            Expr::Symbol(content.algorithm.clone()),
        ),
        (
            Expr::Symbol(Symbol::new("bytes")),
            Expr::Bytes(content.bytes.to_vec()),
        ),
    ])
}

fn content_from_entries(entries: &[(Expr, Expr)]) -> Result<ContentId> {
    ensure_fields(entries, &["ref", "algorithm", "bytes"])?;
    let bytes = match field(entries, "bytes")? {
        Expr::Bytes(bytes) => bytes,
        other => {
            return Err(Error::TypeMismatch {
                expected: "content bytes",
                found: expr_kind(other),
            });
        }
    };
    let bytes: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| Error::Eval("stream content ref bytes must be 32 bytes".to_owned()))?;
    Ok(ContentId::from_bytes(
        symbol_field(entries, "algorithm")?.clone(),
        bytes,
    ))
}

fn ensure_fields(entries: &[(Expr, Expr)], allowed: &[&str]) -> Result<()> {
    for (key, _) in entries {
        let Expr::Symbol(symbol) = key else {
            return Err(Error::TypeMismatch {
                expected: "symbol stream ref field",
                found: expr_kind(key),
            });
        };
        if symbol.namespace.is_none() && allowed.contains(&symbol.name.as_ref()) {
            continue;
        }
        return Err(Error::Eval(format!(
            "unknown stream ref field {}",
            symbol.as_qualified_str()
        )));
    }
    Ok(())
}

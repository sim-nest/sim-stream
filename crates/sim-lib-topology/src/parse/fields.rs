use sim_kernel::{Expr, Result, Symbol};

use super::util::{data_expr, expr_kind, normalize_key, parse_error};

pub(super) struct Field<'a> {
    name: String,
    value: &'a Expr,
}

pub(super) struct Fields<'a> {
    fields: Vec<Field<'a>>,
}

impl<'a> Fields<'a> {
    pub(super) fn from_map(entries: &'a [(Expr, Expr)], path: &str) -> Result<Self> {
        let mut fields = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            let name = match data_expr(key) {
                Expr::Symbol(symbol) => normalize_key(symbol.name.as_ref()),
                Expr::String(value) => normalize_key(value),
                other => {
                    return Err(parse_error(
                        path,
                        format!("expected symbol key, found {}", expr_kind(other)),
                    ));
                }
            };
            fields.push(Field {
                name,
                value: data_expr(value),
            });
        }
        Ok(Self { fields })
    }

    pub(super) fn from_keywords(items: &'a [Expr], path: &str) -> Result<Self> {
        if !items.len().is_multiple_of(2) {
            return Err(parse_error(path, "keyword list has an odd number of items"));
        }
        let mut fields = Vec::with_capacity(items.len() / 2);
        for pair in items.chunks_exact(2) {
            let key = data_expr(&pair[0]);
            let name = match key {
                Expr::Symbol(symbol) if symbol.name.starts_with(':') => {
                    normalize_key(symbol.name.as_ref())
                }
                Expr::String(value) if value.starts_with(':') => normalize_key(value),
                other => {
                    return Err(parse_error(
                        path,
                        format!("expected keyword key, found {}", expr_kind(other)),
                    ));
                }
            };
            fields.push(Field {
                name,
                value: data_expr(&pair[1]),
            });
        }
        Ok(Self { fields })
    }

    pub(super) fn push(&mut self, name: impl Into<String>, value: &'a Expr) {
        self.fields.push(Field {
            name: name.into(),
            value: data_expr(value),
        });
    }

    pub(super) fn get(&self, name: &str) -> Option<&'a Expr> {
        self.fields
            .iter()
            .rev()
            .find(|field| field.name == name)
            .map(|field| field.value)
    }

    pub(super) fn required(&self, name: &str, path: &str) -> Result<&'a Expr> {
        self.get(name)
            .ok_or_else(|| parse_error(path, "missing required field"))
    }

    pub(super) fn unknown_pairs(&self, known: &[&str]) -> Vec<(Symbol, Expr)> {
        self.fields
            .iter()
            .filter(|field| !known.contains(&field.name.as_str()))
            .map(|field| (Symbol::new(field.name.clone()), field.value.clone()))
            .collect()
    }

    pub(super) fn reject_unknown(&self, known: &[&str]) -> Result<()> {
        if let Some(field) = self
            .fields
            .iter()
            .find(|field| !known.contains(&field.name.as_str()))
        {
            Err(parse_error(
                field.name.as_str(),
                format!("unknown field {}", field.name),
            ))
        } else {
            Ok(())
        }
    }

    pub(super) fn into_pairs(self) -> Vec<(Symbol, Expr)> {
        self.fields
            .into_iter()
            .map(|field| (Symbol::new(field.name), field.value.clone()))
            .collect()
    }
}

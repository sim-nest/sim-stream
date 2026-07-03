use sim_kernel::{Error, Expr, NumberLiteral, Result, Symbol};

use crate::{Port, PortMode, PortRef};

#[derive(Clone)]
pub(super) struct Token {
    pub(super) text: String,
    pub(super) line: usize,
    pub(super) column: usize,
}

pub(super) struct KeyValue {
    pub(super) key: String,
    pub(super) value: Expr,
    pub(super) line: usize,
    pub(super) column: usize,
}

pub(super) fn tokenize_line(line: &str, line_no: usize) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut iter = line.char_indices().peekable();
    while let Some((start, ch)) = iter.next() {
        if ch.is_whitespace() {
            continue;
        }
        let column = start + 1;
        let mut text = String::new();
        let mut in_quote = false;
        let mut escaped = false;
        text.push(ch);
        update_quote_state(ch, &mut in_quote, &mut escaped);
        while let Some((_, next)) = iter.peek().copied() {
            if !in_quote && next.is_whitespace() {
                break;
            }
            iter.next();
            text.push(next);
            update_quote_state(next, &mut in_quote, &mut escaped);
        }
        if in_quote {
            return Err(text_error(line_no, column, "unterminated quoted string"));
        }
        tokens.push(Token {
            text,
            line: line_no,
            column,
        });
    }
    Ok(tokens)
}

pub(super) fn parse_key_values(tokens: &[Token]) -> Result<Vec<KeyValue>> {
    tokens
        .iter()
        .map(|token| {
            let Some((key, value)) = token.text.split_once('=') else {
                return Err(text_error(token.line, token.column, "expected key=value"));
            };
            if key.is_empty() {
                return Err(text_error(token.line, token.column, "missing key before ="));
            }
            if value.is_empty() {
                return Err(text_error(
                    token.line,
                    token.column + key.len() + 1,
                    "missing value after =",
                ));
            }
            Ok(KeyValue {
                key: normalize_key(key),
                value: parse_value(value, token.line, token.column + key.len() + 1)?,
                line: token.line,
                column: token.column,
            })
        })
        .collect()
}

pub(super) fn port_ref_from_text(token: &Token, default_port: &str) -> Result<PortRef> {
    let parts = token.text.split(':').collect::<Vec<_>>();
    match parts.as_slice() {
        [node] if !node.is_empty() => Ok(PortRef::new(
            symbol_from_text(node),
            Symbol::new(default_port.to_owned()),
        )),
        [node, port] if !node.is_empty() && !port.is_empty() => {
            Ok(PortRef::new(symbol_from_text(node), symbol_from_text(port)))
        }
        _ => Err(text_error(
            token.line,
            token.column,
            "expected port reference node or node:port",
        )),
    }
}

pub(super) fn ports_from_value(expr: &Expr, line: usize, column: usize) -> Result<Vec<Port>> {
    match expr {
        Expr::Symbol(symbol) => Ok(vec![Port::new(symbol.clone(), PortMode::Value, true)]),
        Expr::String(name) => Ok(vec![Port::value(name.clone(), true)]),
        Expr::List(items) => items
            .iter()
            .map(|item| {
                symbol_value(item, line, column)
                    .map(|symbol| Port::new(symbol, PortMode::Value, true))
            })
            .collect(),
        other => Err(text_error(
            line,
            column,
            format!("expected port symbol or list, found {other:?}"),
        )),
    }
}

pub(super) fn optional_value(expr: Expr) -> Option<Expr> {
    match expr {
        Expr::Nil => None,
        other => Some(other),
    }
}

pub(super) fn optional_symbol(expr: &Expr, line: usize, column: usize) -> Result<Option<Symbol>> {
    match expr {
        Expr::Nil => Ok(None),
        _ => symbol_value(expr, line, column).map(Some),
    }
}

pub(super) fn symbol_value(expr: &Expr, line: usize, column: usize) -> Result<Symbol> {
    match expr {
        Expr::Symbol(symbol) => Ok(symbol.clone()),
        Expr::String(text) => Ok(symbol_from_text(text)),
        other => Err(text_error(
            line,
            column,
            format!("expected symbol, found {other:?}"),
        )),
    }
}

pub(super) fn bool_value(expr: &Expr, line: usize, column: usize) -> Result<bool> {
    match expr {
        Expr::Bool(value) => Ok(*value),
        other => Err(text_error(
            line,
            column,
            format!("expected bool, found {other:?}"),
        )),
    }
}

pub(super) fn optional_u32(expr: &Expr, line: usize, column: usize) -> Result<Option<u32>> {
    match expr {
        Expr::Nil => Ok(None),
        _ => u32_value(expr, line, column).map(Some),
    }
}

pub(super) fn optional_u64(expr: &Expr, line: usize, column: usize) -> Result<Option<u64>> {
    match expr {
        Expr::Nil => Ok(None),
        _ => u64_value(expr, line, column).map(Some),
    }
}

pub(super) fn u32_value(expr: &Expr, line: usize, column: usize) -> Result<u32> {
    u64_value(expr, line, column).and_then(|value| {
        u32::try_from(value).map_err(|_| text_error(line, column, "integer out of range for u32"))
    })
}

pub(super) fn i64_value(expr: &Expr, line: usize, column: usize) -> Result<i64> {
    match expr {
        Expr::Number(number) => number
            .canonical
            .parse::<i64>()
            .map_err(|_| text_error(line, column, "expected integer")),
        other => Err(text_error(
            line,
            column,
            format!("expected integer, found {other:?}"),
        )),
    }
}

pub(super) fn symbol_from_text(text: &str) -> Symbol {
    if let Some((namespace, name)) = text.split_once('/')
        && !namespace.is_empty()
        && !name.is_empty()
    {
        return Symbol::qualified(namespace.to_owned(), name.to_owned());
    }
    if let Some((namespace, name)) = text.split_once(':')
        && !namespace.is_empty()
        && !name.is_empty()
    {
        return Symbol::qualified(namespace.to_owned(), name.to_owned());
    }
    Symbol::new(text.to_owned())
}

pub(super) fn number_expr(text: &str) -> Expr {
    Expr::Number(NumberLiteral {
        domain: Symbol::new("i64"),
        canonical: text.to_owned(),
    })
}

pub(super) fn text_error(line: usize, column: usize, message: impl Into<String>) -> Error {
    Error::Eval(format!(
        "topology text parse error at line {line}, column {column}: {}",
        message.into()
    ))
}

fn update_quote_state(ch: char, in_quote: &mut bool, escaped: &mut bool) {
    if *escaped {
        *escaped = false;
        return;
    }
    if *in_quote && ch == '\\' {
        *escaped = true;
    } else if ch == '"' {
        *in_quote = !*in_quote;
    }
}

fn parse_value(text: &str, line: usize, column: usize) -> Result<Expr> {
    if text.starts_with('"') {
        return unquote(text, line, column).map(Expr::String);
    }
    match text {
        "nil" => Ok(Expr::Nil),
        "true" => Ok(Expr::Bool(true)),
        "false" => Ok(Expr::Bool(false)),
        _ if text.starts_with('[') => parse_list_value(text, line, column),
        _ if is_integer(text) => Ok(number_expr(text)),
        _ => Ok(Expr::Symbol(symbol_from_text(text))),
    }
}

fn parse_list_value(text: &str, line: usize, column: usize) -> Result<Expr> {
    if !text.ends_with(']') {
        return Err(text_error(line, column, "unterminated list value"));
    }
    let inner = &text[1..text.len() - 1];
    if inner.trim().is_empty() {
        return Ok(Expr::List(Vec::new()));
    }
    inner
        .split(',')
        .enumerate()
        .map(|(index, item)| {
            let value = item.trim();
            if value.is_empty() {
                Err(text_error(line, column, "empty list item"))
            } else {
                parse_value(value, line, column + 1 + index)
            }
        })
        .collect::<Result<Vec<_>>>()
        .map(Expr::List)
}

fn unquote(text: &str, line: usize, column: usize) -> Result<String> {
    if !text.ends_with('"') || text.len() == 1 {
        return Err(text_error(line, column, "unterminated quoted string"));
    }
    let mut out = String::new();
    let mut chars = text[1..text.len() - 1].chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => {
                return Err(text_error(
                    line,
                    column,
                    format!("unsupported escape \\{other}"),
                ));
            }
            None => return Err(text_error(line, column, "trailing escape in string")),
        }
    }
    Ok(out)
}

fn u64_value(expr: &Expr, line: usize, column: usize) -> Result<u64> {
    match expr {
        Expr::Number(number) => number
            .canonical
            .parse::<u64>()
            .map_err(|_| text_error(line, column, "expected unsigned integer")),
        other => Err(text_error(
            line,
            column,
            format!("expected unsigned integer, found {other:?}"),
        )),
    }
}

fn is_integer(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())
}

fn normalize_key(name: &str) -> String {
    name.strip_prefix(':')
        .unwrap_or(name)
        .replace('-', "_")
        .to_owned()
}

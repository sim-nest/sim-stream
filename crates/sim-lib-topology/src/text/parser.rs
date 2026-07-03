use sim_kernel::{Expr, Result, Symbol};

use crate::{BudgetExhausted, Cell, Edge, EdgeId, Graph, GraphTest, Node};

use super::value::{
    Token, bool_value, i64_value, optional_symbol, optional_u32, optional_u64, optional_value,
    parse_key_values, port_ref_from_text, ports_from_value, symbol_from_text, symbol_value,
    text_error, tokenize_line, u32_value,
};

/// Parses topology text directly into an in-memory graph.
pub fn graph_from_text(source: &str) -> Result<Graph> {
    TextParser::new(source).parse()
}

struct TextParser<'a> {
    source: &'a str,
    graph: Option<Graph>,
}

impl<'a> TextParser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            graph: None,
        }
    }

    fn parse(mut self) -> Result<Graph> {
        for (index, line) in self.source.lines().enumerate() {
            let line_no = index + 1;
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let tokens = tokenize_line(line, line_no)?;
            if !tokens.is_empty() {
                self.parse_tokens(&tokens)?;
            }
        }
        self.graph
            .ok_or_else(|| text_error(1, 1, "missing topology line"))
    }

    fn parse_tokens(&mut self, tokens: &[Token]) -> Result<()> {
        match tokens[0].text.as_str() {
            "topology" => self.parse_topology(tokens),
            "node" => self.parse_node(tokens),
            "wire" => self.parse_wire(tokens),
            "cell" => self.parse_cell(tokens),
            "budget" => self.parse_budget(tokens),
            "meta" => self.parse_meta(tokens),
            "test" => self.parse_test(tokens),
            other => Err(text_error(
                tokens[0].line,
                tokens[0].column,
                format!("unknown topology text command {other}"),
            )),
        }
    }

    fn parse_topology(&mut self, tokens: &[Token]) -> Result<()> {
        expect_len(tokens, 2, "topology NAME")?;
        if self.graph.is_some() {
            return Err(text_error(
                tokens[0].line,
                tokens[0].column,
                "duplicate topology line",
            ));
        }
        self.graph = Some(Graph::new(symbol_from_text(&tokens[1].text)));
        Ok(())
    }

    fn parse_node(&mut self, tokens: &[Token]) -> Result<()> {
        expect_at_least(tokens, 2, "node NAME [key=value ...]")?;
        let pairs = parse_key_values(&tokens[2..])?;
        let id = symbol_from_text(&tokens[1].text);
        let verb = pairs
            .iter()
            .find(|pair| pair.key == "verb")
            .map(|pair| symbol_value(&pair.value, pair.line, pair.column))
            .transpose()?
            .unwrap_or_else(|| id.clone());
        let mut node = Node::new(id, verb);

        for pair in pairs {
            match pair.key.as_str() {
                "verb" => {}
                "target" => node.target = optional_value(pair.value),
                "role" => node.role = optional_symbol(&pair.value, pair.line, pair.column)?,
                "input" => node.input = optional_value(pair.value),
                "output" => node.output = optional_value(pair.value),
                "in" => node.inputs = ports_from_value(&pair.value, pair.line, pair.column)?,
                "out" => node.outputs = ports_from_value(&pair.value, pair.line, pair.column)?,
                _ => node.options.push((Symbol::new(pair.key), pair.value)),
            }
        }

        self.graph_mut(tokens[0].line, tokens[0].column)?
            .nodes
            .push(node);
        Ok(())
    }

    fn parse_wire(&mut self, tokens: &[Token]) -> Result<()> {
        expect_at_least(tokens, 4, "wire FROM -> TO [key=value ...]")?;
        if tokens[2].text != "->" {
            return Err(text_error(
                tokens[2].line,
                tokens[2].column,
                "expected -> in wire line",
            ));
        }
        let pairs = parse_key_values(&tokens[4..])?;
        let edge_id = self
            .graph_ref(tokens[0].line, tokens[0].column)?
            .edges
            .len() as u32;
        let mut edge = Edge::new(
            EdgeId::new(edge_id),
            port_ref_from_text(&tokens[1], "out")?,
            port_ref_from_text(&tokens[3], "in")?,
        );

        for pair in pairs {
            match pair.key.as_str() {
                "when" => edge.when = optional_value(pair.value),
                "transform" => edge.transform = optional_value(pair.value),
                "as" => edge.as_name = optional_symbol(&pair.value, pair.line, pair.column)?,
                "priority" => edge.priority = i64_value(&pair.value, pair.line, pair.column)?,
                "max_visits" => {
                    edge.max_visits = optional_u32(&pair.value, pair.line, pair.column)?
                }
                "buffer" => edge.buffer = optional_value(pair.value),
                _ => edge.metadata.push((Symbol::new(pair.key), pair.value)),
            }
        }

        self.graph_mut(tokens[0].line, tokens[0].column)?
            .edges
            .push(edge);
        Ok(())
    }

    fn parse_cell(&mut self, tokens: &[Token]) -> Result<()> {
        expect_at_least(tokens, 2, "cell NAME [key=value ...]")?;
        let name = symbol_from_text(&tokens[1].text);
        let mut cell = Cell::new(name, Expr::Nil);
        for pair in parse_key_values(&tokens[2..])? {
            match pair.key.as_str() {
                "shape" => cell.shape = optional_value(pair.value),
                "initial" => cell.initial = pair.value,
                "merge" => cell.merge = optional_symbol(&pair.value, pair.line, pair.column)?,
                "private" => cell.private = bool_value(&pair.value, pair.line, pair.column)?,
                other => {
                    return Err(text_error(
                        pair.line,
                        pair.column,
                        format!("unknown cell field {other}"),
                    ));
                }
            }
        }
        self.graph_mut(tokens[0].line, tokens[0].column)?
            .cells
            .push(cell);
        Ok(())
    }

    fn parse_budget(&mut self, tokens: &[Token]) -> Result<()> {
        for pair in parse_key_values(&tokens[1..])? {
            let budget = &mut self.graph_mut(tokens[0].line, tokens[0].column)?.budget;
            match pair.key.as_str() {
                "max_steps" => budget.max_steps = u32_value(&pair.value, pair.line, pair.column)?,
                "max_node_visits" => {
                    budget.max_node_visits = u32_value(&pair.value, pair.line, pair.column)?
                }
                "max_edge_visits" => {
                    budget.max_edge_visits = u32_value(&pair.value, pair.line, pair.column)?
                }
                "max_outputs" => {
                    budget.max_outputs = u32_value(&pair.value, pair.line, pair.column)?
                }
                "max_child_runs" => {
                    budget.max_child_runs = u32_value(&pair.value, pair.line, pair.column)?
                }
                "deadline_ms" => {
                    budget.deadline_ms = optional_u64(&pair.value, pair.line, pair.column)?
                }
                "on_exhausted" => {
                    budget.on_exhausted =
                        parse_budget_exhausted(&pair.value, pair.line, pair.column)?
                }
                other => {
                    return Err(text_error(
                        pair.line,
                        pair.column,
                        format!("unknown budget field {other}"),
                    ));
                }
            }
        }
        Ok(())
    }

    fn parse_meta(&mut self, tokens: &[Token]) -> Result<()> {
        expect_len(tokens, 2, "meta key=value")?;
        let mut pairs = parse_key_values(&tokens[1..])?;
        let pair = pairs.pop().expect("one pair was required");
        self.graph_mut(tokens[0].line, tokens[0].column)?
            .metadata
            .push((Symbol::new(pair.key), pair.value));
        Ok(())
    }

    fn parse_test(&mut self, tokens: &[Token]) -> Result<()> {
        expect_at_least(tokens, 4, "test NAME input=VALUE expect=VALUE")?;
        let name = symbol_from_text(&tokens[1].text);
        let mut input = None;
        let mut expect = None;
        for pair in parse_key_values(&tokens[2..])? {
            match pair.key.as_str() {
                "input" => input = Some(pair.value),
                "expect" => expect = Some(pair.value),
                other => {
                    return Err(text_error(
                        pair.line,
                        pair.column,
                        format!("unknown test field {other}"),
                    ));
                }
            }
        }
        let input = input
            .ok_or_else(|| text_error(tokens[1].line, tokens[1].column, "missing test input"))?;
        let expect = expect
            .ok_or_else(|| text_error(tokens[1].line, tokens[1].column, "missing test expect"))?;
        self.graph_mut(tokens[0].line, tokens[0].column)?
            .tests
            .push(GraphTest::new(name, input, expect));
        Ok(())
    }

    fn graph_ref(&self, line: usize, column: usize) -> Result<&Graph> {
        self.graph
            .as_ref()
            .ok_or_else(|| text_error(line, column, "topology line must appear first"))
    }

    fn graph_mut(&mut self, line: usize, column: usize) -> Result<&mut Graph> {
        self.graph
            .as_mut()
            .ok_or_else(|| text_error(line, column, "topology line must appear first"))
    }
}

fn parse_budget_exhausted(expr: &Expr, line: usize, column: usize) -> Result<BudgetExhausted> {
    match symbol_value(expr, line, column)?.name.as_ref() {
        "fail" => Ok(BudgetExhausted::Fail),
        "partial" => Ok(BudgetExhausted::Partial),
        other => Err(text_error(
            line,
            column,
            format!("expected fail or partial, found {other}"),
        )),
    }
}

fn expect_len(tokens: &[Token], expected: usize, usage: &str) -> Result<()> {
    if tokens.len() == expected {
        Ok(())
    } else {
        let token = tokens.get(expected).unwrap_or(&tokens[tokens.len() - 1]);
        Err(text_error(
            token.line,
            token.column,
            format!("expected {usage}"),
        ))
    }
}

fn expect_at_least(tokens: &[Token], expected: usize, usage: &str) -> Result<()> {
    if tokens.len() >= expected {
        Ok(())
    } else {
        let token = &tokens[tokens.len() - 1];
        Err(text_error(
            token.line,
            token.column + token.text.len(),
            format!("expected {usage}"),
        ))
    }
}

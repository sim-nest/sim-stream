//! Run-local topology state cells.

use std::collections::BTreeMap;

use sim_kernel::{Cx, Error, Expr, Result, Symbol};
use sim_shape::parse_shape_expr;

use crate::{Cell, Graph};

/// Mutable state cells for one topology run.
#[derive(Clone, Debug)]
pub struct TopologyCells {
    specs: BTreeMap<Symbol, Cell>,
    values: BTreeMap<Symbol, Expr>,
}

impl TopologyCells {
    /// Creates run-local cells from graph declarations.
    pub fn new(graph: &Graph) -> Result<Self> {
        let mut specs = BTreeMap::new();
        let mut values = BTreeMap::new();
        for cell in &graph.cells {
            if specs.insert(cell.name.clone(), cell.clone()).is_some() {
                return Err(Error::Eval(format!(
                    "topology run: duplicate cell {}",
                    cell.name
                )));
            }
            values.insert(cell.name.clone(), cell.initial.clone());
        }
        Ok(Self { specs, values })
    }

    /// Reads a cell value.
    pub fn read(&self, name: &Symbol) -> Result<Expr> {
        self.values
            .get(name)
            .cloned()
            .ok_or_else(|| Error::Eval(format!("topology run: unknown cell {name}")))
    }

    /// Replaces a cell value.
    pub fn write(&mut self, cx: &mut Cx, name: &Symbol, value: Expr) -> Result<Expr> {
        self.check_shape(cx, name, &value)?;
        self.values.insert(name.clone(), value.clone());
        Ok(value)
    }

    /// Appends one value to a cell.
    pub fn append(&mut self, cx: &mut Cx, name: &Symbol, value: Expr) -> Result<Expr> {
        let next = append_value(self.read(name)?, value);
        self.write(cx, name, next)
    }

    /// Merges one value into a cell using its merge strategy.
    pub fn merge(&mut self, cx: &mut Cx, name: &Symbol, value: Expr) -> Result<Expr> {
        let strategy = self
            .spec(name)?
            .merge
            .as_ref()
            .map(|symbol| symbol.name.to_string())
            .unwrap_or_else(|| "last".to_owned());
        let next = match strategy.as_str() {
            "first" => self.read(name)?,
            "last" => value,
            "append" => append_value(self.read(name)?, value),
            other => {
                return Err(Error::Eval(format!(
                    "topology run: unsupported cell merge strategy {other}"
                )));
            }
        };
        self.write(cx, name, next)
    }

    /// Clears a cell to nil.
    pub fn clear(&mut self, cx: &mut Cx, name: &Symbol) -> Result<Expr> {
        self.write(cx, name, Expr::Nil)
    }

    fn spec(&self, name: &Symbol) -> Result<&Cell> {
        self.specs
            .get(name)
            .ok_or_else(|| Error::Eval(format!("topology run: unknown cell {name}")))
    }

    fn check_shape(&self, cx: &mut Cx, name: &Symbol, value: &Expr) -> Result<()> {
        let Some(shape_expr) = self.spec(name)?.shape.as_ref() else {
            return Ok(());
        };
        let shape = parse_shape_expr(shape_expr)?;
        let matched = shape.check_expr(cx, value)?;
        if matched.accepted {
            Ok(())
        } else {
            Err(Error::Eval(format!(
                "topology run: value rejected by shape for cell {name}"
            )))
        }
    }
}

fn append_value(current: Expr, value: Expr) -> Expr {
    match current {
        Expr::Nil => Expr::List(vec![value]),
        Expr::List(mut items) => {
            items.push(value);
            Expr::List(items)
        }
        Expr::Vector(mut items) => {
            items.push(value);
            Expr::Vector(items)
        }
        other => Expr::List(vec![other, value]),
    }
}

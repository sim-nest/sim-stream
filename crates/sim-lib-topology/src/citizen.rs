//! Citizen descriptors that present topology packages, nodes, and edges as
//! runtime objects with stable class symbols.

use sim_citizen_derive::Citizen;
use sim_kernel::{Error, Expr, Result, Symbol};
use sim_value::build::entry;

/// Citizen object wrapping a validated `.simtopo` package source string.
#[derive(Clone, Debug, PartialEq, Citizen)]
#[citizen(symbol = "topology/Package", version = 1)]
pub struct TopologyPackageDescriptor {
    #[citizen(with = "package_source")]
    source: String,
}

/// Citizen object wrapping a validated topology node expression.
#[derive(Clone, Debug, PartialEq, Citizen)]
#[citizen(symbol = "topology/Node", version = 1)]
pub struct TopologyNodeDescriptor {
    #[citizen(with = "node_expr")]
    node: Expr,
}

/// Citizen object wrapping a validated topology edge expression.
#[derive(Clone, Debug, PartialEq, Citizen)]
#[citizen(symbol = "topology/Edge", version = 1)]
pub struct TopologyEdgeDescriptor {
    #[citizen(with = "edge_expr")]
    edge: Expr,
}

impl TopologyPackageDescriptor {
    /// Builds a descriptor, validating the package source.
    pub fn new(source: impl Into<String>) -> Result<Self> {
        let source = source.into();
        package_source::decode(&Expr::String(source.clone()))?;
        Ok(Self { source })
    }

    /// Returns the package source string.
    pub fn source(&self) -> &str {
        &self.source
    }
}

impl Default for TopologyPackageDescriptor {
    fn default() -> Self {
        Self::new(
            r#"
graph:
topology citizen-package
node in verb=in
node out verb=out
wire in -> out
"#,
        )
        .expect("default topology package descriptor should be valid")
    }
}

impl TopologyNodeDescriptor {
    /// Builds a descriptor, validating the node expression.
    pub fn from_expr(node: Expr) -> Result<Self> {
        node_expr::decode(&node)?;
        Ok(Self { node })
    }

    /// Returns the node expression.
    pub fn as_expr(&self) -> &Expr {
        &self.node
    }
}

impl Default for TopologyNodeDescriptor {
    fn default() -> Self {
        Self::from_expr(Expr::Map(vec![
            entry("id", Expr::Symbol(Symbol::new("citizen-node"))),
            entry("verb", Expr::Symbol(Symbol::new("wire"))),
        ]))
        .expect("default topology node descriptor should be valid")
    }
}

impl TopologyEdgeDescriptor {
    /// Builds a descriptor, validating the edge expression.
    pub fn from_expr(edge: Expr) -> Result<Self> {
        edge_expr::decode(&edge)?;
        Ok(Self { edge })
    }

    /// Returns the edge expression.
    pub fn as_expr(&self) -> &Expr {
        &self.edge
    }
}

impl Default for TopologyEdgeDescriptor {
    fn default() -> Self {
        Self::from_expr(Expr::Map(vec![
            entry("from", Expr::Symbol(Symbol::new("citizen-node/out"))),
            entry("to", Expr::Symbol(Symbol::new("citizen-out/in"))),
        ]))
        .expect("default topology edge descriptor should be valid")
    }
}

/// Returns the class symbol for topology package descriptors.
pub fn topology_package_class_symbol() -> Symbol {
    Symbol::qualified("topology", "Package")
}

/// Returns the class symbol for topology node descriptors.
pub fn topology_node_class_symbol() -> Symbol {
    Symbol::qualified("topology", "Node")
}

/// Returns the class symbol for topology edge descriptors.
pub fn topology_edge_class_symbol() -> Symbol {
    Symbol::qualified("topology", "Edge")
}

pub(crate) mod package_source {
    use sim_kernel::{Error, Expr, Result};

    use crate::parse_package;

    pub fn encode(source: &str) -> sim_kernel::Expr {
        sim_kernel::Expr::String(source.to_owned())
    }

    pub fn decode(expr: &Expr) -> Result<String> {
        let Expr::String(source) = expr else {
            return Err(Error::Eval(
                "topology package descriptor source must be a string".to_owned(),
            ));
        };
        parse_package(source)?;
        Ok(source.clone())
    }
}

pub(crate) mod node_expr {
    use sim_kernel::{Error, Expr, Result};

    use super::has_required_field;

    pub fn encode(expr: &Expr) -> Expr {
        expr.clone()
    }

    pub fn decode(expr: &Expr) -> Result<Expr> {
        let Expr::Map(entries) = expr else {
            return Err(Error::Eval(
                "topology node descriptor must be a map".to_owned(),
            ));
        };
        has_required_field(entries, "id", "topology node descriptor")?;
        has_required_field(entries, "verb", "topology node descriptor")?;
        Ok(expr.clone())
    }
}

pub(crate) mod edge_expr {
    use sim_kernel::{Error, Expr, Result};

    use super::has_required_field;

    pub fn encode(expr: &Expr) -> Expr {
        expr.clone()
    }

    pub fn decode(expr: &Expr) -> Result<Expr> {
        let Expr::Map(entries) = expr else {
            return Err(Error::Eval(
                "topology edge descriptor must be a map".to_owned(),
            ));
        };
        has_required_field(entries, "from", "topology edge descriptor")?;
        has_required_field(entries, "to", "topology edge descriptor")?;
        Ok(expr.clone())
    }
}

fn has_required_field(entries: &[(Expr, Expr)], name: &str, context: &str) -> Result<()> {
    if entries
        .iter()
        .any(|(key, _)| field_name(key).as_deref() == Some(name))
    {
        return Ok(());
    }
    Err(Error::Eval(format!("{context} is missing {name}")))
}

fn field_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Symbol(symbol) if symbol.namespace.is_none() => Some(symbol.name.to_string()),
        Expr::String(value) => Some(value.clone()),
        _ => None,
    }
}

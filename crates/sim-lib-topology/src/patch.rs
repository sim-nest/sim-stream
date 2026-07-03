//! Topology patch parsing and clone-apply support.

use sim_kernel::{Cx, Error, Expr, Result, Symbol};

use crate::{
    Budget, Cell, Edge, EdgeId, Graph, Node, NodeId, PortRef, Scheduler, TopologyConnection,
    capability::topology_write_capability, compile_graph, site::connection_from_graph,
};

mod data;

/// A deterministic sequence of topology patch operations.
#[derive(Clone, Debug)]
pub struct TopologyPatch {
    /// Patch operations in application order.
    pub ops: Vec<PatchOp>,
}

impl TopologyPatch {
    /// Parses patch data from the public Lisp operation forms.
    pub fn from_expr(expr: &Expr) -> Result<Self> {
        let ops = data::parse_patch_ops(expr)?;
        if ops.is_empty() {
            return Err(patch_error("patch requires at least one operation"));
        }
        Ok(Self { ops })
    }

    /// Converts patch data back to the public Lisp operation forms.
    pub fn to_expr(&self) -> Expr {
        data::patch_ops_to_expr(&self.ops)
    }
}

/// One topology patch operation.
#[derive(Clone, Debug)]
pub enum PatchOp {
    /// Add a graph node.
    AddNode(Node),
    /// Remove a graph node by id.
    RemoveNode(NodeId),
    /// Replace an existing graph node.
    ReplaceNode {
        /// The node id to replace.
        id: NodeId,
        /// The replacement node.
        node: Node,
    },
    /// Add a graph edge.
    AddEdge {
        /// The edge to add.
        edge: Edge,
        /// Whether the edge id was given explicitly rather than assigned.
        explicit_id: bool,
    },
    /// Remove an edge identified by source and destination endpoints.
    RemoveEdge {
        /// The source endpoint.
        from: PortRef,
        /// The destination endpoint.
        to: PortRef,
    },
    /// Replace an edge identified by source and destination endpoints.
    ReplaceEdge {
        /// The source endpoint of the edge to replace.
        from: PortRef,
        /// The destination endpoint of the edge to replace.
        to: PortRef,
        /// The replacement edge.
        edge: Edge,
        /// Whether the edge id was given explicitly rather than assigned.
        explicit_id: bool,
    },
    /// Add a graph state cell.
    AddCell(Cell),
    /// Replace graph budget settings.
    SetBudget(Budget),
    /// Replace graph scheduler settings.
    SetScheduler(Scheduler),
    /// Set or insert one graph metadata entry.
    SetMetadata {
        /// The metadata key.
        key: Symbol,
        /// The metadata value.
        value: Expr,
    },
}

/// Applies a patch to a clone, validates, compiles, and returns the new graph.
pub fn apply_topology_patch(cx: &mut Cx, source: &Graph, patch: &TopologyPatch) -> Result<Graph> {
    cx.require(&topology_write_capability())?;
    let graph = apply_topology_patch_ops(source, patch)?;
    compile_graph(cx, &graph)?;
    Ok(graph)
}

/// Applies a patch's operations to a copy of `source` without compiling or
/// validating the result. This is the editing surface for tools that build a
/// topology incrementally (for example the Web-UI composer), where intermediate
/// graphs are legitimately incomplete; validation runs later at save or run.
/// Capability gating is the caller's responsibility.
///
/// # Examples
///
/// ```rust
/// use sim_kernel::{Expr, Symbol};
/// use sim_lib_topology::{PatchOp, TopologyPatch, apply_topology_patch_ops, parse_package};
///
/// let package = parse_package(
///     "graph:\ntopology flow\nnode in verb=in\nnode out verb=out\nwire in -> out\n",
/// )
/// .unwrap();
///
/// let patch = TopologyPatch {
///     ops: vec![PatchOp::SetMetadata {
///         key: Symbol::new("note"),
///         value: Expr::String("edited".to_owned()),
///     }],
/// };
/// let edited = apply_topology_patch_ops(&package.graph, &patch).unwrap();
///
/// assert!(
///     edited
///         .metadata
///         .iter()
///         .any(|(key, _)| key == &Symbol::new("note"))
/// );
/// ```
pub fn apply_topology_patch_ops(source: &Graph, patch: &TopologyPatch) -> Result<Graph> {
    let mut graph = source.clone();
    for op in &patch.ops {
        apply_op(&mut graph, op)?;
    }
    Ok(graph)
}

/// Applies a patch and returns a new runnable topology connection.
pub fn patched_connection(
    cx: &mut Cx,
    source: &Graph,
    patch: &TopologyPatch,
) -> Result<TopologyConnection> {
    let graph = apply_topology_patch(cx, source, patch)?;
    connection_from_graph(cx, &graph)
}

fn apply_op(graph: &mut Graph, op: &PatchOp) -> Result<()> {
    match op {
        PatchOp::AddNode(node) => graph.nodes.push(node.clone()),
        PatchOp::RemoveNode(id) => remove_node(graph, id)?,
        PatchOp::ReplaceNode { id, node } => replace_node(graph, id, node)?,
        PatchOp::AddEdge { edge, explicit_id } => add_edge(graph, edge, *explicit_id),
        PatchOp::RemoveEdge { from, to } => remove_edge(graph, from, to)?,
        PatchOp::ReplaceEdge {
            from,
            to,
            edge,
            explicit_id,
        } => replace_edge(graph, from, to, edge, *explicit_id)?,
        PatchOp::AddCell(cell) => graph.cells.push(cell.clone()),
        PatchOp::SetBudget(budget) => graph.budget = budget.clone(),
        PatchOp::SetScheduler(scheduler) => graph.scheduler = scheduler.clone(),
        PatchOp::SetMetadata { key, value } => set_metadata(graph, key, value.clone()),
    }
    Ok(())
}

fn remove_node(graph: &mut Graph, id: &NodeId) -> Result<()> {
    let before = graph.nodes.len();
    graph.nodes.retain(|node| &node.id != id);
    if graph.nodes.len() == before {
        return Err(patch_error(format!(
            "remove-node target {} does not exist",
            id.as_symbol()
        )));
    }
    Ok(())
}

fn replace_node(graph: &mut Graph, id: &NodeId, node: &Node) -> Result<()> {
    if &node.id != id {
        return Err(patch_error(format!(
            "replace-node replacement id {} does not match target {}",
            node.id.as_symbol(),
            id.as_symbol()
        )));
    }
    let Some(slot) = graph.nodes.iter_mut().find(|existing| &existing.id == id) else {
        return Err(patch_error(format!(
            "replace-node target {} does not exist",
            id.as_symbol()
        )));
    };
    *slot = node.clone();
    Ok(())
}

fn add_edge(graph: &mut Graph, edge: &Edge, explicit_id: bool) {
    let mut edge = edge.clone();
    if !explicit_id {
        edge.id = next_edge_id(graph);
    }
    graph.edges.push(edge);
}

fn remove_edge(graph: &mut Graph, from: &PortRef, to: &PortRef) -> Result<()> {
    let before = graph.edges.len();
    graph
        .edges
        .retain(|edge| &edge.from != from || &edge.to != to);
    if graph.edges.len() == before {
        return Err(patch_error("remove-edge target does not exist"));
    }
    Ok(())
}

fn replace_edge(
    graph: &mut Graph,
    from: &PortRef,
    to: &PortRef,
    edge: &Edge,
    explicit_id: bool,
) -> Result<()> {
    let Some(slot) = graph
        .edges
        .iter_mut()
        .find(|existing| &existing.from == from && &existing.to == to)
    else {
        return Err(patch_error("replace-edge target does not exist"));
    };
    let mut replacement = edge.clone();
    if !explicit_id {
        replacement.id = slot.id;
    }
    *slot = replacement;
    Ok(())
}

fn set_metadata(graph: &mut Graph, key: &Symbol, value: Expr) {
    if let Some((_, existing)) = graph.metadata.iter_mut().find(|(name, _)| name == key) {
        *existing = value;
    } else {
        graph.metadata.push((key.clone(), value));
    }
}

fn next_edge_id(graph: &Graph) -> EdgeId {
    EdgeId::new(
        graph
            .edges
            .iter()
            .map(|edge| edge.id.0)
            .max()
            .unwrap_or(0)
            .saturating_add(1),
    )
}

fn patch_error(message: impl Into<String>) -> Error {
    Error::Eval(format!("topology patch error: {}", message.into()))
}

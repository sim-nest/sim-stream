//! Public topology graph model: graphs, nodes, edges, ports, cells, budgets.
//!
//! These are the authoring value types that topology data compiles from.

use sim_kernel::{Expr, Symbol};

/// Canonical topology data API tag used by serialized graphs.
pub const TOPOLOGY_API: &str = "sim.topology.v3";

/// Default graph version for newly constructed in-memory graphs.
pub const DEFAULT_GRAPH_VERSION: &str = "0.1.0";

/// Default maximum scheduler steps for a graph run.
pub const DEFAULT_MAX_STEPS: u32 = 256;

/// Default maximum visits to any node during a graph run.
pub const DEFAULT_MAX_NODE_VISITS: u32 = 64;

/// Default maximum traversals of any edge during a graph run.
pub const DEFAULT_MAX_EDGE_VISITS: u32 = 64;

/// Stable node identifier inside a topology graph.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub Symbol);

impl NodeId {
    /// Creates a node id from an unqualified symbol name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(Symbol::new(name.into()))
    }

    /// Returns the underlying symbol.
    pub fn as_symbol(&self) -> &Symbol {
        &self.0
    }
}

impl From<Symbol> for NodeId {
    fn from(value: Symbol) -> Self {
        Self(value)
    }
}

impl From<&str> for NodeId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for NodeId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Stable edge identifier inside a compiled or parsed topology graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeId(pub u32);

impl EdgeId {
    /// Creates an edge id from a stable zero-based index.
    pub const fn new(index: u32) -> Self {
        Self(index)
    }
}

impl From<u32> for EdgeId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// Reference to a named port on a node.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PortRef {
    /// The node that owns the referenced port.
    pub node: NodeId,
    /// The referenced port name.
    pub port: Symbol,
}

impl PortRef {
    /// Creates a port reference from an explicit node and port symbol.
    pub fn new(node: impl Into<NodeId>, port: Symbol) -> Self {
        Self {
            node: node.into(),
            port,
        }
    }

    /// Creates a port reference from an unqualified port name.
    pub fn named(node: impl Into<NodeId>, port: impl Into<String>) -> Self {
        Self::new(node, Symbol::new(port.into()))
    }

    /// Creates a destination reference using the default `in` port.
    pub fn input(node: impl Into<NodeId>) -> Self {
        Self::named(node, "in")
    }

    /// Creates a source reference using the default `out` port.
    pub fn output(node: impl Into<NodeId>) -> Self {
        Self::named(node, "out")
    }
}

/// Canonical in-memory representation of topology graph data.
#[derive(Clone, Debug)]
pub struct Graph {
    /// Graph name.
    pub name: Symbol,
    /// Graph artifact version.
    pub version: String,
    /// Canonical topology data API tag.
    pub api: String,
    /// Optional public input shape expression.
    pub input: Option<Expr>,
    /// Optional public output shape expression.
    pub output: Option<Expr>,
    /// Graph nodes in deterministic declaration order.
    pub nodes: Vec<Node>,
    /// Graph edges in deterministic declaration order.
    pub edges: Vec<Edge>,
    /// Graph state cells.
    pub cells: Vec<Cell>,
    /// Run scheduling policy.
    pub scheduler: Scheduler,
    /// Run budget limits.
    pub budget: Budget,
    /// Capabilities required by this graph.
    pub capabilities: Vec<Symbol>,
    /// Extra graph metadata.
    pub metadata: Vec<(Symbol, Expr)>,
    /// Embedded graph tests.
    pub tests: Vec<GraphTest>,
}

impl Graph {
    /// Creates an empty graph with deterministic defaults.
    pub fn new(name: Symbol) -> Self {
        Self {
            name,
            version: DEFAULT_GRAPH_VERSION.to_owned(),
            api: TOPOLOGY_API.to_owned(),
            input: None,
            output: None,
            nodes: Vec::new(),
            edges: Vec::new(),
            cells: Vec::new(),
            scheduler: Scheduler::default(),
            budget: Budget::default(),
            capabilities: Vec::new(),
            metadata: Vec::new(),
            tests: Vec::new(),
        }
    }

    /// Creates a minimal graph from an unqualified symbol name.
    pub fn minimal(name: impl Into<String>) -> Self {
        Self::new(Symbol::new(name.into()))
    }

    /// Compatibility constructor for earlier scaffold tests.
    pub fn placeholder() -> Self {
        Self::minimal("topology")
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::minimal("topology")
    }
}

/// A topology graph step with named input and output ports.
#[derive(Clone, Debug)]
pub struct Node {
    /// Node id unique within the graph.
    pub id: NodeId,
    /// Generic node verb, such as `in`, `out`, `call`, or `wire`.
    pub verb: Symbol,
    /// Canonical `in` ports.
    pub inputs: Vec<Port>,
    /// Canonical `out` ports.
    pub outputs: Vec<Port>,
    /// Optional runtime target value expression.
    pub target: Option<Expr>,
    /// Optional role tag used by frame-aware targets.
    pub role: Option<Symbol>,
    /// Optional node input shape expression.
    pub input: Option<Expr>,
    /// Optional node output shape expression.
    pub output: Option<Expr>,
    /// Node options preserved from graph data.
    pub options: Vec<(Symbol, Expr)>,
}

impl Node {
    /// Creates a node with default ports for the provided verb.
    pub fn new(id: impl Into<NodeId>, verb: Symbol) -> Self {
        let (inputs, outputs) = default_ports_for_verb(&verb);
        Self {
            id: id.into(),
            verb,
            inputs,
            outputs,
            target: None,
            role: None,
            input: None,
            output: None,
            options: Vec::new(),
        }
    }

    /// Creates a node from an unqualified verb name.
    pub fn named(id: impl Into<NodeId>, verb: impl Into<String>) -> Self {
        Self::new(id, Symbol::new(verb.into()))
    }

    /// Creates a node with explicitly declared ports.
    pub fn with_ports(
        id: impl Into<NodeId>,
        verb: Symbol,
        inputs: Vec<Port>,
        outputs: Vec<Port>,
    ) -> Self {
        Self {
            id: id.into(),
            verb,
            inputs,
            outputs,
            target: None,
            role: None,
            input: None,
            output: None,
            options: Vec::new(),
        }
    }
}

/// A named input or output port on a topology node.
#[derive(Clone, Debug)]
pub struct Port {
    /// Port name.
    pub name: Symbol,
    /// Optional port shape expression.
    pub shape: Option<Expr>,
    /// Whether packets are values or streams.
    pub mode: PortMode,
    /// Whether this port must be connected or provided.
    pub required: bool,
}

impl Port {
    /// Creates a port from an explicit symbol.
    pub fn new(name: Symbol, mode: PortMode, required: bool) -> Self {
        Self {
            name,
            shape: None,
            mode,
            required,
        }
    }

    /// Creates a value port from an unqualified symbol name.
    pub fn value(name: impl Into<String>, required: bool) -> Self {
        Self::new(Symbol::new(name.into()), PortMode::Value, required)
    }

    /// Creates a stream port from an unqualified symbol name.
    pub fn stream(name: impl Into<String>, required: bool) -> Self {
        Self::new(Symbol::new(name.into()), PortMode::Stream, required)
    }
}

/// Port packet mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PortMode {
    /// The port carries one value packet at a time.
    #[default]
    Value,
    /// The port carries stream packets or stream handles.
    Stream,
}

/// A directed route from one node port to another node port.
#[derive(Clone, Debug)]
pub struct Edge {
    /// Stable edge id.
    pub id: EdgeId,
    /// Source node port. Omitted source ports normalize to `out`.
    pub from: PortRef,
    /// Destination node port. Omitted destination ports normalize to `in`.
    pub to: PortRef,
    /// Optional edge predicate expression.
    pub when: Option<Expr>,
    /// Optional transform expression applied while routing.
    pub transform: Option<Expr>,
    /// Optional wrapper key for the routed value.
    pub as_name: Option<Symbol>,
    /// Deterministic routing priority.
    pub priority: i64,
    /// Optional per-edge visit cap.
    pub max_visits: Option<u32>,
    /// Optional buffer policy expression.
    pub buffer: Option<Expr>,
    /// Extra edge metadata.
    pub metadata: Vec<(Symbol, Expr)>,
}

impl Edge {
    /// Creates an edge with deterministic defaults.
    pub fn new(id: impl Into<EdgeId>, from: PortRef, to: PortRef) -> Self {
        Self {
            id: id.into(),
            from,
            to,
            when: None,
            transform: None,
            as_name: None,
            priority: 0,
            max_visits: None,
            buffer: None,
            metadata: Vec::new(),
        }
    }
}

/// State cell available to nodes during a topology run.
#[derive(Clone, Debug)]
pub struct Cell {
    /// Cell name.
    pub name: Symbol,
    /// Optional cell shape expression.
    pub shape: Option<Expr>,
    /// Initial cell value expression.
    pub initial: Expr,
    /// Optional merge strategy symbol.
    pub merge: Option<Symbol>,
    /// Whether reflection should redact this cell by default.
    pub private: bool,
}

impl Cell {
    /// Creates a public cell with no shape or merge strategy.
    pub fn new(name: Symbol, initial: Expr) -> Self {
        Self {
            name,
            shape: None,
            initial,
            merge: None,
            private: false,
        }
    }
}

/// Deterministic scheduler settings for a topology graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scheduler {
    /// Scheduler mode.
    pub mode: SchedulerMode,
    /// Optional deterministic seed.
    pub seed: Option<u64>,
    /// Maximum number of concurrent node runs.
    pub max_concurrency: u32,
    /// Whether scheduling must preserve deterministic replay.
    pub deterministic: bool,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            mode: SchedulerMode::Sequential,
            seed: None,
            max_concurrency: 1,
            deterministic: true,
        }
    }
}

/// Scheduler strategy for graph execution.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SchedulerMode {
    /// Runs nodes in deterministic sequential order.
    #[default]
    Sequential,
}

/// Bounded execution budget for one topology run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Budget {
    /// Maximum scheduler steps.
    pub max_steps: u32,
    /// Maximum visits to any node.
    pub max_node_visits: u32,
    /// Maximum traversals of any edge.
    pub max_edge_visits: u32,
    /// Maximum emitted outputs.
    pub max_outputs: u32,
    /// Maximum nested child topology runs.
    pub max_child_runs: u32,
    /// Optional wall-clock deadline in milliseconds.
    pub deadline_ms: Option<u64>,
    /// Exhaustion policy.
    pub on_exhausted: BudgetExhausted,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_steps: DEFAULT_MAX_STEPS,
            max_node_visits: DEFAULT_MAX_NODE_VISITS,
            max_edge_visits: DEFAULT_MAX_EDGE_VISITS,
            max_outputs: 64,
            max_child_runs: 16,
            deadline_ms: None,
            on_exhausted: BudgetExhausted::Fail,
        }
    }
}

/// Budget exhaustion behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BudgetExhausted {
    /// Fails the graph run when a budget is exhausted.
    #[default]
    Fail,
    /// Returns the outputs produced before exhaustion.
    Partial,
}

/// Embedded graph-level test case.
#[derive(Clone, Debug)]
pub struct GraphTest {
    /// Test case name.
    pub name: Symbol,
    /// Input expression for the graph.
    pub input: Expr,
    /// Expected output expression or shape expression.
    pub expect: Expr,
    /// Fixture values available while running the test.
    pub fixtures: Vec<(Symbol, Expr)>,
}

impl GraphTest {
    /// Creates a test case without fixtures.
    pub fn new(name: Symbol, input: Expr, expect: Expr) -> Self {
        Self {
            name,
            input,
            expect,
            fixtures: Vec::new(),
        }
    }
}

fn default_ports_for_verb(verb: &Symbol) -> (Vec<Port>, Vec<Port>) {
    match verb.name.as_ref() {
        "in" => (Vec::new(), vec![Port::value("out", true)]),
        "out" => (vec![Port::value("in", true)], Vec::new()),
        "call" => (
            vec![Port::value("in", true)],
            vec![Port::value("out", true), Port::value("error", false)],
        ),
        "branch" => (
            vec![Port::value("in", true)],
            vec![
                Port::value("true", false),
                Port::value("false", false),
                Port::value("else", false),
            ],
        ),
        "merge" => (
            vec![Port::value("in", true)],
            vec![Port::value("out", true)],
        ),
        "tee" => (
            vec![Port::value("in", true)],
            vec![Port::value("out", true)],
        ),
        _ => (
            vec![Port::value("in", true)],
            vec![Port::value("out", true)],
        ),
    }
}

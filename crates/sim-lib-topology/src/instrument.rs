//! Instrument-style topology specs (modules and cords) lowered to graph data.

use sim_kernel::{Expr, Symbol};

use crate::{Edge, Graph, Node, NodeId, Port, PortMode, PortRef};

/// Instrument-patch topology spec: a modular-synth style description of modules
/// wired by cords, lowered to graph data by [`InstrumentTopologyAdapter`].
#[derive(Clone, Debug)]
pub struct InstrumentTopologySpec {
    /// The topology name.
    pub name: Symbol,
    /// The modules (lowered to nodes).
    pub modules: Vec<InstrumentTopologyModule>,
    /// The cords wiring module jacks (lowered to edges).
    pub cords: Vec<InstrumentTopologyCord>,
    /// Graph-level metadata key/value pairs.
    pub metadata: Vec<(Symbol, Expr)>,
}

impl InstrumentTopologySpec {
    /// Starts an empty spec with the given name.
    pub fn new(name: Symbol) -> Self {
        Self {
            name,
            modules: Vec::new(),
            cords: Vec::new(),
            metadata: Vec::new(),
        }
    }

    /// Adds a module, returning the updated spec.
    pub fn with_module(mut self, module: InstrumentTopologyModule) -> Self {
        self.modules.push(module);
        self
    }

    /// Adds a cord, returning the updated spec.
    pub fn with_cord(mut self, cord: InstrumentTopologyCord) -> Self {
        self.cords.push(cord);
        self
    }

    /// Adds a graph-level metadata entry, returning the updated spec.
    pub fn with_metadata(mut self, key: Symbol, value: Expr) -> Self {
        self.metadata.push((key, value));
        self
    }
}

/// One module in an instrument spec: a node kind with input/output jacks,
/// settings, and an opaque raw view.
#[derive(Clone, Debug)]
pub struct InstrumentTopologyModule {
    /// The module's node id.
    pub id: NodeId,
    /// The module kind (becomes the node verb/class symbol).
    pub kind: Symbol,
    /// Input jacks (lowered to input ports).
    pub inputs: Vec<InstrumentTopologyJack>,
    /// Output jacks (lowered to output ports).
    pub outputs: Vec<InstrumentTopologyJack>,
    /// Module settings, lowered to a `settings` node option map.
    pub settings: Vec<(Symbol, Expr)>,
    /// Opaque module state, lowered to a `raw-view` node option map.
    pub raw_view: Vec<(Symbol, Expr)>,
}

impl InstrumentTopologyModule {
    /// Starts a module with the given id and kind.
    pub fn new(id: impl Into<NodeId>, kind: Symbol) -> Self {
        Self {
            id: id.into(),
            kind,
            inputs: Vec::new(),
            outputs: Vec::new(),
            settings: Vec::new(),
            raw_view: Vec::new(),
        }
    }

    /// Adds an input jack, returning the updated module.
    pub fn with_input(mut self, jack: InstrumentTopologyJack) -> Self {
        self.inputs.push(jack);
        self
    }

    /// Adds an output jack, returning the updated module.
    pub fn with_output(mut self, jack: InstrumentTopologyJack) -> Self {
        self.outputs.push(jack);
        self
    }

    /// Adds a setting, returning the updated module.
    pub fn with_setting(mut self, key: Symbol, value: Expr) -> Self {
        self.settings.push((key, value));
        self
    }

    /// Adds a raw-view entry, returning the updated module.
    pub fn with_raw(mut self, key: Symbol, value: Expr) -> Self {
        self.raw_view.push((key, value));
        self
    }
}

/// One jack on a module: a named port with a mode, a required flag, and an
/// optional normalled default supplied when the jack is left unpatched.
#[derive(Clone, Debug)]
pub struct InstrumentTopologyJack {
    /// The jack (port) name.
    pub name: Symbol,
    /// Whether the jack carries values or a stream.
    pub mode: PortMode,
    /// Whether the jack must be wired (unless a normalled default applies).
    pub required: bool,
    /// Value supplied when the jack is left unpatched, if any.
    pub normalled_default: Option<Expr>,
}

impl InstrumentTopologyJack {
    /// Builds a value-mode jack.
    pub fn value(name: impl Into<String>, required: bool) -> Self {
        Self::new(Symbol::new(name.into()), PortMode::Value, required)
    }

    /// Builds a stream-mode jack.
    pub fn stream(name: impl Into<String>, required: bool) -> Self {
        Self::new(Symbol::new(name.into()), PortMode::Stream, required)
    }

    /// Builds a jack with an explicit mode.
    pub fn new(name: Symbol, mode: PortMode, required: bool) -> Self {
        Self {
            name,
            mode,
            required,
            normalled_default: None,
        }
    }

    /// Sets the normalled default value, returning the updated jack.
    pub fn with_normalled_default(mut self, value: Expr) -> Self {
        self.normalled_default = Some(value);
        self
    }
}

/// A patch cord wiring one jack to another, with an optional visit bound.
#[derive(Clone, Debug)]
pub struct InstrumentTopologyCord {
    /// The source port reference.
    pub from: PortRef,
    /// The destination port reference.
    pub to: PortRef,
    /// Maximum times the edge may be traversed in a bounded cycle, if limited.
    pub max_visits: Option<u32>,
}

impl InstrumentTopologyCord {
    /// Builds a cord between two ports.
    pub fn new(from: PortRef, to: PortRef) -> Self {
        Self {
            from,
            to,
            max_visits: None,
        }
    }

    /// Sets the cord's maximum visit count, returning the updated cord.
    pub fn with_max_visits(mut self, max_visits: u32) -> Self {
        self.max_visits = Some(max_visits);
        self
    }
}

/// Lowers an [`InstrumentTopologySpec`] into the canonical [`Graph`] model.
#[derive(Clone, Copy, Debug, Default)]
pub struct InstrumentTopologyAdapter;

impl InstrumentTopologyAdapter {
    /// Lowers a spec to a graph: modules become nodes and cords become edges.
    pub fn graph_from_spec(&self, spec: &InstrumentTopologySpec) -> Graph {
        let mut graph = Graph::new(spec.name.clone());
        graph.metadata = spec.metadata.clone();
        graph.metadata.push((
            Symbol::qualified("topology", "adapter"),
            Expr::Symbol(Symbol::qualified("topology/adapter", "instrument-patch")),
        ));
        graph.nodes = spec.modules.iter().map(module_to_node).collect();
        graph.edges = spec
            .cords
            .iter()
            .enumerate()
            .map(|(index, cord)| cord_to_edge(index as u32, cord))
            .collect();
        graph
    }
}

fn module_to_node(module: &InstrumentTopologyModule) -> Node {
    let mut node = Node::with_ports(
        module.id.clone(),
        module.kind.clone(),
        module.inputs.iter().map(jack_to_port).collect(),
        module.outputs.iter().map(jack_to_port).collect(),
    );
    if !module.settings.is_empty() {
        node.options.push((
            Symbol::new("settings"),
            Expr::Map(symbol_expr_entries(&module.settings)),
        ));
    }
    if !module.raw_view.is_empty() {
        node.options.push((
            Symbol::new("raw-view"),
            Expr::Map(symbol_expr_entries(&module.raw_view)),
        ));
    }
    let normalled = normalled_defaults(&module.inputs, &module.outputs);
    if !normalled.is_empty() {
        node.options
            .push((Symbol::new("normalled-defaults"), Expr::Map(normalled)));
    }
    node
}

fn jack_to_port(jack: &InstrumentTopologyJack) -> Port {
    Port::new(
        jack.name.clone(),
        jack.mode,
        jack.required && jack.normalled_default.is_none(),
    )
}

fn cord_to_edge(index: u32, cord: &InstrumentTopologyCord) -> Edge {
    let mut edge = Edge::new(index, cord.from.clone(), cord.to.clone());
    edge.max_visits = cord.max_visits;
    edge
}

fn normalled_defaults(
    inputs: &[InstrumentTopologyJack],
    outputs: &[InstrumentTopologyJack],
) -> Vec<(Expr, Expr)> {
    inputs
        .iter()
        .chain(outputs)
        .filter_map(|jack| {
            jack.normalled_default
                .as_ref()
                .map(|value| (Expr::Symbol(jack.name.clone()), value.clone()))
        })
        .collect()
}

fn symbol_expr_entries(entries: &[(Symbol, Expr)]) -> Vec<(Expr, Expr)> {
    entries
        .iter()
        .map(|(key, value)| (Expr::Symbol(key.clone()), value.clone()))
        .collect()
}

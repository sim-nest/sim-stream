//! Canonical topology continuation data and codec.

use std::collections::BTreeMap;

use sim_kernel::{Error, Expr, Result, Symbol, Value};

use crate::{
    Budget, BudgetExhausted, CompiledGraph, Graph, Node,
    run::{BudgetLedger, TopologyEvent, TopologyEventKind, WorkItem},
};

/// Result of advancing exactly one queued scheduler item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopologyProgress {
    /// One item ran and more work remains.
    Advanced,
    /// One item ran and emitted one or more public outputs.
    Output(Vec<Expr>),
    /// No queued work remains and no public output was ever produced.
    Exhausted,
    /// No queued work remains and the run has produced output.
    Complete,
}

/// Stable identity for a runtime node binding. It is data, never authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyBindingDescriptor {
    /// Caller-defined stable binding identity.
    pub identity: Symbol,
    /// Source node role captured with the binding.
    pub role: Option<Symbol>,
    /// Source node static options captured in declaration order.
    pub options: Vec<(Symbol, Expr)>,
}

impl TopologyBindingDescriptor {
    /// Captures a stable identity and the source node's generic call context.
    pub fn for_node(identity: impl Into<String>, node: &Node) -> Self {
        Self {
            identity: Symbol::new(identity.into()),
            role: node.role.clone(),
            options: node.options.clone(),
        }
    }
}

/// Runtime bindings supplied by a caller for this process.
#[derive(Clone, Default)]
pub struct TopologyBindings {
    pub(crate) entries: BTreeMap<crate::NodeId, (TopologyBindingDescriptor, Value)>,
}

impl TopologyBindings {
    /// Creates an empty binding set.
    pub fn new() -> Self {
        Self::default()
    }
    /// Adds or replaces a node binding.
    pub fn bind(
        &mut self,
        node: impl Into<crate::NodeId>,
        descriptor: TopologyBindingDescriptor,
        value: Value,
    ) {
        self.entries.insert(node.into(), (descriptor, value));
    }
    pub(crate) fn get(&self, node: &crate::NodeId) -> Option<&(TopologyBindingDescriptor, Value)> {
        self.entries.get(node)
    }
    pub(crate) fn descriptors(&self) -> BTreeMap<crate::NodeId, TopologyBindingDescriptor> {
        self.entries
            .iter()
            .map(|(k, (d, _))| (k.clone(), d.clone()))
            .collect()
    }
}
pub(crate) fn validate_budget_policy(budget: &Budget) -> Result<()> {
    if budget.deadline_ms.is_some() {
        return Err(Error::Eval(
            "topology run: deadline_ms budget policy is unsupported".to_owned(),
        ));
    }
    if budget.on_exhausted == BudgetExhausted::Partial {
        return Err(Error::Eval(
            "topology run: partial exhaustion policy is unsupported".to_owned(),
        ));
    }
    Ok(())
}

/// Canonical, authority-free snapshot of a topology run between work items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyContinuation {
    pub(crate) fingerprint: String,
    pub(crate) queue: Vec<WorkItem>,
    pub(crate) outputs: Vec<Expr>,
    pub(crate) cells: BTreeMap<Symbol, Expr>,
    pub(crate) nonlinear: Expr,
    pub(crate) budget: BudgetLedger,
    pub(crate) events: Vec<TopologyEvent>,
    pub(crate) bindings: BTreeMap<crate::NodeId, TopologyBindingDescriptor>,
}

impl TopologyContinuation {
    /// Stable source graph and compiled-plan fingerprint.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    /// Encodes the sealed, unredacted continuation as canonical expression data.
    pub fn to_expr(&self) -> Expr {
        self.encode(false, &[])
    }
    /// Encodes a reflected view, redacting values of private graph cells.
    pub fn reflected_expr(&self, graph: &Graph) -> Expr {
        let private: Vec<_> = graph
            .cells
            .iter()
            .filter(|c| c.private)
            .map(|c| c.name.clone())
            .collect();
        self.encode(true, &private)
    }
    fn encode(&self, reflected: bool, private: &[Symbol]) -> Expr {
        Expr::Map(vec![
            kv("kind", Expr::Symbol(Symbol::new("topology-continuation"))),
            kv("version", Expr::String("1".into())),
            kv("fingerprint", Expr::String(self.fingerprint.clone())),
            kv(
                "queue",
                Expr::List(self.queue.iter().map(work_expr).collect()),
            ),
            kv("outputs", Expr::List(self.outputs.clone())),
            kv(
                "cells",
                Expr::Map(
                    self.cells
                        .iter()
                        .map(|(k, v)| {
                            (
                                Expr::Symbol(k.clone()),
                                if reflected && private.contains(k) {
                                    Expr::Symbol(Symbol::new("redacted"))
                                } else {
                                    v.clone()
                                },
                            )
                        })
                        .collect(),
                ),
            ),
            kv("nonlinear", self.nonlinear.clone()),
            kv("budget", budget_expr(&self.budget)),
            kv(
                "events",
                Expr::List(self.events.iter().map(event_expr).collect()),
            ),
            kv(
                "bindings",
                Expr::List(
                    self.bindings
                        .iter()
                        .map(|(node, d)| {
                            Expr::List(vec![
                                Expr::Symbol(node.as_symbol().clone()),
                                Expr::Symbol(d.identity.clone()),
                                d.role.clone().map(Expr::Symbol).unwrap_or(Expr::Nil),
                                Expr::Map(
                                    d.options
                                        .iter()
                                        .map(|(k, v)| (Expr::Symbol(k.clone()), v.clone()))
                                        .collect(),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
    /// Decodes and structurally checks canonical continuation data.
    pub fn from_expr(expr: &Expr) -> Result<Self> {
        let m = expr_map(expr)?;
        expect_symbol(field(m, "kind")?, "topology-continuation")?;
        expect_string(field(m, "version")?, "1")?;
        let fingerprint = string(field(m, "fingerprint")?)?.to_owned();
        let queue = expr_list(field(m, "queue")?)?
            .iter()
            .map(parse_work)
            .collect::<Result<_>>()?;
        let outputs = expr_list(field(m, "outputs")?)?.to_vec();
        let cells = expr_map(field(m, "cells")?)?
            .iter()
            .map(|(k, v)| match k {
                Expr::Symbol(s) => Ok((s.clone(), v.clone())),
                _ => Err(Error::Eval(
                    "topology continuation: cell key must be symbol".into(),
                )),
            })
            .collect::<Result<_>>()?;
        let nonlinear = field(m, "nonlinear")?.clone();
        let budget = parse_budget(field(m, "budget")?)?;
        let events = expr_list(field(m, "events")?)?
            .iter()
            .map(parse_event)
            .collect::<Result<_>>()?;
        let bindings = expr_list(field(m, "bindings")?)?
            .iter()
            .map(|row| {
                let x = expr_list(row)?;
                if x.len() != 4 {
                    return Err(Error::Eval(
                        "topology continuation: binding row arity".into(),
                    ));
                };
                Ok((
                    crate::NodeId(symbol(&x[0])?.clone()),
                    TopologyBindingDescriptor {
                        identity: symbol(&x[1])?.clone(),
                        role: match &x[2] {
                            Expr::Nil => None,
                            Expr::Symbol(role) => Some(role.clone()),
                            _ => {
                                return Err(Error::Eval(
                                    "topology continuation: binding role".into(),
                                ));
                            }
                        },
                        options: expr_map(&x[3])?
                            .iter()
                            .map(|(k, v)| match k {
                                Expr::Symbol(k) => Ok((k.clone(), v.clone())),
                                _ => Err(Error::Eval(
                                    "topology continuation: binding option key".into(),
                                )),
                            })
                            .collect::<Result<_>>()?,
                    },
                ))
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            fingerprint,
            queue,
            outputs,
            cells,
            nonlinear,
            budget,
            events,
            bindings,
        })
    }
}
pub(crate) fn topology_fingerprint(graph: &Graph, plan: &CompiledGraph) -> String {
    let source = format!("{:?}|{:?}", crate::text::graph_to_expr(graph), plan);
    let mut h = 0xcbf29ce484222325u64;
    for byte in source.bytes() {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x100000001b3)
    }
    format!("topology-fnv1a64:{h:016x}")
}
fn kv(k: &str, v: Expr) -> (Expr, Expr) {
    (Expr::Symbol(Symbol::new(k)), v)
}
fn work_expr(w: &WorkItem) -> Expr {
    Expr::List(vec![
        Expr::String(w.node_index.to_string()),
        Expr::Symbol(w.port.clone()),
        w.expr.clone(),
    ])
}
fn parse_work(e: &Expr) -> Result<WorkItem> {
    let x = expr_list(e)?;
    if x.len() != 3 {
        return Err(Error::Eval("topology continuation: work item arity".into()));
    }
    Ok(WorkItem {
        node_index: usize_string(&x[0])?,
        port: symbol(&x[1])?.clone(),
        expr: x[2].clone(),
    })
}
fn budget_expr(b: &BudgetLedger) -> Expr {
    Expr::Map(vec![
        kv(
            "limits",
            Expr::List(vec![
                Expr::String(b.limits.max_steps.to_string()),
                Expr::String(b.limits.max_node_visits.to_string()),
                Expr::String(b.limits.max_edge_visits.to_string()),
                Expr::String(b.limits.max_outputs.to_string()),
                Expr::String(b.limits.max_child_runs.to_string()),
            ]),
        ),
        kv("steps", Expr::String(b.steps.to_string())),
        kv(
            "nodes",
            Expr::List(
                b.node_visits
                    .iter()
                    .map(|v| Expr::String(v.to_string()))
                    .collect(),
            ),
        ),
        kv(
            "edges",
            Expr::List(
                b.edge_visits
                    .iter()
                    .map(|v| Expr::String(v.to_string()))
                    .collect(),
            ),
        ),
        kv("outputs", Expr::String(b.outputs.to_string())),
        kv("children", Expr::String(b.child_runs.to_string())),
    ])
}
fn parse_budget(e: &Expr) -> Result<BudgetLedger> {
    let m = expr_map(e)?;
    let l = expr_list(field(m, "limits")?)?;
    if l.len() != 5 {
        return Err(Error::Eval(
            "topology continuation: budget limits arity".into(),
        ));
    }
    let limits = Budget {
        max_steps: u32_string(&l[0])?,
        max_node_visits: u32_string(&l[1])?,
        max_edge_visits: u32_string(&l[2])?,
        max_outputs: u32_string(&l[3])?,
        max_child_runs: u32_string(&l[4])?,
        ..Budget::default()
    };
    Ok(BudgetLedger {
        limits,
        steps: u32_string(field(m, "steps")?)?,
        node_visits: expr_list(field(m, "nodes")?)?
            .iter()
            .map(u32_string)
            .collect::<Result<_>>()?,
        edge_visits: expr_list(field(m, "edges")?)?
            .iter()
            .map(u32_string)
            .collect::<Result<_>>()?,
        outputs: u32_string(field(m, "outputs")?)?,
        child_runs: u32_string(field(m, "children")?)?,
    })
}
fn event_expr(e: &TopologyEvent) -> Expr {
    Expr::List(vec![
        Expr::Symbol(Symbol::new(match e.kind {
            TopologyEventKind::Enqueued => "enqueued",
            TopologyEventKind::NodeStarted => "node-started",
            TopologyEventKind::PortEmitted => "port-emitted",
            TopologyEventKind::EdgeRouted => "edge-routed",
            TopologyEventKind::OutputEmitted => "output-emitted",
        })),
        Expr::String(e.node_index.to_string()),
        e.port.clone().map(Expr::Symbol).unwrap_or(Expr::Nil),
        e.edge_index
            .map(|v| Expr::String(v.to_string()))
            .unwrap_or(Expr::Nil),
        e.expr.clone().unwrap_or(Expr::Nil),
    ])
}
fn parse_event(e: &Expr) -> Result<TopologyEvent> {
    let x = expr_list(e)?;
    if x.len() != 5 {
        return Err(Error::Eval("topology continuation: event arity".into()));
    }
    let kind = match symbol(&x[0])?.name.as_ref() {
        "enqueued" => TopologyEventKind::Enqueued,
        "node-started" => TopologyEventKind::NodeStarted,
        "port-emitted" => TopologyEventKind::PortEmitted,
        "edge-routed" => TopologyEventKind::EdgeRouted,
        "output-emitted" => TopologyEventKind::OutputEmitted,
        _ => return Err(Error::Eval("topology continuation: event kind".into())),
    };
    Ok(TopologyEvent {
        kind,
        node_index: usize_string(&x[1])?,
        port: match &x[2] {
            Expr::Nil => None,
            Expr::Symbol(s) => Some(s.clone()),
            _ => return Err(Error::Eval("topology continuation: event port".into())),
        },
        edge_index: match &x[3] {
            Expr::Nil => None,
            v => Some(usize_string(v)?),
        },
        expr: match &x[4] {
            Expr::Nil => None,
            v => Some(v.clone()),
        },
    })
}
fn expr_map(e: &Expr) -> Result<&[(Expr, Expr)]> {
    if let Expr::Map(v) = e {
        Ok(v)
    } else {
        Err(Error::Eval("topology continuation: expected map".into()))
    }
}
fn expr_list(e: &Expr) -> Result<&[Expr]> {
    if let Expr::List(v) = e {
        Ok(v)
    } else {
        Err(Error::Eval("topology continuation: expected list".into()))
    }
}
fn field<'b>(m: &'b [(Expr, Expr)], k: &str) -> Result<&'b Expr> {
    m.iter()
        .find_map(|(key, v)| {
            matches!(key,Expr::Symbol(s) if s.namespace.is_none()&&s.name.as_ref()==k).then_some(v)
        })
        .ok_or_else(|| Error::Eval(format!("topology continuation: missing {k}")))
}
fn symbol(e: &Expr) -> Result<&Symbol> {
    if let Expr::Symbol(v) = e {
        Ok(v)
    } else {
        Err(Error::Eval("topology continuation: expected symbol".into()))
    }
}
fn string(e: &Expr) -> Result<&str> {
    if let Expr::String(v) = e {
        Ok(v)
    } else {
        Err(Error::Eval("topology continuation: expected string".into()))
    }
}
fn expect_symbol(e: &Expr, w: &str) -> Result<()> {
    if symbol(e)?.name.as_ref() == w {
        Ok(())
    } else {
        Err(Error::Eval("topology continuation: wrong kind".into()))
    }
}
fn expect_string(e: &Expr, w: &str) -> Result<()> {
    if string(e)? == w {
        Ok(())
    } else {
        Err(Error::Eval("topology continuation: wrong version".into()))
    }
}
fn u32_string(e: &Expr) -> Result<u32> {
    string(e)?
        .parse()
        .map_err(|_| Error::Eval("topology continuation: expected u32".into()))
}
fn usize_string(e: &Expr) -> Result<usize> {
    string(e)?
        .parse()
        .map_err(|_| Error::Eval("topology continuation: expected usize".into()))
}

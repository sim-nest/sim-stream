//! Built-in topology verb execution.

use sim_kernel::{Cx, Error, Expr, Result, Symbol};

use crate::{
    CompiledGraph, Graph,
    adapter::{call_target_expr, resolve_target},
    run::{
        BudgetLedger, TopologyCells, TopologyNonlinearState, TopologyPacket, WorkItem,
        predicate_accepts,
    },
};

/// Result of executing one core topology node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerbAction {
    /// Emit a packet from a named output port.
    Emit(TopologyPacket),
    /// Complete the graph with one public output expression.
    Complete {
        /// Index of the completing node.
        node_index: usize,
        /// The public output expression.
        expr: Expr,
    },
}

/// Runs one built-in topology verb.
pub fn run_core_node(
    cx: &mut Cx,
    graph: &Graph,
    plan: &CompiledGraph,
    budget: &mut BudgetLedger,
    cells: &mut TopologyCells,
    nonlinear: &mut TopologyNonlinearState,
    item: &WorkItem,
) -> Result<Vec<VerbAction>> {
    let node = &graph.nodes[plan.nodes[item.node_index].source_index];
    match node.verb.name.as_ref() {
        "in" => emit(item.node_index, "out", item.expr.clone()),
        "out" => Ok(vec![VerbAction::Complete {
            node_index: item.node_index,
            expr: item.expr.clone(),
        }]),
        "wire" => emit(item.node_index, "out", item.expr.clone()),
        "tee" => emit(item.node_index, "out", item.expr.clone()),
        "branch" => {
            let accepted = match &item.expr {
                Expr::Bool(value) => *value,
                _ => {
                    let predicate = node_option(node, "when").ok_or_else(|| {
                        Error::Eval(format!(
                            "topology run: branch node {} requires a when predicate for non-bool input",
                            node.id.as_symbol()
                        ))
                    })?;
                    predicate_accepts(cx, predicate, &item.expr)?
                }
            };
            let desired = if accepted { "true" } else { "false" };
            let port = if has_route(plan, item.node_index, desired) {
                desired
            } else if has_route(plan, item.node_index, "else") {
                "else"
            } else {
                desired
            };
            emit(item.node_index, port, item.expr.clone())
        }
        "cell" => run_cell_node(cx, cells, node, item),
        "merge" => run_merge_node(plan, budget, nonlinear, node, item),
        "race" => run_race_node(cx, nonlinear, node, item),
        "quorum" => run_quorum_node(cx, nonlinear, node, item),
        "reduce" => run_reduce_node(cx, plan, nonlinear, node, item),
        "patch" => run_patch_node(cx, node, item),
        "call" => {
            let target = node.target.as_ref().ok_or_else(|| {
                Error::Eval(format!(
                    "topology run: call node {} has no target",
                    node.id.as_symbol()
                ))
            })?;
            let target = resolve_target(cx, target)?;
            if target.object().as_eval_fabric().is_some() {
                budget.record_child_run()?;
            }
            let output = call_target_expr(cx, target, item.expr.clone())?;
            emit(item.node_index, "out", output)
        }
        other => Err(Error::Eval(format!(
            "topology run: unsupported core verb {other}"
        ))),
    }
}

fn run_merge_node(
    plan: &CompiledGraph,
    budget: &BudgetLedger,
    nonlinear: &mut TopologyNonlinearState,
    node: &crate::Node,
    item: &WorkItem,
) -> Result<Vec<VerbAction>> {
    let mode = option_symbol(node, "mode")?.unwrap_or_else(|| Symbol::new("all"));
    match mode.name.as_ref() {
        "any" => {
            if nonlinear.mark_merge_any_complete(item.node_index) {
                emit(item.node_index, "out", item.expr.clone())
            } else {
                Ok(Vec::new())
            }
        }
        "latest" => {
            let output =
                nonlinear.update_latest(item.node_index, item.port.clone(), item.expr.clone());
            emit(item.node_index, "out", output)
        }
        "all" => {
            let buffered =
                nonlinear.push_merge(item.node_index, item.port.clone(), item.expr.clone());
            budget.check_merge_buffer(buffered)?;
            let required = incoming_count(plan, item.node_index);
            if buffered >= required {
                emit(
                    item.node_index,
                    "out",
                    Expr::List(nonlinear.drain_merge(item.node_index, required)),
                )
            } else {
                Ok(Vec::new())
            }
        }
        "count" => {
            let required = option_u32(node, "count")?.unwrap_or(1).max(1) as usize;
            let buffered =
                nonlinear.push_merge(item.node_index, item.port.clone(), item.expr.clone());
            budget.check_merge_buffer(buffered)?;
            if buffered >= required {
                emit(
                    item.node_index,
                    "out",
                    Expr::List(nonlinear.drain_merge(item.node_index, required)),
                )
            } else {
                Ok(Vec::new())
            }
        }
        other => Err(Error::Eval(format!(
            "topology run: unsupported merge mode {other}"
        ))),
    }
}

fn run_race_node(
    cx: &mut Cx,
    nonlinear: &mut TopologyNonlinearState,
    node: &crate::Node,
    item: &WorkItem,
) -> Result<Vec<VerbAction>> {
    let accepted = match node_option(node, "accept") {
        Some(predicate) => predicate_accepts(cx, predicate, &item.expr)?,
        None => true,
    };
    if accepted && nonlinear.mark_race_complete(item.node_index) {
        emit(item.node_index, "out", item.expr.clone())
    } else {
        Ok(Vec::new())
    }
}

fn run_quorum_node(
    cx: &mut Cx,
    nonlinear: &mut TopologyNonlinearState,
    node: &crate::Node,
    item: &WorkItem,
) -> Result<Vec<VerbAction>> {
    let required = option_u32(node, "n")?.unwrap_or(2).max(1);
    let key = match option_target(node, "key")? {
        Some(target) => {
            let target = resolve_target(cx, &target)?;
            call_target_expr(cx, target, item.expr.clone())?
        }
        None => item.expr.clone(),
    };
    let (value, count) = nonlinear.record_quorum(item.node_index, key, item.expr.clone());
    if count >= required && nonlinear.mark_quorum_complete(item.node_index) {
        emit(item.node_index, "out", value)
    } else {
        Ok(Vec::new())
    }
}

fn run_reduce_node(
    cx: &mut Cx,
    plan: &CompiledGraph,
    nonlinear: &mut TopologyNonlinearState,
    node: &crate::Node,
    item: &WorkItem,
) -> Result<Vec<VerbAction>> {
    let initial = node_option(node, "initial").cloned().unwrap_or(Expr::Nil);
    let current = nonlinear.reduce_current(item.node_index, initial);
    let next = match option_target(node, "target")? {
        Some(target) => {
            let target = resolve_target(cx, &target)?;
            call_target_expr(cx, target, Expr::List(vec![current, item.expr.clone()]))?
        }
        None => match current {
            Expr::Nil => item.expr.clone(),
            other => Expr::List(vec![other, item.expr.clone()]),
        },
    };
    let count = nonlinear.record_reduce(item.node_index, next.clone());
    let emit_partials = option_bool(node, "emit_partials")?.unwrap_or(false);
    if emit_partials || count >= incoming_count(plan, item.node_index) {
        if !emit_partials {
            nonlinear.reset_reduce(item.node_index);
        }
        emit(item.node_index, "out", next)
    } else {
        Ok(Vec::new())
    }
}

fn run_cell_node(
    cx: &mut Cx,
    cells: &mut TopologyCells,
    node: &crate::Node,
    item: &WorkItem,
) -> Result<Vec<VerbAction>> {
    let name = option_symbol(node, "name")?.ok_or_else(|| {
        Error::Eval(format!(
            "topology run: cell node {} requires name",
            node.id.as_symbol()
        ))
    })?;
    let op = option_symbol(node, "op")?.unwrap_or_else(|| Symbol::new("read"));
    let cell_value = match op.name.as_ref() {
        "read" => cells.read(&name)?,
        "write" => cells.write(cx, &name, item.expr.clone())?,
        "append" => cells.append(cx, &name, item.expr.clone())?,
        "merge" => cells.merge(cx, &name, item.expr.clone())?,
        "clear" => cells.clear(cx, &name)?,
        other => {
            return Err(Error::Eval(format!(
                "topology run: unsupported cell op {other}"
            )));
        }
    };
    let default_emit = if op.name.as_ref() == "read" {
        Symbol::new("cell")
    } else {
        Symbol::new("input")
    };
    let emit_mode = option_symbol(node, "emit")?.unwrap_or(default_emit);
    let output = match emit_mode.name.as_ref() {
        "input" => item.expr.clone(),
        "cell" => cell_value,
        "both" => Expr::Map(vec![
            (Expr::Symbol(Symbol::new("input")), item.expr.clone()),
            (Expr::Symbol(Symbol::new("cell")), cell_value),
        ]),
        other => {
            return Err(Error::Eval(format!(
                "topology run: unsupported cell emit mode {other}"
            )));
        }
    };
    emit(item.node_index, "out", output)
}

fn run_patch_node(cx: &mut Cx, node: &crate::Node, item: &WorkItem) -> Result<Vec<VerbAction>> {
    let mode = option_symbol(node, "mode")?.unwrap_or_else(|| Symbol::new("produce"));
    match mode.name.as_ref() {
        "produce" => {
            let proposal = match option_target(node, "target")? {
                Some(target) => {
                    let target = resolve_target(cx, &target)?;
                    call_target_expr(cx, target, item.expr.clone())?
                }
                None => node_option(node, "patch")
                    .cloned()
                    .unwrap_or_else(|| item.expr.clone()),
            };
            crate::TopologyPatch::from_expr(&proposal)?;
            emit(item.node_index, "out", proposal)
        }
        "apply" => Err(Error::Eval(
            "topology run: patch apply must use topology/patch".to_owned(),
        )),
        other => Err(Error::Eval(format!(
            "topology run: unsupported patch mode {other}"
        ))),
    }
}

fn emit(node_index: usize, port: &str, expr: Expr) -> Result<Vec<VerbAction>> {
    Ok(vec![VerbAction::Emit(TopologyPacket {
        node_index,
        port: Symbol::new(port),
        expr,
    })])
}

fn has_route(plan: &CompiledGraph, node_index: usize, port: &str) -> bool {
    plan.outgoing_edges[node_index]
        .iter()
        .any(|edge_index| plan.edges[*edge_index].from.port.name.as_ref() == port)
}

fn incoming_count(plan: &CompiledGraph, node_index: usize) -> usize {
    plan.incoming_edges[node_index].len().max(1)
}

fn node_option<'a>(node: &'a crate::Node, key: &str) -> Option<&'a Expr> {
    node.options
        .iter()
        .find(|(name, _)| name.namespace.is_none() && name.name.as_ref() == key)
        .map(|(_, value)| value)
}

fn option_symbol(node: &crate::Node, key: &str) -> Result<Option<Symbol>> {
    let Some(value) = node_option(node, key) else {
        return Ok(None);
    };
    match value {
        Expr::Symbol(symbol) => Ok(Some(symbol.clone())),
        Expr::String(text) => Ok(Some(Symbol::new(text.clone()))),
        other => Err(Error::Eval(format!(
            "topology run: node option {key} expects symbol or string, got {other:?}"
        ))),
    }
}

fn option_target(node: &crate::Node, key: &str) -> Result<Option<Expr>> {
    match node_option(node, key) {
        Some(value) => Ok(Some(value.clone())),
        None if key == "target" => Ok(node.target.clone()),
        None => Ok(None),
    }
}

fn option_bool(node: &crate::Node, key: &str) -> Result<Option<bool>> {
    let Some(value) = node_option(node, key) else {
        return Ok(None);
    };
    match value {
        Expr::Bool(value) => Ok(Some(*value)),
        Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == "true" => {
            Ok(Some(true))
        }
        Expr::Symbol(symbol) if symbol.namespace.is_none() && symbol.name.as_ref() == "false" => {
            Ok(Some(false))
        }
        other => Err(Error::Eval(format!(
            "topology run: node option {key} expects bool, got {other:?}"
        ))),
    }
}

fn option_u32(node: &crate::Node, key: &str) -> Result<Option<u32>> {
    let Some(value) = node_option(node, key) else {
        return Ok(None);
    };
    match value {
        Expr::Number(number) => number.canonical.parse::<u32>().map(Some).map_err(|_| {
            Error::Eval(format!(
                "topology run: node option {key} expects u32, got {}",
                number.canonical
            ))
        }),
        Expr::String(text) => text.parse::<u32>().map(Some).map_err(|_| {
            Error::Eval(format!(
                "topology run: node option {key} expects u32, got {text}"
            ))
        }),
        other => Err(Error::Eval(format!(
            "topology run: node option {key} expects u32, got {other:?}"
        ))),
    }
}

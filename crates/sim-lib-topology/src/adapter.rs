//! Adapter registry and protocol adapters for topology targets.

use std::sync::Arc;

use sim_kernel::{Cx, Error, Expr, Result, Symbol, Value};

mod builtin;

/// Resolves a graph target expression to a runtime value.
pub fn resolve_target(cx: &mut Cx, target: &Expr) -> Result<Value> {
    if let Expr::Symbol(symbol) = target
        && let Ok(codec) = cx.resolve_codec(symbol)
    {
        return Ok(codec);
    }
    cx.eval_expr(target.clone())
}

/// Calls a topology target with one input expression and returns an expression.
pub fn call_target_expr(cx: &mut Cx, target: Value, input: Expr) -> Result<Expr> {
    TopologyAdapterRegistry::core().call(cx, &target, input, None)
}

/// Adapter contract for a runtime object that can be a topology target.
pub trait TopologyAdapter: Send + Sync {
    /// Stable adapter name.
    fn name(&self) -> Symbol;

    /// Returns true when this adapter can call the target value.
    fn accepts(&self, cx: &mut Cx, target: &Value) -> bool;

    /// Calls the target with one topology packet expression.
    fn call(&self, cx: &mut Cx, target: &Value, input: Expr, role: Option<&Symbol>)
    -> Result<Expr>;
}

/// Deterministic adapter registry.
#[derive(Clone, Default)]
pub struct TopologyAdapterRegistry {
    adapters: Vec<Arc<dyn TopologyAdapter>>,
}

impl TopologyAdapterRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates the default registry of kernel-level topology adapters.
    pub fn core() -> Self {
        let mut registry = Self::new();
        for adapter in builtin::core_adapters() {
            registry.register(adapter).unwrap();
        }
        registry
    }

    /// Registers one adapter at the end of the deterministic dispatch order.
    pub fn register(&mut self, adapter: Arc<dyn TopologyAdapter>) -> Result<()> {
        let name = adapter.name();
        if self.adapters.iter().any(|existing| existing.name() == name) {
            return Err(Error::Eval(format!(
                "topology adapter {name} is already registered"
            )));
        }
        self.adapters.push(adapter);
        Ok(())
    }

    /// Returns adapter names in dispatch order.
    pub fn names(&self) -> Vec<Symbol> {
        self.adapters.iter().map(|adapter| adapter.name()).collect()
    }

    /// Resolves the first adapter that accepts a target.
    pub fn resolve(&self, cx: &mut Cx, target: &Value) -> Option<&dyn TopologyAdapter> {
        self.adapters
            .iter()
            .map(Arc::as_ref)
            .find(|adapter| adapter.accepts(cx, target))
    }

    /// Calls the first adapter that accepts a target.
    pub fn call(
        &self,
        cx: &mut Cx,
        target: &Value,
        input: Expr,
        role: Option<&Symbol>,
    ) -> Result<Expr> {
        let Some(adapter) = self.resolve(cx, target) else {
            return Err(Error::TypeMismatch {
                expected: "topology adapter target",
                found: "unsupported target",
            });
        };
        adapter.call(cx, target, input, role)
    }
}

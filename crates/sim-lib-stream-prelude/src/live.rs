use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use sim_citizen_derive::non_citizen;
use sim_kernel::{CORE_EXPR_CLASS_ID, ClassId, ClassRef, Cx, Error, Expr, Object, ObjectCompat};
use sim_kernel::{Result, Symbol};
use sim_lib_stream_combinators::{StreamCell, stream_cell};

use crate::handle::StreamHandle;

#[derive(Default)]
pub struct StreamRuntime {
    state: Mutex<LiveState>,
}

#[derive(Default)]
struct LiveState {
    streams: BTreeMap<Symbol, LiveStream>,
    cells: BTreeMap<Symbol, Arc<LiveCell>>,
    graphs: BTreeMap<Symbol, GraphHandle>,
}

#[derive(Clone)]
pub struct LiveStream {
    handle: StreamHandle,
    opened_at: Instant,
}

#[non_citizen(
    reason = "live stream cell handle; reconstruct stream/CellDescriptor then realize explicitly",
    kind = "handle",
    descriptor = "stream/CellDescriptor"
)]
pub struct LiveCell {
    id: Symbol,
    cell: StreamCell<f64>,
}

#[non_citizen(
    reason = "live stream graph handle; reconstruct stream/GraphDescriptor then realize explicitly",
    kind = "handle",
    descriptor = "stream/GraphDescriptor"
)]
#[derive(Clone, Debug)]
pub struct GraphHandle {
    id: Symbol,
    source: Symbol,
    target: Option<Symbol>,
}

impl StreamRuntime {
    pub fn register_stream(&self, handle: StreamHandle) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream live catalog"))?;
        state.streams.insert(
            handle.metadata().id().clone(),
            LiveStream {
                handle,
                opened_at: Instant::now(),
            },
        );
        Ok(())
    }

    pub fn stream_entries(&self) -> Result<Vec<LiveStream>> {
        self.state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream live catalog"))
            .map(|state| state.streams.values().cloned().collect())
    }

    pub fn register_cell(&self, cell: Arc<LiveCell>) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream live catalog"))?;
        state.cells.insert(cell.id().clone(), cell);
        Ok(())
    }

    pub fn cell_by_id(&self, id: &Symbol) -> Result<Option<Arc<LiveCell>>> {
        self.state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream live catalog"))
            .map(|state| state.cells.get(id).cloned())
    }

    pub fn reroute(&self, source: Symbol, target: Symbol) -> Result<GraphHandle> {
        let graph = GraphHandle::route(source, target);
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream live catalog"))?;
        state.graphs.insert(graph.id().clone(), graph.clone());
        Ok(graph)
    }

    pub fn cancel_older_than(&self, age: Duration) -> Result<Vec<Symbol>> {
        let entries = self.stream_entries()?;
        let mut cancelled = Vec::new();
        for entry in entries {
            if entry.age() >= age {
                entry.handle.cancel()?;
                cancelled.push(entry.handle.metadata().id().clone());
            }
        }
        Ok(cancelled)
    }
}

impl LiveStream {
    pub fn handle(&self) -> &StreamHandle {
        &self.handle
    }

    pub fn age(&self) -> Duration {
        self.opened_at.elapsed()
    }
}

impl LiveCell {
    pub fn new(id: Symbol, value: f64) -> Self {
        Self {
            id,
            cell: stream_cell(value),
        }
    }

    pub fn id(&self) -> &Symbol {
        &self.id
    }

    pub fn value(&self) -> Result<f64> {
        self.cell.get().map(|snapshot| snapshot.value)
    }

    pub fn version(&self) -> Result<u64> {
        self.cell.get().map(|snapshot| snapshot.version)
    }

    pub fn set(&self, value: f64, expected_version: Option<u64>) -> Result<u64> {
        let expected_version = match expected_version {
            Some(version) => version,
            None => self.cell.get()?.version,
        };
        self.cell.set(value, expected_version)
    }
}

impl Object for LiveCell {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok(format!("#<stream-cell {}>", self.id))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for LiveCell {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        cx.factory()
            .class_stub(ClassId(0), Symbol::qualified("stream", "Cell"))
    }
}

impl GraphHandle {
    pub fn route(source: Symbol, target: Symbol) -> Self {
        let id = Symbol::new(format!("stream/route/{source}->{target}"));
        Self {
            id,
            source,
            target: Some(target),
        }
    }

    pub fn id(&self) -> &Symbol {
        &self.id
    }

    pub fn source(&self) -> &Symbol {
        &self.source
    }

    pub fn target(&self) -> Option<&Symbol> {
        self.target.as_ref()
    }

    pub fn lisp_expr(&self) -> Expr {
        let mut args = vec![Expr::String(self.source.to_string())];
        if let Some(target) = &self.target {
            args.push(Expr::String(target.to_string()));
        }
        Expr::Call {
            operator: Box::new(Expr::Symbol(Symbol::qualified("stream", "reroute!"))),
            args,
        }
    }
}

impl Object for GraphHandle {
    fn display(&self, _cx: &mut Cx) -> Result<String> {
        Ok(format!("#<stream-graph {}>", self.id))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ObjectCompat for GraphHandle {
    fn class(&self, cx: &mut Cx) -> Result<ClassRef> {
        cx.factory()
            .class_stub(CORE_EXPR_CLASS_ID, Symbol::qualified("stream", "Graph"))
    }

    fn as_expr(&self, _cx: &mut Cx) -> Result<Expr> {
        Ok(self.lisp_expr())
    }
}

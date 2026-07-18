use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
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
    catalog_time: Duration,
    streams: BTreeMap<Symbol, LiveStreamRecord>,
    cells: BTreeMap<Symbol, Arc<LiveCell>>,
    graphs: BTreeMap<Symbol, GraphHandle>,
}

struct LiveStreamRecord {
    handle: StreamHandle,
    opened_at: Duration,
}

#[derive(Clone)]
pub struct LiveStream {
    handle: StreamHandle,
    opened_at: Duration,
    catalog_time: Duration,
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
        let opened_at = state.catalog_time;
        state.streams.insert(
            handle.metadata().id().clone(),
            LiveStreamRecord { handle, opened_at },
        );
        Ok(())
    }

    pub fn stream_entries(&self) -> Result<Vec<LiveStream>> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream live catalog"))?;
        Ok(state
            .streams
            .values()
            .map(|entry| LiveStream {
                handle: entry.handle.clone(),
                opened_at: entry.opened_at,
                catalog_time: state.catalog_time,
            })
            .collect())
    }

    pub fn advance_time(&self, delta: Duration) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream live catalog"))?;
        state.catalog_time = state.catalog_time.saturating_add(delta);
        Ok(())
    }

    pub fn catalog_time(&self) -> Result<Duration> {
        self.state
            .lock()
            .map_err(|_| Error::PoisonedLock("stream live catalog"))
            .map(|state| state.catalog_time)
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
            if entry.age() > age {
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
        self.catalog_time.saturating_sub(self.opened_at)
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

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use sim_kernel::Symbol;
    use sim_lib_stream_core::{
        BufferPolicy, StreamDirection, StreamMedia, StreamMetadata, StreamValue,
    };

    use super::*;

    #[test]
    fn logical_catalog_time_controls_age_and_cancel_thresholds() {
        let runtime = StreamRuntime::default();
        let old = source_handle("stream/live-old");
        runtime.register_stream(old.clone()).unwrap();
        runtime.advance_time(Duration::from_secs(5)).unwrap();
        let fresh = source_handle("stream/live-fresh");
        runtime.register_stream(fresh.clone()).unwrap();

        let entries = runtime.stream_entries().unwrap();
        assert_eq!(age_of(&entries, "stream/live-old"), Duration::from_secs(5));
        assert_eq!(
            age_of(&entries, "stream/live-fresh"),
            Duration::from_secs(0)
        );

        assert!(
            runtime
                .cancel_older_than(Duration::from_secs(5))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            runtime.cancel_older_than(Duration::from_secs(4)).unwrap(),
            vec![Symbol::new("stream/live-old")]
        );
        assert!(old.stats().unwrap().cancelled);
        assert!(!fresh.stats().unwrap().cancelled);
    }

    fn source_handle(id: &str) -> StreamHandle {
        let metadata = StreamMetadata::new(
            Symbol::new(id),
            StreamMedia::Data,
            StreamDirection::Source,
            Symbol::qualified("clock", "data"),
            BufferPolicy::bounded(4).unwrap(),
        );
        let stream = Arc::new(StreamValue::push(metadata.clone()));
        StreamHandle::source(metadata, stream)
    }

    fn age_of(entries: &[LiveStream], id: &str) -> Duration {
        entries
            .iter()
            .find(|entry| entry.handle().metadata().id() == &Symbol::new(id))
            .map(LiveStream::age)
            .unwrap_or_else(|| panic!("missing live stream {id}"))
    }
}

//! Test support: a recording `Layer` and a process-global subscriber.
//!
//! `tracing::subscriber::set_default` is thread-scoped; may's worker threads only see the
//! **global** default. Every test binary that spawns coroutines therefore installs one
//! global `Registry` + `Recorder` via [`install`], and tests share it (filtering by span
//! name / event target where they need isolation).

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;

#[derive(Default)]
struct State {
    /// span id -> (name, explicit parent id)
    spans: HashMap<u64, (&'static str, Option<u64>)>,
    /// (event target, explicit parent id, message-ish name)
    events: Vec<(String, Option<u64>)>,
}

/// Records `(span id, parent id, name)` for spans and `(target, parent id)` for events.
///
/// Parents are read from the span/event **attributes** only (explicit `parent:`), never
/// from the thread's contextual span — the recorder must not itself commit the ADR-0001
/// sin while measuring it.
#[derive(Clone, Default)]
pub struct Recorder(Arc<Mutex<State>>);

impl Recorder {
    pub fn new() -> (RecorderLayer, Recorder) {
        let r = Recorder::default();
        (RecorderLayer(r.clone()), r)
    }

    pub fn parent_of(&self, id: &Id) -> Option<Id> {
        let st = self.0.lock().unwrap();
        st.spans
            .get(&id.into_u64())
            .and_then(|(_, p)| *p)
            .map(Id::from_u64)
    }

    pub fn name_of(&self, id: &Id) -> Option<&'static str> {
        self.0
            .lock()
            .unwrap()
            .spans
            .get(&id.into_u64())
            .map(|(n, _)| *n)
    }

    /// Parent of the most recent event with this target.
    pub fn event_parent(&self, target: &str) -> Option<Id> {
        let st = self.0.lock().unwrap();
        st.events
            .iter()
            .rev()
            .find(|(t, _)| t == target)
            .and_then(|(_, p)| *p)
            .map(Id::from_u64)
    }

    pub fn has_event(&self, target: &str) -> bool {
        self.0
            .lock()
            .unwrap()
            .events
            .iter()
            .any(|(t, _)| t == target)
    }

    pub fn span_count(&self) -> usize {
        self.0.lock().unwrap().spans.len()
    }
}

pub struct RecorderLayer(Recorder);

impl<S> Layer<S> for RecorderLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, _ctx: Context<'_, S>) {
        let parent = attrs.parent().map(Id::into_u64);
        self.0
             .0
            .lock()
            .unwrap()
            .spans
            .insert(id.into_u64(), (attrs.metadata().name(), parent));
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let parent = event.parent().map(Id::into_u64);
        self.0
             .0
            .lock()
            .unwrap()
            .events
            .push((event.metadata().target().to_owned(), parent));
    }
}

static GLOBAL: OnceLock<Recorder> = OnceLock::new();

/// Install (once per process) a global `Registry` with a `Recorder` layer and return the
/// recorder. Safe to call from every test in a binary.
pub fn install() -> Recorder {
    GLOBAL
        .get_or_init(|| {
            let (layer, rec) = Recorder::new();
            tracing::subscriber::set_global_default(tracing_subscriber::registry().with(layer))
                .expect("global subscriber installed once");
            rec
        })
        .clone()
}

/// Counts OS-thread migrations of the calling coroutine between calls to `tick`.
pub struct MigrationCounter {
    last: std::thread::ThreadId,
    pub migrations: usize,
}

impl MigrationCounter {
    pub fn new() -> Self {
        Self {
            last: std::thread::current().id(),
            migrations: 0,
        }
    }

    pub fn tick(&mut self) {
        let now = std::thread::current().id();
        if now != self.last {
            self.migrations += 1;
            self.last = now;
        }
    }
}

impl Default for MigrationCounter {
    fn default() -> Self {
        Self::new()
    }
}

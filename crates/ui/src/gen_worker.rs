//! #2257: toolpath generation off the UI thread in the interactive app,
//! so the viewport can show a progress overlay while it runs. The world
//! reads stay on this thread (`StagedGeneration::new`); only the pure
//! generator, verification and stamp run on the worker. The result lands
//! through the same undoable `GenerateOperation` as the synchronous path,
//! and only while its operation is unchanged since staging.
//! Provenance: our own code over std's mpsc and threads, the import
//! worker's pattern in `state.rs`; no HSMWorks material.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};

use camcraft_app::clock::Instant;
use camcraft_app::commands::{CommandError, GenerateOperation, StagedGeneration};
use camcraft_app::model;
use flecs_ecs::prelude::Entity;

use crate::state::{ErrorScope, State};


type Outcome = Result<GenerateOperation, CommandError>;

/// The background generation queue. Ephemeral UI state: never a command,
/// never in the undo history.
#[derive(Default)]
pub struct GenJobs {
    /// Off (the default) keeps the synchronous path: tests, headless runs
    /// and scripts see the toolpath as soon as the queue drains. The
    /// interactive shell turns it on.
    pub background: bool,
    rx: Option<Receiver<Outcome>>,
    running: Option<Running>,
    queue: VecDeque<String>,
    /// Jobs that ended in this batch, for the "2 of 5" count.
    done: usize,
}

/// The job on the worker. The name search scans every named entity, so it
/// runs once, when the job starts; a frame reads the label straight from
/// the found entity, which a rename updates in place. No cache, so no key
/// to go stale (#2257, the #2238 frame-cost rule).
struct Running {
    name: String,
    started: Instant,
    /// The operation's entity; `None` until found, or if it was not found.
    entity: Option<Entity>,
    /// The search ran.
    searched: bool,
}

impl Running {
    fn new(name: String) -> Self {
        Self { name, started: Instant::now(), entity: None, searched: false }
    }
}

#[cfg(test)]
thread_local! {
    static LABEL_LOOKUPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// For tests: how many label searches ran on this thread.
#[cfg(test)]
pub(crate) fn label_lookups() -> usize {
    LABEL_LOOKUPS.with(std::cell::Cell::get)
}

/// What the overlay shows: the operation's label, its place in the batch
/// and the time it has run.
#[derive(Clone, Debug, PartialEq)]
pub struct GenProgress {
    pub label: String,
    pub index: usize,
    pub total: usize,
    pub secs: f64,
}

impl GenJobs {
    /// A generation runs or waits.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.running.is_some() || !self.queue.is_empty()
    }

    /// Drop the running job's result and the queue (the document changed
    /// or the user cancelled). A worker still running finishes in the
    /// background and its result is discarded. Returns the operation that
    /// was running.
    pub(crate) fn abandon(&mut self) -> Option<String> {
        self.rx = None;
        self.queue.clear();
        self.done = 0;
        self.running.take().map(|r| r.name)
    }

    /// For tests: a running job whose result arrives through `rx`. Its
    /// entity is found at the next poll, as a started job's is.
    #[cfg(test)]
    pub(crate) fn run_with(&mut self, name: &str, rx: Receiver<Outcome>) {
        self.rx = Some(rx);
        self.running = Some(Running::new(name.to_owned()));
    }
}

/// The one line the overlay paints.
#[must_use]
pub fn overlay_text(p: &GenProgress) -> String {
    let count = if p.total > 1 { format!(" ({} of {})", p.index, p.total) } else { String::new() };
    format!("Generating {}{count} - {:.0} s", p.label, p.secs.max(0.0))
}

impl State {
    /// The overlay's readout, `None` while nothing generates. It reads the
    /// label from the job's entity and does no world search. A deleted
    /// entity, or one not found, shows the operation's name.
    #[must_use]
    pub fn gen_progress(&self) -> Option<GenProgress> {
        let running = self.gen_jobs.running.as_ref()?;
        let label = running
            .entity
            .and_then(|e| model::live_operation_label(&self.world, &running.name, &e))
            .unwrap_or_else(|| running.name.clone());
        Some(GenProgress {
            label,
            index: self.gen_jobs.done + 1,
            total: self.gen_jobs.done + 1 + self.gen_jobs.queue.len(),
            secs: running.started.elapsed().as_secs_f64(),
        })
    }

    /// Find the running job's entity, once for each job.
    fn find_gen_entity(&mut self) {
        let Some(running) = self.gen_jobs.running.as_mut() else { return };
        if running.searched {
            return;
        }
        #[cfg(test)]
        LABEL_LOOKUPS.with(|n| n.set(n.get() + 1));
        running.entity = model::find_by_name(&self.world, &running.name);
        running.searched = true;
    }

    /// Generate `name`: on the worker when background generation is on,
    /// else here and now.
    pub(crate) fn generate_now_or_queue(&mut self, name: String) {
        if self.gen_jobs.background {
            self.gen_jobs.queue.push_back(name);
            self.start_next_generation();
        } else {
            let result = GenerateOperation::new(&self.world, &name);
            self.land_generation(&name, result, false);
        }
    }

    /// The user's Cancel on the overlay.
    pub(crate) fn cancel_generation(&mut self) {
        if let Some(name) = self.gen_jobs.abandon() {
            self.status = format!("generation of {name} cancelled - its old toolpath stays");
        }
    }

    /// Once a frame: land a finished job, then start the next one.
    pub(crate) fn poll_generation(&mut self) {
        let outcome = match &self.gen_jobs.rx {
            Some(rx) => match rx.try_recv() {
                Ok(result) => Some(Some(result)),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(None),
            },
            None => None,
        };
        if let Some(outcome) = outcome {
            self.gen_jobs.rx = None;
            let name = self.gen_jobs.running.take().map(|r| r.name).unwrap_or_default();
            self.gen_jobs.done += 1;
            match outcome {
                Some(result) => self.land_generation(&name, result, true),
                None => self.refuse(ErrorScope::Operation, format!("generate failed: {name}: the worker stopped without a result")),
            }
        }
        self.start_next_generation();
        self.find_gen_entity();
        if !self.gen_jobs.busy() {
            self.gen_jobs.done = 0;
        }
    }

    fn start_next_generation(&mut self) {
        while self.gen_jobs.running.is_none() {
            let Some(name) = self.gen_jobs.queue.pop_front() else { return };
            // The world reads happen here, on this thread; a refusal at
            // this door lands at once.
            let staged = match StagedGeneration::new(&self.world, &name) {
                Ok(s) => s,
                Err(e) => {
                    self.gen_jobs.done += 1;
                    self.land_generation(&name, Err(e), true);
                    continue;
                }
            };
            let (tx, rx) = mpsc::channel();
            // The slot lets a failed spawn (no threads, as in the browser)
            // take the job back and run it here.
            let slot = Arc::new(Mutex::new(Some(staged)));
            let worker_slot = Arc::clone(&slot);
            let spawned = std::thread::Builder::new().name("generate".into()).spawn(move || {
                let job = worker_slot.lock().ok().and_then(|mut s| s.take());
                if let Some(job) = job {
                    let _ = tx.send(job.finish());
                }
            });
            if spawned.is_ok() {
                self.gen_jobs.rx = Some(rx);
                self.gen_jobs.running = Some(Running::new(name));
                self.find_gen_entity();
            } else {
                let job = slot.lock().ok().and_then(|mut s| s.take());
                self.gen_jobs.done += 1;
                match job {
                    Some(job) => self.land_generation(&name, job.finish(), false),
                    None => self.refuse(ErrorScope::Operation, format!("generate failed: {name}: the worker could not start")),
                }
            }
        }
    }

    /// Write a finished generation through the history, or show its
    /// refusal. A background result whose operation changed since it was
    /// staged is discarded, named.
    fn land_generation(&mut self, name: &str, result: Result<GenerateOperation, CommandError>, background: bool) {
        match result {
            Ok(command) if background && !command.still_current(&self.world) => {
                self.refuse(
                    ErrorScope::Operation,
                    format!("generation of {name} discarded: the operation changed while it generated - generate it again"),
                );
            }
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                let cache = model::find_by_name(&self.world, name).and_then(|e| model::toolpath_of(&self.world, &e));
                // The feed in the summary reads in the document's unit
                // (M1.1); the path itself is mm law, always.
                let unit = self.units();
                self.status = match cache {
                    Some(tp) => tp.path.status_summary(name, tp.warnings.len(), unit),
                    None => format!("generated {name}"),
                };
            }
            Err(err) => {
                self.refuse(ErrorScope::Operation, format!("generate failed: {err}"));
                // #261 part 2: where the generator refused, if it said.
                self.feedback_at = err.location().map(|p| [p.x, p.y, p.z]);
                // Part 3: the moves it had built, drawn as a ghost.
                self.feedback_ghost = err.ghost().cloned();
            }
        }
    }
}


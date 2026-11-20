//! The parallel-lanes executor for batch and release: builds first and alone, then heavy targets
//! before debug suites. Each phase drains its longest-first shared queue, then runs isolated steps
//! one at a time with the whole budget.
//! Provenance: our own build law (AGENTS.md "Three gate tiers", #1703, #2057, #2727); Graham's LPT list
//! scheduling (see `lanes`); standard Rust scoped threads and atomics.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use crate::budget;
use crate::exec::{Done, Run, moved};
use crate::lanes::{self, Schedule};
use crate::plan::{Plan, Step};

fn heavy(s: &Step) -> bool {
    s.args.iter().any(|a| a == "--release") && s.args.iter().any(|a| a == "--ignored")
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

impl Run {
    /// Runs a queued plan (`lanes::schedule`) for batch or release. A failed build stops the run; a VOID
    /// checkout stops it before the next step; a failed suite never stops the others. Writes `lanes::TIMINGS`
    /// for the next run.
    pub(crate) fn run_lanes(&mut self, plan: &Plan) -> Result<(), String> {
        let (history, from) = lanes::history_for(&self.root, &self.label);
        let sched: Schedule = lanes::schedule(&plan.steps, &history);
        let has_heavy = sched.queue.iter().chain(&sched.alone).any(|&i| heavy(&plan.steps[i]));
        let order = match &from {
            Some(f) => format!("longest first, by {}", f.display()),
            None => format!("plan order (no {} of an earlier run in gate-logs/ yet)", lanes::TIMINGS),
        };
        if has_heavy {
            println!("queue order: heavy targets first; {order}");
        } else {
            println!("queue order: {order}");
        }
        let result = self.run_schedule(plan, &sched);
        if !self.dry && !self.done.is_empty() {
            let text = lanes::format_timings(self.done.iter().map(|d| (d.name.as_str(), d.secs)));
            let file = self.log_dir.join(lanes::TIMINGS);
            if let Err(e) = std::fs::write(&file, text) {
                println!("note: could not write {}: {e} (the next run keeps plan order)", file.display());
            }
        }
        result
    }

    fn run_schedule(&mut self, plan: &Plan, sched: &Schedule) -> Result<(), String> {
        for &i in &sched.builds {
            if self.check(&format!("before {}", plan.steps[i].name)) {
                return Ok(());
            }
            println!("-- build alone x -j {}", self.budget);
            let d = Self::execute(&self.root, &self.log_dir, self.dry, i + 1, &plan.steps[i], self.budget, &self.built)?;
            let ok = d.ok;
            self.keep(d);
            if !ok {
                return Ok(());
            }
        }
        // Finish isolated heavy targets too before any debug suite starts (#2372).
        for is_heavy in [true, false] {
            let queue: Vec<usize> = sched.queue.iter().copied().filter(|&i| heavy(&plan.steps[i]) == is_heavy).collect();
            if !queue.is_empty() {
                if self.check(&format!("before the {} queue", self.label)) {
                    return Ok(());
                }
                let lanes = budget::lane_limit(self.budget).min(queue.len()).max(1);
                let jobs = budget::per_lane(self.budget, lanes);
                println!("-- queue: {} step(s) in {lanes} lane(s) x -j {jobs}, each lane takes the next step when it finishes", queue.len());
                let done = self.drain(plan, &queue, lanes, jobs)?;
                self.done.extend(done);
                if self.void.is_some() {
                    return Ok(());
                }
            }
            for &i in sched.alone.iter().filter(|&&i| heavy(&plan.steps[i]) == is_heavy) {
                let s = &plan.steps[i];
                if self.check(&format!("before {}", s.name)) {
                    return Ok(());
                }
                println!("-- alone x -j {}", self.budget);
                let d = Self::execute(&self.root, &self.log_dir, self.dry, i + 1, s, self.budget, &self.built)?;
                self.done.push(d);
            }
        }
        Ok(())
    }

    /// Runs `queue` (plan indices, in pull order) on `lanes` threads of `jobs` each. A dry run prints the
    /// queue in pull order without threads. Results come back in plan order.
    fn drain(&mut self, plan: &Plan, queue: &[usize], lanes: usize, jobs: usize) -> Result<Vec<Done>, String> {
        let (root, logs, dry, guard, built) = (&self.root, &self.log_dir, self.dry, &self.guard, &self.built);
        if dry {
            return queue.iter().map(|&i| Self::execute(root, logs, true, i + 1, &plan.steps[i], jobs, built)).collect();
        }
        let next = AtomicUsize::new(0);
        let stop = AtomicBool::new(false);
        let void: Mutex<Option<String>> = Mutex::new(None);
        let error: Mutex<Option<String>> = Mutex::new(None);
        let done: Mutex<Vec<(usize, Done)>> = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..lanes)
                .map(|_| {
                    scope.spawn(|| {
                        while !stop.load(Ordering::SeqCst) {
                            let k = next.fetch_add(1, Ordering::SeqCst);
                            let Some(&i) = queue.get(k) else { break };
                            let step = &plan.steps[i];
                            if let Some(m) = moved(root, guard, &format!("before {}", step.name)) {
                                stop.store(true, Ordering::SeqCst);
                                let mut v = lock(&void);
                                if v.is_none() {
                                    println!("{m}");
                                    *v = Some(m);
                                }
                                break;
                            }
                            match Self::execute(root, logs, false, i + 1, step, jobs, built) {
                                Ok(d) => lock(&done).push((i, d)),
                                Err(e) => {
                                    stop.store(true, Ordering::SeqCst);
                                    lock(&error).get_or_insert(e);
                                    break;
                                }
                            }
                        }
                    })
                })
                .collect();
            // Join every lane even when one panics; a panicked lane is an error, never a silent pass.
            for h in handles {
                if h.join().is_err() {
                    stop.store(true, Ordering::SeqCst);
                    lock(&error).get_or_insert_with(|| "gate lane thread failed".to_string());
                }
            }
        });
        if let Some(m) = lock(&void).take() {
            self.void = Some(m);
        }
        let mut done = std::mem::take(&mut *lock(&done));
        done.sort_by_key(|(i, _)| *i);
        let done: Vec<Done> = done.into_iter().map(|(_, d)| d).collect();
        if let Some(e) = lock(&error).take() {
            self.done.extend(done);
            return Err(e);
        }
        Ok(done)
    }
}

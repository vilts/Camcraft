//! Operation generation commands: generate one operation, regenerate every stale one.
//!
//! Provenance: our own domain model, following the same command pattern
//! as the import/stock siblings.

use flecs_ecs::prelude::{ComponentId, World};

use crate::commands::{Command, CommandError};
use crate::model;
use crate::operation;
use crate::schema::{Operation, Toolpath};

/// Generate (or re-generate) an operation: the constructor runs the
/// whole pipeline - region, generator, verification, stamp - so a
/// failure refuses the command instead of half-writing the world, and a
/// success carries its finished output as inert fields.
pub struct GenerateOperation {
    name: String,
    prev_operation: Operation,
    prev_cache: Option<Toolpath>,
    next_operation: Operation,
    output: Toolpath,
}

/// The constructor split at M5.3 B's seam: [`stage_generation`] does
/// EVERY world read - the entity, the components, the whole job
/// collection, the stamp - and [`Staged::finish`] runs the pure result.
/// One op does stage and finish together, exactly as before; a batch
/// stages all and finishes in parallel, and because a constructor
/// never writes the world, that batch is byte-identical to the serial
/// loop it replaces.
struct Staged {
    name: String,
    prev_operation: Operation,
    prev_cache: Option<Toolpath>,
    job: operation::GenJob,
    stamp: Option<operation::GenStamp>,
}

/// A generator refusal as the command's error, named for its operation.
/// A refusal that names its spot (#261 part 2; `run_job` already
/// carried it to world) keeps it, so the UI can show where.
fn machining(name: &str, e: &crate::machining2d::GenError) -> CommandError {
    let msg = format!("{name}: {e}");
    // #261 part 3: the moves built before the refusal ride it, display-only.
    if let Some(ghost) = e.ghost() {
        return CommandError::MachiningGhost { msg, at: e.location(), ghost: ghost.clone() };
    }
    match e.location() {
        Some(at) => CommandError::MachiningAt { msg, at },
        None => CommandError::Machining(msg),
    }
}

fn stage_generation(world: &World, name: &str) -> Result<Staged, CommandError> {
    let Some(entity) = model::find_by_name(world, name) else {
        return Err(CommandError::NotFound(name.to_string()));
    };
    let prev_operation = model::operation_of(world, &entity)
        .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
    // M10.1 C: a tool gone or of the wrong family blocks generation by
    // name, before the generator runs - the generators' shape doors stay
    // the backstop behind this one, never the first word.
    if let Some(why) = operation::tool_gate::tool_problem(world, name, &prev_operation) {
        return Err(CommandError::Machining(why));
    }
    let prev_cache = model::toolpath_of(world, &entity);
    let job = operation::collect_job(world, &prev_operation, name).map_err(|e| machining(name, &e))?;
    let stamp = operation::op_stamp(world, &prev_operation, name);
    Ok(Staged {
        name: name.to_string(),
        prev_operation,
        prev_cache,
        job,
        stamp,
    })
}

impl Staged {
    fn finish(self) -> Result<GenerateOperation, CommandError> {
        let (path, warnings) = operation::run_job(self.job).map_err(|e| machining(&self.name, &e))?;
        let stamp = self.stamp.ok_or_else(|| {
            CommandError::Machining(format!(
                "{}: its tool or geometry source vanished mid-generation",
                self.name
            ))
        })?;
        let next_operation = Operation {
            stamp: Some(stamp),
            ..self.prev_operation.clone()
        };
        Ok(GenerateOperation {
            name: self.name,
            prev_operation: self.prev_operation,
            prev_cache: self.prev_cache,
            next_operation,
            output: Toolpath::new(path, warnings),
        })
    }
}

impl GenerateOperation {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        stage_generation(world, name)?.finish()
    }

    /// The operation this command generates.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// #2257: the operation and its cached path are still what they were
    /// when the generation was staged. A background generation that lands
    /// after an edit, an undo or a delete of its operation is discarded:
    /// applying it would write old parameters over the user's newer ones.
    #[must_use]
    pub fn still_current(&self, world: &World) -> bool {
        model::find_by_name(world, &self.name).is_some_and(|e| {
            model::operation_of(world, &e).as_ref() == Some(&self.prev_operation)
                && model::toolpath_of(world, &e) == self.prev_cache
        })
    }
}

/// #2257: one generation split at the M5.3 B seam for a background
/// worker. [`StagedGeneration::new`] does every world read on the caller's
/// thread; [`StagedGeneration::finish`] reads no world and can run on any
/// thread. `GenerateOperation::new` is exactly `new` then `finish`.
pub struct StagedGeneration(Staged);

impl StagedGeneration {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        stage_generation(world, name).map(Self)
    }

    /// The operation this generation is for.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.0.name
    }

    /// Run the generator, verification and stamp on the staged job.
    pub fn finish(self) -> Result<GenerateOperation, CommandError> {
        self.0.finish()
    }
}

impl Command for GenerateOperation {
    fn apply(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, &self.name) else {
            return; // the tree deleted it underneath us; nothing to write
        };
        entity.entity_view(world).set(self.next_operation.clone());
        entity
            .entity_view(world)
            .set(self.output.clone());
    }

    fn invert(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, &self.name) else {
            return;
        };
        entity.entity_view(world).set(self.prev_operation.clone());
        match &self.prev_cache {
            Some(cache) => {
                entity.entity_view(world).set(cache.clone());
            }
            None => {
                // No cache existed before: remove the component by its id
                // (flecs removals are id-based for data components; the
                // value form only works for tags).
                entity
                    .entity_view(world)
                    .remove(Toolpath::entity_id(world));
            }
        }
    }
}

/// The M3.1 payoff command: one click brings the job current after a
/// model revision, instead of rebuilding it op by op. Construction
/// decides everything - it finds the Stale operations in program order
/// and runs each generator exactly as `GenerateOperation` would
/// (validate early, errors still land here) - so `apply` and `invert`
/// are the batch's pure writes. An operation whose regeneration fails is
/// SKIPPED, not fatal: one broken pick must not block the rest of the
/// job (never crash, never gouge - the failed op keeps its old cache and
/// its STALE tag, honestly). Undo inverts the whole batch in reverse,
/// one undo unit for one user action, and operations that were only
/// skipped were never touched, so nothing needs restoring for them.
pub struct RegenerateAll {
    parts: Vec<GenerateOperation>,
    skipped: Vec<(String, String)>,
}

impl RegenerateAll {
    /// Runs every stale operation's generator against the current world.
    /// `Never` and `Ok` operations are left alone - this command is for
    /// what a revision invalidated, not for first-time generation.
    ///
    /// Rounds (#145): an op whose verdict reads the program ahead (#109)
    /// is stamped against its predecessors' cached paths, so one frozen
    /// batch would leave it Stale behind a predecessor regenerated in
    /// the same batch. Each round stages the Stale ops in program order
    /// (M5.3 B: all world reads first, then the jobs through rayon,
    /// ordered), then writes its results so the next round reads them;
    /// the loop stops when nothing new is Stale, capped at the op count.
    /// When construction ends every write is inverted in reverse, so the
    /// world is untouched until `apply` replays the whole batch - one
    /// undo unit. An op still Stale at the cap is named in `skipped`.
    pub fn new(world: &World) -> Self {
        let mut parts: Vec<GenerateOperation> = Vec::new();
        let mut skipped: Vec<(String, String)> = Vec::new();
        let cap = crate::setup::operation_order(world).len().max(1);
        let mut rounds = 0;
        while rounds < cap {
            let (round, staged_any) = Self::round(world, &mut skipped);
            rounds += 1;
            // Stop only when nothing was STAGED (#153 re-review): a round
            // whose every job refused still frees the ops it deferred -
            // a refusal is final, so they run next round against the
            // world as it stands.
            if !staged_any {
                break;
            }
            for part in &round {
                part.apply(world);
            }
            parts.extend(round);
        }
        // Still Stale after the cap (a cycle no order converges): named,
        // never silent - the post refuses it anyway.
        for (name, entity) in crate::setup::operation_order(world) {
            if operation::status_of(world, &entity) == operation::OpStatus::Stale
                && !skipped.iter().any(|(n, _)| *n == name)
            {
                skipped.push((name, format!("still stale after {rounds} regeneration round(s)")));
            }
        }
        for part in parts.iter().rev() {
            part.invert(world);
        }
        RegenerateAll { parts, skipped }
    }

    /// One round: every Stale op not yet refused, staged in program
    /// order against the world as it stands, finished in parallel. An op
    /// that reads the program ahead waits for a later round when an op
    /// ahead of it in its setup is staged in this one (#153 review N1):
    /// it would be stamped against that op's old path and re-run, so
    /// each op generates once and setups still run side by side.
    /// The bool says whether anything was staged (or refused at the
    /// stage door) this round - the loop's progress signal.
    fn round(world: &World, skipped: &mut Vec<(String, String)>) -> (Vec<GenerateOperation>, bool) {
        let mut staged = Vec::new();
        let mut staged_setups: Vec<Option<String>> = Vec::new();
        let mut attempted = false;
        for (name, entity) in crate::setup::operation_order(world) {
            if operation::status_of(world, &entity) != operation::OpStatus::Stale
                || skipped.iter().any(|(n, _)| *n == name)
            {
                continue;
            }
            let Some(op) = model::operation_of(world, &entity) else {
                continue;
            };
            let setup = crate::setup::effective(world, op.setup.as_deref());
            if operation::reads_prior(&op.params) && staged_setups.contains(&setup) {
                continue; // its program ahead moves this round
            }
            attempted = true;
            match stage_generation(world, &name) {
                Ok(job) => {
                    staged.push(job);
                    staged_setups.push(setup);
                }
                Err(err) => skipped.push((name, err.to_string())),
            }
        }
        use rayon::prelude::*;
        let results: Vec<(String, Result<GenerateOperation, CommandError>)> = staged
            .into_par_iter()
            .map(|job| {
                let name = job.name.clone();
                (name, job.finish())
            })
            .collect();
        let mut parts = Vec::new();
        for (name, result) in results {
            match result {
                Ok(part) => parts.push(part),
                Err(err) => skipped.push((name, err.to_string())),
            }
        }
        (parts, attempted)
    }

    /// Nothing will be written: the UI skips the history entirely then.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// How many distinct operations the batch regenerates (an op a
    /// later round re-ran counts once).
    pub fn regenerated(&self) -> usize {
        let mut names: Vec<&str> = self.parts.iter().map(|p| p.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        names.len()
    }

    /// The names whose generators refused, with the refusal text.
    pub fn skipped(&self) -> &[(String, String)] {
        &self.skipped
    }
}

impl Command for RegenerateAll {
    fn apply(&self, world: &World) {
        for part in &self.parts {
            part.apply(world);
        }
    }

    fn invert(&self, world: &World) {
        for part in self.parts.iter().rev() {
            part.invert(world);
        }
    }
}

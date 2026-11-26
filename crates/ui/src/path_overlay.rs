//! The cached CL paths as overlay lines, built once per input and read
//! from a cache on every other frame (#2201): an 8000-move Offset path
//! was cloned and turned into lines again on each frame, which held the
//! preview far over the 10 ms frame target. An entry keys on its inputs
//! (crates/ui/AGENTS.md "Frame cost"): the path's content signature and
//! its warnings, the palette, the viewport ground, the dim and the
//! chevron arm. The chevron arm keys on each model's facet allocation
//! and pose. No document counter is part of a key.
//! Provenance: our own display cache over our own path display; split
//! out of `interact::toolpath_lines`. No vendor material consulted.

use std::sync::{Arc, Mutex, Weak};

use camcraft_app::geometry::Triangle;
use camcraft_app::model;
use camcraft_app::schema::Toolpath;
use camcraft_app::verify::Warning;
use flecs_ecs::prelude::{Entity, World};

use crate::egui_app::colorvision::Semantic;
use crate::interact::MAX_OVERLAY_VERTICES;
use crate::state::State;

/// Unselected operations under "Show all toolpaths" draw behind the
/// selected one.
const OTHERS_DIM: f32 = 0.45;

/// The display inputs of one path's lines, apart from the path itself.
#[derive(Clone, Copy, PartialEq)]
struct Look {
    pal: Semantic,
    ground: egui::Color32,
    dim: u32,
    arm: u64,
}

struct LineEntry {
    entity: Entity,
    sig: u64,
    moves: usize,
    warnings: Vec<Warning>,
    look: Look,
    /// False when the path outgrew the vertex budget on its own.
    whole: bool,
    lines: Arc<Vec<[f32; 6]>>,
}

/// The chevron arm and the model bodies it was measured over.
struct ArmEntry {
    bodies: Vec<(Weak<[Triangle]>, usize, [u64; 7])>,
    arm: f64,
}

#[derive(Default)]
struct Inner {
    lines: Vec<LineEntry>,
    arm: Option<ArmEntry>,
    line_builds: u64,
    arm_builds: u64,
}

/// The path line cache, held on `State` through `FrameCache`.
#[derive(Default)]
pub(crate) struct PathLineCache {
    inner: Mutex<Inner>,
}

fn lock(cache: &PathLineCache) -> std::sync::MutexGuard<'_, Inner> {
    cache.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl PathLineCache {
    /// How many path line builds ran (the counter pin).
    #[cfg(test)]
    pub(crate) fn line_builds(&self) -> u64 {
        lock(self).line_builds
    }

    /// How many chevron arm measures ran (the counter pin).
    #[cfg(test)]
    pub(crate) fn arm_builds(&self) -> u64 {
        lock(self).arm_builds
    }

    /// How many paths hold cached lines.
    #[cfg(test)]
    pub(crate) fn entries(&self) -> usize {
        lock(self).lines.len()
    }

    /// The chevron arm of the document: 1% of the solid extent, floored
    /// so a small part still reads direction. Measured again only when a
    /// model's facets or pose change, or a model comes or goes.
    fn arm(&self, world: &World) -> f64 {
        let bodies: Vec<_> = model::model_entities(world)
            .into_iter()
            .filter_map(|(_, e)| {
                let tris = model::facets_of(world, &e)?;
                let pose = crate::frame_cache::pose_bits(&model::transform_of(world, &e).unwrap_or_default());
                Some((tris, pose))
            })
            .collect();
        let mut inner = lock(self);
        if let Some(a) = &inner.arm
            && a.bodies.len() == bodies.len()
            && a.bodies.iter().zip(&bodies).all(|((w, n, p), (t, q))| {
                w.strong_count() > 0 && std::ptr::eq(w.as_ptr(), Arc::as_ptr(t)) && *n == t.len() && p == q
            })
        {
            return a.arm;
        }
        let extent = model::solid_world_aabb(world)
            .map(|b| (b.max.x - b.min.x).max(b.max.y - b.min.y).max(b.max.z - b.min.z))
            .unwrap_or(0.0);
        let arm = (extent * 0.01).max(0.6);
        inner.arm_builds = inner.arm_builds.saturating_add(1);
        inner.arm = Some(ArmEntry {
            bodies: bodies.iter().map(|(t, p)| (Arc::downgrade(t), t.len(), *p)).collect(),
            arm,
        });
        arm
    }

    /// The lines of `entity`'s cached path under `look`, built when an
    /// input changed. `None` when the operation has no path.
    fn lines_of(&self, world: &World, entity: Entity, look: Look) -> Option<(Arc<Vec<[f32; 6]>>, bool)> {
        model::with_toolpath(world, &entity, |tp: &Toolpath| {
            let mut inner = lock(self);
            let hit = inner.lines.iter().find(|e| {
                e.entity == entity
                    && e.sig == tp.path_sig()
                    && e.moves == tp.path.moves.len()
                    && e.look == look
                    && e.warnings == tp.warnings
            });
            if let Some(e) = hit {
                return (Arc::clone(&e.lines), e.whole);
            }
            let mut lines = Vec::new();
            let pal = look.pal;
            let whole =
                crate::path_lines::cache_lines(&mut lines, tp, &pal, f32::from_bits(look.dim), f64::from_bits(look.arm));
            let lines = Arc::new(lines);
            inner.line_builds = inner.line_builds.saturating_add(1);
            inner.lines.retain(|e| e.entity != entity);
            inner.lines.push(LineEntry {
                entity,
                sig: tp.path_sig(),
                moves: tp.path.moves.len(),
                warnings: tp.warnings.clone(),
                look,
                whole,
                lines: Arc::clone(&lines),
            });
            (lines, whole)
        })
    }

    /// Drop the entries of operations not drawn this frame, so a deleted
    /// or hidden path frees its lines (AGENTS.md: a leak is a bug).
    fn keep_only(&self, drawn: &[Entity]) {
        lock(self).lines.retain(|e| drawn.contains(&e.entity));
    }
}

/// The cached CL paths as overlay segments: straight moves as one line in
/// their kind colour with direction chevrons along feed travel, arc moves
/// as their chord runs. The selected operation always draws bright; other
/// operations show dimmed only under "Show all toolpaths", and never at
/// all when hidden by its browser eye. Staleness is not a display state
/// here - what is drawn is the cache itself, and the tree's STALE badge
/// is where a stale path is called out.
///
/// Public for the docs-image harness (`crates/ui/tests/docs_images.rs`):
/// tutorial pictures must carry the app's own path display - same
/// colours, same chords, same chevrons - never a test-local
/// reconstitution that can drift from what the viewport actually paints.
#[must_use]
pub fn toolpath_lines(state: &State) -> Vec<[f32; 6]> {
    let cache = &state.frame_cache.paths;
    let mut out = Vec::new();
    let arm = cache.arm(&state.world);
    // M13.4: the move kinds wear the viewer's colour-vision palette.
    let pal = crate::egui_app::colorvision::palette_of(&state.world);
    let ground = crate::egui_app::theme::current().viewport;
    let mut drawn = Vec::new();
    for (name, entity) in camcraft_app::setup::operation_order(&state.world) {
        // Browser-eye hidden: the cache still exists and still posts, it
        // just does not draw (display state, never a geometry change).
        if state.hidden_paths.contains(&name) || state.op_hidden_by_setup(&name) {
            continue;
        }
        let selected = model::is_selected(&state.world, &entity);
        if !selected && !state.show_all_paths {
            continue;
        }
        let dim: f32 = if selected { 1.0 } else { OTHERS_DIM };
        let look = Look { pal: *pal, ground, dim: dim.to_bits(), arm: arm.to_bits() };
        let Some((lines, whole)) = cache.lines_of(&state.world, entity, look) else {
            continue;
        };
        drawn.push(entity);
        // #261: warned moves wear the gouge colour (crate::path_lines).
        if !append(&mut out, &lines) || !whole {
            cache.keep_only(&drawn);
            return out;
        }
    }
    cache.keep_only(&drawn);
    // #261 part 3: and draws the moves it had built as a dashed ghost
    // (crate::refusal_ghost) - off the refusal, never off the operation.
    if let Some(g) = state.refusal_ghost()
        && !crate::refusal_ghost::ghost_lines(&mut out, g, pal, arm)
    {
        return out;
    }
    // #261 part 2: a standing generate refusal marks the spot it names.
    if let Some(at) = state.refusal_focus() {
        crate::path_lines::refusal_mark(&mut out, at, pal, arm);
    }
    out
}

/// Append one path's lines within the vertex budget. False (logged)
/// when the budget is spent: the caller draws no further path.
fn append(out: &mut Vec<[f32; 6]>, lines: &[[f32; 6]]) -> bool {
    let room = MAX_OVERLAY_VERTICES.saturating_sub(out.len()) & !1;
    if lines.len() <= room {
        out.extend_from_slice(lines);
        return true;
    }
    out.extend_from_slice(lines.get(..room).unwrap_or(&[]));
    diag_warn!(
        "overlay: the cached paths outgrew the {MAX_OVERLAY_VERTICES}-vertex \
         display budget; the rest is truncated (the full path still posts)"
    );
    false
}

#[cfg(test)]
#[path = "path_overlay_tests.rs"]
mod tests;

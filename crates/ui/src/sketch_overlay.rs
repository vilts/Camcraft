//! The DXF profiles of the active setup in the viewport (#1863): each
//! profile's lines, arcs and circles drawn on the top of its stock, where
//! `sketch::world_pose` places it, and an X mark at each free end of an
//! open chain, in a colour of its own, so an open gap never reads as a
//! closed loop. A profile hidden by its browser eye is not drawn. Display
//! only: nothing here writes the world.
//!
//! Frame cost (crates/ui/AGENTS.md, #2238): the lines of a profile are
//! built once for its data (an `Arc` that a new import replaces) and its
//! pose, and read from [`SketchLineCache`] on each other frame.
//!
//! Provenance: our own display code over this repo's #1862 sketch data;
//! the arc flattening is plain polar sampling. No vendor material consulted.

use std::f64::consts::TAU;
use std::sync::{Arc, Mutex, Weak};

use camcraft_app::dxf::Primitive;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Point3, transform_point};
use camcraft_app::schema::Location;
use camcraft_app::sketch_data::SketchData;
use camcraft_app::{setup, sketch};

use crate::interact::push_line;
use crate::state::State;

/// Profile curves: a pale cyan, apart from the toolpath palette.
pub(crate) const SKETCH_RGB: [f32; 3] = [0.35, 0.85, 0.95];
/// The free ends of open chains: magenta, apart from every curve colour.
pub(crate) const OPEN_END_RGB: [f32; 3] = [0.95, 0.25, 0.80];
/// Segments for a full turn of a circle or an arc.
const SEGMENTS_PER_TURN: f64 = 72.0;
/// The most segments one arc is drawn with.
const MAX_ARC_SEGMENTS: usize = 720;

struct Entry {
    data: Weak<SketchData>,
    pose: [u64; 7],
    lines: Arc<Vec<[f32; 6]>>,
}

#[derive(Default)]
struct Inner {
    entries: Vec<Entry>,
    builds: u64,
}

/// The built lines of each drawn profile, keyed by its data and pose.
#[derive(Default)]
pub(crate) struct SketchLineCache {
    inner: Mutex<Inner>,
}

impl SketchLineCache {
    /// Keep exactly the visible inputs, including distinct poses of shared data.
    fn retain_visible(&self, profiles: &[(Arc<SketchData>, Location)]) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.entries.retain(|entry| {
            profiles.iter().any(|(data, pose)| {
                std::ptr::eq(entry.data.as_ptr(), Arc::as_ptr(data))
                    && entry.pose == crate::frame_cache::pose_bits(pose)
            })
        });
    }

    fn lines(&self, data: &Arc<SketchData>, pose: &Location) -> Arc<Vec<[f32; 6]>> {
        let key = crate::frame_cache::pose_bits(pose);
        let hit = |e: &Entry| std::ptr::eq(e.data.as_ptr(), Arc::as_ptr(data)) && e.pose == key;
        let mut inner = self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.entries.retain(|e| e.data.strong_count() > 0);
        if let Some(e) = inner.entries.iter().find(|e| hit(e)) {
            return Arc::clone(&e.lines);
        }
        let lines = Arc::new(build(data, pose));
        inner.builds = inner.builds.saturating_add(1);
        inner.entries.push(Entry { data: Arc::downgrade(data), pose: key, lines: Arc::clone(&lines) });
        lines
    }

    /// How many line builds this cache ran (the counter pin).
    #[cfg(test)]
    pub(crate) fn builds(&self) -> u64 {
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner).builds
    }
}

/// Every line of the active setup's visible profiles.
pub(crate) fn sketch_lines(state: &State) -> Vec<[f32; 6]> {
    let world = &state.world;
    let active = setup::active(world);
    let mut profiles = Vec::new();
    for name in sketch::placed_in(world, active.as_deref()) {
        if state.hidden_models.contains(&name) {
            continue;
        }
        let Some((entity, profile)) = sketch::find(world, &name) else { continue };
        let Some(pose) = sketch::world_pose(world, &entity) else { continue };
        profiles.push((Arc::clone(&profile.data), pose));
    }
    let cache = &state.frame_cache.sketches;
    cache.retain_visible(&profiles);
    let mut out = Vec::new();
    for (data, pose) in profiles {
        out.extend_from_slice(&cache.lines(&data, &pose));
    }
    out
}

/// A profile's curves and open-end marks at `pose`.
fn build(data: &SketchData, pose: &Location) -> Vec<[f32; 6]> {
    let mut out = Vec::new();
    let on_face = |p: Point2| transform_point(Point3::new(p.x, p.y, 0.0), pose.pos, pose.rot);
    for item in data.primitives() {
        primitive_lines(&mut out, &item.primitive, &on_face);
    }
    let arm = mark_arm(data.bounds());
    for chain in &data.chains().open {
        for end in [chain.start, chain.end] {
            end_mark(&mut out, pose, end, arm);
        }
    }
    out
}

/// One primitive as overlay lines on the face.
fn primitive_lines(out: &mut Vec<[f32; 6]>, primitive: &Primitive, on_face: &impl Fn(Point2) -> Point3) {
    match *primitive {
        Primitive::Line { start, end } => push_line(out, on_face(start), on_face(end), SKETCH_RGB),
        Primitive::Arc { center, radius, start_angle, sweep_angle } => {
            arc_lines(out, center, radius, start_angle, sweep_angle, on_face);
        }
        Primitive::Circle { center, radius } => arc_lines(out, center, radius, 0.0, TAU, on_face),
    }
}

fn arc_lines(
    out: &mut Vec<[f32; 6]>,
    center: Point2,
    radius: f64,
    start: f64,
    sweep: f64,
    on_face: &impl Fn(Point2) -> Point3,
) {
    if !(radius.is_finite() && start.is_finite() && sweep.is_finite()) || radius <= 0.0 {
        return;
    }
    let steps = (sweep.abs() / TAU * SEGMENTS_PER_TURN).ceil();
    let n = if steps.is_finite() { (steps as usize).clamp(2, MAX_ARC_SEGMENTS) } else { 2 };
    let at = |k: usize| {
        let a = start + sweep * k as f64 / n as f64;
        on_face(Point2::new(center.x + radius * a.cos(), center.y + radius * a.sin()))
    };
    let mut prev = at(0);
    for k in 1..=n {
        let next = at(k);
        push_line(out, prev, next, SKETCH_RGB);
        prev = next;
    }
}

/// Half the length of an end mark's strokes: 2 % of the drawing's
/// diagonal, held between 0.5 and 5 mm.
fn mark_arm(bounds: Option<(Point2, Point2)>) -> f64 {
    let diag = bounds.map_or(0.0, |(lo, hi)| (hi.x - lo.x).hypot(hi.y - lo.y));
    if diag.is_finite() { (diag * 0.02).clamp(0.5, 5.0) } else { 0.5 }
}

/// An X on the face at `p`.
fn end_mark(out: &mut Vec<[f32; 6]>, pose: &Location, p: Point2, arm: f64) {
    let at = |dx: f64, dy: f64| transform_point(Point3::new(p.x + dx, p.y + dy, 0.0), pose.pos, pose.rot);
    push_line(out, at(-arm, -arm), at(arm, arm), OPEN_END_RGB);
    push_line(out, at(-arm, arm), at(arm, -arm), OPEN_END_RGB);
}

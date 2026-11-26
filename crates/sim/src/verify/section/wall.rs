//! The wall probe of law (b): whether a region edge has part material beside it in the model section - our own probe (issues #43, #2657), split from `section.rs` by #3831; structure only.

use crate::geo2d::{Point2, Region};

/// How far off a region edge's midpoint the wall probe stands, mm: well
/// past float dust on a section that shares the edge, well inside any
/// feature the tolerance could care about.
const WALL_PROBE: f64 = 0.05;

#[cfg(test)]
thread_local! {
    /// The probes asked on this thread (#3831); the tests read it.
    pub(in crate::verify) static PROBES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whether region edge `p`-`q` is a wall in `section` (#43, #2657):
/// part material stands [`WALL_PROBE`] off its midpoint on either side.
/// An open rim over air has no material beside it and returns `false`.
/// It walks the whole section twice: law (b) asks it only for an edge
/// the move stands near (#3831).
pub(super) fn edge_is_wall(p: Point2, q: Point2, section: &Region) -> bool {
    #[cfg(test)]
    PROBES.with(|c| c.set(c.get() + 1));
    let (dx, dy) = (q.x - p.x, q.y - p.y);
    let len = dx.hypot(dy);
    if len <= f64::EPSILON {
        return false;
    }
    let (nx, ny) = (-dy / len * WALL_PROBE, dx / len * WALL_PROBE);
    let (mx, my) = (0.5 * (p.x + q.x), 0.5 * (p.y + q.y));
    section.contains(Point2::new(mx + nx, my + ny))
        || section.contains(Point2::new(mx - nx, my - ny))
}

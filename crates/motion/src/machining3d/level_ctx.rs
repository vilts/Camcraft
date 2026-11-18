//! The 3-D ladder's level context and its link/substitute arms (moved
//! out of `ladder.rs` by M11.2 C, structure only, plus the traverse plane
//! the user's level link chooses). Provenance: `ladder.rs`'s - the level
//! law, the shell tips, M8.1's dive room and plan law; no vendor
//! material consulted.

use std::cell::Cell;

use super::params::Pocket3DParams;
use super::shell::Field;

use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::entry::chord_hits_plan;
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::{Seg, chord_clears, push_segs};
use crate::plunge_law;

/// The level a dive lands on, bundled once per iteration for the link
/// and substitute arms: the cutter-center region, the level's planned
/// paths, the plane, the hop height, and the op params. These five
/// arrived together at every call site and always will - a dive
/// without a level is not a question anyone can answer.
pub(super) struct LevelCtx<'a> {
    pub(super) cen: &'a Region,
    pub(super) passes: &'a [Vec<Point2>],
    pub(super) z: f64,
    pub(super) hop_z: f64,
    pub(super) p: &'a Pocket3DParams,
    /// The offset shell's memory of the model; None on the plain
    /// ladder, where every tip is the plane itself.
    pub(super) field: Option<&'a Field>,
    /// The plane this level's descent traverses at (M11.2 D): clearance
    /// (every pre-M11.2 path), the retract plane, or - after a proved
    /// keep-down link - the level above itself.
    pub(super) tz: f64,
    /// Whether `tz` is the level above (a keep-down link landed): a
    /// traverse that still has XY to cover then hops over it instead of
    /// rapiding at depth.
    pub(super) kept: bool,
    /// The engagement limit's settling stretch (M11.3 D): the XY length
    /// of level-plane cut still owed the scaled feed after this level's
    /// entry - one tool diameter when the limit is on, consumed by
    /// `feed_leg`; `0` (every pre-limit path) feeds at the cutting feed.
    pub(super) settle: Cell<f64>,
    /// The HSM plan's proved arc walks per pass (M11.4, `hsm`): `None`
    /// on every other strategy, where each pass walks its points.
    pub(super) arcs: Option<&'a [Vec<Seg>]>,
}

/// Traverse to `to` at the level's plane: a rapid at `tz` - or, after a
/// keep-down link (the tool stands at depth), a hop over at the retract
/// plane and back down, never a rapid across at depth.
pub(super) fn traverse(b: &mut PathBuilder, lvl: &LevelCtx, to: Point2) {
    let at = b.at();
    if at.x == to.x && at.y == to.y {
        return;
    }
    if lvl.kept {
        b.push(MoveKind::Rapid, Point3::new(at.x, at.y, lvl.hop_z), None);
        b.push(MoveKind::Rapid, Point3::new(to.x, to.y, lvl.hop_z), None);
    }
    b.push(MoveKind::Rapid, Point3::new(to.x, to.y, lvl.tz), None);
}

impl LevelCtx<'_> {
    /// The plane an XY cut vertex of this level lands on: the shell
    /// tip at `q` over a decision disc of `r + ℓ` (ℓ = the tool
    /// radius, the chord cap `feed_leg` subdivides to - the disc is
    /// one tool diameter wide), floored at the level's own stepdown
    /// budget and at the pocket's drawn floor - the shell's early
    /// descent stops where the program stops, never deeper; or the
    /// plane itself with no shell mounted, the plain ladder's value
    /// byte for byte. `extra` widens the disc for a vertex whose
    /// sweep is wider than the chords reaching it (a helix orbit, by
    /// its radius).
    pub(super) fn tip(&self, q: Point2, extra: f64) -> f64 {
        match self.field {
            None => self.z,
            Some(f) => {
                let floor_z = (self.z - self.p.stepdown).max(self.p.bottom_z);
                f.tip(q, self.z, self.p.tool_diameter + extra, self.p.stock_to_leave, floor_z)
            }
        }
    }
}

fn hop_over_leg(b: &mut PathBuilder, lvl: &LevelCtx, q: Point2, zq: f64) {
    let at = b.at();
    b.push(MoveKind::Rapid, Point3::new(at.x, at.y, lvl.hop_z), None);
    b.push(MoveKind::Rapid, Point3::new(q.x, q.y, lvl.hop_z), None);
    let h = lvl.p.links.approach.plunge_height;
    let top = lvl.p.top_z;
    let hz = (top + h).min(lvl.hop_z.max(top));
    if h > 0.0 && hz.is_finite() && hz < b.at().z && hz > zq {
        b.push(MoveKind::Rapid, Point3::new(q.x, q.y, hz), None);
    }
    // Crossing an earlier leg does not prove the destination column clear.
    // Refuse an unsafe axial re-entry just as `plunge_from_top` does.
    plunge_law::guard(
        b,
        lvl.p.links.approach.center_cutting,
        lvl.p.tool_diameter * 0.5,
        q,
        zq,
        top,
        "a crossing hop",
    );
    b.push(MoveKind::Plunge, Point3::new(q.x, q.y, zq), Some(lvl.p.plunge_feed));
    let left = lvl.settle.get();
    if left > 0.0 && !left.is_nan() && zq >= at.z - 1e-9 {
        let d = (q.x - at.x).hypot(q.y - at.y);
        let rest = if d.is_finite() { (left - d).max(0.0) } else { 0.0 };
        lvl.settle.set(if rest > 1e-6 { rest } else { 0.0 });
    }
}

/// Feed from the builder's current point to `to`: with no shell, the
/// plain ladder's single move at the level plane; with the shell, the
/// leg split into chords of at most ℓ with every vertex landing on
/// its own tip - the cap is what makes the endpoint tips bound the
/// interpolation between them (`machining3d::shell`'s rising-leg
/// gouge). `extra` widens only the LAST vertex's decision disc.
pub(super) fn feed_leg(b: &mut PathBuilder, lvl: &LevelCtx, to: Point2, extra: f64) {
    let z = lvl.z;
    if lvl.field.is_none() {
        cut(b, lvl, to, z, true);
        return;
    }
    let at = b.at();
    let cap = lvl.p.tool_diameter * 0.5;
    let n = ((to.x - at.x).hypot(to.y - at.y) / cap).ceil();
    // A leg over a thousand tool diameters long is nonsense
    // geometry, not a reason to hang: it rides the plane, exactly as
    // the plain ladder's leg would - and the plane is safe by the
    // level law. (The comparison also routes a NaN distance here.)
    if n > 1024.0 || !n.is_finite() {
        cut(b, lvl, to, z, false);
        return;
    }
    let n = (n as usize).max(1);
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let q = Point2::new(at.x + (to.x - at.x) * t, at.y + (to.y - at.y) * t);
        let zq = lvl.tip(q, if i == n { extra } else { 0.0 });
        let cur = b.at();
        if crate::machining2d::seg_crosses_prior(Point2::new(cur.x, cur.y), q, cur.z, zq, b.moves()) {
            hop_over_leg(b, lvl, q, zq);
        } else {
            cut(b, lvl, q, zq, false);
        }
    }
}

/// One cutting feed onto `(q, zq)`, the engagement limit's settling
/// stretch read first (M11.3 D): while `lvl.settle` is owed, the move
/// runs at the scaled feed. A plane move (`split`) that crosses the
/// stretch's end is split there - lattice-snapped, the planar ladder's
/// law - and its rest runs at the cutting feed; a shell chord (no
/// split: a new vertex would need its own tip) is slowed whole, never
/// faster than asked (a chord is at most the tool radius long).
fn cut(b: &mut PathBuilder, lvl: &LevelCtx, q: Point2, zq: f64, split: bool) {
    let feed = lvl.p.feed;
    let left = lvl.settle.get();
    if left.is_nan() || left <= 0.0 {
        b.push(MoveKind::Feed, Point3::new(q.x, q.y, zq), Some(feed));
        return;
    }
    let slow = feed * lvl.p.links.approach.engagement_scale();
    let at = b.at();
    let d = (q.x - at.x).hypot(q.y - at.y);
    let level = (at.z - zq).abs() <= 1e-9;
    if split && level && d.is_finite() && d - left > 1e-6 && left > 1e-6 {
        let f = left / d;
        let s = Point2::new(lattice(at.x + (q.x - at.x) * f), lattice(at.y + (q.y - at.y) * f));
        b.push(MoveKind::Feed, Point3::new(s.x, s.y, zq), Some(slow));
        b.push(MoveKind::Feed, Point3::new(q.x, q.y, zq), Some(feed));
        lvl.settle.set(0.0);
        return;
    }
    b.push(MoveKind::Feed, Point3::new(q.x, q.y, zq), Some(slow));
    if zq < at.z - 1e-9 {
        // Still descending (the shell ramp's tail onto its tip, a chord
        // following the surface down): slowed, but the stretch counts
        // from the cut that settles at depth - never less slowing.
        return;
    }
    let rest = if d.is_finite() { (left - d).max(0.0) } else { 0.0 };
    lvl.settle.set(if rest > 1e-6 { rest } else { 0.0 });
}

/// The link from a dive that just finished at `d` up on plane `z` to
/// the pass start `e`: feed it when the chord is proven (inside this
/// level's center, crossing none of the level's planned paths), fly it
/// over - hop, traverse, dive - when it is not. Shared by every entry
/// arm so the law reads once (the 3-D seat of the proof; the 2-D
/// ladder runs the same law in `machining2d::levels`).
pub(super) fn dive_link(b: &mut PathBuilder, lvl: &LevelCtx, d: Point2, e: Point2) {
    if chord_clears(lvl.cen, d, e) && !chord_hits_plan(d, e, lvl.passes) {
        feed_leg(b, lvl, e, 0.0);
    } else {
        // M11.3: the second dive is named, never silent.
        b.note(format!(
            "entry: a second dive at the pass start ({:.3}, {:.3}) - the link from the entry at ({:.3}, {:.3}) is not proved clear at depth",
            e.x, e.y, d.x, d.y
        ));
        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, lvl.hop_z), None);
        b.push(MoveKind::Rapid, Point3::new(e.x, e.y, lvl.hop_z), None);
        plunge_from_top(b, lvl, e, lvl.p.plunge_feed);
        // The second dive bores full width again: the engagement
        // limit's stretch, while still owed, starts over from it.
        if lvl.settle.get() > 0.0 {
            lvl.settle.set(lvl.p.tool_diameter);
        }
    }
}

/// The axial feed onto `xy`'s tip (M11.3): a rapid first to the plunge
/// height over the top of the cut - air by the generator's contract -
/// held under the retract plane, then the feed.
pub(super) fn plunge_from_top(b: &mut PathBuilder, lvl: &LevelCtx, xy: Point2, feed: f64) {
    let z = lvl.tip(xy, 0.0);
    let h = lvl.p.links.approach.plunge_height;
    let top = lvl.p.top_z;
    let hz = (top + h).min(lvl.hop_z.max(top));
    if h > 0.0 && hz.is_finite() && hz < b.at().z && hz > z {
        b.push(MoveKind::Rapid, Point3::new(xy.x, xy.y, hz), None);
    }
    // The centre-cutting law (`plunge_law`): the top of the cut is the
    // highest the material can stand, so the check errs to solid.
    let cc = lvl.p.links.approach.center_cutting;
    plunge_law::guard(b, cc, lvl.p.tool_diameter * 0.5, xy, z, top, "a plunge");
    b.push(MoveKind::Plunge, Point3::new(xy.x, xy.y, z), Some(feed));
}

/// The entry arm's honorable failure (M8.1): the stated motion could
/// not be honored at this level for `why`, so the ladder substitutes
/// the axial dive at `at` - the pass start itself where it stands off
/// the wall, else a point along the first pass that does and links back
/// to the start at depth (M11.3 owed: one descent per level, the link
/// along the pass's own line), else the disc's own floor room
/// (`descent::plunge_at` chooses, in that order) - and says so in the
/// path's notes. A floor with no room either (`None`) - an erosion that
/// vanished, a region past the triangulation cap - falls back to the
/// wall dive this ladder knew before Phase 8, still loudly.
pub(super) fn substituted_dive(b: &mut PathBuilder, lvl: &LevelCtx, e: Point2, at: Option<Point2>, why: &str) -> Option<Point2> {
    let pf = lvl.p.links.approach.plunge(lvl.p.plunge_feed);
    match at {
        Some(d) => {
            b.note(format!("entry: {why} - axial dive on the floor instead at ({:.3}, {:.3})", d.x, d.y));
            traverse(b, lvl, d);
            plunge_from_top(b, lvl, d, pf);
            if d.x == e.x && d.y == e.y {
                return None;
            }
            dive_link(b, lvl, d, e);
            Some(d)
        }
        None => {
            b.note(format!(
                "entry: {why} - the floor has no dive room either; dived at the pass start"
            ));
            traverse(b, lvl, e);
            b.push(MoveKind::Rapid, Point3::new(e.x, e.y, lvl.tz), None);
            plunge_from_top(b, lvl, e, pf);
            None
        }
    }
}

/// Does the helix bore about orbit center `c` cross a leg the shell
/// left lifted under the previous plane `pz`? A leg's XY is level
/// invariant - one tiling walked at every plane - so the candidates
/// are the previous level's own legs, sampled with the cap and tip
/// that level's walker posted (the `extra = 0` disc: the narrowest
/// read, the conservative one). Crossing is the verifier's law,
/// exactly: XY reaches the bore's circle, and the leg's Z interval
/// overlaps the bore's band `(z, pz)` with positive measure - which
/// for legs floored at `z` reads as `min tip < pz` and `max tip > z`.
/// Legs at any earlier plane floor at `pz` or above and cannot
/// overlap the band at all; this level's legs are cut under the
/// bore's last depth; so the one plane is the complete ask.
pub(super) fn bore_meets_lifted_leg(lvl: &LevelCtx, pz: f64, c: Point2) -> bool {
    let Some(f) = lvl.field else {
        return false;
    };
    let p = lvl.p;
    let (rh, cap) = (p.helix_radius, (p.tool_diameter * 0.5).max(f64::EPSILON));
    let floor = (pz - p.stepdown).max(p.bottom_z);
    let z = lvl.z;
    lvl.passes.iter().any(|pass| {
        pass.windows(2).any(|w| {
            let (a, b) = (w[0], w[1]);
            if seg_reach(a, b, c) >= rh - 1e-9 {
                return false; // stays clear of the bore's circle
            }
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let n = (dx.hypot(dy) / cap).ceil();
            if !n.is_finite() || n > 1024.0 {
                return true; // nonsense geometry: refuse loudly
            }
            let n = (n as usize).max(1);
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for i in 0..=n {
                let t = i as f64 / n as f64;
                let q = Point2::new(a.x + dx * t, a.y + dy * t);
                let zq = f.tip(q, pz, p.tool_diameter, p.stock_to_leave, floor);
                lo = lo.min(zq);
                hi = hi.max(zq);
            }
            lo < pz - 1e-9 && hi > z + 1e-9
        })
    })
}

/// The closest an XY segment comes to a point: 0 when it straddles
/// or contains it.
fn seg_reach(a: Point2, b: Point2, c: Point2) -> f64 {
    let (ux, uy) = (b.x - a.x, b.y - a.y);
    let uu = ux * ux + uy * uy;
    let t = if uu > 0.0 {
        (((c.x - a.x) * ux + (c.y - a.y) * uy) / uu).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a.x + ux * t - c.x).hypot(a.y + uy * t - c.y)
}

/// Feed-walk a pass's points at the level (plane `z`, or the shell
/// tip of every chord between them; the first point is the current
/// position; `skip_first` avoids a zero move). Ring closure is
/// a property of the point DATA, not a motion added here: a waterline
/// loop arrives with its start point repeated at the end (each ring is
/// emitted closed by `waterline_passes`, and a chain of rings carries a
/// closing point per ring), so the walk's final feed lands each ring
/// back on its own start. Zigzag rows arrive open and stay open. An
/// earlier draft closed passes here by feeding the walk's last point
/// back to the first; for a lone ring that was a zero-length duplicate
/// of the data's own closing, but for a chained multi-ring pass it
/// dragged a diagonal across every intermediate ring - the boss crossing
/// bug. The law is: walk the data, add nothing. Pass `pi` of an HSM
/// level rides its proved arcs (M11.4) on the plain ladder once the
/// engagement limit's settling stretch is spent; the points otherwise.
pub(super) fn walk_pass(b: &mut PathBuilder, pass: &[Point2], lvl: &LevelCtx, pi: usize, skip_first: bool) {
    if skip_first
        && lvl.field.is_none()
        && lvl.settle.get() <= 0.0
        && let Some(segs) = lvl.arcs.and_then(|a| a.get(pi))
    {
        push_segs(b, segs, lvl.z, lvl.p.feed);
        return;
    }
    for q in pass.iter().skip(if skip_first { 1 } else { 0 }) {
        feed_leg(b, lvl, *q, 0.0);
    }
}

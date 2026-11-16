//! Law (f2), the posed-contact law: a LEANING pose on a cutting move
//! of a ball finish is a claim of contact. The lean laws that author
//! these poses (the stamp's surface-normal lean, the swarf ruling, the
//! drive curve's fan rest) are functions of the contact normal under
//! the ball - no contact, no normal, no lean - so a cutting move that
//! holds a lean while its ball stands off the part is a path whose
//! pose and position disagree: the ball is not where the plan put it.
//! That is the pivot-law failure class (a pose turned about the TIP
//! instead of the ball centre carries the ball `r·(a - up)` off its
//! skin), and it is invisible to the envelope's other halves: the
//! facet gouge law sees no bite in air, and the ridge rest law only
//! judges surface points two passes came near - which an all-air
//! pass never is.
//!
//! The one lean that does NOT claim contact is the shank escape (M5.1
//! stage F): a ball in air leans because the upright shank or holder
//! at the same centre would touch the part. It is excused exactly
//! when that is true - the upright band stack at the same ball centre
//! enters the solid ([`super::solid_index`], the solid law's own
//! exact reader). Everything else is judged: the ball centre (tip
//! plus `r` along the pose, the stream's tip contract) must stand
//! within `r + tol + leave + band` of the mesh, `band` the stamp's
//! micron contact lattice.
//!
//! A feed whose pose CHANGES between two leans (M13.7's mid-move
//! reading, [`super::midpose`]) claims contact all along it: its
//! ball is also judged at the poses the controller lerps between the
//! ends, interval by interval, at the worst gap of the move. A plunge
//! is an entry: only its end claims (the law judged it there before,
//! and still does).
//!
//! Upright moves are not judged here: an upright cut claims nothing
//! (rasters cross open floor, links and roughing levels clear air by
//! design), and "every path point must touch" would convict all of
//! them. The verdict reads as [`Warning::Envelope`] rest - material
//! the claimed contact never reached - at the worst move.
//!
//! Provenance: ball-centre / tool-tip CL geometry and tangency from
//! public multi-axis CAM literature (ISO 6983 tool-length practice);
//! point-to-triangle distance from Ericson, "Real-Time Collision
//! Detection" 5.1 (via `stock_sim::dist_seg_tri`). Our own law; no
//! vendor material consulted.

use super::midpose;
use super::probe::{Cutter, Frame};
use super::solid_index::SolidIndex;
use super::types::Warning;
use super::upright::is_upright;
use crate::cl::{normalize_axis, MoveKind, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::stock_sim::dist_seg_tri;
use crate::tools::ToolGeom;

/// The stamp's contact lattice: a ball within this of tangency is the
/// tangency the lean law read (`stamp.rs`: 0.01 + 1e-6·r).
const CONTACT_BAND: f64 = 0.01;

/// Law (f2): `Some(Envelope)` at the worst leaning cut whose ball
/// stands off the part beyond the gate, `None` when every lean is a
/// contact (or an excused shank escape). `r` is the ball radius,
/// `tol` the finish budget, `leave` the stock the op leaves.
pub fn posed_contact(
    path: &Toolpath,
    facets: &[Triangle],
    tool: &ToolGeom,
    r: f64,
    tol: f64,
    leave: f64,
) -> Option<Warning> {
    if facets.is_empty() || !(r.is_finite() && r > 0.0) {
        return None;
    }
    let gate = tol.max(0.0) + leave.max(0.0) + CONTACT_BAND + 1e-6 * r;
    let grid = Grid::build(facets, r + gate);
    let atoms = Cutter::flat(tool).atoms();
    let body: Vec<_> = atoms.iter().copied().filter(|a| a.z0 >= tool.flute_len).collect();
    let reach = atoms.iter().map(|a| a.z1).fold(0.0, f64::max);
    let poses = midpose::poses(path);
    let mut index: Option<Option<SolidIndex>> = None;
    let mut worst: Option<(usize, f64)> = None;
    // The gap a leaning ball at `tip` under unit pose `a` stands off
    // the part beyond the gate; None when it touches or is excused.
    let mut judge = |tip: Point3, a: [f64; 3]| -> Option<f64> {
        let c = Point3::new(tip.x + r * a[0], tip.y + r * a[1], tip.z + r * a[2]);
        let gap = grid.dist_within(c, facets) - r;
        if gap <= gate {
            return None;
        }
        // The shank escape: an upright tool at this same centre would
        // put a band into the part, so the lean answers the shank,
        // not a contact. Built lazily - most paths never ask.
        // A mesh the solid reader cannot read honestly excuses
        // nothing (fail closed: the lean is judged).
        if !body.is_empty()
            && let Some(ix) = index.get_or_insert_with(|| SolidIndex::build(facets)).as_mut()
        {
            let fr = Frame::new(Point3::new(c.x, c.y, c.z - r), None);
            if ix.hits(&fr, [0.0, 0.0, 0.0], &body, 0.0, 0.0, &[], &[]) {
                return None;
            }
        }
        Some(if gap.is_finite() { gap } else { brute(c, facets) - r })
    };
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid || !m.to.is_finite() {
            continue;
        }
        let Some(a) = m.axis.and_then(normalize_axis).filter(|a| !is_upright(a)) else {
            continue;
        };
        let mut gap = judge(m.to, a);
        // Mid-move (M13.7): a FEED that leans at BOTH ends claims
        // contact all along - the lerped poses between are contact
        // claims too, so each interval's ball is judged where the
        // controller puts it. A move leaning out of (or into) the
        // upright is an approach: its upright end claims nothing, and
        // neither do the poses on the way; a plunge is the entry from
        // the clearance down to its first contact - only its end
        // claims, its travel is air by design (a ring's plunge-in
        // turns from the last ring's pose to the new one's while it
        // descends 20 mm, and the end-pose reading rightly passes
        // it). Constant poses pay nothing.
        if m.kind == MoveKind::Feed
            && let Some((fa, fb)) = poses.get(i).and_then(|&(f, t)| midpose::change(f, t))
            && !is_upright(&fa)
            && !is_upright(&fb)
        {
            for iv in midpose::intervals(m, fa, fb, reach, CONTACT_BAND) {
                if let Some(g) = judge(iv.pm, iv.am)
                    && gap.is_none_or(|w| g > w)
                {
                    gap = Some(g);
                }
            }
        }
        if let Some(gap) = gap
            && worst.is_none_or(|(_, g)| gap > g)
        {
            worst = Some((i, gap));
        }
    }
    worst.map(|(at, g)| Warning::Envelope {
        at,
        gouge_mm: 0.0,
        rest_mm: g.clamp(0.0, 1e9),
    })
}

fn brute(p: Point3, facets: &[Triangle]) -> f64 {
    facets.iter().map(|t| dist_seg_tri(p, p, t)).fold(f64::INFINITY, f64::min)
}

/// A uniform XY bucket grid over the facets, cell = the reach asked
/// of it: a query visits its own cell and the eight around it, which
/// holds every facet whose XY box comes within `reach`.
struct Grid {
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
}

impl Grid {
    fn build(facets: &[Triangle], reach: f64) -> Self {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for t in facets {
            for v in [t.a, t.b, t.c] {
                if v.is_finite() {
                    x0 = x0.min(v.x);
                    y0 = y0.min(v.y);
                    x1 = x1.max(v.x);
                    y1 = y1.max(v.y);
                }
            }
        }
        if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
            return Self { x0: 0.0, y0: 0.0, cell: 1.0, nx: 0, ny: 0, cells: Vec::new() };
        }
        // Never more than 512 cells a side: a coarser grid is still
        // exact (a superset per query), only slower.
        let span = (x1 - x0).max(y1 - y0).max(1e-9);
        let cell = reach.max(span / 512.0).max(1e-6);
        let nx = (((x1 - x0) / cell).floor() as usize + 1).min(513);
        let ny = (((y1 - y0) / cell).floor() as usize + 1).min(513);
        let mut cells = vec![Vec::new(); nx * ny];
        let ix = |v: f64, o: f64, n: usize| (((v - o) / cell).floor().max(0.0) as usize).min(n - 1);
        for (fi, t) in facets.iter().enumerate() {
            if !(t.a.is_finite() && t.b.is_finite() && t.c.is_finite()) {
                continue;
            }
            let (i0, i1) = (ix(t.a.x.min(t.b.x).min(t.c.x), x0, nx), ix(t.a.x.max(t.b.x).max(t.c.x), x0, nx));
            let (j0, j1) = (ix(t.a.y.min(t.b.y).min(t.c.y), y0, ny), ix(t.a.y.max(t.b.y).max(t.c.y), y0, ny));
            for j in j0..=j1 {
                for i in i0..=i1 {
                    cells[j * nx + i].push(fi as u32);
                }
            }
        }
        Self { x0, y0, cell, nx, ny, cells }
    }

    /// The distance from `p` to the nearest facet whose XY box lies
    /// within one cell (the reach) of it; infinity when none does -
    /// then nothing stands within the reach at all.
    fn dist_within(&self, p: Point3, facets: &[Triangle]) -> f64 {
        if self.nx == 0 || self.ny == 0 {
            return f64::INFINITY;
        }
        let fi = ((p.x - self.x0) / self.cell).floor();
        let fj = ((p.y - self.y0) / self.cell).floor();
        if !(fi.is_finite() && fj.is_finite()) {
            return f64::INFINITY;
        }
        let mut best = f64::INFINITY;
        for dj in -1i64..=1 {
            for di in -1i64..=1 {
                let (i, j) = (fi as i64 + di, fj as i64 + dj);
                if i < 0 || j < 0 || i >= self.nx as i64 || j >= self.ny as i64 {
                    continue;
                }
                for &f in &self.cells[j as usize * self.nx + i as usize] {
                    if let Some(t) = facets.get(f as usize) {
                        best = best.min(dist_seg_tri(p, p, t));
                    }
                }
            }
        }
        best
    }
}

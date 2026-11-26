//! Pass start Inside / Outside (#3316): the rings of one planar area as
//! level curves of the distance from the START side only, clipped to the
//! cutter-centre area as open pieces. A ring of the whole area
//! (`waterscan`) splits at a narrow place into closed loops that carry
//! arcs of both sides, so no order of whole loops cuts the far side late
//! enough; a level curve of one side has no arc of the other side.
//!
//! Outside: ring 0 is the stretch of the area's boundary on its outer
//! edge, ring j the outer edge (holes filled) eroded by `j` stepovers and
//! clipped to the area, and the walls (the holes' and keep-outs' edges)
//! come last. Inside is the mirror: ring 0 on the holes, ring j the holes
//! grown by `j` stepovers, the outer wall last.
//!
//! Provenance: contour-parallel pocketing as level sets of the distance
//! to the boundary (Held, "On the Computational Geometry of Pocket
//! Machining", Springer LNCS 500, 1991), measured from one side; the
//! side split, the order and the proofs are ours. No vendor material
//! consulted.

use crate::geo2d::{Loop, Point2, Region, RingClip};

use super::face::GenError;
use super::level_order::{Piece, WALL, order};
use super::pass_start::{PassStart, StartSide};
use super::pocket::PocketParams;
use super::pocket_plan::plan_passes;
use super::pocket_strategy::PocketStrategy;
use super::ring_fill::{Band, RING, closed_walk, fills};
use super::ring_side::{mirror, share_ends};
use super::ring::dist;

/// How far (mm) the area's own boundary may stand from the start side
/// and still be read as on it: 10 um, twice Clipper's 5 um arc flats
/// (`ARC_TOL_UNITS`). #3824: the area and the start side flatten the
/// same round joins apart, up to 5 um each; at the old micron a stretch
/// of the part wall where the two flats part by more than that read as
/// the other side - on fig8, two 0.117 mm slivers of the wall that the
/// wall ring left as gaps and Outside cut first, each reached by a lift
/// over the part and a plunge into material.
pub(super) const SIDE_EPS: f64 = 1e-2;

/// Pieces shorter than this (mm) are dropped: a stub where a wall meets
/// the start side, which the neighbouring pieces' discs already cover.
const MIN_PIECE: f64 = 1e-2;

/// The ring plan of one area for `side`, with the warnings it owes the
/// op page. Auto, a row strategy, and an Inside with no hole to start
/// from keep the planner's old plan byte for byte. The completeness
/// guard: a start-side plan whose passes leave part of the cutter-centre
/// area unswept is never posted; the area takes the Auto plan, named.
pub(super) fn ring_plan(
    region: &Region,
    center: &Region,
    p: &PocketParams,
    strategy: PocketStrategy,
    side: &StartSide,
) -> Result<(Vec<Vec<Point2>>, Vec<String>), GenError> {
    let off = p.tool_diameter * 0.5 + p.stock_to_leave;
    side_plan(region, center, (off, p.tool_diameter * 0.5), (p.tiling_step(), p.climb), strategy, side)
}

/// The 3-D rough's level (#3361): [`ring_plan`] over a cutter-centre
/// area that is its own outline (the level law erodes the target once,
/// so the start side is read off `center`'s own edges). Auto, a row
/// strategy and an Inside with no hole keep `plan_passes` byte for byte.
pub fn level_ring_plan(
    center: &Region,
    tool_r: f64,
    step: f64,
    climb: bool,
    strategy: PocketStrategy,
    side: &StartSide,
) -> Result<(Vec<Vec<Point2>>, Vec<String>), GenError> {
    side_plan(center, center, (0.0, tool_r), (step, climb), strategy, side)
}

fn side_plan(
    region: &Region,
    center: &Region,
    (off, tool_r): (f64, f64),
    (step, climb): (f64, bool),
    strategy: PocketStrategy,
    side: &StartSide,
) -> Result<(Vec<Vec<Point2>>, Vec<String>), GenError> {
    let rings = matches!(strategy, PocketStrategy::Waterline | PocketStrategy::OffsetRings | PocketStrategy::Adaptive);
    if side.start != PassStart::Auto
        && rings
        && let Some(pieces) = level_pieces(region, center, off, step, climb, side, tool_r)?
    {
        let plan = order(pieces, center, tool_r, side)?;
        let left = unswept(center, &plan.passes, tool_r)?;
        if left <= LEFT_TOL {
            return Ok((plan.passes, plan.notes));
        }
        let auto = plan_passes(center, step, climb, strategy)?;
        return Ok((auto, vec![side.fallback_note(left)]));
    }
    Ok((plan_passes(center, step, climb, strategy)?, Vec::new()))
}

/// The most area (mm2) of the cutter-centre area that the passes may leave
/// unswept before the plan counts as incomplete.
const LEFT_TOL: f64 = 0.1;

/// The area of `center` that no disc of radius `r` on `passes` reaches,
/// read after a 50 um erosion: the arc-flat and ring-end slivers of a
/// complete plan vanish, a standing column (any patch wider than 0.1 mm)
/// stays. Zero for a complete plan.
pub(super) fn unswept(center: &Region, passes: &[Vec<Point2>], r: f64) -> Result<f64, GenError> {
    let swept = Region::sweep(passes, r * (1.0 - 1e-3))?;
    Ok(center.difference(&swept)?.offset(-0.05)?.area().abs())
}

/// The pieces of every ring of `center` (the cutter-centre area of
/// `region` eroded by `off`), each walked so that the uncut side is on
/// the right for `climb` (the core-first planner's sense). Each part of
/// the material (an outer loop with its own holes; material inside a
/// hole is a part of its own) starts from its own edges. `None` when no
/// part has the start side (Inside on an area with no hole).
///
/// `around` (#3744): the area surrounds the part, so every start side
/// cuts ONE ring set, the distance from the part (its holes): Outside
/// walks the Inside pieces mirrored (`ring_side::mirror`). A part with
/// no hole keeps its own rings (core first for Inside, from its edge for
/// Outside: offsets of the same edge).
pub(super) fn level_pieces(
    region: &Region,
    center: &Region,
    off: f64,
    step: f64,
    climb: bool,
    side: &StartSide,
    tool_r: f64,
) -> Result<Option<Vec<Piece>>, GenError> {
    let (start, around) = (side.start, side.around_part);
    let Some((lo, hi)) = center.bounds() else {
        return Err(GenError::Empty("empty pocket region".into()));
    };
    if !(step.is_finite() && step > 0.0 && off.is_finite()) {
        return Err(GenError::Invalid("the ring step must be positive and finite".into()));
    }
    if start == PassStart::Auto {
        return Ok(None);
    }
    let cap = (dist(lo, hi) / step + 10.0).max(16.0) as u32;
    let mut out = Vec::new();
    let mut started = false;
    for part in material_parts(region)? {
        let c = center.intersect(&part)?;
        if c.is_empty() {
            continue;
        }
        let filled = Region::new(part.loops().iter().filter(|l| l.is_ccw()).cloned().collect())?;
        let holes = filled.difference(&part)?;
        let has_holes = holes.area().abs() > 1e-9;
        let from_holes = start == PassStart::Inside || (around && has_holes);
        let first = out.len();
        let ok = if from_holes && !has_holes {
            core_first(&c, step, climb, cap, &mut out)?
        } else if from_holes {
            started = true;
            // The distance from the holes: the holes' centre keep-out grown.
            // `off` 0: the area is its own outline (the 3-D level).
            let hc = if off == 0.0 { holes } else { holes.offset(off)? };
            sides(&c, &hc.offset(SIDE_EPS)?, true, climb, &mut out)?;
            let mut done = false;
            let mut last = 0;
            for j in 1..=cap {
                let g = hc.offset(f64::from(j) * step)?;
                // `g` lies on the left of its loops and is the cut side.
                // Ring `j` is level `RING * j`: its fills (#3788) lie between.
                curves(&c, &g, RING.saturating_mul(j), !climb, &mut out)?;
                last = j;
                if c.difference(&g)?.is_empty() {
                    done = true;
                    break;
                }
            }
            if side.around_part {
                let fill = Band { c: &c, hc: &hc, j: 0, step, tool_r, climb };
                add_fills(&mut out, first, &fill, last)?;
            }
            if start == PassStart::Outside {
                mirror(out.get_mut(first..).unwrap_or_default());
            }
            done
        } else {
            started = true;
            // The distance from the outer edge: the filled part eroded.
            let fc = if off == 0.0 { filled } else { filled.offset(-off)? };
            sides(&c, &fc.offset(-SIDE_EPS)?, false, climb, &mut out)?;
            let mut done = false;
            for j in 1..=cap {
                let g = fc.offset(-f64::from(j) * step)?;
                if g.is_empty() {
                    done = true;
                    break;
                }
                // `g` lies on the left of its loops and is the uncut side.
                curves(&c, &g, j, climb, &mut out)?;
            }
            done
        };
        if !ok {
            return Err(GenError::Invalid("pass start rings did not converge; check the stepover".into()));
        }
    }
    if !started {
        return Ok(None);
    }
    share_ends(&mut out);
    Ok(Some(out))
}

/// The fills (#3788) of each band of one part's rings, `out[first..]`.
fn add_fills(out: &mut Vec<Piece>, first: usize, band: &Band<'_>, last: u32) -> Result<(), GenError> {
    let walks = |out: &[Piece], lev: u32| -> Vec<Vec<Point2>> {
        out.iter().skip(first).filter(|p| !p.core && p.level == lev).map(closed_walk).collect()
    };
    let mut add = Vec::new();
    for j in 1..=last {
        let (inner, outer) = (walks(out, RING.saturating_mul(j - 1)), walks(out, RING.saturating_mul(j)));
        add.extend(fills(&Band { j, ..*band }, &inner, &outer)?);
    }
    out.extend(add);
    Ok(())
}

/// The parts of the material: each outer (counter-clockwise) loop with
/// the holes directly inside it. Material inside a hole is a part of its
/// own, at any depth of nesting.
fn material_parts(region: &Region) -> Result<Vec<Region>, GenError> {
    let loops = region.loops();
    let inside = |outer: &Loop, p: Point2| Region::new(vec![outer.clone()]).map(|r| r.contains(p));
    let mut parts = Vec::new();
    for (i, o) in loops.iter().enumerate().filter(|(_, l)| l.is_ccw()) {
        let mut own = vec![o.clone()];
        for h in loops.iter().filter(|l| !l.is_ccw()) {
            let Some(&p) = h.points().first() else { continue };
            if !inside(o, p)? {
                continue;
            }
            // A hole inside a nearer outer loop (itself inside `o`) is that one's.
            let mut nearer = false;
            for (k, o2) in loops.iter().enumerate().filter(|(_, l)| l.is_ccw()) {
                let Some(&q) = o2.points().first() else { continue };
                if k != i && inside(o, q)? && inside(o2, p)? {
                    nearer = true;
                    break;
                }
            }
            if !nearer {
                own.push(h.clone());
            }
        }
        parts.push(Region::new(own)?);
    }
    Ok(parts)
}

/// A part with no hole to start Inside from: its rings core first, as
/// Auto cuts them (whole rings, walked like Auto's), marked as such.
fn core_first(c: &Region, step: f64, climb: bool, cap: u32, out: &mut Vec<Piece>) -> Result<bool, GenError> {
    let mut rings: Vec<(u32, Vec<Point2>)> = Vec::new();
    for k in 0..=cap {
        let ring = if k == 0 { c.clone() } else { c.offset(-f64::from(k) * step)? };
        if ring.is_empty() {
            for (j, pts) in rings {
                out.push(Piece { level: 1 + k - j, pts, closed: true, core: true });
            }
            return Ok(true);
        }
        for l in ring.loops() {
            if l.points().len() >= 2 {
                rings.push((k, walk(l.points(), !climb)));
            }
        }
    }
    Ok(false)
}

/// The area's own boundary split by `start_side` (inside it for Inside,
/// outside it for Outside): ring 0 on the start side, the walls (level
/// [`WALL`]) elsewhere. The area lies on the left of its loops, and it
/// is the uncut side of ring 0 and the cut side of a wall.
pub(super) fn sides(center: &Region, start_side: &Region, start_inside: bool, climb: bool, out: &mut Vec<Piece>) -> Result<(), GenError> {
    for l in center.loops() {
        let ring = walk(l.points(), climb);
        push(out, 0, &ring, start_side.clip_ring(&ring, start_inside)?);
        // The walls from the same walk, then turned: where a wall meets
        // ring 0 both carry the very same point.
        let back = walk(&ring, true);
        let wall = match start_side.clip_ring(&ring, !start_inside)? {
            RingClip::Pieces(ps) => RingClip::Pieces(ps.into_iter().rev().map(|p| walk(&p, true)).collect()),
            whole => whole,
        };
        push(out, WALL, &back, wall);
    }
    Ok(())
}

/// Ring `level`: the loops of `g` clipped to `center`, reversed when
/// `reverse`.
fn curves(center: &Region, g: &Region, level: u32, reverse: bool, out: &mut Vec<Piece>) -> Result<(), GenError> {
    for l in g.loops() {
        let ring = walk(l.points(), reverse);
        push(out, level, &ring, center.clip_ring(&ring, true)?);
    }
    Ok(())
}

fn walk(pts: &[Point2], reverse: bool) -> Vec<Point2> {
    if reverse { pts.iter().rev().copied().collect() } else { pts.to_vec() }
}

fn push(out: &mut Vec<Piece>, level: u32, ring: &[Point2], clip: RingClip) {
    match clip {
        RingClip::Whole if ring.len() >= 2 => out.push(Piece { level, pts: ring.to_vec(), closed: true, core: false }),
        RingClip::Whole => {}
        RingClip::Pieces(ps) => {
            for pts in ps {
                let len: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
                if pts.len() >= 2 && len >= MIN_PIECE {
                    out.push(Piece { level, pts, closed: false, core: false });
                }
            }
        }
    }
}

#[cfg(test)]
mod guard_tests {
    use super::{LEFT_TOL, unswept};
    use crate::geo2d::{Loop, Point2, Region};

    fn circle(r: f64) -> Vec<Point2> {
        (0..128).map(|k| {
            let a = f64::from(k) * std::f64::consts::TAU / 128.0;
            Point2::new(r * a.cos(), r * a.sin())
        })
        .collect()
    }

    /// The completeness guard reads a standing column, never a sliver: a
    /// lone ring on a disc of radius 10 leaves the core (radius 6) unswept;
    /// rings 4 mm apart down to the middle leave nothing.
    #[test]
    fn the_guard_reads_a_standing_core_and_passes_a_whole_plan() {
        let Ok(l) = Loop::new(circle(10.0)) else { panic!("loop") };
        let Ok(disc) = Region::new(vec![l]) else { panic!("disc") };
        let ring = |r: f64| {
            let mut c = circle(r);
            c.push(c[0]);
            c
        };
        let Ok(lone) = unswept(&disc, &[ring(10.0)], 4.0) else { panic!("unswept") };
        assert!(lone > 100.0 && lone < 114.0, "the core of radius 6 stands: {lone}");
        let Ok(whole) = unswept(&disc, &[ring(10.0), ring(6.0), ring(2.0)], 4.0) else { panic!("unswept") };
        assert!(whole <= LEFT_TOL, "a whole plan: {whole}");
        assert!(unswept(&disc, &[vec![Point2::new(f64::NAN, 0.0)]], 4.0).is_err(), "NaN is refused");
    }
}

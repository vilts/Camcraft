//! Contour under an open boundary (M11.5 owed): the picked open chain is
//! the contour itself - the cutter rides it one offset away on the chosen
//! side (tool radius plus stock to leave, plus the Boundary offset, as
//! the containment places the tool), run on straight past both ends by
//! the Boundary extension, and the chain contour's own lead-in / lead-out
//! enters and leaves beyond those ends.
//!
//! The offset curve is not a new offsetter: it is the chosen side of the
//! boundary of the disc swept along the chain (`Region::sweep`, the
//! clipper2 Minkowski buffer) - the locus at exactly the offset from the
//! chain, so the tool edge never crosses it. A chain that crosses itself,
//! or whose offset folds onto itself (the buffer grows a hole, or the
//! chosen side breaks into pieces), is refused by name, never trimmed.
//! The viewport's fill (`footprint`) is that same run dilated by the
//! tool radius, so the band and the path cannot disagree.
//!
//! Provenance: open-profile contouring of published CAM practice (cutter
//! radius compensation to one side of an open profile, RS-274 G41/G42
//! semantics) and the textbook offset-curve-as-Minkowski-boundary
//! construction; the laws are ours. No vendor material consulted.

use super::*;
use super::boundary::{Family, OpenGeom, pick_side_left};
use super::boundary_turn::sides_of_chain;
use crate::machining2d::{ContourParams, contour_along_chain};
use crate::units::Unit;

/// Points closer than this (mm) are one point.
const SAME: f64 = 1e-9;
/// Slack (mm) of the cap / run-end classification: well above the
/// buffer's micron lattice, well below any real edge.
const TOL: f64 = 1e-3;

/// The open pass: the tool-centre run along the open boundary and the
/// path riding it. `base` is the operation's own area (what the
/// selection or source encloses) - it only decides which side is Inside.
pub(super) fn open_pass(base: &Region, ctx: &AreaCtx, cp: &ContourParams) -> Result<Toolpath, GenError> {
    let Some((_, side)) = &ctx.open else {
        return Err(unreached("contour open boundary"));
    };
    let c = &ctx.controls;
    let (run, d) = open_run(base, ctx, cp.stock_to_leave)?;
    let mut path = contour_along_chain(&run, cp)?;
    path.notes.push(format!(
        "contour: rides the open boundary {d:.3} mm to its {} side (tool centre), {} entering and leaving past its ends",
        if *side == ChainSide::Inside { "Inside (A)" } else { "Outside (B)" },
        if c.extension > 0.0 { "extended by the Boundary extension," } else { "its leads" },
    ));
    Ok(path)
}

/// The band the open pass sweeps, for the viewport (M11.5 owed): the
/// SAME tool-centre run the pass rides, run on past its start and end by
/// the approach's `ext_in` / `ext_out` (the start is the run's last
/// point under conventional travel), dilated by the tool radius. The
/// leads are entry and exit motion, not the band - as a closed ring's
/// region leaves its leads out.
pub(super) fn open_band(
    base: &Region,
    ctx: &AreaCtx,
    stock_to_leave: f64,
    climb: bool,
    ext_in: f64,
    ext_out: f64,
) -> Result<Region, GenError> {
    let (run, _) = open_run(base, ctx, stock_to_leave)?;
    let (e0, e1) = if climb { (ext_in, ext_out) } else { (ext_out, ext_in) };
    Ok(Region::sweep(&[extended_ends(&run, e0, e1)], ctx.tool_r)?)
}

/// The tool-centre run of the open pass and its offset from the chain:
/// the chain run on past both ends by the Boundary extension, offset to
/// the chosen side by the containment's placement, the Boundary offset
/// and the stock to leave, ordered with the material on its right (the
/// climb order, #1555). Every refusal of the pass is named here.
pub(super) fn open_run(base: &Region, ctx: &AreaCtx, stock_to_leave: f64) -> Result<(Vec<Point2>, f64), GenError> {
    let Some((geom, side)) = &ctx.open else {
        return Err(unreached("contour open boundary"));
    };
    if ctx.family != Family::Contour {
        return Err(unreached("contour open boundary family"));
    }
    if let OpenGeom::Chains(cs) = geom {
        return Err(GenError::Invalid(format!(
            "a Contour rides one open chain - its open boundary spells {} separate chains; pick the edges between them or use a clearing kind to cut between them",
            cs.len()
        )));
    }
    let OpenGeom::Chain(raw) = geom else {
        return Err(GenError::Invalid(
            "a Contour's open boundary must be an open chain - a closed ring is ridden by picking it as the selection".into(),
        ));
    };
    if !ctx.extra.is_empty() {
        return Err(GenError::Invalid(
            "include / exclude boundaries do not shape a Contour riding an open boundary - remove them or the open boundary".into(),
        ));
    }
    let chain = normalized(raw)?;
    if crosses_itself(&chain) {
        return Err(GenError::Invalid(
            "the open boundary crosses or touches itself - a Contour cannot ride it to one side; pick a simple chain".into(),
        ));
    }
    if base.is_empty() {
        return Err(GenError::Invalid(
            "a Contour's open boundary picks its side against the operation's own area - the selection bounds none; add the face or ring the chain trims".into(),
        ));
    }
    // Which side of the chain is Inside: the boundary law's own rule.
    let r = ctx.tool_r;
    let reach = match &ctx.stock {
        Some(s) => s.union(base)?.offset_sharp(2.0 * r)?,
        None => base.offset_sharp(2.0 * r)?,
    };
    let (left, right, _) = sides_of_chain(&chain, &reach)?;
    let mut tool_left = (*side == ChainSide::Inside) == pick_side_left(base, &left, &right)?;
    // Where the tool centre stands: one radius out (its edge on the
    // chain), on it, or one radius across - plus the Boundary offset and
    // the stock to leave.
    let c = &ctx.controls;
    let place = match c.containment {
        None | Some(Containment::Outside) => r,
        Some(Containment::On) => 0.0,
        Some(Containment::Inside) => -r,
    };
    let mut d = place + c.offset + stock_to_leave;
    if !d.is_finite() {
        return Err(GenError::Invalid("the open boundary's offset is not a finite distance".into()));
    }
    if d < 0.0 {
        tool_left = !tool_left;
        d = -d;
    }
    let ext = extended(&chain, c.extension);
    // #1555: the run is handed on with the material on its RIGHT, the
    // order climb travels it under M3 (conventional reverses it). The
    // offset comes back along the CCW tube, the chain - the material's
    // side - on its left, so it turns round; a run on the chain itself
    // keeps the chain's order when the tool stands left of it.
    let mut run = if d < TOL { ext } else { offset_run(&ext, d, tool_left, ctx.unit)? };
    if d >= TOL || !tool_left {
        run.reverse();
    }
    Ok((run, d))
}

/// Repeated points out, oriented from the lexicographically lower end
/// (the order `sides_of_chain` reads its left by), at least one edge.
fn normalized(raw: &[Point2]) -> Result<Vec<Point2>, GenError> {
    let mut p: Vec<Point2> = Vec::with_capacity(raw.len());
    for q in raw {
        if !(q.x.is_finite() && q.y.is_finite()) {
            return Err(GenError::Invalid("the open boundary holds a non-finite point - re-pick it".into()));
        }
        if p.last().is_none_or(|l: &Point2| dist(*l, *q) > SAME) {
            p.push(*q);
        }
    }
    let (Some(&first), Some(&last)) = (p.first(), p.last()) else {
        return Err(GenError::Invalid("the open boundary holds no points - re-pick it".into()));
    };
    if p.len() < 2 {
        return Err(GenError::Invalid("the open boundary is a single point - re-pick a real edge".into()));
    }
    if dist(first, last) <= TOL {
        return Err(GenError::Invalid(
            "the open boundary closes on itself - a Contour rides a closed ring by picking it as the selection".into(),
        ));
    }
    if (last.x, last.y) < (first.x, first.y) {
        p.reverse();
    }
    Ok(p)
}

fn dist(a: Point2, b: Point2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Closest distance between segments ab and cd (zero when they cross).
pub(super) fn seg_seg(a: Point2, b: Point2, c: Point2, d: Point2) -> f64 {
    let (d1, d2) = (cross(a, b, c), cross(a, b, d));
    let (d3, d4) = (cross(c, d, a), cross(c, d, b));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return 0.0;
    }
    let pd = |p: Point2, a: Point2, b: Point2| foot(p, a, b).1;
    pd(a, c, d).min(pd(b, c, d)).min(pd(c, a, b)).min(pd(d, a, b))
}

/// The unclamped parameter of `p`'s foot on ab and the clamped distance.
fn foot(p: Point2, a: Point2, b: Point2) -> (f64, f64) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    if l2 <= 0.0 {
        return (0.0, dist(p, a));
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2;
    let tc = t.clamp(0.0, 1.0);
    (t, dist(p, Point2::new(a.x + dx * tc, a.y + dy * tc)))
}

/// Whether two non-adjacent edges of the chain cross or touch.
fn crosses_itself(p: &[Point2]) -> bool {
    let n = p.len();
    for i in 0..n.saturating_sub(1) {
        for j in (i + 2)..n.saturating_sub(1) {
            if seg_seg(p[i], p[i + 1], p[j], p[j + 1]) <= TOL {
                return true;
            }
        }
    }
    false
}

/// The chain run on straight past both ends by `e` mm along its end
/// tangents.
fn extended(p: &[Point2], e: f64) -> Vec<Point2> {
    extended_ends(p, e, e)
}

/// The chain run on straight past its first point by `e0` mm and its
/// last by `e1` mm along its end tangents; a non-positive amount leaves
/// that end alone.
fn extended_ends(p: &[Point2], e0: f64, e1: f64) -> Vec<Point2> {
    let mut v = p.to_vec();
    let n = v.len();
    if n < 2 {
        return v;
    }
    let push = |a: Point2, b: Point2, e: f64| {
        let l = dist(a, b);
        Point2::new(a.x + (a.x - b.x) / l * e, a.y + (a.y - b.y) / l * e)
    };
    if e1 > 0.0 && e1.is_finite() {
        v.push(push(v[n - 1], v[n - 2], e1));
    }
    if e0 > 0.0 && e0.is_finite() {
        v.insert(0, push(v[0], v[1], e0));
    }
    v
}

/// The chosen side of the chain's offset at `d`: delegates to
/// `machining2d::offset_run_side` and formats the boundary refusal.
fn offset_run(ext: &[Point2], d: f64, left: bool, unit: Unit) -> Result<Vec<Point2>, GenError> {
    crate::machining2d::offset_run_side(ext, d, left)?.ok_or_else(|| {
        GenError::Invalid(format!(
            "the open boundary's offset of {} {} folds onto itself (the chain comes back within twice \
             the offset of itself) - pick a gentler chain or a smaller tool / offset",
            crate::units::len(d, unit, 3),
            unit.suffix()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_right_side_of_a_straight_chain_runs_with_it() {
        let ext = vec![Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)];
        let run = offset_run(&ext, 2.0, false, Unit::Millimeters).expect("run");
        assert!(run.iter().all(|q| (q.y + 2.0).abs() < 1e-3), "{run:?}");
        assert!((run[0].x - 0.0).abs() < 1e-9 && (run[run.len() - 1].x - 10.0).abs() < 1e-9);
        let run = offset_run(&ext, 2.0, true, Unit::Millimeters).expect("run");
        assert!(run.iter().all(|q| (q.y - 2.0).abs() < 1e-3), "{run:?}");
        assert!(run[0].x > run[run.len() - 1].x, "the left side runs against the chain");
    }

    #[test]
    fn a_crossing_chain_and_a_folding_offset_are_refused() {
        let x = [Point2::new(0.0, 0.0), Point2::new(10.0, 10.0), Point2::new(10.0, 0.0), Point2::new(0.0, 10.0)];
        assert!(crosses_itself(&x));
        // A hairpin 1 mm wide offset by 2 mm folds.
        let u = vec![Point2::new(0.0, 0.0), Point2::new(20.0, 0.0), Point2::new(20.0, 1.0), Point2::new(0.0, 1.0)];
        assert!(!crosses_itself(&u));
        assert!(offset_run(&u, 2.0, true, Unit::Millimeters).is_err() || offset_run(&u, 2.0, false, Unit::Millimeters).is_err());
        assert!(normalized(&[Point2::new(1.0, 1.0), Point2::new(1.0, 1.0)]).is_err());
    }
}

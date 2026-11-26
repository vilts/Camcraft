//! Split from `finishing3d.rs` by M5.0.1; structure only. #3362 adds
//! the start side of the nested offset loops (our own design, the
//! shared "Pass starts at" law of #3316).

use crate::cl::{PathBuilder, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{GenError, PassStart};

use super::drape;
use super::params::{Finish3DParams, FinishPattern, refinement_floor, scallop_stepdown, scallop_stepover, validate};
use super::grid::SupSet;
use super::support::{build_sups, plunge_support};
use super::march::{emit_runs, march_curve};
use super::raster::{placement_guard, rings_and_tail};
/// The center-shadow of the shallow terrain: the union of every
/// shallow facet's XY triangle TRANSLATED by `r·m_xy` - the set of
/// XY points where a ball can rest tangent to that facet AT SOME
/// height (a plane's tangent centers form the plane shifted by the
/// contact radius along its lean; the XY shadow of that sheet is the
/// shifted triangle). Steep facets contribute nothing - the march
/// reads no tangent from them, and the rings own the flanks, exactly
/// the jurisdiction every other family keeps. Holes of the shadow
/// are genuine islands (or air the tool sees from above); a lifted
/// sample there answers through `support_at`'s own ball-clear law,
/// so a wavefront that cannot be cut simply breaks - never a lie.
pub(super) fn shadow_region(sups: &SupSet, r: f64) -> Result<Region, GenError> {
    let mut loops: Vec<Loop> = Vec::new();
    for s in sups.iter().filter(|s| s.shallow) {
        let pts: Vec<Point2> = s
            .tri
            .iter()
            .map(|v| Point2::new(v.x + r * s.m.x, v.y + r * s.m.y))
            .collect();
        // Orientation decided by the signed sum, not by faith: a
        // flipped winding still enters the union CCW, and `Region`
        // then reads holes by nesting parity like every other union.
        let area: f64 = (0..3).map(|i| {
            pts[i].x * pts[(i + 1) % 3].y - pts[(i + 1) % 3].x * pts[i].y
        }).sum::<f64>()
            * 0.5;
        let pts = if area < 0.0 {
            let mut v = pts;
            v.reverse();
            v
        } else {
            pts
        };
        if let Ok(l) = Loop::new(pts) && l.area().abs() > 1e-12 {
            loops.push(l);
        }
    }
    if loops.is_empty() {
        return Ok(Region::empty());
    }
    Ok(Region::union_all(loops)?)
}

/// One closed loop, marched: arc length along the polygon becomes
/// the parameter (cumulative chords, summed in walk order so the
/// numbers are the traveler's), so `march_curve`'s laws speak
/// millimeters exactly as they do on a raster line. The march ends
/// where the ground ends: a fully-valid loop comes back CLOSED -
/// the clamp sample at `t = L` lands on the entry point again, and
/// `push_run_pt`'s law keeps it as the closing move, not a
/// duplicate; a loop that breaks at a shelf mouth stays a piece.
pub(super) fn march_loop(
    pts: &[Point2],
    sup: &impl Fn(Point2) -> Option<f64>,
    delta0: f64,
    tol: f64,
    eps_chord: f64,
    floor_mm: f64,
) -> Vec<Vec<(Point2, f64)>> {
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    let mut cum: Vec<f64> = Vec::with_capacity(n + 1);
    cum.push(0.0);
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let next = cum[i] + (b.x - a.x).hypot(b.y - a.y);
        if !next.is_finite() {
            return Vec::new();
        }
        cum.push(next);
    }
    let len = cum[n];
    if !len.is_finite() || len <= 0.0 {
        return Vec::new();
    }
    let pos = |t: f64| {
        let j = match cum.binary_search_by(|c| c.total_cmp(&t)) {
            Ok(j) => j.min(n - 1),
            Err(j) => j.saturating_sub(1).min(n - 1),
        };
        let (t0, t1) = (cum[j], cum[j + 1]);
        let u = if t1 > t0 {
            ((t - t0) / (t1 - t0)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Point2::new(
            pts[j].x + (pts[(j + 1) % n].x - pts[j].x) * u,
            pts[j].y + (pts[(j + 1) % n].y - pts[j].y) * u,
        )
    };
    march_curve(
        0.0,
        len,
        &pos,
        &|a, b| {
            let (pa, pb) = (pos(a), pos(b));
            (pb.x - pa.x).hypot(pb.y - pa.y)
        },
        &|_| 1.0,
        sup,
        delta0,
        tol,
        eps_chord,
        floor_mm,
    )
}

/// The shared core of the two around-the-model families (M2.2e):
/// `shadow` is the terrain's center-shadow; `max_rings` bounds the
/// shrink (Project: boundary only, `Some(0)`; Contour: every pass
/// until the region collapses, `None`). Loops march outermost-first
/// (then by entry point, so the order is a fact about the model,
/// not about clipper's memory), and the rings take the flanks at
/// the close, as in every family. `start` (#3362) orders the offset
/// passes: `Auto` and `Outside` cut the outermost pass first (the
/// order of every earlier file), `Inside` cuts the innermost pass
/// first. Inside one pass the loop order does not change, and no
/// loop is added or dropped: only the sequence of the passes moves.
#[allow(clippy::too_many_arguments)]
pub(super) fn shadow_pass(
    b: &mut PathBuilder,
    p: &Finish3DParams,
    r: f64,
    sups: &SupSet,
    shadow: &Region,
    max_rings: Option<usize>,
    family: &str,
    start: PassStart,
) -> Result<bool, GenError> {
    if shadow.is_empty() {
        return Ok(false); // all-steep model: the rings alone carry it
    }
    let mz = sups
        .iter()
        .filter(|s| s.shallow)
        .fold(1.0f64, |acc, s| acc.min(s.m.z));
    let step = p.stepover.unwrap_or(scallop_stepover(r, p.tolerance) * mz);
    if max_rings.is_none() {
        // The spacing law speaks only where spacing is used: the
        // boundary-only family (Project) marches its one rim whatever
        // `stepover` says - the Waterline's own "as if this enum had
        // never arrived" precedent - while the shrinking family must
        // answer for the hours a hairline stepover is asking for.
        let (mut x0, mut x1, mut y0, mut y1) =
            (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for l in shadow.loops() {
            for q in l.points() {
                x0 = x0.min(q.x);
                x1 = x1.max(q.x);
                y0 = y0.min(q.y);
                y1 = y1.max(q.y);
            }
        }
        let diag = (x1 - x0).hypot(y1 - y0);
        const MAX_RINGS: f64 = 8192.0;
        let est = (diag * 0.5 / step).ceil().max(1.0);
        placement_guard(step, est, family, MAX_RINGS)?;
    }
    const MAX_RINGS_CAP: usize = 8192;
    let delta0 = scallop_stepdown(r, p.tolerance).max(1e-6);
    let trimmed = std::cell::Cell::new(0usize);
    let sup = |q: Point2| {
        let (c, cut) = plunge_support(q.x, q.y, r, p.bottom_z, p.clearance_z + r, sups);
        if cut {
            trimmed.set(trimmed.get() + 1);
        }
        c
    };
    // One entry per offset pass, outermost first.
    let mut passes: Vec<Vec<Vec<(Point2, f64)>>> = Vec::new();
    let mut reg = shadow.clone();
    let mut k = 0usize;
    while !reg.is_empty() {
        let mut loops: Vec<&Loop> = reg.loops().iter().collect();
        loops.sort_by(|a, b| {
            b.area().abs().total_cmp(&a.area().abs()).then_with(|| {
                let (pa, pb) = (a.points()[0], b.points()[0]);
                pa.x.total_cmp(&pb.x).then_with(|| pa.y.total_cmp(&pb.y))
            })
        });
        let mut runs: Vec<Vec<(Point2, f64)>> = Vec::new();
        for l in loops {
            let stored: &[Point2] = l.points();
            if stored.len() < 3 {
                continue;
            }
            let poly: Vec<Point2> = if p.climb {
                stored.to_vec()
            } else {
                stored.iter().rev().copied().collect()
            };
            runs.extend(march_loop(
                &poly,
                &sup,
                delta0,
                p.tolerance,
                p.tolerance * 0.25,
                refinement_floor(delta0, p.min_step),
            ));
        }
        passes.push(runs);
        k += 1;
        // The guard above already refused a family needing more than
        // the cap; this bound is the belt - geometry that refuses to
        // collapse (a numerically immortal sliver) must still leave
        // the loop with an honest long path, never forever.
        if max_rings.is_some_and(|m| k > m) || k > MAX_RINGS_CAP + 2 {
            break;
        }
        reg = reg.offset(-step)?;
    }
    if trimmed.get() > 0 {
        b.note(format!(
            "{family}: {} ground samples refused - their plunge column from clearance crosses the solid; what hides there stands as rest, reported not cut",
            trimmed.get()
        ));
    }
    if start == PassStart::Inside {
        passes.reverse();
    }
    let runs: Vec<Vec<(Point2, f64)>> = passes.into_iter().flatten().collect();
    if runs.is_empty() {
        return Ok(false);
    }
    emit_runs(b, p, r, sups, &runs, false);
    Ok(true)
}

/// Contour finishing (M2.2e): concentric loops of the terrain's
/// center-shadow, shrunk one stepover per pass until the region
/// collapses at its medial axis. The XY spacing of successive loops
/// is exactly the offset, so on the ground they meet the same cusp
/// law a raster's line spacing meets - and where a loop crosses a
/// shelf mouth the march's break law cuts it into pieces that
/// re-enter at the hop, never a chord dragged through an edge.
/// Then the rings take the flanks, as in every family close.
pub fn contour(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    contour_from(facets, p, PassStart::Auto)
}

/// Does this pattern read "Pass starts at" (#3362)? Only the two
/// families of nested offset loops do: Contour and Flat. The others
/// have no inside or outside order (disjoint rings at one Z, rows,
/// chords, one curve, crease chains), and a saved value there moves
/// nothing.
#[must_use]
pub fn reads_pass_start(pattern: FinishPattern) -> bool {
    matches!(pattern, FinishPattern::Contour | FinishPattern::Flat)
}

/// [`contour`] with the start side of its offset passes (#3362).
/// `Auto` is [`contour`] byte for byte.
pub fn contour_from(
    facets: &[Triangle],
    p: &Finish3DParams,
    start: PassStart,
) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let shadow = shadow_region(&sups, r)?;
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let emitted = shadow_pass(&mut b, p, r, &sups, &shadow, None, "contour", start)?;
    rings_and_tail(b, p, r, facets, emitted)
}

/// Project finishing (M2.2e): the model's own silhouette, XY kept
/// and Z lifted onto the surface - the boundary's contact line,
/// where the tool grazes the part's edge wherever that edge climbs.
/// It is the contour family with `max_rings = 0`: same shadow, same
/// march laws, one loop set. The rings below it finish the flanks
/// the lifted rim only touches at a point. M2.5 D3 generalizes the
/// name: after the silhouette, the part's OWN closed boundary loops -
/// a trim's rim, a hole's edge - are draped onto the surface too
/// ([`super::drape`]); a closed solid has none, and for one this
/// answers exactly as it did before the drape existed.
pub fn project(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let shadow = shadow_region(&sups, r)?;
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let mut emitted = shadow_pass(&mut b, p, r, &sups, &shadow, Some(0), "project", PassStart::Auto)?;
    emitted |= drape::drape_pass(&mut b, p, r, &sups, facets, emitted);
    rings_and_tail(b, p, r, facets, emitted)
}

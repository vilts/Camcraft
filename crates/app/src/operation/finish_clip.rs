//! The 3D finishing family's area control (M11.5 owed, the user's
//! complaint: "lack of control on area affected"): every `Finish3D`
//! pattern is generated on the whole model, as it always was, and the
//! path is then CLIPPED to the area the boundary law resolved - the
//! tool centre keeps to [`centre_area`], each cut move keeps the parts
//! inside it, and the tool leaves and re-enters through the clearance
//! plane - vertically, or along the kept chain for a tool that may not
//! plunge (`finish_entry::ramp_into`, M13.10). Nothing is re-generated: a kept
//! point is a point the whole-model kernel already certified, a
//! vertical line above it is air by the 3-axis tangent law, so the
//! clipped path cuts nowhere the unclipped one did not.
//!
//! Provenance: our own design notes - clipping a surface toolpath to a
//! 2D containment boundary at tool centre / edge is the textbook
//! vocabulary of any CAM manual's 3D finishing chapter; the segment /
//! polygon clip is the plane-sweep parametric construction. No vendor
//! material consulted.

use super::*;
use crate::cl::{Move, MoveKind};

/// Below this (mm^2) the centre area is nothing.
const AREA_EPS: f64 = 1e-4;
/// Two parameters closer than this on one segment are one crossing.
const T_EPS: f64 = 1e-12;

/// Where the tool CENTRE may stand: the area grown by the finish's
/// native reach (the ball rides a wall with its edge on the silhouette,
/// its centre a radius out), less a radius around every forbidden zone
/// (an exclude boundary, the far side of an open boundary), so the
/// whole tool stays out of it.
pub(super) fn centre_area(area: &Region, forbidden: &[(&'static str, Region)], r: f64) -> Result<Region, GenError> {
    let mut c = area.offset(r)?;
    for (_, f) in forbidden {
        c = c.difference(&f.offset(r)?)?;
    }
    if c.area().abs() < AREA_EPS {
        return Err(GenError::Empty(
            "the Boundary settings leave no area for the tool to stand in - relax them".into(),
        ));
    }
    Ok(c)
}

fn inside(allow: &Region, p: Point3) -> bool {
    allow.contains(Point2::new(p.x, p.y))
}

pub(super) fn lerp(a: Point3, b: Point3, t: f64) -> Point3 {
    Point3::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t)
}

/// The parameter on `ab` where it crosses `pq`, strictly inside both.
fn crossing(a: Point2, b: Point2, p: Point2, q: Point2) -> Option<f64> {
    let (rx, ry) = (b.x - a.x, b.y - a.y);
    let (sx, sy) = (q.x - p.x, q.y - p.y);
    let den = rx * sy - ry * sx;
    if den.abs() < 1e-15 {
        return None;
    }
    let (wx, wy) = (p.x - a.x, p.y - a.y);
    let t = (wx * sy - wy * sx) / den;
    let u = (wx * ry - wy * rx) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

/// The boundary edges of `allow` with their bounding boxes, read once.
struct Edges {
    segs: Vec<(Point2, Point2)>,
    lo: Point2,
    hi: Point2,
}

impl Edges {
    fn of(allow: &Region) -> Self {
        let mut segs = Vec::new();
        let (mut lo, mut hi) = (Point2::new(f64::INFINITY, f64::INFINITY), Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
        for l in allow.loops() {
            let pts = l.points();
            let n = pts.len();
            for i in 0..n {
                let (p, q) = (pts[i], pts[(i + 1) % n]);
                lo = Point2::new(lo.x.min(p.x), lo.y.min(p.y));
                hi = Point2::new(hi.x.max(p.x), hi.y.max(p.y));
                segs.push((p, q));
            }
        }
        Edges { segs, lo, hi }
    }

    /// The pieces of the straight move `a -> b` inside `allow`, in order.
    fn clip(&self, allow: &Region, a: Point3, b: Point3) -> Vec<(Point3, Point3)> {
        let (a2, b2) = (Point2::new(a.x, a.y), Point2::new(b.x, b.y));
        let (sx0, sx1) = (a2.x.min(b2.x), a2.x.max(b2.x));
        let (sy0, sy1) = (a2.y.min(b2.y), a2.y.max(b2.y));
        if sx1 < self.lo.x || sx0 > self.hi.x || sy1 < self.lo.y || sy0 > self.hi.y {
            return Vec::new();
        }
        let mut ts = vec![0.0, 1.0];
        for (p, q) in &self.segs {
            let (ex0, ex1) = (p.x.min(q.x), p.x.max(q.x));
            let (ey0, ey1) = (p.y.min(q.y), p.y.max(q.y));
            if sx1 < ex0 || sx0 > ex1 || sy1 < ey0 || sy0 > ey1 {
                continue;
            }
            if let Some(t) = crossing(a2, b2, *p, *q) {
                ts.push(t);
            }
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() < T_EPS);
        let mut out: Vec<(Point3, Point3)> = Vec::new();
        for w in ts.windows(2) {
            let (p, q) = (lerp(a, b, w[0]), lerp(a, b, w[1]));
            if p == q {
                continue;
            }
            let mid = lerp(a, b, (w[0] + w[1]) * 0.5);
            if !inside(allow, mid) {
                continue;
            }
            match out.last_mut() {
                Some(last) if last.1 == p => last.1 = q,
                _ => out.push((p, q)),
            }
        }
        out
    }
}

/// The way from where the output stands to `p`: up to the clearance
/// plane, across at it, and vertically down to `p` at the plunge feed.
fn link(out: &mut Vec<Move>, pos: Option<Point3>, p: Point3, clearance: f64, plunge_feed: f64) {
    let over = Point3::new(p.x, p.y, clearance.max(p.z));
    let mut cur = pos;
    if let Some(s) = pos
        && s.z < over.z
    {
        let up = Point3::new(s.x, s.y, over.z);
        out.push(Move { rotary: None, kind: MoveKind::Rapid, from: s, to: up, feed: None, arc: None, axis: None });
        cur = Some(up);
    }
    if let Some(c) = cur
        && c != over
    {
        out.push(Move { rotary: None, kind: MoveKind::Rapid, from: c, to: over, feed: None, arc: None, axis: None });
    }
    if over != p {
        out.push(Move { rotary: None, kind: MoveKind::Plunge, from: over, to: p, feed: Some(plunge_feed), arc: None, axis: None });
    }
}

/// The contiguous runs of `path` inside `allow`: rapids survive only
/// where they continue a run and land inside; every cut move keeps its
/// inside pieces, chorded where an arc crosses the boundary. Each run
/// is a chain (every move starts where the last ended) the output
/// re-enters once.
fn runs(path: &Toolpath, allow: &Region, tol: f64) -> Vec<Vec<Move>> {
    let edges = Edges::of(allow);
    let mut runs: Vec<Vec<Move>> = Vec::new();
    let mut pos: Option<Point3> = None;
    fn push(runs: &mut Vec<Vec<Move>>, pos: &mut Option<Point3>, m: Move) {
        match runs.last_mut() {
            Some(r) if *pos == Some(m.from) => r.push(m),
            _ => runs.push(vec![m]),
        }
        *pos = Some(m.to);
    }
    for m in &path.moves {
        if m.kind == MoveKind::Rapid {
            if pos == Some(m.from) && m.arc.is_none() && inside(allow, m.to) {
                push(&mut runs, &mut pos, *m);
            }
            continue;
        }
        let segs = m.segments3(tol);
        let whole = m.arc.is_some() && pos == Some(m.from) && segs.iter().all(|(a, b)| inside(allow, *a) && inside(allow, *b));
        if whole {
            push(&mut runs, &mut pos, *m);
            continue;
        }
        for (a, b) in segs {
            for (p, q) in edges.clip(allow, a, b) {
                push(&mut runs, &mut pos, Move { rotary: None, kind: m.kind, from: p, to: q, feed: m.feed, arc: None, axis: m.axis });
            }
        }
    }
    runs
}

/// `path` clipped to `allow` (the tool-centre area), see [`runs`]; each
/// re-entry rides [`link`] - vertically, or, with `entry_ramp` on and a
/// finite solid top `ramp_top` (the non-centre-cutting tool's retry),
/// along `finish_entry::ramp_into` where it fits. A path left with no cut at all
/// refuses by name.
pub(super) fn clip_path(path: Toolpath, allow: &Region, fp: &Finish3DParams, ramp_top: f64) -> Result<Toolpath, GenError> {
    let clearance = fp.clearance_z;
    let mut out: Vec<Move> = Vec::with_capacity(path.moves.len());
    let mut pos: Option<Point3> = None;
    let mut kept = 0usize;
    for run in runs(&path, allow, fp.tolerance) {
        let Some(first) = run.first() else { continue };
        let ramp = (fp.entry_ramp && ramp_top.is_finite())
            .then(|| super::finish_entry::ramp_into(&run, ramp_top, fp.ramp_angle, fp.tolerance))
            .flatten();
        match ramp {
            Some(legs) => {
                link(&mut out, pos, legs[0], clearance, fp.plunge_feed);
                for w in legs.windows(2) {
                    out.push(Move { rotary: None, kind: MoveKind::Feed, from: w[0], to: w[1], feed: Some(fp.plunge_feed), arc: None, axis: None });
                }
            }
            None => link(&mut out, pos, first.from, clearance, fp.plunge_feed),
        }
        kept += run.iter().filter(|m| m.kind != MoveKind::Rapid).count();
        pos = run.last().map(|m| m.to);
        out.extend(run);
    }
    if kept == 0 {
        return Err(GenError::Empty(
            "the Boundary settings leave none of the finishing path inside the area - relax them".into(),
        ));
    }
    if let Some(p) = pos
        && p.z < clearance
    {
        out.push(Move { rotary: None, kind: MoveKind::Rapid, from: p, to: Point3::new(p.x, p.y, clearance), feed: None, arc: None, axis: None });
    }
    Ok(Toolpath { moves: out, cycles: path.cycles, notes: path.notes, comps: path.comps, threads: Vec::new(), probes: path.probes, breaks: path.breaks, plasma: path.plasma })
}

#[cfg(test)]
mod law {
    use super::*;

    fn square(h: f64) -> Region {
        Region::new(vec![
            Loop::new(vec![Point2::new(-h, -h), Point2::new(h, -h), Point2::new(h, h), Point2::new(-h, h)]).expect("loop"),
        ])
        .expect("region")
    }

    fn feed(a: (f64, f64, f64), b: (f64, f64, f64)) -> Move {
        Move {
            rotary: None,
            kind: MoveKind::Feed,
            from: Point3::new(a.0, a.1, a.2),
            to: Point3::new(b.0, b.1, b.2),
            feed: Some(500.0),
            arc: None,
            axis: None,
        }
    }

    fn params() -> Finish3DParams {
        Finish3DParams {
            gap_link: finishing3d::GapLink::Jump,
            pattern: finishing3d::FinishPattern::Parallel,
            stepover: None,
            tool_diameter: 6.0,
            tolerance: 0.01,
            clearance_z: 30.0,
            retract_z: None,
            top_z: 10.0,
            bottom_z: 0.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            ramp: false,
            min_step: None,
            ramp_angle: 45.0,
            entry_ramp: false,
            steep_stepover: None,
            pencil_reference: None,
            waterlines: true,
        }
    }

    #[test]
    fn a_row_through_the_area_keeps_its_inside_piece_and_relinks_through_clearance() {
        let allow = square(10.0);
        let path = Toolpath::new(vec![feed((-20.0, 0.0, 5.0), (20.0, 0.0, 5.0)), feed((20.0, 0.0, 5.0), (20.0, 3.0, 5.0))]);
        let t = clip_path(path, &allow, &params(), f64::INFINITY).expect("clipped");
        let feeds: Vec<&Move> = t.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
        assert_eq!(feeds.len(), 1, "{:?}", t.moves);
        assert!((feeds[0].from.x + 10.0).abs() < 1e-9 && (feeds[0].to.x - 10.0).abs() < 1e-9);
        assert_eq!(t.moves[0].kind, MoveKind::Plunge, "enters vertically from clearance");
        assert_eq!(t.moves[0].from.z, 30.0);
        assert_eq!(t.moves.last().map(|m| m.kind), Some(MoveKind::Rapid), "leaves to clearance");
    }

    #[test]
    fn a_path_wholly_outside_refuses_by_name() {
        let allow = square(10.0);
        let path = Toolpath::new(vec![feed((15.0, 15.0, 5.0), (20.0, 15.0, 5.0))]);
        let err = clip_path(path, &allow, &params(), f64::INFINITY).expect_err("nothing inside").to_string();
        assert!(err.contains("none of the finishing path"), "{err}");
    }

    /// M13.10: with `entry_ramp` on and a solid top 2 above the row,
    /// the re-entry runs 2 along the kept row at 45 degrees - from the
    /// top plane down onto the row's start - and the row is then cut
    /// whole; the legs stand over the row, inside the area.
    #[test]
    fn a_re_entry_under_the_solid_top_ramps_along_the_kept_row() {
        let allow = square(10.0);
        let path = Toolpath::new(vec![feed((-20.0, 0.0, 5.0), (20.0, 0.0, 5.0))]);
        let fp = Finish3DParams { entry_ramp: true, ..params() };
        let t = clip_path(path, &allow, &fp, 7.0).expect("clipped");
        assert_eq!(t.moves[0].kind, MoveKind::Plunge, "{:?}", t.moves);
        assert_eq!(t.moves[0].to, Point3::new(-8.0, 0.0, 7.0), "the plunge lands on the top plane, in air");
        assert_eq!(t.moves[1].kind, MoveKind::Feed);
        assert_eq!((t.moves[1].from, t.moves[1].to), (Point3::new(-8.0, 0.0, 7.0), Point3::new(-10.0, 0.0, 5.0)));
        assert_eq!(t.moves[1].feed, Some(100.0), "the ramp descends at the plunge feed");
        assert_eq!((t.moves[2].from, t.moves[2].to), (Point3::new(-10.0, 0.0, 5.0), Point3::new(10.0, 0.0, 5.0)));
    }

    /// The ramp walks the chain across several moves; a chain shorter
    /// than the ramp, or `entry_ramp` off, or no solid top, keeps the
    /// vertical plunge (the audit's business, refused by name there).
    #[test]
    fn the_ramp_walks_the_chain_and_a_piece_too_small_keeps_the_plunge() {
        let allow = square(10.0);
        let fp = Finish3DParams { entry_ramp: true, ..params() };
        let row = || Toolpath::new(vec![feed((-20.0, 0.0, 5.0), (-9.0, 0.0, 5.0)), feed((-9.0, 0.0, 5.0), (20.0, 0.0, 5.0))]);
        let t = clip_path(row(), &allow, &fp, 10.0).expect("clipped");
        let legs: Vec<&Move> = t.moves.iter().take_while(|m| m.kind != MoveKind::Feed || m.to.z < m.from.z).collect();
        assert_eq!(legs.len(), 3, "a plunge and two ramp legs over the two chords: {:?}", t.moves);
        let near = |p: Point3, x: f64, z: f64| (p.x - x).abs() < 1e-9 && p.y.abs() < 1e-9 && (p.z - z).abs() < 1e-9;
        assert!(near(legs[1].from, -5.0, 10.0), "the far point, lifted to the top: {:?}", legs[1]);
        assert!(near(legs[1].to, -9.0, 6.0), "over the chain's vertex: {:?}", legs[1]);
        assert_eq!(legs[2].to, Point3::new(-10.0, 0.0, 5.0));
        for (top, p) in [(30.0, fp), (10.0, params())] {
            let t = clip_path(row(), &allow, &p, top).expect("clipped");
            assert_eq!(t.moves[0].kind, MoveKind::Plunge);
            assert_eq!(t.moves[0].to, Point3::new(-10.0, 0.0, 5.0), "the plunge stands: {:?}", t.moves);
        }
        let t = clip_path(row(), &allow, &fp, f64::INFINITY).expect("clipped");
        assert_eq!(t.moves[0].to, Point3::new(-10.0, 0.0, 5.0), "a centre-cutting tool's plunge stands");
    }

    /// A chain that rises above the ramp's line would be bitten; the
    /// ramp stands down and the plunge stays.
    #[test]
    fn a_bump_in_the_chain_refuses_the_ramp() {
        let allow = square(10.0);
        let fp = Finish3DParams { entry_ramp: true, ..params() };
        // Top 8 over a row at 5: the ramp line reaches 7 at the second
        // vertex, where the chain stands at 7.9.
        let path = Toolpath::new(vec![
            feed((-20.0, 0.0, 5.0), (-9.0, 0.0, 5.0)),
            feed((-9.0, 0.0, 5.0), (-8.0, 0.0, 7.9)),
            feed((-8.0, 0.0, 7.9), (20.0, 0.0, 5.0)),
        ]);
        let t = clip_path(path, &allow, &fp, 8.0).expect("clipped");
        assert_eq!(t.moves[0].kind, MoveKind::Plunge);
        assert!((t.moves[0].to.x + 10.0).abs() < 1e-9, "{:?}", t.moves);
    }

    #[test]
    fn the_centre_area_reaches_a_radius_past_the_area_and_a_radius_short_of_a_forbidden_zone() {
        let c = centre_area(&square(10.0), &[("an exclude boundary", square(2.0))], 3.0).expect("centre area");
        assert!(c.contains(Point2::new(12.5, 0.0)) && !c.contains(Point2::new(13.5, 0.0)));
        assert!(!c.contains(Point2::new(4.5, 0.0)) && c.contains(Point2::new(5.5, 0.0)));
    }
}

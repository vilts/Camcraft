//! The microlift return (M11.4 C round 4): the HSM plan's link between
//! two biting pieces of its spiral (`adaptive::lift` proved it: inside
//! the center area, its swept disc over cleared stock only, crossing no
//! other return and no entry orbit). Posted as feeds: a climb to its
//! lift over the floor (`adaptive::MICROLIFT`, or a multiple of it where
//! the returns below would cross), the lifted traverse
//! (straight, or routed through its via points), and the ramp down
//! along the next piece's own air lead (the spiral's path there, so the
//! descent reads only the air the plan read) - the tool never drags its
//! end face over the floor it just cut and never rides the walls to get
//! there: the lifted, non-cutting return of trochoidal and adaptive
//! clearing (Otkur & Lazoglu 2007, Int. J. Mach. Tools Manuf. 47). The
//! ramp's run is ours.

use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::adaptive::Return;
use super::levels::Motion;

/// A ramp runs this many times its rise (about 18 degrees).
pub(super) const RUN_PER_RISE: f64 = 3.0;

/// The return's vertices from `a` (on the plane): each XY point with its
/// height over the plane, lattice-snapped. Up along `ret.up` (straight up
/// where it is empty), across at its lift through `ret.via` to the
/// ramp's top, down `ret.down` by its own length (ending on the next
/// pass's start) - a one-point `down` descends along the traverse's own
/// last run.
pub(super) fn profile(a: Point2, ret: &Return) -> Vec<(Point2, f64)> {
    let mut out = vec![(a, 0.0)];
    let Some(&e) = ret.down.last() else { return out };
    let snap = |p: Point2| Point2::new(lattice(p.x), lattice(p.y));
    let seg = |w: &[Point2]| (w[1].x - w[0].x).hypot(w[1].y - w[0].y);
    let hz = lattice(ret.lift);
    if ret.high {
        // Over the retract plane: the feeds start at the ramp's top.
        let mut out = vec![(snap(ret.down[0]), hz)];
        let total: f64 = ret.down.windows(2).map(seg).sum();
        let mut acc = 0.0;
        for w in ret.down.windows(2) {
            acc += seg(w);
            let f = if total > 1e-12 { (acc / total).min(1.0) } else { 1.0 };
            out.push((snap(w[1]), lattice(hz * (1.0 - f))));
        }
        return out;
    }
    // A polyline from `from` walked by its own length, the height running
    // from `h0` to `h1`.
    let ramp = |out: &mut Vec<(Point2, f64)>, from: Point2, pts: &[Point2], (h0, h1): (f64, f64)| {
        let mut poly = vec![from];
        poly.extend_from_slice(pts);
        let total: f64 = poly.windows(2).map(seg).sum();
        let mut acc = 0.0;
        for w in poly.windows(2) {
            acc += seg(w);
            let f = if total > 1e-12 && total.is_finite() { (acc / total).min(1.0) } else { 1.0 };
            out.push((snap(w[1]), lattice(h0 + (h1 - h0) * f)));
        }
    };
    // The climb.
    let top = ret.up.last().copied().unwrap_or(a);
    if ret.up.is_empty() {
        out.push((snap(a), hz));
    } else {
        ramp(&mut out, a, &ret.up, (0.0, hz));
    }
    // The traverse, all at the lift but a one-point ramp's last run.
    let mut trav: Vec<Point2> = ret.via.clone();
    trav.push(ret.down[0]);
    if ret.down.len() >= 2 {
        for &q in &trav {
            out.push((snap(q), hz));
        }
        ramp(&mut out, ret.down[0], &ret.down[1..], (hz, 0.0));
        return out;
    }
    let mut poly = vec![top];
    poly.extend_from_slice(&trav);
    let total: f64 = poly.windows(2).map(seg).sum();
    let fall = (ret.lift * RUN_PER_RISE).min(total);
    let mut acc = 0.0;
    for w in poly.windows(2) {
        let l = seg(w);
        // The ramp's top, where it falls inside this leg.
        let c = total - fall;
        if c > acc + 1e-9 && c < acc + l - 1e-9 && l.is_finite() {
            let t = (c - acc) / l;
            out.push((snap(Point2::new(w[0].x + (w[1].x - w[0].x) * t, w[0].y + (w[1].y - w[0].y) * t)), hz));
        }
        acc += l;
        let h = if fall > 1e-12 { hz * ((total - acc) / fall).clamp(0.0, 1.0) } else { 0.0 };
        out.push((snap(w[1]), lattice(h)));
    }
    if let Some(last) = out.last_mut() {
        *last = (snap(e), 0.0);
    }
    out
}

/// Feed from where the tool stands on plane `z` along [`profile`] onto
/// the next pass's start on plane `z`.
pub(super) fn link(b: &mut PathBuilder, ret: &Return, z: f64, m: &Motion) {
    link_at(b, ret, z, m.hop_z, m.feed);
}

fn push_prof(b: &mut PathBuilder, prof: &[(Point2, f64)], z: f64, hop_z: f64, feed: f64, high: bool) {
    let at = b.at();
    if high {
        // Up, across and down to the lift in rapid (vertical rapids in
        // the cut's own kerf and column; the lateral one over the
        // retract plane), then down the lead at feed.
        let Some(&(top, hz)) = prof.first() else { return };
        let hop = hop_z.max(z + hz);
        b.push(MoveKind::Rapid, Point3::new(at.x, at.y, hop), None);
        b.push(MoveKind::Rapid, Point3::new(top.x, top.y, hop), None);
        b.push(MoveKind::Rapid, Point3::new(top.x, top.y, lattice(z + hz)), None);
    }
    for &(p, dz) in prof.iter().skip(1) {
        let zz = if dz > 0.0 { lattice(z + dz) } else { z };
        b.push(MoveKind::Feed, Point3::new(p.x, p.y, zz), Some(feed));
    }
}

fn try_prof(b: &mut PathBuilder, prof: &[(Point2, f64)], z: f64, hop_z: f64, feed: f64, high: bool) -> bool {
    if high && prof.len() < 2 {
        return false;
    }
    let mut probe = PathBuilder::new(b.at());
    push_prof(&mut probe, prof, z, hop_z, feed, high);
    if super::arc_cross::moves_cross_prior(probe.moves(), b.moves()) {
        return false;
    }
    push_prof(b, prof, z, hop_z, feed, high);
    true
}

/// [`link`] on bare numbers: the hop plane a `high` return rides and
/// the cutting feed (the 3-D ladder's per-level port, `machining3d::hsm`).
pub(crate) fn link_at(b: &mut PathBuilder, ret: &Return, z: f64, hop_z: f64, feed: f64) {
    let at = b.at();
    let Some(end) = ret.down.last().or(ret.via.last()).or(ret.up.last()).copied() else {
        return;
    };
    let prior = b.moves();
    // Nearest earlier horizontal cut plane strictly above `z` (scanned
    // before this level's own moves so same-level plateaus never ratchet).
    let first_at_z = prior.iter().position(|m| m.kind != MoveKind::Rapid && (m.to.z - z).abs() <= 1e-6).unwrap_or(prior.len());
    let gap = prior[..first_at_z]
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() <= 1e-6 && m.from.z > z + 1e-6)
        .map(|m| m.from.z - z)
        .reduce(f64::min);
    if let Some(g) = gap
        && g <= 0.02
    {
        let hop = hop_z.max(z + 0.1);
        b.push(MoveKind::Rapid, Point3::new(at.x, at.y, hop), None);
        b.push(MoveKind::Rapid, Point3::new(end.x, end.y, hop), None);
        b.push(MoveKind::Plunge, Point3::new(end.x, end.y, z), Some(feed));
        return;
    }
    let lift = match gap {
        Some(g) if g <= 1.5 + 1e-2 || ret.lift >= g - 1e-2 => {
            let ceil = g - 0.01;
            let denom = ret.lift.max(1.5);
            lattice((ret.lift * ceil / denom).max(0.005)).min(lattice(ceil))
        }
        _ => ret.lift,
    };
    let adjusted = Return { lift, ..ret.clone() };
    let prof = profile(Point2::new(at.x, at.y), &adjusted);
    if try_prof(b, &prof, z, hop_z, feed, adjusted.high) {
        return;
    }
    if !adjusted.high {
        // A low return proves feeds over cleared stock, not a rapid's
        // whole landing disc. Only the planner's `LinkLaw::high` has
        // that proof; changing the flag here would bypass it. Neither
        // a rapid onto the lead nor an axial fallback is proved here.
        b.note_once(crate::approach::MICROLIFT_REFUSAL);
        return;
    }
    let hop = hop_z.max(z + lift);
    b.push(MoveKind::Rapid, Point3::new(at.x, at.y, hop), None);
    b.push(MoveKind::Rapid, Point3::new(end.x, end.y, hop), None);
    b.push(MoveKind::Plunge, Point3::new(end.x, end.y, z), Some(feed));
}

#[cfg(test)]
mod tests {
    use super::link_at;
    use crate::cl::{MoveKind, PathBuilder};
    use crate::geo2d::Point2;
    use crate::geometry::Point3;
    use crate::machining2d::adaptive::Return;
    use crate::verify::crossed_pairs;

    #[test]
    fn microlift_link_stays_below_prior_level_and_avoids_entry_crossings() {
        let run_case = |prior_from: f64, prior_to: f64, z_lvl: f64, lift: f64| {
            let mut b = PathBuilder::new(Point3::new(-10.0, 0.0, 5.0));
            b.push(MoveKind::Rapid, Point3::new(-10.0, 0.0, prior_from), None);
            b.push(MoveKind::Feed, Point3::new(10.0, 0.0, prior_to), Some(500.0));
            if (z_lvl - prior_to).abs() > 1e-9 {
                b.push(MoveKind::Rapid, Point3::new(0.0, -10.0, 5.0), None);
                b.push(MoveKind::Plunge, Point3::new(0.0, -10.0, z_lvl), Some(150.0));
            }
            b.push(MoveKind::Feed, Point3::new(0.0, -5.0, z_lvl), Some(500.0));
            let ret = Return {
                lift,
                high: false,
                up: vec![Point2::new(0.0, -3.0)],
                via: vec![Point2::new(0.0, 3.0)],
                down: vec![Point2::new(0.0, 5.0)],
            };
            link_at(&mut b, &ret, z_lvl, 5.0, 500.0);
            b.push(MoveKind::Feed, Point3::new(0.0, 10.0, z_lvl), Some(500.0));
            let path = b.finish();
            crossed_pairs(&path, 0.01)
        };

        // Case 1: stepdown=0.30, lift=0.30 reaches exact prior plane z=0.0
        assert!(run_case(0.0, 0.0, -0.30, 0.30).is_empty(), "stepdown=0.30");
        // Case 2: stepdown=0.50 with escalated lift=0.60 over prior ramp/level
        // [0.0, 0.30] and planar cut at -0.15 must scale strictly below -0.15.
        {
            let mut b = PathBuilder::new(Point3::new(-10.0, 0.0, 5.0));
            b.push(MoveKind::Rapid, Point3::new(-10.0, 0.0, 0.0), None);
            b.push(MoveKind::Feed, Point3::new(10.0, 0.0, 0.0), Some(500.0));
            b.push(MoveKind::Feed, Point3::new(10.0, 0.0, 0.30), Some(500.0));
            b.push(MoveKind::Feed, Point3::new(-10.0, 0.0, 0.0), Some(500.0));
            b.push(MoveKind::Rapid, Point3::new(-10.0, -4.0, 5.0), None);
            b.push(MoveKind::Plunge, Point3::new(-10.0, -4.0, -0.15), Some(150.0));
            b.push(MoveKind::Feed, Point3::new(10.0, -4.0, -0.15), Some(500.0));
            b.push(MoveKind::Rapid, Point3::new(0.0, -10.0, 5.0), None);
            b.push(MoveKind::Plunge, Point3::new(0.0, -10.0, -0.50), Some(150.0));
            b.push(MoveKind::Feed, Point3::new(0.0, -5.0, -0.50), Some(500.0));
            let ret = Return {
                lift: 0.60,
                high: false,
                up: vec![Point2::new(0.0, -3.0)],
                via: vec![Point2::new(0.0, 3.0)],
                down: vec![Point2::new(0.0, 5.0)],
            };
            let before = b.moves_len();
            link_at(&mut b, &ret, -0.50, 5.0, 500.0);
            let path = b.finish();
            let c = crossed_pairs(&path, 0.01);
            assert!(c.is_empty(), "escalated lift 0.60 at z=-0.50 vs prior z=0.0/-0.15: {c:?}");
            let link_feeds: Vec<_> = path.moves[before..].iter().filter(|m| m.kind == MoveKind::Feed).collect();
            assert!(
                link_feeds.iter().any(|m| m.to.z > -0.45) && link_feeds.iter().all(|m| m.to.z < -0.15 - 1e-6),
                "scaled lift stayed strictly between -0.50 and prior -0.15: {link_feeds:?}"
            );
        }
        // Case 3: A crossing low return has no proved rapid landing.
        // Refuse it rather than promoting it or substituting a plunge.
        {
            let ramp_crossing_y = |y: f64| {
                let mut b = PathBuilder::new(Point3::new(-10.0, y, 5.0));
                b.push(MoveKind::Rapid, Point3::new(-10.0, y, 0.50), None);
                b.push(MoveKind::Feed, Point3::new(10.0, y, 0.0), Some(500.0));
                b.push(MoveKind::Rapid, Point3::new(0.0, -10.0, 5.0), None);
                b.push(MoveKind::Plunge, Point3::new(0.0, -10.0, 0.0), Some(150.0));
                b.push(MoveKind::Feed, Point3::new(0.0, -5.0, 0.0), Some(500.0));
                let ret = Return {
                    lift: 0.30,
                    high: false,
                    up: vec![Point2::new(0.0, -3.0)],
                    via: vec![Point2::new(0.0, 3.0)],
                    down: vec![Point2::new(0.0, 5.0)],
                };
                let before = b.moves_len();
                link_at(&mut b, &ret, 0.0, 5.0, 500.0);
                let path = b.finish();
                assert!(crate::approach::refusal(&path).is_some(), "the crossing return refuses the operation");
                assert_eq!(path.moves.len(), before, "the refused return posts no unproved entry");
                crossed_pairs(&path, 0.01)
            };
            assert!(ramp_crossing_y(-10.0 / 3.0).is_empty(), "same-level ramp crossing up leg");
            assert!(ramp_crossing_y(4.25).is_empty(), "same-level ramp crossing down leg");
        }
        // Case 4: Tiny gap (0.01 mm) too thin for safe low lift -> must retract cleanly
        assert!(run_case(0.0, 0.0, -0.01, 0.30).is_empty(), "tiny gap 0.01");
    }
}

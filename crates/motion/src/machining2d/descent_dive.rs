//! The planar ladder's dives (split out of `descent.rs` by M11.3 at the
//! 400-line seam): the axial feed down from just over the material, the
//! at-depth link from a dive to the pass start - or the named second
//! dive - and M8.1's substituted dive. Provenance: standard CNC plunge
//! entries per public G-code manuals and machining handbooks; the laws
//! are this crate's (M8.1's plan law, M11.3's one-entry law). No vendor
//! material consulted.

use crate::approach::{CENTER_CUT_REFUSAL, NOT_CENTER_CUTTING};
use crate::cl::{Move, MoveKind, PathBuilder, lattice};
use crate::entry::Entry3D;
use crate::plunge_law;
use crate::entry::{chord_hits_plan, disc_clears, dive_room, helix_about};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::links::helix_turns;

use super::descent::{Cuts, Descent, Lvl, helix_parts, make_helix, plan_arm, push, ramp_fallback};
use super::levels::{Cert, Motion};

/// The axial feed down onto `(xy, z)` from where the tool stands over
/// `xy` (M11.3, the user's hand-test: no feed through tens of mm of
/// air): a rapid first to the plunge height over `mat_top` - the
/// material's top at this level, air above by the superset law - held
/// under the retract plane, then the feed. A zero plunge height feeds
/// the whole way, as every pre-M11.3 path did.
pub(super) fn feed_down(b: &mut PathBuilder, xy: Point2, z: f64, mat_top: f64, m: &Motion, feed: f64) {
    let h = m.links.approach.plunge_height;
    let hz = (mat_top + h).min(m.hop_z.max(mat_top));
    if h > 0.0 && hz.is_finite() && hz < b.at().z && hz > z {
        b.push(MoveKind::Rapid, Point3::new(xy.x, xy.y, hz), None);
    }
    // The centre-cutting law's backstop (`plunge_law`): every planar
    // plunge asks it; a site that can stand a helix or ramp in asked
    // first.
    plunge_law::guard(b, m.links.approach.center_cutting, m.tool_r(), xy, z, mat_top, "a plunge");
    b.push(MoveKind::Plunge, Point3::new(xy.x, xy.y, z), Some(feed));
}

/// The link from a just-completed dive at `d` on plane `z` to the pass
/// start `e`: nothing when the dive ended on it; a feed at depth when the
/// chord is proven (inside the center, crossing none of the level's
/// planned paths) - the cut out of the descent, at the lead-in feed
/// (M11.3 D); else hop, traverse and dive again - the second dive M11.3
/// names, never silent. Returns the dive point when a link was cut.
pub(super) fn dive_link(b: &mut PathBuilder, l: &Lvl, d: Point2, e: Point2, z: f64, m: &Motion) -> Option<Point2> {
    if d.x == e.x && d.y == e.y {
        return None;
    }
    link_at_depth(b, l.cen, l.passes, d, e, z, l.mat_top, m);
    Some(d)
}

/// [`dive_link`]'s law for any caller (the contour's bore side).
#[allow(clippy::too_many_arguments)]
pub(super) fn link_at_depth(
    b: &mut PathBuilder,
    cen: &Cert,
    passes: &[Vec<Point2>],
    d: Point2,
    e: Point2,
    z: f64,
    mat_top: f64,
    m: &Motion,
) {
    if cen.chord_ok(d, e) && !chord_hits_plan(d, e, passes) {
        b.push(MoveKind::Feed, Point3::new(e.x, e.y, z), Some(m.links.approach.lead_feed(m.feed)));
    } else {
        // A second dive into solid refuses a tool that is not
        // centre-cutting, by the one plunge law.
        let cc = m.links.approach.center_cutting;
        plunge_law::guard(b, cc, m.tool_r(), e, z, mat_top, "the second dive at the pass start");
        b.note(format!(
            "entry: a second dive at the pass start ({:.3}, {:.3}) - the link from the entry at ({:.3}, {:.3}) \
             is not proved clear at depth",
            e.x, e.y, d.x, d.y
        ));
        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, m.hop_z), None);
        b.push(MoveKind::Rapid, Point3::new(e.x, e.y, m.hop_z), None);
        feed_down(b, e, z, mat_top, m, m.plunge_feed);
    }
}

/// Does the planned descent drop axially into the level's material -
/// a plunge, a substituted dive, a sweep-capped arc whose top sits under
/// the material's top (the rest of its drop is an axial feed), a ramp
/// "chord" with no run? `Some(why)` for the centre-cutting law to stand
/// a helix or ramp in.
pub(super) fn axial_why(d: &Descent, l: &Lvl) -> Option<&'static str> {
    const EPS: f64 = 1e-6;
    let e = l.pass.first()?;
    match d {
        Descent::Plunge(_) => Some("the plunge arm"),
        Descent::Sub(why) => Some(*why),
        Descent::Arc(lead, zf) if lead.top < zf - EPS && lead.top < l.mat_top - EPS => {
            Some("the arc's capped sweep ends in an axial drop")
        }
        Descent::Ramp(s, _) if (s.x - e.x).hypot(s.y - e.y) <= EPS => Some("the ramp has no run - it is a plunge"),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn helix_clears_prior(
    prior: &[Move],
    l: &Lvl,
    start: Point2,
    center: Point2,
    from: f64,
    z: f64,
    turns: f64,
    m: &Motion,
) -> bool {
    if prior.is_empty() {
        return true;
    }
    let a = &m.links.approach;
    let mut probe = PathBuilder::new(Point3::new(start.x, start.y, from));
    helix_about(&mut probe, start, center, from, z, turns, a.descent_feed(m.feed), !a.helix_cw);
    if let Some(&e) = l.pass.first()
        && (start.x != e.x || start.y != e.y)
        && links_to(l, start, e)
    {
        probe.push(MoveKind::Feed, Point3::new(e.x, e.y, z), Some(a.lead_feed(m.feed)));
    }
    !super::arc_cross::moves_cross_prior(probe.moves(), prior)
}

pub(super) fn find_clear_helix_raw(
    prior: &[Move],
    l: &Lvl,
    from: f64,
    z: f64,
    zig: bool,
    m: &Motion,
    must_link: bool,
) -> Option<(Point2, Point2, f64, f64)> {
    let e = *l.pass.first()?;
    let a = &m.links.approach;
    for r in a.helix_radii(m.helix_radius) {
        let pitch = if zig { m.links.helix_pitch } else { a.entry_pitch(r) };
        let Some(turns) = helix_turns(from - z, r, pitch) else { continue };
        for q in std::iter::once(e).chain(along_pass(l, m)) {
            if must_link && (q.x != e.x || q.y != e.y) && !links_to(l, q, e) {
                continue;
            }
            for k in 0..8 {
                let (sn, cs) = (f64::from(k) * std::f64::consts::FRAC_PI_4).sin_cos();
                let c = Point2::new(lattice(q.x + r * cs), lattice(q.y + r * sn));
                if disc_clears(l.cen.room(), c, r + 1e-3) && helix_clears_prior(prior, l, q, c, from, z, turns, m) {
                    return Some((q, c, turns, r));
                }
            }
        }
        if !must_link && let Some(c) = dive_room(l.cen.room(), r + 1e-3) {
            let q = Point2::new(lattice(c.x - r), lattice(c.y));
            let center = Point2::new(lattice(q.x + r), lattice(q.y));
            if helix_clears_prior(prior, l, q, center, from, z, turns, m) {
                return Some((q, center, turns, r));
            }
        }
    }
    None
}

pub(super) fn helix_cross_fallback(b: &mut PathBuilder, l: &Lvl, e: Point2, z: f64, from: f64, tz: f64, m: &Motion) -> Cuts {
    let safe_tz = if tz < m.hop_z - 1e-9 {
        let cur = b.at();
        b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, m.hop_z), None);
        m.hop_z
    } else {
        tz
    };
    if let Some(r) = ramp_fallback(l, z, from, m) && moves_clear(b, &r, l, z, safe_tz, m) {
        b.note_once("entry: the helix crosses an earlier cut - ramp along the pass instead");
        return push(b, &r, l, z, safe_tz, m);
    }
    if !m.links.approach.center_cutting {
        b.note_once(&format!("{CENTER_CUT_REFUSAL} - any helix or ramp crosses an earlier cut at Z {z:.3}"));
        return Vec::new();
    }
    substituted_dive(b, l, e, z, m, "any helix crosses an earlier cut", safe_tz)
        .map(|d| vec![vec![d, e]])
        .unwrap_or_default()
}

pub(super) fn moves_clear(b: &PathBuilder, d: &Descent, l: &Lvl, z: f64, plane: f64, m: &Motion) -> bool {
    let mut trial = b.clone();
    super::descent::push(&mut trial, d, l, z, plane, m);
    trial
        .moves()
        .get(b.moves_len()..)
        .is_some_and(|moves| !super::arc_cross::moves_cross_prior(moves, b.moves()))
}

fn helix_is_clear(prior: &[Move], l: &Lvl, d: &Descent, z: f64, m: &Motion) -> bool {
    let Some((s, c, from, turns, _)) = helix_parts(d) else { return true };
    helix_clears_prior(prior, l, s, c, from, z, turns, m)
}

fn find_clear_helix_for(prior: &[Move], l: &Lvl, from: f64, z: f64, m: &Motion, must_link: bool) -> Option<Descent> {
    let (start, center, turns, r) = find_clear_helix_raw(prior, l, from, z, false, m, must_link)?;
    Some(make_helix(start, center, from, turns, r))
}

/// A pass start inside a level that a tool which is not centre-cutting
/// would plunge into solid (M11.3, the between-pass law): entered like a
/// level - the helix onto it, else the ramp along the pass - from `from`,
/// or refused by name. Never a plunge.
pub(super) fn restart(b: &PathBuilder, l: &Lvl, leg: Point2, z: f64, from: f64, plane: f64, m: &Motion) -> Descent {
    let e = l.pass.first().copied().unwrap_or(leg);
    if let Some(d) = super::entry_warn::outside_arc_restart(b, l, leg, e, z, plane, m) {
        return d;
    }
    let site = format!("the pass start ({:.3}, {:.3})", e.x, e.y);
    if let h @ Descent::Helix(..) = plan_arm(Entry3D::Helix, false, l, leg, z, from, m)
        && helix_is_clear(b.moves(), l, &h, z, m)
        && moves_clear(b, &h, l, z, plane, m)
    {
        return Descent::Named(Box::new(h), format!("{NOT_CENTER_CUTTING} at a pass start inside a level - helix entry instead"));
    }
    if let Some(r) = ramp_fallback(l, z, from, m)
        && moves_clear(b, &r, l, z, plane, m)
    {
        return Descent::Named(Box::new(r), format!("{NOT_CENTER_CUTTING} at a pass start inside a level - ramp along the pass instead"));
    }
    Descent::Refused(format!(
        "{CENTER_CUT_REFUSAL} - {site} would be a plunge into solid at Z {z:.3}, and no helix or ramp fits without crossing prior cuts"
    ))
}

/// A pass start inside a level that a centre-cutting tool would plunge
/// into solid, where the user asked for a helix, ramp or arc entry
/// (#2265: a second piece of the area, a ring its neighbours have not
/// cleared): entered by the asked arm onto it - a helix only where it
/// closes on the start or links to it at depth, and crossing no prior cut -
/// else the ramp along the pass, else the plunge. Each stand-in is
/// named with the site, the level and the reason; a plunge into material
/// is never silent.
pub(super) fn entry_restart(b: &PathBuilder, l: &Lvl, leg: Point2, z: f64, from: f64, plane: f64, m: &Motion) -> Descent {
    let e = l.pass.first().copied().unwrap_or(leg);
    if let Some(d) = super::entry_warn::outside_arc_restart(b, l, leg, e, z, plane, m) {
        return d;
    }
    let d = plan_arm(m.entry, false, l, leg, z, from, m);
    let why = match &d {
        Descent::Helix(h) => {
            let linked = (h.start.x == e.x && h.start.y == e.y) || links_to(l, h.start, e);
            if linked && helix_is_clear(b.moves(), l, &d, z, m) && moves_clear(b, &d, l, z, plane, m) {
                return d;
            }
            if let Some(alt) = find_clear_helix_for(b.moves(), l, from, z, m, true)
                && moves_clear(b, &alt, l, z, plane, m)
            {
                return alt;
            }
            if !linked {
                "the helix fits only away from the pass start"
            } else {
                "the helix would cross prior cuts"
            }
        }
        Descent::Sub(why) => *why,
        _ => match axial_why(&d, l) {
            None if moves_clear(b, &d, l, z, plane, m) => return d,
            None => "the entry would cross prior cuts",
            Some(why) => why,
        },
    };
    let site = format!("the pass start ({:.3}, {:.3}) at Z {z:.3} is in material and {why}", e.x, e.y);
    let clear = |d: &Descent| moves_clear(b, d, l, z, plane, m);
    // #3746: a start-side pass is entered from the air outside the stock
    // box where a lead reaches its start clear of the plan and the cuts.
    if let Some(a) = super::air_entry::restart(l, m).filter(|d| clear(d)) {
        return Descent::Named(Box::new(a), format!("entry: {site} - entered from the air outside the stock box instead"));
    }
    match ramp_fallback(l, z, from, m).filter(|r| clear(r)) {
        Some(r) => Descent::Named(Box::new(r), format!("entry: {site} - ramp along the pass instead")),
        // #3746: on a start-side pass, a helix that closes on the start
        // (or links to it) and crosses no cut, before the plunge into
        // material.
        None if m.air.is_some()
            && m.entry != Entry3D::Helix
            && let Some(h) = find_clear_helix_for(b.moves(), l, from, z, m, true).filter(|h| clear(h)) =>
        {
            Descent::Named(Box::new(h), format!("entry: {site} - helix entry instead"))
        }
        None => Descent::Named(
            Box::new(Descent::Plunge(e)),
            format!("entry: {site}, and no ramp fits clear of prior cuts - plunged into material at the plunge feed"),
        ),
    }
}

/// The entry family's honorable failure (M8.1): the stated motion
/// could not be honored at this level for `why`, so the ladder
/// substitutes the axial dive - at the pass start where it stands off
/// the wall (M11.3), else at the floor's own room - and says so on the
/// status line with where it dived. A floor with no room either falls
/// back to the wall dive the pocket used to post, still loud. `tz` is
/// the traverse plane.
pub(super) fn substituted_dive(
    b: &mut PathBuilder,
    l: &Lvl,
    e: Point2,
    z: f64,
    m: &Motion,
    why: &str,
    tz: f64,
) -> Option<Point2> {
    let pf = m.links.approach.plunge(m.plunge_feed);
    match dive_point(l, e, m) {
        Some(d) => {
            b.note(format!("entry: {why} - axial dive on the floor instead at ({:.3}, {:.3})", d.x, d.y));
            let cur = b.at();
            if cur.x != d.x || cur.y != d.y {
                b.push(MoveKind::Rapid, Point3::new(d.x, d.y, tz), None);
            }
            feed_down(b, d, z, l.mat_top, m, pf);
            dive_link(b, l, d, e, z, m)
        }
        None => {
            b.note(format!(
                "entry: {why} - the floor has no dive room either; dived at the pass start"
            ));
            b.push(MoveKind::Rapid, Point3::new(e.x, e.y, tz), None);
            feed_down(b, e, z, l.mat_top, m, pf);
            None
        }
    }
}

/// How far a plunge at the pass start must stand off the area's wall so
/// the dwell never marks a finished face: a twentieth of the tool, at
/// least 10 um.
pub(super) fn wall_margin(m: &Motion) -> f64 {
    (m.arc_radius * 0.05).max(0.01)
}

/// Points along the first pass past its start, every half millimetre up
/// to four tool diameters (or the pass's end): where an entry may land
/// when the start itself sits on the wall, joined back to it by a feed
/// along the pass's own line.
pub(super) fn along_pass(l: &Lvl, m: &Motion) -> Vec<Point2> {
    const STEP: f64 = 0.5;
    let reach = (m.arc_radius * 4.0).max(4.0);
    let mut out = Vec::new();
    let mut run = 0.0;
    for w in l.pass.windows(2) {
        let d = (w[1].x - w[0].x).hypot(w[1].y - w[0].y);
        if !(d.is_finite() && d > 0.0) {
            continue;
        }
        let mut t = (STEP - run % STEP) % STEP;
        if t == 0.0 {
            t = STEP;
        }
        // The pass point, then the points a wall margin to either side of
        // it - a row that runs ON the wall still has room just inside.
        let off = wall_margin(m) * 1.5;
        let (nx, ny) = (-(w[1].y - w[0].y) / d * off, (w[1].x - w[0].x) / d * off);
        while t <= d && run + t <= reach && out.len() < 1200 {
            let f = t / d;
            let (px, py) = (w[0].x + (w[1].x - w[0].x) * f, w[0].y + (w[1].y - w[0].y) * f);
            for (dx, dy) in [(0.0, 0.0), (nx, ny), (-nx, -ny)] {
                out.push(Point2::new(lattice(px + dx), lattice(py + dy)));
            }
            t += STEP;
        }
        run += d;
        if run > reach {
            break;
        }
    }
    out
}

/// May a dive at `q` link to the pass start `e` at depth - inside the
/// certified area, crossing no planned pass (the dive link's own law)?
pub(super) fn links_to(l: &Lvl, q: Point2, e: Point2) -> bool {
    l.cen.chord_ok(q, e) && !chord_hits_plan(q, e, l.passes)
}

/// Where an axial dive lands (M11.3, one entry per level): the pass start
/// when it stands off the wall; else the first point along the first pass
/// that does and links back to the start at depth; else the floor's room
/// (the M8.1 dive, its link proved or the second dive named).
pub(super) fn dive_point(l: &Lvl, e: Point2, m: &Motion) -> Option<Point2> {
    // The band boundary is a cutting circle. Landing on its start keeps
    // the descent in the same column on each level of the ring.
    if matches!(l.cen, Cert::RingDisk { .. }) && l.cen.chord_ok(e, e) {
        return Some(e);
    }
    let margin = wall_margin(m);
    if disc_clears(l.cen.room(), e, margin) {
        return Some(e);
    }
    along_pass(l, m)
        .into_iter()
        .find(|&q| disc_clears(l.cen.room(), q, margin) && links_to(l, q, e))
        .or_else(|| dive_room(l.cen.room(), 0.0))
}

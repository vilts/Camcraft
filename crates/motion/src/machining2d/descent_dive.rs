//! The planar ladder's dives (split out of `descent.rs` by M11.3 at the
//! 400-line seam): the axial feed down from just over the material, the
//! at-depth link from a dive to the pass start - or the named second
//! dive - and M8.1's substituted dive. Provenance: standard CNC plunge
//! entries per public G-code manuals and machining handbooks; the laws
//! are this crate's (M8.1's plan law, M11.3's one-entry law). No vendor
//! material consulted.

use crate::approach::{CENTER_CUT_REFUSAL, NOT_CENTER_CUTTING};
use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::entry::Entry3D;
use crate::plunge_law;
use crate::entry::{chord_hits_plan, disc_clears, dive_room};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::descent::{Descent, Lvl, plan_arm, ramp_fallback};
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

/// A pass start inside a level that a tool which is not centre-cutting
/// would plunge into solid (M11.3, the between-pass law): entered like a
/// level - the helix onto it, else the ramp along the pass - from `from`,
/// or refused by name. Never a plunge.
pub(super) fn restart(l: &Lvl, leg: Point2, z: f64, from: f64, m: &Motion) -> Descent {
    let e = l.pass.first().copied().unwrap_or(leg);
    let site = format!("the pass start ({:.3}, {:.3})", e.x, e.y);
    if let h @ Descent::Helix(..) = plan_arm(Entry3D::Helix, false, l, leg, z, from, m) {
        return Descent::Named(Box::new(h), format!("{NOT_CENTER_CUTTING} at a pass start inside a level - helix entry instead"));
    }
    if let Some(r) = ramp_fallback(l, z, from, m) {
        return Descent::Named(Box::new(r), format!("{NOT_CENTER_CUTTING} at a pass start inside a level - ramp along the pass instead"));
    }
    Descent::Refused(format!(
        "{CENTER_CUT_REFUSAL} - {site} would be a plunge into solid at Z {z:.3}, and no helix or ramp fits onto it"
    ))
}

/// A pass start inside a level that a centre-cutting tool would plunge
/// into solid, where the user asked for a helix, ramp or arc entry
/// (#2265: a second piece of the area, a ring its neighbours have not
/// cleared): entered by the asked arm onto it - a helix only where it
/// closes on the start or links to it at depth - else the ramp along the
/// pass, else the plunge. Each stand-in is named with the site, the
/// level and the reason; a plunge into material is never silent.
pub(super) fn entry_restart(l: &Lvl, leg: Point2, z: f64, from: f64, m: &Motion) -> Descent {
    let e = l.pass.first().copied().unwrap_or(leg);
    let d = plan_arm(m.entry, false, l, leg, z, from, m);
    let why = match &d {
        Descent::Helix(h) if (h.start.x == e.x && h.start.y == e.y) || links_to(l, h.start, e) => return d,
        Descent::Helix(_) => "the helix fits only away from the pass start",
        Descent::Sub(why) => *why,
        _ => match axial_why(&d, l) {
            None => return d,
            Some(why) => why,
        },
    };
    let site = format!("the pass start ({:.3}, {:.3}) at Z {z:.3} is in material and {why}", e.x, e.y);
    match ramp_fallback(l, z, from, m) {
        Some(r) => Descent::Named(Box::new(r), format!("entry: {site} - ramp along the pass instead")),
        None => Descent::Named(
            Box::new(Descent::Plunge(e)),
            format!("entry: {site}, and the pass is too short to ramp - plunged into material at the plunge feed"),
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
    let margin = wall_margin(m);
    if disc_clears(l.cen.room(), e, margin) {
        return Some(e);
    }
    along_pass(l, m)
        .into_iter()
        .find(|&q| disc_clears(l.cen.room(), q, margin) && links_to(l, q, e))
        .or_else(|| dive_room(l.cen.room(), 0.0))
}

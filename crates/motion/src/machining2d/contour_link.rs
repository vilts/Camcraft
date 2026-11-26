//! How a multi-depth contour moves from one level to the next and from
//! one ring to the next - the user's choice (`links::Links`), never a
//! premade one. Provenance: the standard multi-depth profile transitions
//! of published CAM practice - lead out, retract (to clearance or the
//! retract plane), traverse and re-enter by the chosen entry; or stay at
//! depth and descend on the ring itself: ramp along it at a stated
//! maximum angle, helix along it at a pitch, plunge at the seam, or a
//! vertical-plane arc onto the seam (the cutter center never leaves the
//! proven ring or the room the lead law proved). RS-274 / ISO 6983
//! linear interpolation for the ramp's 3-axis feeds. No vendor material.

use crate::approach::NOT_CENTER_CUTTING;
use crate::cl::{MoveKind, PathBuilder};
use crate::entry::Entry3D;
use crate::entry_arc::{arc_lead_swept, push_vertical_arc, seg_dir};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::links::{Link, Links};

use super::arcs::Seg;
use super::contour::ContourParams;
use super::contour_depth::Ladder;
use super::contour_entry::say;
use super::fillet::Legal;
use super::lead_fit::{segs_hold, slack};
use super::ring::dist;

/// Default maximum ramp angle (degrees) of the stay-down ramp - the
/// links' own default, re-exported under its pre-M11.2 name.
pub const DEFAULT_LEVEL_RAMP_ANGLE: f64 = crate::links::DEFAULT_RAMP_ANGLE;
/// Laps of the ring a single ramp may take; past this the ring is too
/// short for the angle and the level retracts instead.
const MAX_RAMP_LAPS: usize = 64;

/// How a stay-down level is descended onto, planned per schedule item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stay {
    /// Ramp (or helix) along the ring, this many laps.
    Laps(usize),
    /// Plunge straight down at the seam - on the ring the level law
    /// proved at this depth.
    Plunge,
    /// The vertical-plane arc onto the seam, proved at push time; it
    /// falls back to the seam plunge, named.
    Arc,
}

/// Laps the ramp needs to descend `dz` along a ring of length `len` at
/// no more than `angle_deg`; `None` when the ring cannot hold it.
pub(super) fn ramp_laps(len: f64, dz: f64, angle_deg: f64) -> Option<usize> {
    let tan = angle_deg.to_radians().tan();
    if !(len.is_finite() && len > 1e-6 && dz.is_finite() && dz >= 0.0 && tan.is_finite() && tan > 0.0) {
        return None;
    }
    let run = dz / tan;
    let laps = (run / len).ceil().max(1.0);
    (laps <= MAX_RAMP_LAPS as f64).then_some(laps as usize)
}

/// Laps a helix along the ring takes for `dz` at `pitch` per lap; a zero
/// pitch descends in one lap.
fn helix_laps(dz: f64, pitch: f64) -> Option<usize> {
    let laps = if pitch > 0.0 { (dz / pitch).ceil().max(1.0) } else { 1.0 };
    (laps.is_finite() && laps <= MAX_RAMP_LAPS as f64).then_some(laps as usize)
}

/// The one plunge law (`plunge_law`) at a contour plunge onto `(xy, z)`
/// under the material top `mat_top`.
pub(super) fn guard(b: &mut PathBuilder, p: &ContourParams, xy: Point2, z: f64, mat_top: f64, site: &str) {
    let cc = p.links.approach.center_cutting;
    crate::plunge_law::guard(b, cc, p.tool_diameter * 0.5, xy, z, mat_top, site);
}

/// Push the ramp: from the ring's seam `poly[0]` at `z_from`, `laps`
/// times around the closed polyline `poly`, descending linearly in arc
/// length to `z_to` exactly at the seam. Every feed point lies on the
/// ring, so the center never leaves the path the level law proved.
pub(super) fn push_ramp(b: &mut PathBuilder, poly: &[Point2], laps: usize, z_from: f64, z_to: f64, feed: f64) {
    let n = poly.len();
    if n < 2 {
        return;
    }
    let lap_len: f64 = poly.windows(2).map(|w| dist(w[0], w[1])).sum();
    let total = lap_len * laps as f64;
    if !(total.is_finite() && total > 0.0) {
        return;
    }
    let mut s = 0.0;
    for lap in 0..laps {
        for (k, w) in poly.windows(2).enumerate() {
            s += dist(w[0], w[1]);
            let last = lap + 1 == laps && k + 2 == n;
            let z = if last { z_to } else { z_from - (z_from - z_to) * (s / total) };
            b.push(MoveKind::Feed, Point3::new(w[1].x, w[1].y, z), Some(feed));
        }
    }
}

/// Push a planned stay-down descent: the tool stands on the seam
/// `poly[0]` at `zp` (its lead-out skipped). `Arc` is proved here - its
/// lead chord behind the seam must hold the lead room - and falls back
/// to the plunge at the seam, named.
#[allow(clippy::too_many_arguments)]
pub(super) fn push_stay(
    b: &mut PathBuilder,
    stay: Stay,
    poly: &[Point2],
    walk: &[Seg],
    zp: f64,
    z: f64,
    p: &ContourParams,
    legal: &Legal,
    notes: &mut Vec<String>,
) {
    let Some(&seam) = poly.first() else {
        return;
    };
    match stay {
        Stay::Laps(laps) => push_ramp(b, poly, laps, zp, z, p.feed),
        Stay::Plunge => {
            guard(b, p, seam, z, zp, "the stay-down plunge at the seam");
            b.push(MoveKind::Plunge, Point3::new(seam.x, seam.y, z), Some(p.plunge_feed));
        }
        Stay::Arc => {
            let tol = slack(p);
            let holds = |a: Point2, q: Point2| segs_hold(legal, a, &[Seg::Line(vec![q])], tol);
            let arc = walk
                .first()
                .and_then(|s| seg_dir(seam, s))
                .ok_or("the first cut has no direction to be tangent to")
                .and_then(|d| {
                    let ap = &p.links.approach;
                    let r = ap.arc_nominal(p.tool_diameter);
                    arc_lead_swept(seam, d, zp, z, r, ap.arc_sweep_rad(), p.tolerance, holds)
                });
            match arc {
                Ok(a) => {
                    // Back along the proved lead chord at the level just
                    // cut, then the arc down onto the seam.
                    b.push(MoveKind::Feed, Point3::new(a.start.x, a.start.y, zp), Some(p.feed));
                    guard(b, p, a.start, a.top, zp, "the stay-down arc's capped drop");
                    push_vertical_arc(b, &a, p.links.approach.descent_feed(p.feed), p.plunge_feed);
                }
                Err(why) => {
                    let len: f64 = poly.windows(2).map(|w| dist(w[0], w[1])).sum();
                    if !p.links.approach.center_cutting
                        && let Some(n) = ramp_laps(len, zp - z, p.links.ramp_angle)
                    {
                        // M11.3: no plunge for a tool that may not plunge.
                        say(notes, format!("{NOT_CENTER_CUTTING} - no stay-down arc at the seam ({why}); ramped along the ring instead"));
                        push_ramp(b, poly, n, zp, z, p.feed);
                        return;
                    }
                    guard(b, p, seam, z, zp, "the stay-down plunge at the seam");
                    say(
                        notes,
                        format!(
                            "level link: no stay-down arc onto Z {z:.3} at the seam ({:.3}, {:.3}) - \
                             {why}; plunged on the ring there",
                            seam.x, seam.y
                        ),
                    );
                    b.push(MoveKind::Plunge, Point3::new(seam.x, seam.y, z), Some(p.plunge_feed));
                }
            }
        }
    }
}

/// The cut order and its stay-down links. Level by level (every ring at
/// a level, then the next level) unless the level link is Keep down;
/// ring by ring then, each ring all the way down before the next, so a
/// ring's levels follow one another and the descent can link them.
/// `stay[i]` is how item `i` is descended onto from item `i - 1`, `None`
/// where item `i` is entered from above; every stay-down level that
/// cannot stay is named in `notes`.
pub(super) fn plan_links(
    p: &ContourParams,
    lad: &Ladder,
    plan: &[Vec<Point2>],
    notes: &mut Vec<String>,
) -> (Vec<(f64, usize)>, Vec<Option<Stay>>) {
    let is_tabbed = |z: f64| p.tab_count > 0 && z < p.bottom_z + p.tab_height;
    let loops = lad.per_ring.len();
    let keep = p.links.level == Link::KeepDown;
    let sched: Vec<(f64, usize)> = if keep {
        (0..loops).flat_map(|li| lad.ring(li).iter().map(move |&z| (z, li))).collect()
    } else {
        lad.zs.iter()
            .flat_map(|&z| (0..loops).filter(move |&li| lad.walks(li, z)).map(move |li| (z, li)))
            .collect()
    };
    let mut stay: Vec<Option<Stay>> = vec![None; sched.len()];
    if !keep {
        return (sched, stay);
    }
    let arm = p.links.arm(p.entry);
    let word = match arm {
        Entry3D::Ramp => "ramp",
        Entry3D::Helix => "helix",
        Entry3D::Plunge => "plunge",
        Entry3D::Arc => "arc",
    };
    for i in 1..sched.len() {
        let ((z, li), (zp, lp)) = (sched[i], sched[i - 1]);
        if li != lp {
            continue;
        }
        let at = plan[li].first().copied().unwrap_or(Point2::new(0.0, 0.0));
        let refuse = |notes: &mut Vec<String>, why: String| {
            say(
                notes,
                format!(
                    "level link: no stay-down {word} onto Z {z:.3} on the ring at ({:.3}, {:.3}) - \
                     {why}; retract and re-enter there",
                    at.x, at.y
                ),
            );
        };
        if is_tabbed(z) || is_tabbed(zp) {
            refuse(notes, "a standing tab is material on the ring".into());
            continue;
        }
        let len: f64 = plan[li].windows(2).map(|w| dist(w[0], w[1])).sum();
        stay[i] = match arm {
            Entry3D::Ramp => match ramp_laps(len, zp - z, p.links.ramp_angle) {
                Some(n) => Some(Stay::Laps(n)),
                None => {
                    refuse(
                        notes,
                        format!(
                            "the ring is too short for a {:.1} deg ramp over {:.3} mm",
                            p.links.ramp_angle,
                            zp - z
                        ),
                    );
                    None
                }
            },
            Entry3D::Helix => match helix_laps(zp - z, p.links.helix_pitch) {
                Some(n) => Some(Stay::Laps(n)),
                None => {
                    refuse(notes, format!("a {:.3} mm pitch asks too many laps", p.links.helix_pitch));
                    None
                }
            },
            // M11.3: a tool that is not centre-cutting never plunges at
            // the seam - it ramps along the ring at the links' angle, or
            // re-enters from above (where the one plunge law reads it).
            Entry3D::Plunge if !p.links.approach.center_cutting => match ramp_laps(len, zp - z, p.links.ramp_angle) {
                Some(n) => {
                    say(notes, format!("{NOT_CENTER_CUTTING} - the stay-down plunge ramps along the ring instead"));
                    Some(Stay::Laps(n))
                }
                None => {
                    refuse(notes, "the tool is not centre-cutting and the ring is too short to ramp".into());
                    None
                }
            },
            Entry3D::Plunge => Some(Stay::Plunge),
            Entry3D::Arc => Some(Stay::Arc),
        };
    }
    (sched, stay)
}

/// The wall arc's ring-to-ring hop under pass Clearance: feed up to the
/// hop plane, then rapid to clearance.
pub(super) fn wall_arc_hop(b: &mut PathBuilder, p: &ContourParams, hop_z: f64) {
    let cur = b.at();
    b.push(MoveKind::Feed, Point3::new(cur.x, cur.y, hop_z), Some(p.feed));
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
}

/// Tab window to tab window: up over the tab, across, down (pecked where
/// the dive would go deep under the disc, #3678: `uncut` is
/// `contour_peck::uncut_floor`).
pub(super) fn tab_window_dive(b: &mut PathBuilder, p: &ContourParams, e2: Point2, hop_z: f64, lead_top: f64, uncut: Option<f64>) {
    let cur = b.at();
    b.push(MoveKind::Feed, Point3::new(cur.x, cur.y, hop_z), Some(p.feed));
    b.push(MoveKind::Rapid, Point3::new(e2.x, e2.y, hop_z), None);
    guard(b, p, e2, lead_top, p.entry_top(), "the dive past a tab");
    let pf = p.plunge_feed;
    super::contour_peck::dive(b, p, e2, lead_top, uncut, hop_z, pf, |b, d| b.push(MoveKind::Plunge, Point3::new(e2.x, e2.y, d), Some(pf)));
}

/// A bore-side ring that follows another one at its level, entered by the
/// user's arm instead of the pass link's plunge into material.
pub(super) struct RingEntry {
    pub arm: Entry3D,
    /// The plane of the traverse between the rings: the pass link's own
    /// (clearance for `Link::Clearance`, the hop plane otherwise - also for
    /// a keep-down link that failed its proof, as `ring_link` does).
    pub plane: f64,
}

/// The entry of a ring after another one: the entry on the path's first
/// level, the transition below it. `None` where the pass link keeps the
/// ring (the plunge entry, a tabbed level, the wall side's arc - its own
/// hop and wall arc enter that ring). On the wall side the ramp and the
/// helix take the wall-side ramp (#3917, `contour_wall_ramp`).
pub(super) fn ring_entry(p: &ContourParams, applies: bool, inside: bool, first_level: bool, arm: Entry3D, hop_z: f64) -> Option<RingEntry> {
    let arm = if first_level { p.entry } else { arm };
    let plane = if p.links.pass == Link::Clearance { p.clearance_z } else { hop_z };
    let armed = if inside { arm != Entry3D::Plunge } else { matches!(arm, Entry3D::Ramp | Entry3D::Helix) };
    (applies && armed).then_some(RingEntry { arm, plane })
}

/// The ring-to-ring move within one level (the pass link). `Ok(true)`:
/// the link was posted and the tool stands on `e2` at `z`. Keep down
/// feeds there when the straight link holds the lead room, crosses no
/// planned ring, stays under the stay-down limit and no tab stands on
/// the level; otherwise - and for the retract choices - the hop template
/// lifts, at the retract plane or clearance.
#[allow(clippy::too_many_arguments)]
pub(super) fn ring_link(
    b: &mut PathBuilder,
    p: &ContourParams,
    legal: &Legal,
    cut: &[&[Vec<Point2>]],
    e2: Point2,
    z: f64,
    hop_z: f64,
    tabbed: bool,
    lifted: &mut crate::links::LinkTally,
    enter: bool,
) -> bool {
    let cur = b.at();
    let at = Point2::new(cur.x, cur.y);
    if p.links.pass == Link::KeepDown {
        let proof = keep_down_proof(p, legal, cut, at, e2, tabbed, &p.links);
        match proof {
            Ok(()) => {
                b.push(MoveKind::Feed, Point3::new(e2.x, e2.y, z), Some(p.feed));
                return true;
            }
            Err(why) => lifted.lift(why, at, z),
        }
    }
    // The new ring is entered by the entry the user chose (a ramp or a
    // helix), not by this link's straight plunge into material: nothing is
    // posted, the caller engages the level.
    if enter {
        return false;
    }
    if cur.z < hop_z {
        b.push(MoveKind::Feed, Point3::new(cur.x, cur.y, hop_z), Some(p.feed));
    }
    if p.links.pass == Link::Clearance {
        b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
        b.push(MoveKind::Rapid, Point3::new(e2.x, e2.y, p.clearance_z), None);
    }
    b.push(MoveKind::Rapid, Point3::new(e2.x, e2.y, hop_z), None);
    guard(b, p, e2, z, p.entry_top(), "the ring-to-ring dive at the lead-in");
    b.push(MoveKind::Plunge, Point3::new(e2.x, e2.y, z), Some(p.plunge_feed));
    true
}

/// The contour's keep-down law: the cutter center on the legal side of
/// the part offset all along the link (`lead_fit`'s room law), no planned
/// ring crossed, within the limit, and no tab standing on the level.
fn keep_down_proof(
    p: &ContourParams,
    legal: &Legal,
    cut: &[&[Vec<Point2>]],
    a: Point2,
    e: Point2,
    tabbed: bool,
    links: &Links,
) -> Result<(), &'static str> {
    if tabbed {
        return Err("a standing tab is material the link cannot see");
    }
    if !links.within_stay_down(dist(a, e)) {
        return Err(crate::links::TOO_LONG);
    }
    let room = segs_hold(legal, a, &[Seg::Line(vec![e])], slack(p));
    if room && !cut.iter().any(|c| crate::entry::chord_hits_plan(a, e, c)) {
        Ok(())
    } else {
        Err(crate::links::NOT_PROVED)
    }
}

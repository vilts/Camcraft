//! Offset contouring (split from `machining2d.rs` by M5.0.1); M11.1 B
//! adds the optional corner fillets - provenance in `fillet.rs`
//! (tangent circle to two lines, RS-274 / ISO 6983 G2/G3); M11.2 C/D the
//! user's level transitions and links (`contour_link`, `links.rs`).

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::entry::Entry3D;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::links::{Link, LinkTally};

use super::arcs::{fit_arcs, push_segs};
use super::contour_depth::{Ladder, ladder};
use super::contour_entry::{Engage, crosses, say, seam_leads, wall_arc, wall_arc_top};
use super::contour_comp::{self, SpanCtx};
use super::contour_link::{plan_links, push_stay, ring_link};
use super::contour_step::{Said, Step, engage_level};
use super::levels::{Cert, Motion};
use super::ring::Ring;
use super::face::GenError;
use super::fillet::{Legal, fillet_corners};
use super::fillet_tally::{FilletTally, fillet_floor};
use super::lead_ext::{axis_reseat, extend_leads, push_lead_down, retrace};
use super::lead_fit::reseat;

pub use super::contour_params::{ContourParams, Lead, Side};


/// Offset-contour `region` (a material cross-section): every level walks
/// the cutter-center loops of the region grown by `r + leave`, so the
/// tool edge grazes the boundary with `leave = 0` and stops a finishing
/// allowance short of it otherwise. The plan phrases this as
/// `- (r - leave)`; that is the same curve written from the void side -
/// here the region is material, so both outer walls and island/hole
/// walls come out of one positive offset. [`Side::Inside`] negates that
/// one term - the region erodes and each loop traverses reversed, so the
/// cutter rides the boundary from the void side with material still on
/// the left (Auto is this Outside law inside the generator). Loop order
/// is deterministic (largest absolute area first, canonically oriented,
/// and traversed per the climb/side XOR).
///
/// Between subpaths (tab hops) and between loops of one level the tool
/// retracts to the hop plane - `retract_z`, clamped to `clearance_z` so
/// a legacy retract parked above clearance cannot make the hop climb
/// higher than the clearance move it replaced, with the clamp echoed in
/// the panel - rapids across in air, and plunges again. A level change
/// still lifts to full `clearance_z`: the new level engages material the
/// previous one never touched, and crosses into it by the `entry`
/// family (M8.2 A) - every arm certified against the one center this
/// ladder shares, on the bore side where that center is bounded air.
/// Rapids inside the footprint below the
/// safe plane are the verifier's business (step 7), flagged, never
/// hidden.
pub fn contour(region: &Region, p: &ContourParams) -> Result<Toolpath, GenError> {
    contour_impl(region, p, None)
}

/// [`contour`] of a region sectioned off a finished part: each ring is
/// walked only at the levels where the part's wall it trims still
/// exists (`contour_depth`) - a blind feature's ring ends at its floor
/// plus the stock to leave, and the dropped levels are named in the
/// notes. Through features and outer walls keep every level, so a part
/// whose rings all run the full depth posts exactly [`contour`]'s path.
pub fn contour_on_part(
    region: &Region,
    part: &[Triangle],
    p: &ContourParams,
) -> Result<Toolpath, GenError> {
    contour_impl(region, p, Some((part, &[])))
}

/// [`contour_on_part`] of a part that is the union of touching bodies
/// (M13.11): `bodies` are the per-solid index ranges of `part`, and the
/// depth ladder reads each plane as the 2D union of the bodies' own
/// sections - two models butted face to face cut every level the one
/// block they stand for cuts. Fewer than two ranges is
/// [`contour_on_part`], byte for byte.
pub fn contour_on_bodies(
    region: &Region,
    part: &[Triangle],
    bodies: &[std::ops::Range<usize>],
    p: &ContourParams,
) -> Result<Toolpath, GenError> {
    contour_impl(region, p, Some((part, bodies)))
}

fn contour_impl(
    region: &Region,
    p: &ContourParams,
    part: Option<(&[Triangle], &[std::ops::Range<usize>])>,
) -> Result<Toolpath, GenError> {
    p.validate()?;
    // Control-side compensation (M13.11): its parameter door first.
    contour_comp::door(p)?;
    if region.is_empty() {
        return Err(GenError::Empty("empty footprint".into()));
    }
    let r = p.tool_diameter * 0.5;
    let inside = p.side == Side::Inside;
    let d = r + p.stock_to_leave;
    let center = region.offset(if inside { -d } else { d })?;
    if center.is_empty() {
        return Err(GenError::Empty(if inside {
            "the bore is narrower than the tool - switch Side to Outside or pick a smaller tool"
                .into()
        } else {
            "offset boundary collapsed".into()
        }));
    }
    // The contour the post programs under G41/G42 (M13.11): the part
    // at the stock to leave; `None` for every path cut in computer.
    let prog = contour_comp::programmed_region(region, p)?;
    let flat = p.levels()?;
    let mut loops: Vec<&Loop> = center.loops().iter().collect();
    loops.sort_by(|a, b| b.area().abs().total_cmp(&a.area().abs()));

    // The rings every level plans to walk, as closed polylines - the
    // link chords of M8.1's plan law get proven against these. Tabbed
    // levels walk a subset, which errs conservative: an unproven chord
    // hops rather than dragging across a ring line.
    let plan: Vec<Vec<Point2>> = loops
        .iter()
        .map(|l| {
            let mut v: Vec<Point2> = if p.climb != inside {
                l.points().to_vec()
            } else {
                l.points().iter().rev().copied().collect()
            };
            if v.len() > 1 {
                let (first, last) = (v[0], v[v.len() - 1]);
                if first.x != last.x || first.y != last.y {
                    v.push(first);
                }
            }
            v
        })
        .collect();
    // The shared motion envelope: dive_link and substituted_dive read
    // their lifts and feeds through it, so the contour's arms speak
    // the pocket ladders' word verbatim (hop = retract clamped at
    // clearance, exactly what `hop` below computes).
    let m = Motion::new(p.clearance_z, Some(p.retract_z), p.feed, p.plunge_feed)
        .with_entry(p.entry, p.helix_radius)
        .with_arc_radius(p.tool_diameter)
        .with_links(p.links, p.helix_radius);
    let mut said = Said::default();
    let cert = Cert::Region(&center);
    // Climb travel keeps the part on the left on both sides (see the
    // per-loop flip below), so the air - the only side a fillet may
    // stand on - is right for climb, left for conventional.
    let legal = Legal { region: &center, inside, air_left: Some(!p.climb) };
    let mut tally = FilletTally::default();
    // Lead shrinks, refusals and reseated starts, each said once.
    let mut lead_notes: Vec<String> = Vec::new();
    // Which ring is cut at which level: all of them at every level for
    // a flat region; against a part, each ring down to its own floor.
    let lad = match part {
        None => Ladder::uniform(&flat, loops.len()),
        Some((f, bodies)) => ladder(&plan, f, bodies, &flat, r, p.stock_to_leave, &mut lead_notes),
    };
    if lad.zs.is_empty() {
        // Every ring stands in the part at its first level: nothing is
        // legal to cut, and an empty path would read as a clean one.
        return Err(GenError::Invalid(format!(
            "every contour ring cuts into the part at its first level - nothing to machine \
             without a gouge ({})",
            lead_notes.join("; ")
        )));
    }
    let lad = &lad;

    let eng = Engage { p, center: &center, cert: &cert, plan: &plan, m: &m, legal: &legal };

    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let hop = p.retract_z.min(p.clearance_z);
    let mut first_entry = true;
    let mut lifted = LinkTally::default();
    // The dive links cut on one plane (the bore side's descents): planar
    // feeds a keep-down ring link at that plane must not cross.
    let mut dives: (f64, Vec<Vec<Point2>>) = (f64::NAN, Vec::new());
    let is_tabbed = |z: f64| p.tab_count > 0 && z < p.bottom_z + p.tab_height;
    // The cut order, and where the stay-down link joins it (contour_link).
    let (sched, stay) = plan_links(p, lad, &plan, &mut lead_notes);
    for (si, &(z, li)) in sched.iter().enumerate() {
        let l = loops[li];
        let tabbed = is_tabbed(z);
        // A tab-hop template traverses at `hop` between windows, and a
        // standing tab is material AT the window down to the level -
        // below `bottom_z + tab_height` the hop plane itself runs
        // through the tab. On tabbed levels the traverse therefore
        // rides the tab top when that is higher (a no-op on sane files
        // unless the tab truly stands above the plane).
        let hop_z = if tabbed { hop.max(p.bottom_z + p.tab_height) } else { hop };
        // Another ring was the last cut, at this same level: link ring
        // to ring; anything else crosses in from above.
        let same_level = si > 0 && sched[si - 1].0.to_bits() == z.to_bits();
        // This item is reached by staying down / the next one leaves so.
        let stay_in = stay[si];
        let stay_out = stay.get(si + 1).copied().flatten().is_some();
        // This ring's own ladder: the level above it is the one IT was
        // last cut at, whatever another ring's floor added.
        let rz = lad.ring(li);
        let rzi = rz.iter().position(|q| q.to_bits() == z.to_bits()).unwrap_or(0);
        // Canonical orientation is material-on-the-left travel;
        // conventional climbing reverses it. An Inside region was
        // eroded, not grown, so its stored winding puts the (outside)
        // material on the right - flip with the sign and the rule.
        let pts: Vec<Point2> = if p.climb != inside {
            l.points().to_vec()
        } else {
            l.points().iter().rev().copied().collect()
        };
        // A closed ring whose start leaves the lead no room (a bore
        // inset's corner) starts mid-run instead; tabbed levels keep the
        // start their windows are measured from.
        // M11.3 C: an axis-aligned start first (an arc entry tangent to
        // it posts as a true G18/G19), then the lead room's own reseat.
        let ap = &p.links.approach;
        let pts = if !tabbed && ap.axis_start { axis_reseat(&pts, ap.axis_tol).unwrap_or(pts) } else { pts };
        // A control-compensated ring seams mid-run (contour_comp).
        let pts = if prog.is_some() && !tabbed { contour_comp::seam_mid_run(&pts) } else { pts };
        let pts = if tabbed { pts } else { reseat(&pts, p, &legal, &mut lead_notes).unwrap_or(pts) };
        let ring = Ring::new(pts);
        // The overlap past the start on closed rings (M11.3 C): not on
        // tabbed levels (their windows are the tab law's) and not under a
        // keep-down level link (the next level descends from the seam).
        let ov = if tabbed || ap.overlap <= 0.0 {
            0.0
        } else if p.links.level == Link::KeepDown {
            say(&mut lead_notes, "lead: the overlap stands down under the keep-down level link - the next level descends from the seam".into());
            0.0
        } else {
            ap.overlap.min(ring.len)
        };
        let keeps = if tabbed {
            ring.tab_keeps(p.tab_count as usize, p.tab_width)
        } else {
            vec![(0.0, ring.len)]
        };
        for (k, &(a, e)) in keeps.iter().enumerate() {
            let poly = ring.arc_between(a, e);
            if poly.len() < 2 {
                continue;
            }
            let mut walk = fit_arcs(&poly, p.tolerance);
            if p.blend_turns {
                let (w, t) = fillet_corners(&walk, poly[0], r, p.tolerance, &legal);
                walk = w;
                tally.add(t);
            }
            // Both leads keep the cutter center in the legal room
            // (lead_fit.rs), meet the wall on its own tangent, and stay
            // off each other at the seam (contour_entry::seam_leads); the
            // extensions ride the same room (lead_ext).
            let (entry, lead, lead_off) = seam_leads(&walk, &poly, p, &legal, &mut lead_notes);
            let (entry, lead, lead_off) = extend_leads(entry, lead, lead_off, &walk, &poly, p, &legal, &mut lead_notes);
            // The overlap (M11.3 C): the walk retraces its own first
            // moves - the same segments, so the same chords, never a new
            // crossing - and the lead-out leaves from where that ends.
            let mut window = (poly, walk, entry, lead, lead_off);
            if ov > 0.0 {
                let (rpoly, rwalk) = retrace(&window.0, &window.1, ov);
                let (en, li, lo) = seam_leads(&rwalk, &rpoly, p, &legal, &mut lead_notes);
                let (en, li, lo) = extend_leads(en, li, lo, &rwalk, &rpoly, p, &legal, &mut lead_notes);
                let last = rpoly[rpoly.len() - 1];
                if crosses(en, &li, last, &lo) {
                    say(&mut lead_notes, format!("lead: the {ov:.3} mm overlap's lead-out crosses the lead-in at ({:.3}, {:.3}) - overlap dropped", last.x, last.y));
                } else {
                    window = (rpoly, rwalk, en, li, lo);
                }
            }
            let (poly, walk, entry, lead, lead_off) = window;
            let e2 = Point2::new(entry.x, entry.y);
            let lf = ap.lead_feed(p.feed);
            // The vertical lead-in's top (M11.3 C): a dive at the lead's
            // start stops this high and the lead descends along itself.
            let lead_top = if ap.lead_height > 0.0 && !lead.is_empty() { z + ap.lead_height } else { z };
            if let (Some(s), Some(&zp)) = (stay_in, rzi.checked_sub(1).and_then(|i| rz.get(i))) {
                // Stayed down: the tool stands on the seam at the level
                // above (its lead-out skipped); descend on the ring.
                push_stay(&mut b, s, &poly, &walk, zp, z, p, &legal, &mut lead_notes);
                first_entry = false;
                push_segs(&mut b, &walk, z, p.feed);
                if !stay_out {
                    push_segs(&mut b, &lead_off, z, p.feed);
                }
                continue;
            }
            // The level change's motion: the entry on the path's first
            // engagement, the user's transition after it.
            let arm = if first_entry { p.entry } else { p.links.arm(p.entry) };
            // The wall side's arc: every engagement of an untabbed
            // outside ring descends a proved arc beside the wall
            // (contour_entry::wall_arc); a tabbed level, or a ring with
            // no room for one, takes the graze and says why.
            // Ring to ring at one level re-enters by the arc too (the
            // pre-M11.2 shape) - unless the pass link keeps down, which
            // feeds straight over where proved (contour_link::ring_link).
            let ring_to_ring = k == 0 && same_level && !first_entry;
            let wall_arc_on = !inside
                && arm == Entry3D::Arc
                && k == 0
                && !(ring_to_ring && p.links.pass == Link::KeepDown);
            if wall_arc_on && ring_to_ring && p.links.pass == Link::Clearance {
                let cur = b.at();
                b.push(MoveKind::Feed, Point3::new(cur.x, cur.y, hop_z), Some(p.feed));
                b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
            }
            let mut arced = false;
            if wall_arc_on && !tabbed {
                let z_top = wall_arc_top(p, rz, rzi, hop_z);
                let first = lead.first().or(walk.first());
                match wall_arc(&mut b, &eng, e2, z, z_top, hop_z, first) {
                    Ok(()) => arced = true,
                    Err(why) => say(
                        &mut lead_notes,
                        format!(
                            "entry: no arc fits beside the wall at the lead-in ({:.3}, {:.3}) - {why}; \
                             grazing dive there instead",
                            e2.x, e2.y
                        ),
                    ),
                }
            } else if wall_arc_on {
                say(
                    &mut lead_notes,
                    "entry: the arc stands down on tabbed levels - a standing tab is material \
                     beside the wall the room law cannot see; grazing dive at the lead-in there"
                        .into(),
                );
            }
            if arced {
                // Engaged: the arc ends standing at (e2, z).
            } else if ring_to_ring {
                // Ring to ring within one level: the pass link, proved
                // against the rings and this level's dive links.
                let cut = if dives.0.to_bits() == z.to_bits() { dives.1.as_slice() } else { &[] };
                ring_link(&mut b, p, &legal, &[&plan, cut], e2, z, hop_z, tabbed, &mut lifted);
            } else if k == 0 {
                // The level's first engagement: a crossing, or the path's
                // first entry (prev_z falls back to clearance there).
                let prev_z = if rzi == 0 { p.clearance_z } else { rz[rzi - 1] };
                let step = Step { arm, e2, z, prev_z, first: first_entry, inside, tabbed, lead_top };
                if let Some(d) = engage_level(&mut b, &eng, &step, &lead, &walk, &mut said) {
                    if dives.0.to_bits() != z.to_bits() {
                        dives = (z, Vec::new());
                    }
                    dives.1.push(vec![d, e2]);
                }
            } else {
                // Tab window to tab window: up over the tab, across, down.
                let cur = b.at();
                b.push(MoveKind::Feed, Point3::new(cur.x, cur.y, hop_z), Some(p.feed));
                b.push(MoveKind::Rapid, Point3::new(e2.x, e2.y, hop_z), None);
                super::contour_link::guard(&mut b, p, e2, lead_top, p.top_z, "the dive past a tab");
                b.push(MoveKind::Plunge, Point3::new(e2.x, e2.y, lead_top), Some(p.plunge_feed));
            }
            first_entry = false;
            let at = b.at();
            let lead_at = b.moves_len();
            if at.x == e2.x && at.y == e2.y && at.z > z {
                push_lead_down(&mut b, &lead, at.z, z, lf);
            } else {
                if lead_top > z {
                    say(&mut lead_notes, "lead: the vertical lead-in rides only a dive at the lead-in's start - this entry lands on the cut plane".into());
                }
                push_segs(&mut b, &lead, z, lf);
            }
            let walk_at = b.moves_len();
            push_segs(&mut b, &walk, z, p.feed);
            let out_at = b.moves_len();
            if !stay_out {
                push_segs(&mut b, &lead_off, z, p.feed);
            }
            // The control's span (M13.11): the lead-in move that lands
            // on the wall engages, the lead-out move that leaves it
            // cancels; a lead the room law refused is a refusal here
            // too, by name - never a centre path under a Control op.
            if let Some(prog) = &prog {
                if walk_at == lead_at || b.moves_len() == out_at {
                    return Err(contour_comp::refused_as(p, format!(
                        "needs a lead-in and a lead-out at ({:.3}, {:.3}) - the room law refused one ({})",
                        e2.x,
                        e2.y,
                        lead_notes.join("; ")
                    ))
                    .with_ghost_moves(b.moves()));
                }
                // #871: a part-way refusal hands back the rings so far.
                contour_comp::push_span(&mut b, &SpanCtx { prog, p }, walk_at - 1, out_at)
                    .map_err(|e| e.with_ghost_moves(b.moves()))?;
            }
        }
    }
    lifted.say(&mut b, "ring-to-ring");
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    for n in lead_notes {
        b.note(n);
    }
    for n in tally.notes("walked", fillet_floor(p.tolerance)) {
        b.note(n);
    }

    let path = b.finish();
    if let Some(why) = crate::approach::refusal(&path) {
        // #261 part 3: the moves built so far ride the refusal, display-only.
        return Err(GenError::Invalid(why).with_ghost(&path));
    }
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}


/// Which side of travel is air: canonical (climb) travel keeps material on
/// the left, so air is right; reversed travel flips both.
pub(super) fn air_side(t: Point2, climb: bool) -> Point2 {
    if climb {
        Point2::new(t.y, -t.x)
    } else {
        Point2::new(-t.y, t.x)
    }
}

pub(super) fn unit(dx: f64, dy: f64) -> Point2 {
    let m = (dx * dx + dy * dy).sqrt();
    if m == 0.0 || !m.is_finite() {
        Point2::new(1.0, 0.0)
    } else {
        Point2::new(dx / m, dy / m)
    }
}

//! Offset contouring (split from `machining2d.rs` by M5.0.1); M11.1 B
//! adds the optional corner fillets - provenance in `fillet.rs`
//! (tangent circle to two lines, RS-274 / ISO 6983 G2/G3); M11.2 C/D the
//! user's level transitions and links (`contour_link`, `links.rs`).

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::entry::Entry3D;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::links::{Link, LinkTally};

use super::arcs::push_segs;
use super::contour_depth::Ladder;
use super::contour_entry::{Engage, crosses, flatten, say, seam_leads, wall_arc, wall_arc_top};
use super::contour_comp::{self, SpanCtx};
use super::contour_link::{plan_links, push_stay, ring_entry, ring_link, tab_window_dive, wall_arc_hop};
use super::contour_peck::uncut_floor;
use super::contour_step::{Said, Step, engage_level};
use super::levels::{Cert, Motion};
use super::ring::Ring;
use super::tab_bridge::{cached, fit_tabs, keep_cut};
use super::face::GenError;
use super::fillet::Legal;
use super::fillet_tally::{FilletTally, fillet_floor};
use super::contour_start;
use super::lead_ext::{extend_leads, push_lead_down, retrace};

pub use super::contour_params::{ContourParams, Lead, Side};
pub use super::contour_region::{contour, contour_on_bodies, contour_on_part, contour_on_picked};

/// Offset loops, programmed region and walking polylines prepared for one
/// contour band; shared with `contour_whole`.
pub(super) struct PreparedBand {
    pub center: Region,
    pub prog: Option<Region>,
    pub loops: Vec<Loop>,
    pub plan: Vec<Vec<Point2>>,
    /// The lead room's region when it is not `center`: `center` with the
    /// rings that another band cuts on a shared level fenced off (#2677).
    pub room: Option<Region>,
}

pub(super) fn prepare_band(region: &Region, p: &ContourParams) -> Result<PreparedBand, GenError> {
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
    let mut loops: Vec<Loop> = center.loops().to_vec();
    loops.sort_by(|a, b| b.area().abs().total_cmp(&a.area().abs()));
    // The rings every level plans to walk, as closed polylines - the
    // link chords of M8.1's plan law get proven against these. Tabbed
    // levels walk a subset, which errs conservative: an unproven chord
    // hops rather than dragging across a ring line.
    let plan = loops
        .iter()
        .map(|l| {
            let mut v: Vec<Point2> = super::ring::walked(l.points(), p.climb == inside, inside);
            if v.len() > 1 {
                let (first, last) = (v[0], v[v.len() - 1]);
                if first.x != last.x || first.y != last.y {
                    v.push(first);
                }
            }
            v
        })
        .collect();
    Ok(PreparedBand { center, prog, loops, plan, room: None })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_band(
    b: &mut PathBuilder,
    band: &PreparedBand,
    p: &ContourParams,
    lad: &Ladder,
    pick: Option<(usize, Point2)>,
    exit: Option<(usize, Point2)>,
    said: &mut Said,
    lead_notes: &mut Vec<String>,
    tally: &mut FilletTally,
) -> Result<(), GenError> {
    let inside = p.side == Side::Inside;
    // The shared motion envelope: dive_link and substituted_dive read
    // their lifts and feeds through it, so the contour's arms speak
    // the pocket ladders' word verbatim (hop = retract clamped at
    // clearance, exactly what `hop` below computes).
    let m = Motion::new(p.clearance_z, Some(p.retract_z), p.feed, p.plunge_feed)
        .with_entry(p.entry, p.helix_radius)
        .with_arc_radius(p.tool_diameter)
        .with_links(p.links, p.helix_radius);
    let cert = Cert::Region(&band.center);
    // Climb travel keeps the part on the RIGHT on both sides (#1555, see
    // the per-loop flip below), so the air - the only side a fillet may
    // stand on - is left for climb, right for conventional.
    let legal = Legal { region: band.room.as_ref().unwrap_or(&band.center), inside, air_left: Some(p.climb) };
    let eng = Engage { p, center: &band.center, cert: &cert, plan: &band.plan, m: &m, legal: &legal };
    let prog = &band.prog;
    let hop = p.retract_z.min(p.clearance_z);
    let mut first_entry = true;
    let mut lifted = LinkTally::default();
    // The dive links cut on one plane (the bore side's descents): planar
    // feeds a keep-down ring link at that plane must not cross.
    let mut dives: (f64, Vec<Vec<Point2>>) = (f64::NAN, Vec::new());
    // The last ring's leads: a keep-down link proves itself against them
    // and the next ring's, cut paths too (#1555: climb's leads cross it).
    let mut last_leads: Vec<Vec<Point2>> = Vec::new();
    let is_tabbed = |z: f64| p.tab_count > 0 && z < p.bottom_z + p.tab_height;
    // #2480 / #2561: each ring's tab windows, fitted once (tab_bridge) and
    // held against every other cut of the tabbed levels (tab_clear).
    // Only a level that a ring is cut at holds its tabs against it.
    let tab_levels: Vec<Vec<u64>> =
        (0..band.loops.len()).map(|li| lad.ring(li).iter().filter(|&&z| is_tabbed(z)).map(|z| z.to_bits()).collect()).collect();
    let mut fits = super::tab_clear::fit_loops(&band.loops, &tab_levels, pick, p, &legal);
    // The cut order, and where the stay-down link joins it (contour_link).
    let (sched, stay) = plan_links(p, lad, &band.plan, lead_notes);
    for (si, &(z, li)) in sched.iter().enumerate() {
        let l = &band.loops[li];
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
        // #1555: with the M3 spindle every 2D op posts, climb milling
        // keeps the material on the RIGHT of travel (down milling: the
        // tooth meets the stock at full chip and leaves it thin) - an
        // outside ring runs clockwise, an inside one counter-clockwise;
        // conventional reverses it. A stored ring keeps its area on the
        // left: grown (Outside), that area is the part, so climb walks
        // it reversed; eroded (Inside), it is the bore's air and the
        // material already stands right of the stored walk.
        let pts: Vec<Point2> = super::ring::walked(l.points(), p.climb == inside, inside);
        // The ring's start (contour_start::seat): the pick, or an axis start,
        // a comp seam, the lead room's reseat; tabbed levels keep the start
        // their windows are measured from; #3453 starts the untabbed levels
        // of a tabbed ring that shares no level (no other ring or band) at
        // its first window, so a shared level keeps its seam.
        let ap = &p.links.approach;
        let picked = pick.filter(|&(pi, _)| pi == li).map(|(_, q)| q);
        let seated = contour_start::seat(pts.clone(), picked, tabbed, prog.is_some(), p, &legal, lead_notes);
        let alone = !tabbed && picked.is_none() && band.room.is_none() && !(0..band.loops.len()).any(|lj| lj != li && rz.iter().any(|&q| lad.walks(lj, q)));
        let seated = super::tab_clear::level_start(seated, pts, fits.get(&li).filter(|_| alone && !tab_levels[li].is_empty()));
        let ring = Ring::new(seated);
        // The overlap past the start on closed rings (M11.3 C): not on
        // tabbed levels (their windows are the tab law's) and not under a
        // keep-down level link (the next level descends from the seam).
        let ov = if tabbed || ap.overlap <= 0.0 {
            0.0
        } else if p.links.level == Link::KeepDown {
            say(lead_notes, "lead: the overlap stands down under the keep-down level link - the next level descends from the seam".into());
            0.0
        } else {
            ap.overlap.min(ring.len)
        };
        let keeps = if tabbed {
            // #2480 / #1473: windows grown until the bridge is the tab
            // width at corners and past the leads; from a picked start
            // the windows end on the spacing marks.
            cached(&mut fits, li, || fit_tabs(&ring, p, picked.is_some(), &legal), lead_notes)
        } else {
            vec![(0.0, ring.len)]
        };
        for (k, &ke) in keeps.iter().enumerate() {
            // Both leads keep the cutter center in the legal room
            // (lead_fit.rs), meet the wall on its own tangent, and stay
            // off each other at the seam (contour_entry::seam_leads); the
            // extensions ride the same room (lead_ext).
            let Some((poly, walk, entry, lead, lead_off)) = keep_cut(&ring, ke, p, &legal, lead_notes, tally) else {
                continue;
            };
            if picked.is_some() && k == 0 {
                // The first window begins at the pick (tabbed levels too);
                // a later one begins after a tab, the tab law's start.
                contour_start::lead_held(p, poly[0], &lead)?;
            }
            // The overlap (M11.3 C): the walk retraces its own first
            // moves - the same segments, so the same chords, never a new
            // crossing - and the lead-out leaves from where that ends.
            let mut window = (poly, walk, entry, lead, lead_off);
            if let Some((_, q)) = exit.filter(|&(ei, _)| ei == li) {
                // #1473: on past the seam to the user's exit point.
                window = contour_start::exit_at(window, q, p, &legal, lead_notes)?;
            } else if ov > 0.0 {
                let (rpoly, rwalk) = retrace(&window.0, &window.1, ov);
                let (en, li, lo) = seam_leads(&rwalk, &rpoly, p, &legal, lead_notes);
                let (en, li, lo) = extend_leads(en, li, lo, &rwalk, &rpoly, p, &legal, lead_notes);
                let last = rpoly[rpoly.len() - 1];
                if crosses(en, &li, last, &lo) {
                    say(lead_notes, format!("lead: the {ov:.3} mm overlap's lead-out crosses the lead-in at ({:.3}, {:.3}) - overlap dropped", last.x, last.y));
                } else {
                    window = (rpoly, rwalk, en, li, lo);
                }
            }
            let (poly, walk, entry, lead, lead_off) = window;
            let e2 = Point2::new(entry.x, entry.y);
            let here = vec![flatten(e2, &lead), flatten(poly.last().copied().unwrap_or(e2), &lead_off)];
            let link_leads: Vec<Vec<Point2>> = std::mem::replace(&mut last_leads, here.clone()).into_iter().chain(here).collect();
            let lf = ap.lead_feed(p.feed);
            // The vertical lead-in's top (M11.3 C): a dive at the lead's
            // start stops this high and the lead descends along itself.
            let lead_top = if ap.lead_height > 0.0 && !lead.is_empty() { z + ap.lead_height } else { z };
            if let (Some(s), Some(&zp)) = (stay_in, rzi.checked_sub(1).and_then(|i| rz.get(i))) {
                // Stayed down: the tool stands on the seam at the level
                // above (its lead-out skipped); descend on the ring.
                push_stay(b, s, &poly, &walk, zp, z, p, &legal, lead_notes);
                first_entry = false;
                push_segs(b, &walk, z, p.feed);
                if !stay_out {
                    push_segs(b, &lead_off, z, p.feed);
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
            // A ring after another one is entered by the user's arm too, never
            // by the pass link's plunge into material (#3830, #3917 wall side).
            let ring_in = ring_entry(p, ring_to_ring && !tabbed, inside, rzi == 0, arm, hop_z);
            let enter_ring = ring_in.is_some();
            let wall_arc_on = !inside
                && arm == Entry3D::Arc
                && k == 0
                && !(ring_to_ring && p.links.pass == Link::KeepDown);
            if wall_arc_on && ring_to_ring && p.links.pass == Link::Clearance {
                wall_arc_hop(b, p, hop_z);
            }
            let mut arced = false;
            // #3132: a None lead-in adds no XY travel at the seam - the
            // wall arc runs back along the seam's tangent like a lead.
            let no_lead_in = p.lead == Lead::None;
            if wall_arc_on && !tabbed && !no_lead_in {
                let z_top = wall_arc_top(p, rz, rzi, hop_z);
                let first = lead.first().or(walk.first());
                match wall_arc(b, &eng, e2, z, z_top, hop_z, first) {
                    Ok(()) => arced = true,
                    Err(why) => say(
                        lead_notes,
                        format!(
                            "entry: no arc fits beside the wall at the lead-in ({:.3}, {:.3}) - {why}; \
                             grazing dive there instead",
                            e2.x, e2.y
                        ),
                    ),
                }
            } else if wall_arc_on && !tabbed {
                say(lead_notes, "entry: the arc stands down with no lead-in - its descent would run along the seam like a lead; grazing dive at the ring's start".into());
            } else if wall_arc_on {
                say(
                    lead_notes,
                    "entry: the arc stands down on tabbed levels - a standing tab is material \
                     beside the wall the room law cannot see; grazing dive at the lead-in there"
                        .into(),
                );
            }
            // Ring to ring within one level: the pass link, proved against
            // the rings and this level's dive links.
            let linked = !arced && ring_to_ring && {
                let cut = if dives.0.to_bits() == z.to_bits() { dives.1.as_slice() } else { &[] };
                ring_link(b, p, &legal, &[&band.plan, cut, &link_leads], e2, z, hop_z, tabbed, &mut lifted, enter_ring)
            };
            if arced || linked {
                // Engaged: the arc ends standing at (e2, z), or the pass link posted the move.
            } else if k == 0 {
                // The level's first engagement: a crossing, or the path's
                // first entry (prev_z falls back to clearance there).
                let prev_z = if rzi == 0 { p.clearance_z } else { rz[rzi - 1] };
                // #2004: the column of this descent is open down to prev_z
                // when the descent one level up was the same arm.
                let arm = ring_in.as_ref().map_or(arm, |r| r.arm);
                let opened = rzi >= 2 || (rzi == 1 && arm == p.entry);
                let step = Step { arm, e2, z, prev_z, first: first_entry, opened, inside, tabbed, lead_top, uncut: uncut_floor(p, rz, rzi), traverse: ring_in.as_ref().map(|r| r.plane) };
                if let Some(d) = engage_level(b, &eng, &step, &lead, &walk, said) {
                    if dives.0.to_bits() != z.to_bits() {
                        dives = (z, Vec::new());
                    }
                    dives.1.push(vec![d, e2]);
                }
            } else {
                tab_window_dive(b, p, e2, hop_z, lead_top, uncut_floor(p, rz, rzi));
            }
            first_entry = false;
            let at = b.at();
            let lead_at = b.moves_len();
            if at.x == e2.x && at.y == e2.y && at.z > z {
                push_lead_down(b, &lead, at.z, z, lf);
            } else {
                if lead_top > z {
                    say(lead_notes, "lead: the vertical lead-in rides only a dive at the lead-in's start - this entry lands on the cut plane".into());
                }
                push_segs(b, &lead, z, lf);
            }
            let walk_at = b.moves_len();
            push_segs(b, &walk, z, p.feed);
            let out_at = b.moves_len();
            if !stay_out {
                push_segs(b, &lead_off, z, p.feed);
            }
            // The control's span (M13.11): the lead-in move that lands
            // on the wall engages, the lead-out move that leaves it
            // cancels; a lead the room law refused is a refusal here
            // too, by name - never a centre path under a Control op.
            if let Some(prog) = prog {
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
                contour_comp::push_span(b, &SpanCtx { prog, p, picked }, walk_at - 1, out_at)
                    .map_err(|e| e.with_ghost_moves(b.moves()))?;
            }
        }
    }
    lifted.say(b, "ring-to-ring");
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    Ok(())
}

pub(super) fn finish_contour(
    mut b: PathBuilder,
    lead_notes: Vec<String>,
    tally: &FilletTally,
    p: &ContourParams,
) -> Result<Toolpath, GenError> {
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


/// Which side of travel is air: climb travel (M3, #1555) keeps the
/// material on the right, so air is left; conventional flips both.
pub(super) fn air_side(t: Point2, climb: bool) -> Point2 {
    if climb {
        Point2::new(-t.y, t.x)
    } else {
        Point2::new(t.y, -t.x)
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

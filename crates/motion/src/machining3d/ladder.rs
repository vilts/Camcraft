//! The 3-D roughing level ladder (split from `machining3d.rs` by M5.0.1;
//! M11.2 C/D adds the user's level transitions and links - provenance in
//! `links.rs`, the descents in `descent.rs`).

use super::descent::{self, Ask, descend};
use super::hsm;
use super::level_ctx::{LevelCtx, feed_leg, walk_pass};
use super::params::{Entry3D, Pocket3DParams};
use super::rest::level_target;
use super::shell::Field;

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::entry::chord_hits_plan;
use crate::geo2d::{Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::links::{Link, LinkTally, NOT_PROVED, TOO_LONG};
use crate::machining2d::{chord_clears, GenError};
use crate::plunge_law;
use crate::stock_sim::Columns;
/// One ladder run: the level loop shared by both entry points. `zs` is
/// the plane list, `rest` the post-coarse state whose still-standing
/// columns the pass is aimed at (None: the pure level law), `mask` the
/// state LEFT BY PREVIOUS OPERATIONS that bounds the coarse run
/// itself (M3.1 D: rest machining - a column already machined at or
/// below this plane is cut by nobody twice), and `start` where the
/// builder begins - the home position for the coarse run, the coarse
/// path's end for the rest run, so the two legs chain into one path.
///
/// Why the mask cannot endanger an entry motion: a ramp's legality is
/// the islands superset - a chord legal in the deepest (most crowded)
/// center is legal at every plane above it. The mask term shrinks
/// upward too, but a chord point the mask removes at an intermediate
/// plane is a column whose standing top sits below that plane, and a
/// cutter whose bottom travels at or above a column's top touches
/// nothing there. Air, never a wall: the island term is the only one
/// with walls to prove against, and the island superset is untouched.
///
/// `shell` (M2.5 F) leaves the region law exactly as the plain ladder
/// has it - the level's islands, XY-eroded at `r + leave`, the only
/// XY set the entry arms and the dive superset law ever read - and
/// lifts every CUT VERTEX off the level plane onto the model's offset
/// shell: `min(plane, roof(q, r + ℓ) + leave)`, floored at the plane's
/// own stepdown budget and the pocket's drawn floor, so no level
/// bites deeper axially than a plain ladder's dive already does and
/// none descends past `bottom_z`. ℓ, the chord cap `feed_leg` subdivides
/// every XY leg to, is the tool radius: a vertex decides within one
/// diameter, and because both of a capped leg's endpoints decide
/// within `r + ℓ` of themselves, no interpolation between the two
/// bounds can slip under a part point the leg's tube covers - the
/// rising-leg gouge an uncapped leg is open to (see
/// `machining3d::shell`). A ring XY inside the eroded center has no
/// part above the plane within its disc at all, so a lifted vertex
/// bites part material nowhere, exactly as the flat one never did.
/// Where the surface is steep the roof outgrows the plane and the
/// vertex stays AT it - the waterline the plain ladder always cut
/// there; where the surface is shallow it follows the surface at the
/// leave standoff, and the staircase's pyramid between plane and
/// surface is gone from the single pass. Rapids and hops never lift:
/// they cut nothing. See `machining3d::entry::pocket3d_offset`.
// One slot per law the run threads: the model, the plan, the planes, the
// rest mask, the program mask, the tip source (M2.5 F's `shell`), the
// start - flat reads for one walk, like the march kernel's ten.
#[allow(clippy::too_many_arguments)]
pub(super) fn ladder(
    facets: &[Triangle],
    footprint: &Region,
    p: &Pocket3DParams,
    zs: &[f64],
    rest: Option<&Columns<'_>>,
    mask: Option<&Columns<'_>>,
    start: Point3,
    shell: bool,
    seam: Option<&hsm::Seam>,
) -> Result<Toolpath, GenError> {
    let r = p.tool_diameter * 0.5;
    let hop_z = p.retract_z.unwrap_or(p.clearance_z).min(p.clearance_z);
    // The shell's roof query boxes the model once per call; the plain
    // ladder never pays for it and never reads it (every leg then
    // lands at its plane, exactly as always).
    let field = match shell {
        true => Some(Field::new(facets).map_err(GenError::Invalid)?),
        false => None,
    };

    let mut b = PathBuilder::new(start);
    // The last level that actually cut: its plane and center area, the
    // most crowded the entry motions of the next level can meet (a
    // deeper level's target only grows). None until one has cut.
    let mut prev_state: Option<(f64, Region)> = None;
    // That level's planned paths: a keep-down link at its plane must
    // cross none of them (M11.2 D). Keep-down lifts, tallied and named.
    let mut prev_passes: Vec<Vec<Point2>> = Vec::new();
    // (The previous level's dive link rides in it too: a planar cut at
    // that plane, which the Crossing law reads like any pass.)
    let mut lifted = LinkTally::default();
    for &z in zs {
        // The level law, in two region ops: the footprint minus the part
        // material standing above the plane - its cross-section and the
        // shadow of anything hanging over it (a blind floor's underside,
        // an overhang), so a level under a closed floor has no area
        // there - then the cutter-center area by one erosion. The shadow
        // is what keeps the superset law below true: target(z) only
        // grows as z falls, so a center point at z is open from above.
        let target = level_target(facets, footprint, z)?;
        let target = match rest {
            // The level law on rest terms: only columns the simulated
            // state still has standing above this plane. The islands
            // subtraction already ran, and the nesting law keeps the
            // intersection honest: a part column above z is an island,
            // so the sim's coarser top cannot smuggle part material
            // into a rest target.
            None => target,
            Some(dex) => target.intersect(&dex.above(z)?)?,
        };
        let target = match mask {
            // The same intersection for what EARLIER operations left
            // standing (M3.1 D): a column they machined to at or below
            // this plane must not be tiled again. Pure subtraction -
            // the masked target is a subset of the unmasked one, and
            // skipping a column with nothing standing above the plane
            // cannot cut anything.
            None => target,
            Some(dex) => target.intersect(&dex.above(z)?)?,
        };
        if target.is_empty() {
            continue; // nothing above this plane: model material across
                      // the whole footprint, or nothing standing at all
        }
        // The center's erosion: the XY law, both rungs of the ladder -
        // the shell lifts the legs' tips, never this region (the dive
        // superset law reads exactly this set at exactly this plane).
        let center = target.offset(-(r + p.stock_to_leave))?;
        if center.is_empty() {
            continue; // narrower than the tool at this level: the
                      // staircase (or a wall this tool cannot enter) -
                      // rest material's later problem, never a lie now
        }
        // The level's plan: the strategy's tiling, or the HSM plan
        // (M11.4, `hsm`) - whose entry bore the level's params carry.
        // #871: a part-way refusal hands back the levels so far.
        let lp = hsm::plan_level(&mut b, &center, p, z, hop_z, rest.is_some(), shell, seam)
            .map_err(|e| e.with_ghost_moves(b.moves()))?;
        let passes = &lp.passes;
        let level_p = lp.params(p);
        let Some(first_pass) = passes.first() else {
            continue;
        };
        let Some(&entry) = first_pass.first() else {
            continue;
        };
        // The plane an entry descends from: the previous cutting level's
        // own, or - nothing has cut below the surface yet - the safe
        // plane. The superset law makes both honest: room at the
        // destination is room across every plane above it (M8.1: the
        // arms read this at the FIRST level too).
        let prev_z = prev_state.as_ref().map_or(p.clearance_z, |(q, _)| *q);
        let at = b.at();
        let leg = Point2::new(at.x, at.y);
        // The motion: the entry on the ladder's first level, the user's
        // transition after it (M11.2 C). A rest pass starts inside
        // planes already cut, so every arm stands down to the wall dive.
        let crossing = prev_state.is_some();
        let ask = Ask {
            arm: if crossing { p.links.arm(p.entry) } else { p.entry },
            transition: crossing && p.links.transition.is_some(),
            rest: rest.is_some(),
            entry,
            first_pass,
            prev_z,
            prev_cut: prev_state.as_ref().map(|(q, _)| *q),
        };
        let mut lvl = LevelCtx {
            cen: &center,
            passes,
            z,
            hop_z,
            p: &level_p,
            field: field.as_ref(),
            tz: p.clearance_z,
            kept: false,
            settle: std::cell::Cell::new(0.0),
            arcs: lp.arcs.as_deref(),
        };
        // The level link (M11.2 D). Before the first cut, and always on
        // a rest pass (its planes sit inside floors the coarse run owns,
        // where no keep-down proof is written), the climb to clearance
        // every 3-D ladder has posted.
        let link = if crossing { p.links.level } else { Link::Clearance };
        let link = match (link, rest.is_some()) {
            (Link::KeepDown, true) => {
                lifted.lift("a rest pass starts inside floors already cut", leg, prev_z);
                Link::Retract
            }
            (l, _) => l,
        };
        match link {
            Link::Clearance => {
                if at.z != p.clearance_z {
                    b.push(MoveKind::Rapid, Point3::new(at.x, at.y, p.clearance_z), None);
                }
            }
            Link::Retract => {
                b.push(MoveKind::Rapid, Point3::new(at.x, at.y, hop_z), None);
                lvl.tz = hop_z;
            }
            Link::KeepDown => {
                // Stay on the level just cut: reach the descent's start
                // at that plane by a feed proved inside THIS level's
                // center (a subset of every plane above it, so no part
                // stands within the disc there), crossing none of the
                // previous level's paths (cut at that plane), within the
                // stay-down limit; else lift to the retract plane, named.
                let s = descent::start(&lvl, &ask, leg);
                let proof = if !p.links.within_stay_down((s.x - leg.x).hypot(s.y - leg.y)) {
                    Err(TOO_LONG)
                } else if (s.x == leg.x && s.y == leg.y)
                    || (chord_clears(&center, leg, s) && !chord_hits_plan(leg, s, &prev_passes))
                {
                    Ok(())
                } else {
                    Err(NOT_PROVED)
                };
                match proof {
                    Ok(()) => {
                        b.push(MoveKind::Feed, Point3::new(s.x, s.y, prev_z), Some(p.feed));
                        lvl.tz = prev_z;
                        lvl.kept = true;
                    }
                    Err(why) => {
                        lifted.lift(why, leg, prev_z);
                        b.push(MoveKind::Rapid, Point3::new(at.x, at.y, hop_z), None);
                        lvl.tz = hop_z;
                    }
                }
            }
        }
        // The engagement limit (M11.3 D, the planar ladder's law): the
        // level's first tool diameter of plane cut out of its descent -
        // the dive link and the first pass - runs at the scaled feed,
        // the rest of the level at the cutting feed. Off, nothing owed.
        if p.links.approach.engagement_scale() < 1.0 {
            lvl.settle.set(p.tool_diameter);
        }
        let dove = descend(&mut b, &lvl, &ask);
        lvl.tz = p.clearance_z;
        lvl.kept = false;
        walk_pass(&mut b, first_pass, &lvl, 0, /*skip_first=*/ true);
        lvl.settle.set(0.0);
        // Remaining passes: connect as far as the floor allows - and no
        // further than the level's own planned paths allow (the boss
        // bug: a chord can clear the cutter-center area yet cross every
        // ring scheduled under it, which `verify` then reports as a
        // Crossing at the cut depth; fly over it instead). The pass link
        // (M11.2 D): keep down where proved (every earlier 3-D rough),
        // or always lift - to the retract plane or to clearance.
        let mut restarts: Vec<Vec<Point2>> = Vec::new();
        for (pi, pass) in passes.iter().enumerate().skip(1) {
            let Some(&next_entry) = pass.first() else {
                continue;
            };
            // The HSM plan's microlift return (M11.4): proved by the plan
            // over the stock it cleared, posted as feeds over the floor.
            if let Some(Some(ret)) = lp.joins.get(pi) {
                hsm::microlift(&mut b, ret, z, hop_z, p.feed);
                walk_pass(&mut b, pass, &lvl, pi, /*skip_first=*/ true);
                continue;
            }
            let prev = b.at();
            let p2 = Point2::new(prev.x, prev.y);
            // An HSM level's unjoined pass takes the hop (M11.4): the
            // plan's budget counted it, and no chord at depth is proved
            // against the stock it would cross.
            let link = if lp.hsm { Link::Retract } else { p.links.pass };
            let plane = match link {
                Link::KeepDown => {
                    let proved = chord_clears(&center, p2, next_entry) && !chord_hits_plan(p2, next_entry, passes);
                    if proved && p.links.within_stay_down((next_entry.x - p2.x).hypot(next_entry.y - p2.y)) {
                        None
                    } else {
                        if proved {
                            // Proved but past the limit - the user's
                            // number, named. (An unproved chord hops by
                            // the plan law, as this ladder always has.)
                            lifted.lift(TOO_LONG, p2, z);
                        }
                        Some(hop_z)
                    }
                }
                Link::Retract => Some(hop_z),
                Link::Clearance => Some(p.clearance_z),
            };
            match plane {
                // Feed straight on at this level (the join chord the
                // test just proved) - on the shell, vertex by vertex.
                None => feed_leg(&mut b, &lvl, next_entry, 0.0),
                Some(hz) => {
                    b.push(MoveKind::Rapid, Point3::new(prev.x, prev.y, hz), None);
                    // M11.3: a tool that is not centre-cutting enters a
                    // pass start its neighbours have not cleared like a
                    // level (helix, ramp along the pass) or the op is
                    // refused by name (`plunge_law`).
                    let again = Ask {
                        arm: Entry3D::Plunge,
                        transition: false,
                        rest: false,
                        entry: next_entry,
                        first_pass: pass,
                        prev_z: prev_state.as_ref().map_or(hz, |(q, _)| *q),
                        prev_cut: prev_state.as_ref().map(|(q, _)| *q),
                    };
                    let (tip, top) = (lvl.tip(next_entry, 0.0), descent::mat_top(&lvl, &again));
                    if !p.links.approach.center_cutting && plunge_law::into_solid(&b, next_entry, tip, top, r) {
                        lvl.tz = hz;
                        let why = "a pass start inside the level";
                        if let Some(d) = descent::fallback(&mut b, &lvl, &again, why, false) {
                            restarts.push(vec![d, next_entry]);
                        }
                        lvl.tz = p.clearance_z;
                        walk_pass(&mut b, pass, &lvl, pi, /*skip_first=*/ true);
                        continue;
                    }
                    b.push(MoveKind::Rapid, Point3::new(next_entry.x, next_entry.y, hz), None);
                    b.push(
                        MoveKind::Plunge,
                        Point3::new(next_entry.x, next_entry.y, lvl.tip(next_entry, 0.0)),
                        Some(p.plunge_feed),
                    );
                }
            }
            walk_pass(&mut b, pass, &lvl, pi, /*skip_first=*/ true);
        }
        // Hand this level's plan to the next one's entry legality: it
        // is the deepest cut so far, so its center bounds every plane
        // a descent from here will cross.
        prev_passes = passes.clone();
        prev_passes.extend(dove.map(|d| vec![d, entry]));
        prev_passes.extend(restarts);
        prev_state = Some((z, center));
    }
    lifted.say(&mut b, "level/pass");
    if let Some(n) = p.links.approach.engagement_note()
        && prev_state.is_some()
    {
        b.note_once(&n);
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    let path = b.finish();
    if let Some(why) = crate::approach::refusal(&path) {
        // #261 part 3: the moves built so far ride the refusal, display-only.
        return Err(GenError::Invalid(why).with_ghost(&path));
    }
    if path.moves.is_empty() {
        return Err(GenError::Empty(
            "nothing to cut: the model fills the pocket at every level".into(),
        ));
    }
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}


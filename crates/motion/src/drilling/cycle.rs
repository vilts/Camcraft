//! Split from `drilling.rs` by M5.0.1; structure only.

use super::hole::{CHIP_BREAK, CycleChoice, CycleParams, FIT_TOL, Hole, SPOT_DEPTH, THROUGH_CLEAR, auto_kind};

use crate::cl::{lattice, Cycle, CycleKind, MoveKind, PathBuilder, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::machining2d::GenError;
use crate::tools::ToolKind;
/// Machine a hole list: a contiguous cut group per hole (positioning
/// rapids sit between the groups) plus one [`Cycle`] record each. The
/// chain is chained and finite by construction; nonsense in,
/// `GenError` out.
pub fn drill(p: &CycleParams) -> Result<Toolpath, GenError> {
    let d = p.tool.diameter;
    if !(d.is_finite() && d > 0.0) || !p.r_z.is_finite() || !p.clearance_z.is_finite() {
        return Err(GenError::Invalid("drill tool or heights not usable".into()));
    }
    if !p.holes.iter().all(|h| h.all_finite() && h.radius > 0.0 && h.floor_z < h.entry_z) {
        return Err(GenError::Invalid("malformed hole list".into()));
    }
    if !(p.feed.is_finite() && p.plunge_feed.is_finite() && p.dwell.is_finite() && p.dwell >= 0.0) {
        return Err(GenError::Invalid("cycle parameters not usable".into()));
    }
    if p.holes.is_empty() {
        return Err(GenError::Empty("no holes to drill".into()));
    }
    let tap_feed = p.tool.rpm * p.tool.pitch;
    let taps = matches!(p.cycle, CycleChoice::Tap | CycleChoice::FloatingTap)
        || (matches!(p.cycle, CycleChoice::Auto) && p.tool.kind == ToolKind::Tap);
    if taps && !(tap_feed.is_finite() && tap_feed > 0.0) {
        return Err(GenError::Invalid("a tap cycle needs a positive pitch x rpm feed".into()));
    }

    // Snap once; every move and every cycle reads these, so the two
    // truths agree bit for bit, not by luck.
    let hs: Vec<Hole> = p
        .holes
        .iter()
        .map(|h| Hole {
            xy: Point2::new(lattice(h.xy.x), lattice(h.xy.y)),
            radius: lattice(h.radius),
            entry_z: lattice(h.entry_z),
            floor_z: lattice(h.floor_z),
            through: h.through,
            stepped: h.stepped,
        })
        .collect();

    let start_z = p.clearance_z.max(p.r_z);
    let hop = p.hop_z.unwrap_or(p.clearance_z).max(p.r_z);
    let mut b = PathBuilder::new(Point3::new(hs[0].xy.x, hs[0].xy.y, start_z));
    for (i, h) in hs.iter().enumerate() {
        if i > 0 {
            // The previous hole's exit left the tool at (prev_xy, hop):
            // move over, then down onto this hole's R plane.
            b.push(MoveKind::Rapid, Point3::new(h.xy.x, h.xy.y, hop), None);
        }
        b.push(MoveKind::Rapid, Point3::new(h.xy.x, h.xy.y, p.r_z), None);
        let floor_bottom = h.floor_z - if h.through { THROUGH_CLEAR } else { 0.0 };
        let bottom = match p.bottom_z {
            Some(z) => z,
            // SPOT_DEPTH's own words: the dimple hangs off the ENTRY
            // plane - the hole's mouth - not the R plane, so a hole on
            // a pocket floor gets its dimple and not an air cut
            // (M8.3; the radial frame already read the mouth, cross.rs).
            None if matches!(p.cycle, CycleChoice::Spot) => (h.entry_z - SPOT_DEPTH).max(h.floor_z),
            None => floor_bottom,
        };
        // #261 part 2: every refusal about this hole points at its mouth.
        let mouth = Point3::new(h.xy.x, h.xy.y, h.entry_z);
        if bottom >= p.r_z {
            return Err(GenError::invalid_at(
                format!("hole at ({}, {}) bottoms at {bottom}, not below the R plane {}", h.xy.x, h.xy.y, p.r_z),
                mouth,
            ));
        }
        let kind = match p.cycle {
            CycleChoice::Auto => auto_kind(p.tool.kind, p.r_z - bottom, d),
            _ => resolve_choice(p),
        };
        refuse_solid_cycle(p.tool, kind, mouth)?;
        // The bore door (M8.3): the slide's whole cutting diameter
        // travels inside the hole's wall, so a tool wider than the
        // wall's radius is a wall-cutting plunge. And a stepped hole
        // (a folded counterbore) has a ring the cycle's single radius
        // cannot reach. Refused here, by name, not left to the
        // verifier's generic gouge after the fact.
        if matches!(kind, CycleKind::Bore | CycleKind::BackBore | CycleKind::Ream) {
            if h.stepped {
                return Err(GenError::invalid_at(
                    format!(
                        "{} cycle at ({}, {}): that hole is a counterbore - the cycle would bore \
                         the narrow dia. {} through and miss the ring",
                        if kind == CycleKind::Ream { "ream" } else { "bore" },
                        h.xy.x,
                        h.xy.y,
                        2.0 * h.radius
                    ),
                    mouth,
                ));
            }
            if 0.5 * d > h.radius + FIT_TOL {
                return Err(GenError::invalid_at(
                    format!(
                        "{} tool dia. {d} will not enter the dia. {} hole at ({}, {})",
                        if kind == CycleKind::Ream { "ream" } else { "bore" },
                        2.0 * h.radius,
                        h.xy.x,
                        h.xy.y
                    ),
                    mouth,
                ));
            }
        }
        let fm = b.moves_len();
        let mut peck = 0.0;
        match kind {
            CycleKind::Spot | CycleKind::Drill => {
                b.push(MoveKind::Plunge, Point3::new(h.xy.x, h.xy.y, bottom), Some(p.plunge_feed));
                b.push(MoveKind::Rapid, Point3::new(h.xy.x, h.xy.y, p.r_z), None);
            }
            CycleKind::Peck | CycleKind::Deep => {
                let auto = if kind == CycleKind::Peck { 3.0 * d } else { d };
                let step = if p.peck > 0.0 { p.peck } else { auto }.max(0.25 * d);
                if !(step.is_finite() && step > 0.0) || (p.r_z - bottom) / step > 4096.0 {
                    return Err(GenError::Invalid("peck plan unusable".into()));
                }
                peck = step;
                // Pecks march DOWN by step: z decreases. The retract
                // (a G73 hop up, or a G83 full return to R) follows
                // every peck but the last, which reaches the bottom.
                let mut t = (p.r_z - step).max(bottom);
                loop {
                    b.push(MoveKind::Plunge, Point3::new(h.xy.x, h.xy.y, t), Some(p.plunge_feed));
                    if t <= bottom {
                        break;
                    }
                    let up = if kind == CycleKind::Peck {
                        (t + CHIP_BREAK).min(p.r_z)
                    } else {
                        p.r_z
                    };
                    b.push(MoveKind::Rapid, Point3::new(h.xy.x, h.xy.y, up), None);
                    t = (t - step).max(bottom);
                }
                if kind == CycleKind::Peck {
                    b.push(MoveKind::Rapid, Point3::new(h.xy.x, h.xy.y, p.r_z), None);
                }
            }
            CycleKind::Tap | CycleKind::FloatingTap => {
                b.push(MoveKind::Feed, Point3::new(h.xy.x, h.xy.y, bottom), Some(tap_feed));
                b.push(MoveKind::Feed, Point3::new(h.xy.x, h.xy.y, p.r_z), Some(tap_feed));
            }
            CycleKind::Bore => {
                b.push(MoveKind::Plunge, Point3::new(h.xy.x, h.xy.y, bottom), Some(p.plunge_feed));
                b.push(MoveKind::Rapid, Point3::new(h.xy.x, h.xy.y, p.r_z), None);
            }
            CycleKind::BackBore => {
                b.push(MoveKind::Plunge, Point3::new(h.xy.x, h.xy.y, bottom), Some(p.plunge_feed));
                b.push(MoveKind::Feed, Point3::new(h.xy.x, h.xy.y, p.r_z), Some(p.feed));
            }
            CycleKind::Ream => {
                // G85: one F word drives both directions, so the chain
                // feeds out at the feed it fed in at - what the canned
                // cycle does is what the verifier and simulation read.
                b.push(MoveKind::Plunge, Point3::new(h.xy.x, h.xy.y, bottom), Some(p.plunge_feed));
                b.push(MoveKind::Feed, Point3::new(h.xy.x, h.xy.y, p.r_z), Some(p.plunge_feed));
            }
        }
        let dwell = match kind {
            CycleKind::Spot | CycleKind::Drill | CycleKind::Deep | CycleKind::Bore | CycleKind::FloatingTap => p.dwell,
            _ => 0.0,
        };
        b.push_cycle(Cycle {
            kind,
            xy: h.xy,
            r_z: p.r_z,
            bottom_z: bottom,
            entry_z: h.entry_z,
            peck,
            dwell,
            first_move: fm,
        });
        // Lift to the hop plane for the traverse to the next hole.
        b.push(MoveKind::Rapid, Point3::new(h.xy.x, h.xy.y, hop), None);
    }
    let last = hs[hs.len() - 1];
    b.push(MoveKind::Rapid, Point3::new(last.xy.x, last.xy.y, start_z), None);
    let t = b.finish();
    if !t.is_chained() || !t.all_finite() {
        // The builder cannot produce this; if it ever did, the path is
        // not fit to leave this function.
        return Err(GenError::Invalid("drill chain self-check failed".into()));
    }
    Ok(t)
}

/// The centre-cutting law on a hole (M11.3 owed (5)): spot, drill and
/// peck cycles feed the tool's whole end axially into solid, so a tool
/// that is not centre-cutting is refused there by definition. Tap, bore
/// and back-bore cycles open a hole that is already drilled - the cycle
/// itself says so - and pass.
/// `at` is the hole's mouth: the sentence names its X and Y, the
/// refusal points at it (#261 part 2).
pub(super) fn refuse_solid_cycle(tool: &crate::tools::Tool, kind: CycleKind, at: Point3) -> Result<(), GenError> {
    if tool.center_cutting
        || matches!(kind, CycleKind::Tap | CycleKind::FloatingTap | CycleKind::Bore | CycleKind::BackBore | CycleKind::Ream)
    {
        return Ok(());
    }
    let (x, y) = (at.x, at.y);
    Err(GenError::invalid_at(
        format!(
            "{} - a {kind:?} cycle at ({x}, {y}) plunges into solid; drill the hole with a centre-cutting tool \
             (a tap or bore cycle reads it as pre-drilled)",
            crate::approach::CENTER_CUT_REFUSAL
        ),
        at,
    ))
}

pub(super) fn resolve_choice(p: &CycleParams) -> CycleKind {
    match p.cycle {
        CycleChoice::Auto => CycleKind::Drill, // never taken: drill() routes Auto through auto_kind
        CycleChoice::Spot => CycleKind::Spot,
        CycleChoice::Drill => CycleKind::Drill,
        CycleChoice::Peck => CycleKind::Peck,
        CycleChoice::Tap => CycleKind::Tap,
        CycleChoice::FloatingTap => CycleKind::FloatingTap,
        CycleChoice::Bore => CycleKind::Bore,
        CycleChoice::BackBore => CycleKind::BackBore,
        CycleChoice::Ream => CycleKind::Ream,
    }
}

// ------------------------------------------------------------------- tests

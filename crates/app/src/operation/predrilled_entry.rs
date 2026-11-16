//! The holes a pre-drilled pocket entry may plunge down (#1449, first
//! slice): the hole centres of the Drill operations ahead of the pocket
//! in its setup, each PROVED open in the stock the program ahead leaves
//! standing (the prior grid the drilling and thread laws read,
//! [`super::drill_prior`]) before the generator may see it. A hole is
//! handed over only when
//! - the drill's own diameter is at least the pocket tool's plus the
//!   clearance on each side ([`CLEARANCE`]) - checked on the drill's
//!   stated size, so a hole the grid's cells cannot resolve is never
//!   counted as wide enough;
//! - the grid holds no material over the tool's disc plus clearance
//!   above the depth the hole is counted to; that depth is the deeper of
//!   the grid's reading and the drill's point cone at the disc's radius
//!   (a drill-point cone counts only where it is wider than the disc).
//!
//! Anything else is named on the path and left out, so the pocket keeps
//! its asked helix or ramp there: an unproved hole is never air.
//!
//! Provenance: machine-tool handbook practice (a start hole drilled
//! oversize for the end mill that plunges down it; the 118-degree twist
//! drill point); the prior-grid proof is our own design. No vendor
//! material consulted.

use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::Point2;
use crate::machining2d::PreHole;
use crate::rest::DrillAhead;
use crate::stock_sim::{Columns, Dexel};

/// Radial clearance (mm) a pre-drilled hole must give the tool on each
/// side: the hole's diameter must be at least the tool's plus twice this.
pub const CLEARANCE: f64 = 0.1;

/// Two hole centres closer than this (mm) are one hole.
const SAME_HOLE: f64 = 1e-3;

/// One drilled hole read off a drill's moves: its centre, the drill's
/// radius, the lowest point the tip reached, the point's cotangent.
struct Drilled {
    at: Point2,
    radius: f64,
    tip: f64,
    cot: f64,
}

/// Every hole the drills ahead made: the XY of each vertical cutting
/// descent, its deepest tip kept.
fn drilled(drills: &[DrillAhead]) -> Vec<Drilled> {
    let mut out: Vec<Drilled> = Vec::new();
    for d in drills {
        for m in d.moves.iter().filter(|m| m.kind != MoveKind::Rapid && m.arc.is_none()) {
            let vertical = (m.to.x - m.from.x).hypot(m.to.y - m.from.y) <= 1e-9;
            if !(vertical && m.to.z < m.from.z && m.to.x.is_finite() && m.to.y.is_finite() && m.to.z.is_finite()) {
                continue;
            }
            let at = Point2::new(m.to.x, m.to.y);
            match out.iter_mut().find(|h| (h.at.x - at.x).hypot(h.at.y - at.y) <= SAME_HOLE && h.radius == d.radius) {
                Some(h) => h.tip = h.tip.min(m.to.z),
                None => out.push(Drilled { at, radius: d.radius, tip: m.to.z, cot: d.cone.unwrap_or(0.0).max(0.0) }),
            }
        }
    }
    out
}

/// The proved holes for a pocket tool of radius `tool_r`, and the notes
/// naming every hole left out. `prior` is the grid the program ahead
/// leaves (`None`: nothing ahead could be read - nothing is proved);
/// `posed` is a 3+2 frame, whose path the column grid does not read.
pub(super) fn proved_holes(drills: &[DrillAhead], tool_r: f64, prior: Option<&Dexel>, posed: bool) -> (Vec<PreHole>, Vec<String>) {
    let holes = drilled(drills);
    let mut notes = Vec::new();
    if holes.is_empty() {
        notes.push(
            "entry: pre-drilled entry is on, but no drilling operation ahead of this pocket in its setup has made a hole - the asked entry stands"
                .to_string(),
        );
        return (Vec::new(), notes);
    }
    if posed {
        notes.push("entry: pre-drilled holes are not read on a 3+2 posed pocket yet - the asked entry stands".to_string());
        return (Vec::new(), notes);
    }
    let need = tool_r + CLEARANCE;
    let mut out = Vec::new();
    for h in holes {
        let (x, y) = (h.at.x, h.at.y);
        if !(need.is_finite() && h.radius >= need - 1e-9) {
            notes.push(format!(
                "entry: the drilled hole at ({x:.3}, {y:.3}) is {:.3} mm across - under the {:.3} mm the {:.3} mm tool needs with {CLEARANCE:.3} mm clearance a side; not used",
                h.radius * 2.0,
                need * 2.0,
                tool_r * 2.0
            ));
            continue;
        }
        let Some(dex) = prior else {
            notes.push(format!(
                "entry: the drilled hole at ({x:.3}, {y:.3}) cannot be proved open - the stock the program ahead leaves cannot be read; not used"
            ));
            continue;
        };
        match crate::verify::prior_surface(&Columns::of_grid(dex), h.at, need) {
            Some(top) if top.is_finite() => {
                // The point cone is full width only above tip + r cot.
                let cone_floor = h.tip + need * h.cot;
                out.push(PreHole { at: h.at, floor: top.max(cone_floor) });
            }
            _ => notes.push(format!(
                "entry: the drilled hole at ({x:.3}, {y:.3}) cannot be proved open - the stock grid is unreadable there; not used"
            )),
        }
    }
    (out, notes)
}

/// The notes laid on the generated path, each once.
pub(super) fn say(path: &mut Toolpath, notes: Vec<String>) {
    for n in notes {
        if !path.notes.contains(&n) {
            path.notes.push(n);
        }
    }
}

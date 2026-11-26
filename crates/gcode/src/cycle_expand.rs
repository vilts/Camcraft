//! Expansion of one canned-cycle hole into explicit `cl::Move` chains and its `cl::Cycle` record.
//! Provenance: cycle motion from RS-274/NGC (NISTIR 6556 §3.5.16) and the LinuxCNC G-code reference (G73, G81-G89); CamCraft drilling chains (`crates/motion/src/drilling/cycle.rs`).

use camcraft_cl::cl::{lattice, Cycle, CycleKind, MoveKind, PathBuilder};
use camcraft_geometry::geo2d::Point2;
use camcraft_geometry::geometry::Point3;

use crate::types::{CycleType, GcodeError};

/// Most pecks in one hole. The peck loops stop at this count in all cases.
pub const MAX_PECKS: usize = 4096;

/// One resolved hole, all levels in mm in the work frame.
#[derive(Clone, Copy, Debug)]
pub struct Hole {
    pub line: usize,
    pub kind: CycleType,
    pub hx: f64,
    pub hy: f64,
    pub r_z: f64,
    pub bottom_z: f64,
    /// The clear level at the end of the hole: R (G99) or the initial level if higher (G98).
    pub clear_z: f64,
    pub peck_mm: f64,
    pub dwell_s: f64,
    pub feed: f64,
    pub chip_break_retract_mm: f64,
}

/// Number of pecks that a peck hole needs, or `None` when Q cannot make progress.
#[must_use]
pub fn peck_count(r_z: f64, bottom_z: f64, peck_mm: f64) -> Option<usize> {
    let n = ((r_z - bottom_z) / peck_mm).ceil();
    if n.is_finite() && n >= 1.0 && n <= MAX_PECKS as f64 {
        Some(n as usize)
    } else {
        None
    }
}

/// Upper bound on the moves that one hole emits (positioning, pecks and the retract).
#[must_use]
pub fn moves_per_hole(kind: CycleType, pecks: usize) -> usize {
    match kind {
        CycleType::ChipBreakPeck | CycleType::DeepPeck => 4 + 2 * pecks,
        _ => 6,
    }
}

struct Emit<'b> {
    line: usize,
    builder: &'b mut PathBuilder,
    move_lines: &'b mut Vec<usize>,
}

impl Emit<'_> {
    fn push(&mut self, kind: MoveKind, to: Point3, feed: Option<f64>) {
        let before = self.builder.moves_len();
        self.builder.push(kind, to, feed);
        for _ in before..self.builder.moves_len() {
            self.move_lines.push(self.line);
        }
    }
}

/// Emit one hole onto `builder` and move the cursor `cur` to its clear level.
pub fn expand_hole(
    h: &Hole,
    cur: &mut Point3,
    builder: &mut PathBuilder,
    move_lines: &mut Vec<usize>,
) -> Result<(), GcodeError> {
    let (hx, hy) = (h.hx, h.hy);
    let at = |z: f64| Point3::new(hx, hy, z);
    let mut e = Emit { line: h.line, builder, move_lines };

    // Preliminary positioning: lift to R first if below R, then XY, then down to R.
    if cur.z < h.r_z - 1e-9 {
        e.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, h.r_z), None);
        cur.z = h.r_z;
    }
    e.push(MoveKind::Rapid, at(cur.z), None);
    e.push(MoveKind::Rapid, at(h.r_z), None);
    let first_move = e.builder.moves_len();
    let feed = Some(h.feed);

    match h.kind {
        CycleType::Drill | CycleType::DwellDrill | CycleType::Bore => {
            // G81/G82 rapid out; G86 stops the spindle and rapids out.
            e.push(MoveKind::Plunge, at(h.bottom_z), feed);
            e.push(MoveKind::Rapid, at(h.clear_z), None);
        }
        CycleType::ChipBreakPeck | CycleType::DeepPeck => {
            peck_down(h, &mut e)?;
            e.push(MoveKind::Rapid, at(h.clear_z), None);
        }
        CycleType::Tap | CycleType::Ream => {
            // G84/G85: feed out to R, then rapid to the clear level when it is higher.
            let down = if h.kind == CycleType::Tap { MoveKind::Feed } else { MoveKind::Plunge };
            e.push(down, at(h.bottom_z), feed);
            e.push(MoveKind::Feed, at(h.r_z), feed);
            if h.clear_z > h.r_z + 1e-9 {
                e.push(MoveKind::Rapid, at(h.clear_z), None);
            }
        }
        CycleType::BackBore => {
            // G89: dwell at the bottom, then feed out all the way to the clear level
            // (#3657 review 7; LinuxCNC G89).
            e.push(MoveKind::Plunge, at(h.bottom_z), feed);
            e.push(MoveKind::Feed, at(h.clear_z), feed);
        }
    }
    *cur = at(h.clear_z);

    let cl_kind = match h.kind {
        CycleType::ChipBreakPeck => CycleKind::Peck,
        CycleType::Drill | CycleType::DwellDrill => CycleKind::Drill,
        CycleType::DeepPeck => CycleKind::Deep,
        CycleType::Tap => CycleKind::Tap,
        CycleType::Ream => CycleKind::Ream,
        CycleType::Bore => CycleKind::Bore,
        CycleType::BackBore => CycleKind::BackBore,
    };
    e.builder.push_cycle(Cycle {
        kind: cl_kind,
        xy: Point2::new(lattice(hx), lattice(hy)),
        r_z: lattice(h.r_z),
        bottom_z: lattice(h.bottom_z),
        entry_z: lattice(h.r_z),
        peck: lattice(h.peck_mm),
        dwell: h.dwell_s,
        first_move,
    });
    Ok(())
}

/// The G73/G83 peck chain. Each peck must move strictly deeper, and the loop stops
/// at [`MAX_PECKS`] (#3657 review 1): near 1e20 a subtraction of Q can leave the
/// same f64, so a loop on the depth alone could run forever.
fn peck_down(h: &Hole, e: &mut Emit<'_>) -> Result<(), GcodeError> {
    let at = |z: f64| Point3::new(h.hx, h.hy, z);
    let no_progress = || GcodeError::InvalidCycle {
        line: h.line,
        message: format!(
            "peck increment Q ({}) does not make progress from R ({}) to Z ({}) in {MAX_PECKS} pecks",
            h.peck_mm, h.r_z, h.bottom_z
        ),
    };
    let mut prev = h.r_z;
    let mut t = (h.r_z - h.peck_mm).max(h.bottom_z);
    for _ in 0..MAX_PECKS {
        if t >= prev {
            return Err(no_progress());
        }
        e.push(MoveKind::Plunge, at(t), Some(h.feed));
        if t <= h.bottom_z {
            return Ok(());
        }
        let up = if h.kind == CycleType::ChipBreakPeck {
            (t + h.chip_break_retract_mm).min(h.r_z)
        } else {
            h.r_z
        };
        e.push(MoveKind::Rapid, at(up), None);
        prev = t;
        t = (t - h.peck_mm).max(h.bottom_z);
    }
    Err(no_progress())
}

//! The guided sheet job (#3070): one checklist that sets up a flat-sheet
//! router job - the sheet's size and thickness, where Z zero is (the
//! sheet top or the spoilboard), how far a through cut may go into the
//! spoilboard (the allowance, always stated), where XY zero is (front
//! left by default) and how high the clamps or screw heads stand above
//! the sheet. It is written by [`crate::commands::SetSheetJob`] over the
//! existing stock and WCS commands, and kept on the setup
//! (`SetupDef::sheet_job`) so the post can hold the program to it.
//!
//! ## What it does to a program
//!
//! [`program_problems`] refuses the post by name when an operation of a
//! sheet-job setup:
//! - goes deeper than the sheet bottom less the allowance (into the
//!   spoilboard past what was stated, toward the machine bed);
//! - rapids across in XY below the clamp plane (sheet top plus the
//!   stated clamp clearance);
//! - or when the setup's stock or Z zero no longer agree with the job
//!   (edited after the checklist ran), so the stated numbers would lie.
//!
//! Provenance: our own design (#3070). The conventions (Z zero at the
//! sheet top or at the spoilboard, a small through-cut allowance into a
//! sacrificial spoilboard, a front-left XY zero) are general CNC-router
//! practice from machining handbooks. No vendor material consulted.

use flecs_ecs::prelude::World;
use serde::{Deserialize, Serialize};

use crate::geometry::Point3;
use crate::model;
use crate::schema::Location;
use crate::setup;
use crate::cl::{Move, MoveKind};

/// The deepest through-cut allowance a job may state, mm: a typo guard.
/// A spoilboard's sacrificial skin is a few millimetres; past this the
/// "allowance" is a cut into the board itself.
pub const MAX_ALLOWANCE: f64 = 5.0;

/// The highest clamp clearance a job may state, mm (a typo guard).
pub const MAX_CLAMP_CLEARANCE: f64 = 200.0;

/// How far a move may sit past a stated plane and still meet it, mm:
/// paths stand on a 1 um lattice, so this is float noise only.
const TOL: f64 = 1e-4;

/// Where the program's Z zero is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetZero {
    /// Z0 on the sheet's top face (the probe touches the sheet).
    Top,
    /// Z0 on the spoilboard, the sheet's bottom face.
    Spoilboard,
}

/// Where the program's XY zero is on the sheet, seen from the front of
/// the machine (front is -Y).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetCorner {
    FrontLeft,
    FrontRight,
    BackLeft,
    BackRight,
    Centre,
}

impl SheetZero {
    /// The word files and the MCP tool use.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Spoilboard => "spoilboard",
        }
    }

    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        [Self::Top, Self::Spoilboard].into_iter().find(|z| z.word() == w)
    }
}

impl SheetCorner {
    const ALL: [Self; 5] = [Self::FrontLeft, Self::FrontRight, Self::BackLeft, Self::BackRight, Self::Centre];

    /// The word files and the MCP tool use.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::FrontLeft => "front_left",
            Self::FrontRight => "front_right",
            Self::BackLeft => "back_left",
            Self::BackRight => "back_right",
            Self::Centre => "centre",
        }
    }

    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.word() == w)
    }
}

/// The checklist's answers, mm. `length` runs along X, `width` along Y.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SheetJob {
    pub length: f64,
    pub width: f64,
    pub thickness: f64,
    pub z_zero: SheetZero,
    pub xy_zero: SheetCorner,
    pub allowance: f64,
    pub clamp_clearance: f64,
}

/// Why `job` cannot be a sheet job; `None` when it can. The stock sizes
/// are also judged by `SetStock` (its own bound).
#[must_use]
pub fn invalid(job: &SheetJob) -> Option<String> {
    for (name, v) in [("length", job.length), ("width", job.width), ("thickness", job.thickness)] {
        if !v.is_finite() || v <= 0.0 {
            return Some(format!("the sheet {name} must be a positive size in mm, got {v}"));
        }
    }
    if !job.allowance.is_finite() || job.allowance < 0.0 {
        return Some(format!("the through-cut allowance must be 0 mm or more, got {}", job.allowance));
    }
    if job.allowance > MAX_ALLOWANCE {
        return Some(format!(
            "a through-cut allowance of {} mm cuts into the spoilboard past its sacrificial skin (at most {MAX_ALLOWANCE} mm)",
            job.allowance
        ));
    }
    if !job.clamp_clearance.is_finite() || job.clamp_clearance < 0.0 {
        return Some(format!("the clamp clearance must be 0 mm or more, got {}", job.clamp_clearance));
    }
    if job.clamp_clearance > MAX_CLAMP_CLEARANCE {
        return Some(format!(
            "a clamp clearance of {} mm is past any clamp (at most {MAX_CLAMP_CLEARANCE} mm)",
            job.clamp_clearance
        ));
    }
    None
}

/// The stock placement and the WCS origin of `job`, for a sheet whose
/// XY centre is at `(cx, cy)` and whose bottom face is at `z0`. The box
/// stock is centred in XY on its location and rises from it in Z.
#[must_use]
pub fn placement(job: &SheetJob, cx: f64, cy: f64, z0: f64) -> (Location, Location) {
    let stock = Location { pos: Point3 { x: cx, y: cy, z: z0 }, ..Location::default() };
    let (hx, hy) = (job.length / 2.0, job.width / 2.0);
    let (x, y) = match job.xy_zero {
        SheetCorner::FrontLeft => (cx - hx, cy - hy),
        SheetCorner::FrontRight => (cx + hx, cy - hy),
        SheetCorner::BackLeft => (cx - hx, cy + hy),
        SheetCorner::BackRight => (cx + hx, cy + hy),
        SheetCorner::Centre => (cx, cy),
    };
    let z = match job.z_zero {
        SheetZero::Top => z0 + job.thickness,
        SheetZero::Spoilboard => z0,
    };
    (stock, Location { pos: Point3 { x, y, z }, ..Location::default() })
}

/// The sheet job a setup states; `None` when it states none (or the
/// stored one is no longer valid - the loader refuses that).
#[must_use]
pub fn of(world: &World, setup_name: &str) -> Option<SheetJob> {
    setup::def_of(world, setup_name)?.sheet_job.filter(|j| invalid(j).is_none())
}

/// The post gate's sheet-job clause (see the module doc): every program
/// operation of a sheet-job setup that goes past the allowance or
/// rapids below the clamp plane, and every sheet-job setup whose stock
/// or Z zero no longer match the job. Empty when no setup states one.
#[must_use]
pub fn program_problems(world: &World) -> Vec<String> {
    let Ok(ops) = crate::posting::program_ops(world) else {
        return Vec::new(); // program_ops' own refusal holds the post
    };
    let mut out = Vec::new();
    let mut judged: Vec<String> = Vec::new();
    for (name, e) in ops {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        let Some(s) = setup::effective(world, op.setup.as_deref()) else {
            continue;
        };
        let Some(job) = of(world, &s) else {
            continue;
        };
        let (_, wcs) = crate::posting::wcs_context(world, Some(&s));
        let Some(b) = model::stock_world_aabb_for(world, Some(&s)) else {
            if !judged.contains(&s) {
                out.push(format!("setup '{s}': states a sheet job but has no stock - run the sheet job again"));
                judged.push(s);
            }
            continue;
        };
        if !judged.contains(&s) {
            out.extend(setup_drift(&s, &job, b.min.z, b.max.z, wcs[2]));
            judged.push(s.clone());
        }
        let label = model::operation_label(world, &name, &e);
        out.extend(model::with_toolpath(world, &e, |t| path_problems(&label, &s, &job, &t.path.moves, b.min.z, wcs[2])).unwrap_or_default());
    }
    out
}

/// The setup's stock and Z zero against what its job stated.
fn setup_drift(s: &str, job: &SheetJob, bottom: f64, top: f64, wcs_z: f64) -> Vec<String> {
    let mut out = Vec::new();
    if ((top - bottom) - job.thickness).abs() > TOL {
        out.push(format!(
            "setup '{s}': the stock is {:.3} mm thick, but the sheet job states a {:.3} mm sheet - run the sheet job again",
            top - bottom,
            job.thickness
        ));
        return out; // the Z zero below is measured on the wrong sheet
    }
    let want = match job.z_zero {
        SheetZero::Top => top,
        SheetZero::Spoilboard => bottom,
    };
    if (wcs_z - want).abs() > TOL {
        out.push(format!(
            "setup '{s}': Z zero is no longer at the {} the sheet job states ({:.3} mm off) - run the sheet job again",
            match job.z_zero {
                SheetZero::Top => "sheet top",
                SheetZero::Spoilboard => "spoilboard",
            },
            wcs_z - want
        ));
    }
    out
}

/// One operation's moves against the allowance floor and the clamp
/// plane. Heights print in program Z (from the setup's Z zero).
fn path_problems(label: &str, s: &str, job: &SheetJob, moves: &[Move], bottom: f64, wcs_z: f64) -> Vec<String> {
    let floor = bottom - job.allowance;
    let clamp = bottom + job.thickness + job.clamp_clearance;
    let mut out = Vec::new();
    let deepest = moves.iter().flat_map(|m| [m.from.z, m.to.z]).fold(f64::INFINITY, f64::min);
    if deepest < floor - TOL {
        out.push(format!(
            "{label}: cuts to Z{:.3}, {:.3} mm into the spoilboard, past setup '{s}''s through-cut allowance of {:.3} mm (floor Z{:.3}) - raise the operation's bottom height or state a larger allowance",
            deepest - wcs_z,
            bottom - deepest,
            job.allowance,
            floor - wcs_z
        ));
    }
    let across = |m: &&Move| {
        m.kind == MoveKind::Rapid && ((m.to.x - m.from.x).abs() > TOL || (m.to.y - m.from.y).abs() > TOL)
    };
    let low = moves.iter().filter(across).map(|m| m.from.z.min(m.to.z)).reduce(f64::min);
    if let Some(low) = low.filter(|z| *z < clamp - TOL) {
        out.push(format!(
            "{label}: rapids across at Z{:.3}, below setup '{s}''s clamp plane Z{:.3} (sheet top plus {:.3} mm clamp clearance) - raise the operation's clearance and retract heights",
            low - wcs_z,
            clamp - wcs_z,
            job.clamp_clearance
        ));
    }
    out
}

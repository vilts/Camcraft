//! Modular tool-body assembly (#361 / #1040): combine multiple vendor
//! models (cutting head, adapter, clamping unit) into one judged and
//! drawn `ToolBody`, mating pieces by ISO 13399 coordinate systems
//! (`MCS` <-> `CSW`) or explicit placement, and removing a piece in one
//! undoable step.
//!
//! Provenance: rigid-frame alignment from standard linear algebra and
//! ISO/TS 13399 coordinate system roles (PCS, MCS, CIP, CSW); our own
//! implementation, no vendor source or binaries consulted.

use std::path::Path;
use std::sync::Arc;

use camcraft_tools::lathe_holder::LathePlacement;

use crate::geometry::import::step_csys::{self, CsysRole, NamedCsys};
use crate::geometry::{Point3, Triangle};
use crate::schema::ToolBody;
use crate::tool_body::{self, BodyFrame};
use crate::tool_body_lathe;
use camcraft_tools::holder_profile::MAX_BODY_HEIGHT;

#[path = "tool_body_mate.rs"]
mod mate;

/// Coordinate-system match tolerance for distinguishing non-unit inputs.
const MIN_LEN: f64 = 1e-12;

/// One named coordinate system carried with a piece in the tool frame.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyCsys {
    pub name: String,
    pub role: CsysRole,
    pub origin: [f64; 3],
    pub z: [f64; 3],
    pub x: [f64; 3],
}

impl BodyCsys {
    /// Construct and validate a right-handed orthonormal frame by name.
    pub fn new(name: String, origin: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Result<Self, String> {
        let trimmed = name.trim().to_owned();
        if trimmed.is_empty() {
            return Err("coordinate system name cannot be empty".into());
        }
        if !origin.iter().all(|v| v.is_finite()) {
            return Err(format!("coordinate system '{trimmed}' has a non-finite origin"));
        }
        let uz = unit(z).ok_or_else(|| format!("coordinate system '{trimmed}' has a zero or non-finite z axis"))?;
        // Project x normal to z so tiny serialization rounding recovers an exact frame.
        let zx = dot(uz, x);
        let x_perp = [x[0] - zx * uz[0], x[1] - zx * uz[1], x[2] - zx * uz[2]];
        let ux = unit(x_perp).ok_or_else(|| format!("coordinate system '{trimmed}' has a zero or collinear x axis"))?;
        Ok(Self {
            role: CsysRole::classify(&trimmed),
            name: trimmed,
            origin,
            z: uz,
            x: ux,
        })
    }

    #[must_use]
    pub fn from_named(c: &NamedCsys) -> Self {
        Self { name: c.name.clone(), role: c.role, origin: c.origin, z: c.z, x: c.x }
    }

    /// Completing right-handed unit y axis (`z x x`).
    #[must_use]
    pub fn y(&self) -> [f64; 3] {
        cross(self.z, self.x)
    }
}

/// One component in a tool's assembled body, with facets already in the
/// tool frame alongside its carried coordinate systems.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyPiece {
    pub source: String,
    pub facets: Arc<[Triangle]>,
    pub csys: Vec<BodyCsys>,
}

/// How an attached piece is placed relative to the existing assembly.
#[derive(Clone, Debug, PartialEq)]
pub enum PiecePlace {
    /// Mate `piece_csys` on the incoming piece onto `target_csys` on
    /// `target_piece` (`None` searches all existing pieces). When both
    /// names are `None`, auto-mates a unique ISO 13399 `MCS` <-> `CSW*`
    /// pair and refuses absent or ambiguous mounts by name.
    Csys {
        target_piece: Option<usize>,
        target_csys: Option<String>,
        piece_csys: Option<String>,
    },
    /// Place explicitly into a spindle-frame assembly (`tool_body::place`).
    Spindle { axis: [f64; 3], offset: f64 },
    /// Place explicitly into a lathe-frame assembly (`tool_body_lathe::place`).
    Lathe(LathePlacement),
}

impl PiecePlace {
    /// Auto-mate by unambiguous ISO 13399 `MCS` <-> `CSW*` pair.
    #[must_use]
    pub fn auto_csys() -> Self {
        Self::Csys { target_piece: None, target_csys: None, piece_csys: None }
    }

    /// Explicitly mate `piece_csys` onto `target_csys` (on `target_piece`,
    /// or searching all existing pieces if `None`).
    #[must_use]
    pub fn csys(target_piece: Option<usize>, target_csys: &str, piece_csys: &str) -> Self {
        Self::Csys {
            target_piece,
            target_csys: Some(target_csys.to_owned()),
            piece_csys: Some(piece_csys.to_owned()),
        }
    }
}

/// A vendor model read from disk for piece assembly (model frame).
pub struct LoadedPiece {
    pub source: String,
    pub facets: Vec<Triangle>,
    pub csys: Vec<BodyCsys>,
}

/// Read a piece file through the single body-file door and extract any
/// valid ISO 13399 / named STEP coordinate systems (model frame).
pub fn read_piece_file(path: &Path) -> Result<LoadedPiece, String> {
    let file = crate::tool_body_file::read(path)?;
    let csys = read_csys_if_step(path)?;
    Ok(LoadedPiece { source: file.source, facets: file.facets, csys })
}

/// Read named coordinate systems if `path` is a STEP file; non-STEP files
/// carry none.
pub fn read_csys_if_step(path: &Path) -> Result<Vec<BodyCsys>, String> {
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    if ext != "step" && ext != "stp" {
        return Ok(Vec::new());
    }
    let data = std::fs::read(path).map_err(|e| format!("I/O error: {e}"))?;
    let report = step_csys::read_tool_csys(&data).map_err(|e| format!("coordinate systems unreadable: {e}"))?;
    Ok(report.systems.iter().map(BodyCsys::from_named).collect())
}

/// Transform `csys` from model frame into the spindle tool frame using
/// the exact mapping of `tool_body::place`.
pub fn transform_csys_spindle(csys: &[BodyCsys], axis: [f64; 3], offset: f64) -> Result<Vec<BodyCsys>, String> {
    let n = dot(axis, axis).sqrt();
    if !(n.is_finite() && n > MIN_LEN) {
        return Err("the tool axis direction must be a finite, non-zero vector".into());
    }
    if !(offset.is_finite() && offset.abs() <= MAX_BODY_HEIGHT) {
        return Err(format!("the tip offset must be a finite length within {MAX_BODY_HEIGHT} mm"));
    }
    let a = [axis[0] / n, axis[1] / n, axis[2] / n];
    let helper = if a[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let u = unit(cross(helper, a)).ok_or("the tool axis direction must be a finite, non-zero vector")?;
    let v = cross(a, u);
    let map_pt = |p: [f64; 3]| [dot(p, u), dot(p, v), dot(p, a) + offset];
    let map_dir = |d: [f64; 3]| [dot(d, u), dot(d, v), dot(d, a)];
    csys.iter()
        .map(|c| BodyCsys::new(c.name.clone(), map_pt(c.origin), map_dir(c.z), map_dir(c.x)))
        .collect()
}

/// Transform `csys` from model frame into the turning tool frame using
/// the exact mapping of `LathePlacement::place`.
pub fn transform_csys_lathe(csys: &[BodyCsys], place: &LathePlacement) -> Result<Vec<BodyCsys>, String> {
    let [x, y, z] = place.basis().map_err(|e| e.to_string())?;
    let t = place.tip;
    let map_pt = |p: [f64; 3]| {
        let d = [p[0] - t[0], p[1] - t[1], p[2] - t[2]];
        [dot(d, x), dot(d, y), dot(d, z)]
    };
    let map_dir = |d: [f64; 3]| [dot(d, x), dot(d, y), dot(d, z)];
    csys.iter()
        .map(|c| BodyCsys::new(c.name.clone(), map_pt(c.origin), map_dir(c.z), map_dir(c.x)))
        .collect()
}

/// Build an initial 1-piece list when a body carries or lacks CSYS.
#[must_use]
pub fn single_piece(source: String, facets: Arc<[Triangle]>, csys: Vec<BodyCsys>) -> Vec<BodyPiece> {
    vec![BodyPiece { source, facets, csys }]
}

/// Combined display source name across pieces.
#[must_use]
pub fn combined_source(pieces: &[BodyPiece]) -> String {
    pieces.iter().map(|p| p.source.as_str()).collect::<Vec<_>>().join(" + ")
}

/// Return the body's piece slice (a legacy/single body without explicit
/// pieces yields one synthetic piece view via clone only when needed).
#[must_use]
pub fn pieces_of(body: &ToolBody) -> Vec<BodyPiece> {
    if !body.pieces.is_empty() {
        return body.pieces.clone();
    }
    vec![BodyPiece { source: body.source.clone(), facets: Arc::clone(&body.facets), csys: Vec::new() }]
}

/// Place `piece` in tool coordinates relative to `existing` and return a
/// rebuilt `ToolBody` holding all pieces combined.
pub fn attach_piece(
    tool_name: &str,
    prev: &ToolBody,
    piece: LoadedPiece,
    place: &PiecePlace,
) -> Result<ToolBody, String> {
    if prev.spun.is_some() {
        return Err(format!(
            "tool '{tool_name}' carries a drawn holder profile - place a modelled piece first before attaching modular pieces"
        ));
    }
    let existing = pieces_of(prev);
    let placed_piece = match place {
        PiecePlace::Csys { target_piece, target_csys, piece_csys } => {
            let (target_c, piece_c) =
                mate::resolve(tool_name, &existing, &piece, *target_piece, target_csys.as_deref(), piece_csys.as_deref())?;
            mate_piece(&piece, piece_c, target_c)?
        }
        PiecePlace::Spindle { axis, offset } => {
            if matches!(prev.frame, BodyFrame::Lathe { .. }) {
                return Err(format!("tool '{tool_name}' carries a lathe-frame body, not a spindle one"));
            }
            let facets = tool_body::place(&piece.facets, *axis, *offset)?;
            let csys = transform_csys_spindle(&piece.csys, *axis, *offset)?;
            BodyPiece { source: piece.source, facets: Arc::from(facets), csys }
        }
        PiecePlace::Lathe(lp) => {
            if matches!(prev.frame, BodyFrame::Spindle { .. }) {
                return Err(format!("tool '{tool_name}' carries a spindle-frame body, not a lathe one"));
            }
            let facets = tool_body_lathe::place(&piece.facets, lp)?;
            let csys = transform_csys_lathe(&piece.csys, lp)?;
            BodyPiece { source: piece.source, facets: Arc::from(facets), csys }
        }
    };
    let mut next_pieces = existing;
    next_pieces.push(placed_piece);
    rebuild_body(prev, next_pieces)
}

/// Remove piece `index` from a multi-piece body and rebuild the combined
/// envelope/clearance. Refuses removing the last remaining piece or an
/// out-of-range index by name.
pub fn remove_piece(tool_name: &str, prev: &ToolBody, index: usize) -> Result<ToolBody, String> {
    let mut pieces = pieces_of(prev);
    if index >= pieces.len() {
        return Err(format!(
            "piece index {index} is out of range (tool '{tool_name}' has {} piece(s))",
            pieces.len()
        ));
    }
    if pieces.len() <= 1 {
        return Err(format!(
            "cannot remove the only piece of tool '{tool_name}' (use clear to remove the body)"
        ));
    }
    pieces.remove(index);
    rebuild_body(prev, pieces)
}

/// Rebuild a `ToolBody` (combined source, combined facets, re-derived
/// spindle envelope or lathe clearance) from `pieces`.
pub fn rebuild_body(template: &ToolBody, pieces: Vec<BodyPiece>) -> Result<ToolBody, String> {
    if pieces.is_empty() {
        return Err("tool body assembly has no pieces".into());
    }
    let source = combined_source(&pieces);
    let combined: Vec<Triangle> = pieces.iter().flat_map(|p| p.facets.iter().copied()).collect();
    let mut body = match &template.frame {
        BodyFrame::Spindle { axis, offset, .. } => tool_body::build(&source, *axis, *offset, combined)?,
        BodyFrame::Lathe { place, zone, .. } => tool_body_lathe::build(&source, *place, *zone, combined)?,
    };
    body.pieces = pieces;
    Ok(body)
}

/// Transform `piece` cleanly so its local `piece_c` aligns identically onto
/// `target_c` (already expressed in the tool frame).
fn mate_piece(piece: &LoadedPiece, piece_c: &BodyCsys, target_c: &BodyCsys) -> Result<BodyPiece, String> {
    let px = piece_c.x;
    let py = piece_c.y();
    let pz = piece_c.z;
    let po = piece_c.origin;
    let tx = target_c.x;
    let ty = target_c.y();
    let tz = target_c.z;
    let to = target_c.origin;
    let map_pt = |p: [f64; 3]| -> [f64; 3] {
        let d = [p[0] - po[0], p[1] - po[1], p[2] - po[2]];
        let cx = dot(d, px);
        let cy = dot(d, py);
        let cz = dot(d, pz);
        [
            to[0] + cx * tx[0] + cy * ty[0] + cz * tz[0],
            to[1] + cx * tx[1] + cy * ty[1] + cz * tz[1],
            to[2] + cx * tx[2] + cy * ty[2] + cz * tz[2],
        ]
    };
    let map_dir = |v: [f64; 3]| -> [f64; 3] {
        let cx = dot(v, px);
        let cy = dot(v, py);
        let cz = dot(v, pz);
        [
            cx * tx[0] + cy * ty[0] + cz * tz[0],
            cx * tx[1] + cy * ty[1] + cz * tz[1],
            cx * tx[2] + cy * ty[2] + cz * tz[2],
        ]
    };
    let facets: Vec<Triangle> = piece
        .facets
        .iter()
        .map(|t| Triangle::new(pt(map_pt([t.a.x, t.a.y, t.a.z])), pt(map_pt([t.b.x, t.b.y, t.b.z])), pt(map_pt([t.c.x, t.c.y, t.c.z]))))
        .collect();
    let csys = piece
        .csys
        .iter()
        .map(|c| BodyCsys::new(c.name.clone(), map_pt(c.origin), map_dir(c.z), map_dir(c.x)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BodyPiece { source: piece.source.clone(), facets: Arc::from(facets), csys })
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = dot(v, v).sqrt();
    (n.is_finite() && n > MIN_LEN && v.iter().all(|c| c.is_finite())).then(|| [v[0] / n, v[1] / n, v[2] / n])
}

fn pt(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

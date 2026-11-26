//! Mirror a path across a vertical plane (#3011).
//!
//! Provenance: the reflection `p -> p - 2 ((p - P) . n) n`; the
//! G2 <-> G3 and G41 <-> G42 swap is the RS-274 / ISO 6983 direction
//! rule, the one `post::invert` and `post::wcs_flip` apply. Climb and
//! conventional milling are general machining practice. Our own model;
//! no vendor material consulted.
//!
//! A reflection turns every climb cut into a conventional cut.
//! [`MirrorMode::GeometricMirror`] accepts that: the exact mirror image.
//! [`MirrorMode::PreserveClimb`] then walks each cutting pass backwards
//! (`transform_climb.rs`), which restores the original turning sense
//! and the original cutter side. A horizontal plane is refused for the
//! reason `post::wcs_flip` refuses a flipped Z; a tilted plane, a tilted
//! move and a plasma statement are refused too.

use crate::geometry::Point3;

use super::super::path::Toolpath;
use super::super::pose::is_vertical;
use super::{climb, map_path, refuse_records, Rigid, TransformError};

/// A plane through `point` with the normal `normal` (any length). Only
/// a vertical plane (`normal` with no Z part) mirrors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MirrorPlane {
    pub point: Point3,
    pub normal: [f64; 3],
}

/// What the mirror keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirrorMode {
    /// The exact mirror image: G2 <-> G3, G41 <-> G42, climb becomes
    /// conventional.
    GeometricMirror,
    /// The mirror image cut in the original sense: each pass reversed
    /// between its entry and its retract. Only the families that
    /// [`PassFamily::reversible`] names.
    PreserveClimb(PassFamily),
}

/// The kind of path the caller mirrors: the cl layer cannot read the
/// operation from the moves, so the caller states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassFamily {
    /// A 2D contour.
    Contour,
    /// A profile (outside or inside the part).
    Profile,
    /// A chamfer or deburr pass.
    Chamfer,
    /// A path that enters on a helical ramp.
    HelicalRamp,
    /// Adaptive clearing.
    Adaptive,
    /// Multi-pass roughing (pocket or face clearing).
    MultiPassRoughing,
    /// Anything else.
    Other,
}

impl PassFamily {
    /// The refusal for a family whose passes cannot reverse safely, or
    /// None for a family that can.
    #[must_use]
    pub fn reversible(self) -> Option<&'static str> {
        match self {
            PassFamily::Contour | PassFamily::Profile | PassFamily::Chamfer => None,
            PassFamily::HelicalRamp => Some("a helical ramp entry cannot run backwards"),
            PassFamily::Adaptive => Some("adaptive clearing engagement changes when it runs backwards"),
            PassFamily::MultiPassRoughing => Some("multi-pass roughing changes its engagement when it runs backwards"),
            PassFamily::Other => Some("only a 2D contour, profile or chamfer path reverses"),
        }
    }
}

/// Mirror the path across `plane` in `mode`.
pub fn mirror_plane(tp: &Toolpath, plane: MirrorPlane, mode: MirrorMode) -> Result<Toolpath, TransformError> {
    let n = vertical_normal(plane)?;
    if let MirrorMode::PreserveClimb(f) = mode
        && let Some(why) = f.reversible()
    {
        return Err(TransformError::NotReversible(why.into()));
    }
    refuse_records(tp)?;
    if tp.moves.iter().any(|m| m.axis.is_some_and(|a| !is_vertical(&a))) {
        return Err(TransformError::TiltedMove);
    }
    if tp.plasma.is_some() {
        return Err(TransformError::PlasmaCut);
    }
    // m = I - 2 n n^T, t = 2 (P . n) n: exact for an axis-aligned plane.
    let m: [[f64; 3]; 3] =
        std::array::from_fn(|i| std::array::from_fn(|j| f64::from(u8::from(i == j)) - 2.0 * n[i] * n[j]));
    let k = 2.0 * (plane.point.x * n[0] + plane.point.y * n[1]);
    let mirrored = map_path(tp, &Rigid { m, t: [k * n[0], k * n[1], 0.0] })?;
    match mode {
        MirrorMode::GeometricMirror => Ok(mirrored),
        MirrorMode::PreserveClimb(_) => climb::reverse_passes(mirrored),
    }
}

/// The unit normal of a vertical plane, or the refusal by name.
fn vertical_normal(plane: MirrorPlane) -> Result<[f64; 3], TransformError> {
    let [x, y, z] = plane.normal;
    if !plane.point.is_finite() || ![x, y, z].iter().all(|v| v.is_finite()) {
        return Err(TransformError::BadInput("the mirror plane"));
    }
    let len = (x * x + y * y + z * z).sqrt();
    if !len.is_finite() || len < 1e-12 {
        return Err(TransformError::BadInput("the mirror plane normal"));
    }
    let (x, y, z) = (x / len, y / len, z / len);
    if z.abs() > 1e-12 {
        return Err(if x.hypot(y) <= 1e-12 {
            TransformError::HorizontalMirrorPlane
        } else {
            TransformError::TiltedMirrorPlane
        });
    }
    let h = x.hypot(y);
    Ok([x / h, y / h, 0.0])
}

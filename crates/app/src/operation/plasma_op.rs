//! The Plasma Profile operation's seed and generation (#670): the op's
//! stored numbers resolved against the world (heights, the torch's
//! kerf), the picked contours or the model's through-silhouette as the
//! geometry, and `plasma::plasma_profile` run over them.
//!
//! Provenance: our own glue over the motion crate's plasma generator,
//! whose module head cites the public sources (Hypertherm's cut
//! direction guidance, the LinuxCNC Plasma Primer §13, Tormach's 1300PL
//! manual and PathPilot post guidelines).

use crate::cl::Toolpath;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Triangle;
use crate::geometry::slice;
use crate::machining2d::GenError;
use crate::plasma::{plasma_profile, PlasmaParams, SMALL_HOLE_DIA, SMALL_HOLE_FEED};
use crate::tools::{Tool, ToolKind};
use crate::verify::Warning;

use super::kinds::Carried;
use super::material::FOOTPRINT_SAMPLES;
use super::types::{OpParams, PlasmaComp, PlasmaOp};
use super::ResolvedHeights;

/// The plasma draft: the carried heights (plate top and bottom), the
/// torch's own kerf, computer compensation, arc leads of 5 mm in and
/// 3 mm out, the Primer's small-hole numbers, no overburn, and every
/// controller number blank (AutoFS decides).
pub(super) fn seed_plasma_profile(c: &Carried) -> OpParams {
    OpParams::PlasmaProfile(PlasmaOp {
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        kerf: None,
        comp: PlasmaComp::Computer,
        reversed: false,
        lead_in: 5.0,
        lead_out: 3.0,
        small_hole_dia: SMALL_HOLE_DIA,
        small_hole_feed: SMALL_HOLE_FEED,
        overburn: 0.0,
        feed: None,
        pierce_height: None,
        cut_height: None,
        pierce_delay: None,
    })
}

/// The generator's numbers from the op's and the world's.
///
/// `model_excess` is how much coarser than promised the model the part
/// silhouette is read from is (#1053): the kerf law gives it back.
pub fn plasma_params(op: &PlasmaOp, tool: &Tool, hs: &ResolvedHeights, model_excess: f64) -> Result<PlasmaParams, GenError> {
    if tool.kind != ToolKind::Torch {
        return Err(GenError::Invalid(format!(
            "a plasma profile mounts a plasma torch, not a {} - its diameter is the kerf width",
            tool.kind
        )));
    }
    if let Some(why) = tool.torch_refusal() {
        return Err(GenError::Invalid(why));
    }
    let (Some(top), Some(bottom)) = (hs.top, hs.bottom) else {
        return Err(GenError::Invalid("plasma profile: the plate's top and bottom must both resolve".into()));
    };
    if !(top.is_finite() && bottom.is_finite()) || bottom > top {
        return Err(GenError::Invalid(format!(
            "plasma profile: the plate's bottom ({bottom:.3}) stands above its top ({top:.3})"
        )));
    }
    Ok(PlasmaParams {
        kerf: op.kerf.unwrap_or(tool.diameter),
        control_comp: op.comp == PlasmaComp::Control,
        reversed: op.reversed,
        lead_in: op.lead_in,
        lead_out: op.lead_out,
        small_hole_dia: op.small_hole_dia,
        small_hole_feed: op.small_hole_feed,
        overburn: op.overburn,
        thickness: top - bottom,
        feed: op.feed,
        clearance_z: hs.clearance,
        cut_z: top + op.cut_height.unwrap_or(0.0),
        pierce_height: op.pierce_height,
        cut_height: op.cut_height,
        pierce_delay: op.pierce_delay,
        model_excess,
    })
}

/// Generate: `picks` is the picked contours (closed rings, open runs)
/// when the source is a selection; otherwise `base` - the model's
/// through-silhouette - is cut whole, every ring of it a contour. The
/// kerf may never reach `facets`' silhouette, judged with that model's
/// excess given back (#1053).
pub(super) fn generate_plasma(
    op: &PlasmaOp,
    tool: &Tool,
    hs: &ResolvedHeights,
    picks: Option<(Vec<Loop>, Vec<Vec<Point2>>)>,
    base: impl FnOnce() -> Result<Region, GenError>,
    (facets, model_excess): (&[Triangle], f64),
) -> Result<(Toolpath, Vec<Warning>), GenError> {
    let p = plasma_params(op, tool, hs, model_excess)?;
    let part = if facets.is_empty() {
        Region::empty()
    } else {
        slice::project(facets, FOOTPRINT_SAMPLES)
            .map_err(|e| GenError::Invalid(format!("plasma profile: the part's silhouette is unreadable: {e}")))?
    };
    let (loops, chains) = match picks {
        Some(geom) => geom,
        None => (base()?.loops().to_vec(), Vec::new()),
    };
    let path = plasma_profile(&loops, &chains, &part, &p)?;
    Ok((path, Vec::new()))
}

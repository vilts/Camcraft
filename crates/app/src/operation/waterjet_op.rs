//! The Waterjet Profile operation's seed and generation (#1883): the
//! op's stored numbers resolved against the world (heights, the
//! nozzle's kerf), the picked contours or the model's through-
//! silhouette as the geometry, and `waterjet::waterjet_profile` run
//! over them.
//!
//! Provenance: our own glue over the motion crate's waterjet generator,
//! whose module head cites the public sources (general abrasive-waterjet
//! practice; RS-274 / ISO 6983).

use crate::cl::Toolpath;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::slice;
use crate::geometry::Triangle;
use crate::machining2d::GenError;
use crate::tools::{Tool, ToolKind};
use crate::verify::Warning;
use crate::waterjet::{waterjet_profile, JetTabs, LeadShape, OpenSide, WaterjetParams};

use super::kinds::Carried;
use super::material::FOOTPRINT_SAMPLES;
use super::types::{JetLead, JetSide, OpParams, WaterjetOp};
use super::ResolvedHeights;

/// The waterjet draft: the carried heights (sheet top and bottom), the
/// nozzle's own kerf, computer compensation, arc leads of 3 mm in and
/// 2 mm out, open runs on their centre line, no tabs, arcs kept, and a
/// conservative feed and dwell for the user to state from the cut data.
pub(super) fn seed_waterjet_profile(c: &Carried) -> OpParams {
    OpParams::WaterjetProfile(WaterjetOp {
        clearance_z: c.clearance,
        top_z: c.top,
        bottom_z: c.bottom,
        kerf: None,
        control_comp: false,
        reversed: false,
        lead: JetLead::Arc,
        lead_in: 3.0,
        lead_out: 2.0,
        open_side: JetSide::Center,
        tabs: None,
        feed: 100.0,
        pierce_time: 1.0,
        abrasive_delay: 0.0,
        linearize: None,
        picks: crate::path_picks::PathPicks::default(),
    })
}

/// The generator's numbers from the op's and the world's.
///
/// `model_excess` is how much coarser than promised the model the part
/// silhouette is read from is (#1053): the kerf law gives it back.
pub fn waterjet_params(op: &WaterjetOp, tool: &Tool, hs: &ResolvedHeights, model_excess: f64) -> Result<WaterjetParams, GenError> {
    if tool.kind != ToolKind::Waterjet {
        return Err(GenError::Invalid(format!(
            "a waterjet profile mounts a waterjet nozzle, not a {} - its diameter is the kerf width",
            tool.kind
        )));
    }
    if let Some(why) = crate::waterjet_tools::refusal(tool) {
        return Err(GenError::Invalid(format!("waterjet profile: {why}")));
    }
    let (Some(top), Some(bottom)) = (hs.top, hs.bottom) else {
        return Err(GenError::Invalid("waterjet profile: the sheet's top and bottom must both resolve".into()));
    };
    if !(top.is_finite() && bottom.is_finite()) || bottom > top {
        return Err(GenError::Invalid(format!(
            "waterjet profile: the sheet's bottom ({bottom:.3}) stands above its top ({top:.3})"
        )));
    }
    Ok(WaterjetParams {
        kerf: op.kerf.unwrap_or(tool.diameter),
        control_comp: op.control_comp,
        reversed: op.reversed,
        lead_shape: match op.lead {
            JetLead::Arc => LeadShape::Arc,
            JetLead::Straight => LeadShape::Straight,
        },
        lead_in: op.lead_in,
        lead_out: op.lead_out,
        open_side: match op.open_side {
            JetSide::Center => OpenSide::Center,
            JetSide::Left => OpenSide::Left,
            JetSide::Right => OpenSide::Right,
        },
        tabs: op.tabs.as_ref().map(|t| JetTabs {
            count: t.count,
            width: t.width,
            at: t.at.iter().map(|p| Point2::new(p[0], p[1])).collect(),
        }),
        pierce_time: op.pierce_time,
        abrasive_delay: op.abrasive_delay,
        linearize: op.linearize,
        feed: op.feed,
        clearance_z: hs.clearance,
        cut_z: top,
        model_excess,
    })
}

/// Generate: `picks` is the picked contours (closed rings, open runs)
/// when the source is a selection; otherwise `base` - the model's
/// through-silhouette - is cut whole. The kerf may never reach
/// `facets`' silhouette, judged with that model's excess given back.
pub(super) fn generate_waterjet(
    op: &WaterjetOp,
    tool: &Tool,
    hs: &ResolvedHeights,
    picks: Option<(Vec<Loop>, Vec<Vec<Point2>>)>,
    base: impl FnOnce() -> Result<Region, GenError>,
    (facets, model_excess): (&[Triangle], f64),
) -> Result<(Toolpath, Vec<Warning>), GenError> {
    let p = waterjet_params(op, tool, hs, model_excess)?;
    let part = if facets.is_empty() {
        Region::empty()
    } else {
        slice::project(facets, FOOTPRINT_SAMPLES)
            .map_err(|e| GenError::Invalid(format!("waterjet profile: the part's silhouette is unreadable: {e}")))?
    };
    let (loops, chains) = match picks {
        Some(geom) => geom,
        None => (base()?.loops().to_vec(), Vec::new()),
    };
    let path = waterjet_profile(&loops, &chains, &part, &p)?;
    Ok((path, Vec::new()))
}

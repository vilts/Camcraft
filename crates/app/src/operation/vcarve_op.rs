//! The V-Carve operation's seed and its tool doors (#1443): the
//! operation's parameters and the mounted tool resolved into the motion
//! crate's `v_carve` call.
//!
//! Provenance: our own glue over `machining2d::v_carve`, whose module
//! head names its public sources (the medial axis transform papers and
//! the handbook cone law). No vendor material consulted.

use crate::cl::Toolpath;
use crate::geo2d::Region;
use crate::machining2d::{self, GenError, VCarveParams};
use crate::tools::{Tool, ToolKind};

use super::kinds::Carried;
use super::types::{Height, OpParams};
use super::ResolvedHeights;

/// The kind's name in its refusals.
pub const LABEL: &str = "V-Carve";

/// The draft's maximum depth under its top, mm.
const SEED_DEPTH: f64 = 2.0;

/// The V-carve draft: the carried heights and feeds, a maximum depth of
/// 2 mm under the top, one level, and no flat clearing. The numbers are
/// a draft to edit.
pub(super) fn seed_vcarve(c: &Carried) -> OpParams {
    OpParams::VCarve {
        clearance_z: c.clearance,
        retract_z: None,
        top_z: c.top,
        bottom_z: Height::of(c.top.reference, c.top.offset - SEED_DEPTH),
        stepdown: c.stepdown,
        feed: c.feed,
        plunge_feed: c.plunge,
        flat_clear: false,
        flat_stepover: 0.25,
        tilt: c.tilt,
        picks: crate::path_picks::PathPicks::default(),
    }
}

/// Generate the V-carve of `region` with `tool`. The tool must be a
/// cone (the V-bit), and its flute must cover the depth the cone cuts.
///
/// # Errors
/// A tool that is not a V-bit, a flute shorter than the cut, a missing
/// height, and each refusal of the kernel, by name.
pub(super) fn carve(
    region: &Region,
    tool: &Tool,
    tool_name: &str,
    hl: &ResolvedHeights,
    params: &OpParams,
) -> Result<Toolpath, GenError> {
    let OpParams::VCarve { stepdown, feed, plunge_feed, flat_clear, flat_stepover, .. } = params else {
        return Err(GenError::Invalid(format!("{LABEL} was handed another operation's parameters")));
    };
    if tool.kind != ToolKind::Taper {
        return Err(GenError::Invalid(format!(
            "{LABEL} cuts with the cone of a V-bit - it needs a chamfer mill (V-bit), '{}' is a {}",
            tool_name, tool.kind
        )));
    }
    let need = |z: Option<f64>, what: &str| {
        z.ok_or_else(|| GenError::Invalid(format!("{LABEL} needs {what}: set the height")))
    };
    let p = VCarveParams {
        angle_deg: tool.tip_angle,
        tool_diameter: tool.diameter,
        clearance_z: hl.clearance,
        retract_z: hl.retract,
        top_z: need(hl.top, "Top of Part Z")?,
        bottom_z: need(hl.bottom, "Bottom of Part Z")?,
        stepdown: *stepdown,
        feed: *feed,
        plunge_feed: *plunge_feed,
        flat_clear: *flat_clear,
        flat_stepover: *flat_stepover,
    };
    // The deepest the cone goes: the maximum depth, or the cone's own
    // height where that is less. The flute must cover it.
    let tan_half = p.tan_half();
    if tan_half.is_finite() && tan_half > 0.0 {
        let deepest = (p.top_z - p.bottom_z).min(p.cone_height());
        if tool.flute_len < deepest - 1e-9 {
            return Err(GenError::Invalid(format!(
                "the cone of '{}' has {} mm of flute, short of the {:.1} mm maximum depth of this {LABEL}",
                tool_name, tool.flute_len, deepest
            )));
        }
    }
    machining2d::v_carve(region, &p)
}

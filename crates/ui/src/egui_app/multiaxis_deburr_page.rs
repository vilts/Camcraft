//! The operation page: the Multi-Axis Deburr kind's fields (#1894) - the
//! Deburr's break size, edge filters and unreachable-edge policy on the
//! Geometry tab (with the stock and part law in one visible line), the
//! axis mode, lean and tilt cap on the Passes tab (the shank and holder
//! guard shown as always on: the kind has no switch for it), the window
//! and the feeds. Each value edits the page draft, and Apply rides the
//! command history.
//!
//! Provenance: our own UI over the operation parameter model
//! (`operation::multiaxis_deburr_op` names the kind's sources); no
//! vendor material or text.

use super::fields::*;
use super::kind_editors::axis_editor;
use camcraft_app::finishing3d::UnreachableEdges;
use camcraft_app::multiaxis::DeburrAxisMode;

/// The setup law, said before the user meets the refusal: the guards
/// judge the posed tool only against the source model.
pub(super) const MA_DEBURR_SETUP_HINT: &str =
    "Stock must be the source model itself, and the setup can hold no other part: the tilted tool is checked against this model only.";
/// The shank and holder guard is always on for this kind.
pub(super) const MA_DEBURR_SHANK_HINT: &str =
    "the cone, shank and holder are always checked against the part on every pose; a pose that hits is tilted away or the edge is out of reach";

pub(super) fn fields(ui: &mut Ui, world: &World, params: &mut OpParams, cx: &mut Cx<'_>) {
    let (u, tab, op_ctx) = (cx.u, cx.tab, cx.op_ctx);
    let OpParams::MultiAxisDeburr {
        width,
        clearance_z,
        top_z,
        bottom_z,
        feed,
        plunge_feed,
        exclude_edges,
        min_edge_angle_deg,
        exclude_holes,
        hole_min_diameter,
        hole_max_diameter,
        unreachable,
        sizing,
        axis_mode,
        lead_deg,
        lag_deg,
        max_tilt_deg,
        ..
    } = params
    else {
        return;
    };
    if tab == SEC_GEO {
        consequence(ui, MA_DEBURR_SETUP_HINT);
        super::deburr_page::sizing(ui, u, width, sizing);
        super::deburr_page::filters(
            ui,
            world,
            u,
            cx.picks,
            exclude_edges,
            min_edge_angle_deg,
            exclude_holes,
            hole_min_diameter,
            hole_max_diameter,
        );
        let mut i = usize::from(*unreachable == UnreachableEdges::Refuse);
        prop_row(ui, "Unreachable edges", |ui| combo(ui, "ma_deburr.unreachable", &mut i, &["Skip with note", "Refuse"]));
        *unreachable = if i == 1 { UnreachableEdges::Refuse } else { UnreachableEdges::SkipWithNote };
        hint(ui, "an edge with no clear pose inside the max tilt is out of reach: Skip with note leaves it and breaks the others, Refuse stops the operation");
    }
    if tab == SEC_PASSES {
        let mut m = usize::from(*axis_mode == DeburrAxisMode::Axis4);
        prop_row(ui, "Axis mode", |ui| combo(ui, "ma_deburr.axis_mode", &mut m, &["5-axis tilt", "4-axis rotary (A about X)"]));
        *axis_mode = if m == 1 { DeburrAxisMode::Axis4 } else { DeburrAxisMode::Axis5 };
        hint(ui, "the cone's flank lies on each break; 5-axis turns the axis freely about the break, 4-axis keeps it square to the A axis");
        axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
        hint(ui, "here the max tilt is measured from the edge's natural pose, and lead minus lag must stay inside it");
        // #1894 review: the shank and holder guard has no switch on this
        // kind. The kernel always runs it, so the page shows a fact.
        prop_row(ui, "Shank and holder guard", |ui| ui.label("Always on"));
        hint(ui, MA_DEBURR_SHANK_HINT);
    }
    if tab == SEC_HEIGHTS {
        height_editor(ui, "Clearance Z", world, op_ctx.as_deref(), clearance_z, "ma_deburr.clearance");
        height_editor(ui, "Window Top Z", world, op_ctx.as_deref(), top_z, "ma_deburr.top");
        height_editor(ui, "Window Bottom Z", world, op_ctx.as_deref(), bottom_z, "ma_deburr.bottom");
        hint(ui, "the two planes the EDGES must stand between: an edge running outside the window is clipped along its own line");
    }
    if tab == SEC_FEEDS {
        prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
        prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
    }
}

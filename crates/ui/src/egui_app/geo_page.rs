//! The Geometry tab's pick block (split out of `pages.rs` with the
//! property grid, user review 2026-11-06): what a viewport click selects,
//! the picked features as chips (hover lights one, x removes it), what the
//! selection spells, and the pick's own refusal under the picks it is
//! about. Pick mode is view state; every selection change is a command.
//!
//! Provenance: our own port of the M-c / M-e geometry block; no vendor
//! material involved.

use egui::{RichText, Ui};

use camcraft_app::operation::{self, FeatureSel, GeoSel, SrcRef};

use crate::state::{CommandKind, ErrorScope, GeoPick, State};

use super::colorvision::text_inks;
use super::layout::group;

/// The Edge mode's one-line hint: how a loop gets closed.
pub const EDGE_HINT: &str = "Ctrl+click adds an edge · Shift+click picks the whole loop";

pub(super) fn pick_block(ui: &mut Ui, state: &mut State, target: &str, live_setup: Option<&str>) {
    group(ui, "Selection", None, |ui| body(ui, state, target, live_setup));
}

fn body(ui: &mut Ui, state: &mut State, target: &str, live_setup: Option<&str>) {
    // Viewport pick mode - transient view state: the mode itself is not
    // undoable; what a Face/Edge click changes is.
    ui.horizontal_wrapped(|ui| {
        ui.label("Click selects:").on_hover_text("Click in the viewport - Ctrl adds; in Edge mode Shift takes the whole loop and Alt one edge.");
        let mut pick_mode = match state.geo_pick {
            GeoPick::Model => 0,
            GeoPick::Face => 1,
            GeoPick::Edge => 2,
            GeoPick::Vertex => 3,
        };
        ui.radio_value(&mut pick_mode, 0, "Models");
        ui.radio_value(&mut pick_mode, 1, "Faces");
        ui.radio_value(&mut pick_mode, 2, "Edges");
        ui.radio_value(&mut pick_mode, 3, "Vertices");
        state.geo_pick = match pick_mode {
            1 => GeoPick::Face,
            2 => GeoPick::Edge,
            3 => GeoPick::Vertex,
            _ => GeoPick::Model,
        };
    });
    if state.geo_pick == GeoPick::Edge {
        // Multi-select was there but nobody could find it (user hand
        // test 2026-11-06): say it where the mode is chosen.
        ui.label(RichText::new(EDGE_HINT).weak());
    }
    if state.op_ui.src_kind == 2 {
        chips(ui, state, target);
        // #187: the 3-2-1 alignment reads the picks as datums - a flat
        // face for Z, a straight edge for X, a vertex or bore for the
        // origin. The app builds the frame (or names what to re-pick).
        let aligned = camcraft_app::pick::align_321(&state.world, &state.op_ui.geo);
        let tip = match &aligned {
            Ok(_) => "place the WCS slot chosen in the WCS pane: Z out of the picked flat face, X along the picked \
                      edge, origin at the picked vertex or bore (else the edge's end), on the face"
                .to_owned(),
            Err(why) => format!("3-2-1 alignment: {why}"),
        };
        if ui.add_enabled(aligned.is_ok(), egui::Button::new("Align WCS (3-2-1)").small())
            .on_hover_text(&tip)
            .on_disabled_hover_text(&tip)
            .clicked()
        {
            crate::vertex_pick::queue_wcs_321(state);
        }
    }
    let (notes, refusal) =
        operation::selection_account(&state.world, live_setup, &state.op_ui.geo.clone(), &state.op_ui.params.clone());
    for note in notes {
        ui.label(RichText::new(note).weak());
    }
    // The pick's own refusal, under the picks it is about: the stored
    // selection's (the walk Generate would refuse), else the pick door's
    // when the last pick was turned away.
    let door = state
        .error_for(ErrorScope::Operation)
        .filter(|m| m.starts_with("geometry selection not updated") || m.starts_with("links not updated"))
        .map(str::to_owned);
    if let Some(msg) = refusal.or(door) {
        super::error_text::error_text(ui, text_inks(ui, &state.world).error, &msg);
    }
    if state.op_ui.src_kind == 1 && !state.op_ui.src_name.is_empty() {
        let n = state.op_ui.src_name.clone();
        ui.label(format!("Whole of '{n}' - the first pick replaces this."));
    } else if state.op_ui.src_kind != 2 {
        ui.label("Whole stock contour (default) - picks bound the operation.");
    }
}

/// The picked features, one chip each: hovering lights the feature in
/// the viewport (the shared interact law), x removes it - the last one
/// falls back to the stock source.
fn chips(ui: &mut Ui, state: &mut State, target: &str) {
    let chips: Vec<(String, bool, GeoSel)> =
        state.geo_chips().into_iter().map(|c| (c.label, c.missing, c.sel)).collect();
    for (i, (label, missing, sel)) in chips.into_iter().enumerate() {
        let vertex = match sel.feature {
            FeatureSel::Vertex(id) if !missing => Some((sel.entity.clone(), id)),
            _ => None,
        };
        // A picked face (or welded curved wall) offers its centroid; a
        // flat one also offers Z only. The targets are the app's to say.
        let face = match sel.feature {
            FeatureSel::Face(_) | FeatureSel::Patch(_) if !missing => {
                camcraft_app::pick::face_origin(&state.world, &sel.entity, sel.feature)
                    .ok()
                    .map(|t| (sel.entity.clone(), sel.feature, t))
            }
            _ => None,
        };
        let remove = ui
            .horizontal(|ui| {
                // M13.4 stage 2 hook: a picked corner can place the WCS
                // slot chosen in the WCS pane, through SetWcsOrigin.
                if let Some((entity, id)) = &vertex
                    && ui.small_button("Set origin").on_hover_text("set the work origin at this vertex").clicked()
                {
                    crate::vertex_pick::queue_origin_at_vertex(state, entity, *id);
                }
                if let Some((entity, feat, t)) = &face {
                    let tip = if t.planar {
                        "set the work origin at this face's centroid"
                    } else {
                        "set the work origin at this curved face's centroid (a full bore: its axis at mid-height)"
                    };
                    if ui.small_button("Set origin").on_hover_text(tip).clicked() {
                        crate::vertex_pick::queue_origin_at_face(state, entity, *feat, false);
                    }
                    if t.plane_z.is_some()
                        && ui
                            .small_button("Set origin (Z only)")
                            .on_hover_text("keep the origin's X and Y; move Z to this flat face")
                            .clicked()
                    {
                        crate::vertex_pick::queue_origin_at_face(state, entity, *feat, true);
                    }
                }
                let resp = if missing {
                    ui.colored_label(text_inks(ui, &state.world).missing, &label)
                } else {
                    ui.label(&label)
                };
                if resp.hovered() {
                    state.geo_hover = Some(sel);
                }
                ui.small_button("×").on_hover_text("remove this pick").clicked()
            })
            .inner;
        if remove {
            let mut rest: Vec<GeoSel> = state.op_ui.geo.clone();
            if i < rest.len() {
                rest.remove(i);
                if rest.is_empty() {
                    state.op_ui.geo.clear();
                    state.op_ui.src_kind = 0;
                    state.pending.push_back(CommandKind::EditOperationLinks {
                        name: target.to_owned(),
                        tool: None,
                        src: Some(SrcRef::Stock),
                    });
                } else {
                    state.op_ui.geo = rest.clone();
                    state.pending.push_back(CommandKind::SetOperationGeometry { name: target.to_owned(), items: rest });
                }
            }
        }
    }
    if ui.small_button("Clear").on_hover_text("drop every pick - the stock bounds the operation again").clicked() {
        state.pending.push_back(CommandKind::EditOperationLinks {
            name: target.to_owned(),
            tool: None,
            src: Some(SrcRef::Stock),
        });
    }
}

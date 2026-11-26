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

use crate::shortcut_keys::{chord, command_key};
use crate::state::{CommandKind, ErrorScope, GeoPick, State};

use super::colorvision::text_inks;
use super::layout::group;
use super::theme::{Tokens, clamp};

/// The Edge mode's one-line hint: how a loop gets closed. The add key is
/// the platform command key (Cmd on macOS), the one the pick reads
/// (`mods.command`, #1238).
pub fn edge_hint() -> String {
    format!("{} adds an edge · Shift+click picks the whole loop", chord("click"))
}

/// The "Click selects" hover: what each modifier does to a viewport pick.
pub fn click_selects_hint() -> String {
    format!(
        "Click in the viewport - {key} adds; in Edge mode Shift takes the whole loop and Alt one edge. \
         {key}+drag adds every visible edge (Edge) or face (Face) in a box - hidden ones too with Pick through on: drag right for what lies inside it, left for what it touches.",
        key = command_key()
    )
}

/// #1514: the plain "picking on / off" line under the tab bar. On: viewport
/// clicks edit this operation's picks (the Geometry tab, or a boundary row
/// armed). Off: every other tab - the selection is locked, clicks do nothing.
pub(super) fn pick_mode_line(ui: &mut Ui, state: &State) {
    if crate::interact::op_pick_locked(state) {
        ui.label(RichText::new("Picking off - selection locked").weak());
    } else {
        ui.label(RichText::new("Picking on - clicks edit the selection").strong());
    }
}

pub(super) fn pick_block(ui: &mut Ui, state: &mut State, target: &str, live_setup: Option<&str>, pal: &Tokens) {
    group(ui, "Selection", None, |ui| body(ui, state, target, live_setup, pal));
}

fn body(ui: &mut Ui, state: &mut State, target: &str, live_setup: Option<&str>, pal: &Tokens) {
    // Viewport pick mode - transient view state: the mode itself is not
    // undoable; what a Face/Edge click changes is.
    ui.horizontal_wrapped(|ui| {
        ui.label("Click selects:").on_hover_text(click_selects_hint());
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
        ui.label(RichText::new(edge_hint()).weak());
    }
    if state.op_ui.src_kind == 2 {
        crate::frame_cost::time("ui/properties.op.picks.chips", || chips(ui, state, target, pal));
        // #187: the 3-2-1 alignment reads the picks as datums - a flat
        // face for Z, a straight edge for X, a vertex or bore for the
        // origin. The app builds the frame (or names what to re-pick).
        let aligned = crate::frame_cost::time("ui/properties.op.picks.align_321", || {
            crate::vertex_pick::wcs_321_checked(state)
        });
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
    let (notes, refusal) = crate::frame_cost::time("ui/properties.op.picks.selection_account", || {
        operation::selection_account(&state.world, live_setup, &state.op_ui.geo.clone(), &state.op_ui.params.clone())
    });
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
    let sketches = camcraft_app::sketch::placed_in(&state.world, live_setup);
    if !sketches.is_empty() {
        crate::frame_cost::time("ui/properties.op.picks.sketch_layers", || {
            sketch_layer_picks(ui, state, target, &sketches);
        });
    }
}

fn sketch_layer_picks(ui: &mut Ui, state: &mut State, target: &str, sketches: &[String]) {
    let groups = crate::sketch_memo::sketch_layers(&state.world, sketches);
    if groups.is_empty() {
        return;
    }
    ui.add_space(4.0);
    for group in groups.iter() {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Sketch '{}' layers:", group.entity));
            for entry in &group.layers {
                let sel = GeoSel {
                    entity: group.entity.clone(),
                    feature: FeatureSel::SketchLayer(entry.layer.clone()),
                };
                let picked = state.op_ui.src_kind == 2 && state.op_ui.geo.contains(&sel);
                let resp = ui
                    .selectable_label(picked, &entry.label)
                    .on_hover_text("Toggle all cuttable chains on this DXF sketch layer");
                if resp.hovered() {
                    state.geo_hover = Some(sel.clone());
                }
                if resp.clicked() {
                    let mut items = if state.op_ui.src_kind == 2 {
                        state.op_ui.geo.clone()
                    } else {
                        Vec::new()
                    };
                    if let Some(idx) = items.iter().position(|g| *g == sel) {
                        items.remove(idx);
                    } else {
                        items.push(sel);
                    }
                    if items.is_empty() {
                        state.op_ui.geo.clear();
                        state.op_ui.src_kind = 0;
                        state.pending.push_back(CommandKind::EditOperationLinks {
                            name: target.to_owned(),
                            tool: None,
                            src: Some(SrcRef::Stock),
                        });
                    } else {
                        state.op_ui.geo = items.clone();
                        state.op_ui.src_kind = 2;
                        state.pending.push_back(CommandKind::SetOperationGeometry {
                            name: target.to_owned(),
                            items,
                        });
                    }
                }
            }
        });
    }
}

/// The picked features, one chip each: hovering lights the feature in
/// the viewport (the shared interact law), x removes it - the last one
/// falls back to the stock source.
fn chips(ui: &mut Ui, state: &mut State, target: &str, pal: &Tokens) {
    let chips: Vec<(String, bool, GeoSel)> =
        state.geo_chips().into_iter().map(|c| (c.label, c.missing, c.sel)).collect();
    for (i, (label, missing, sel)) in chips.into_iter().enumerate() {
        let vertex = match &sel.feature {
            FeatureSel::Vertex(id) if !missing => Some((sel.entity.clone(), *id)),
            _ => None,
        };
        // A picked face (or welded curved wall) offers its centroid; a
        // flat one also offers Z only. The targets are the app's to say.
        let face = match &sel.feature {
            FeatureSel::Face(_) | FeatureSel::Patch(_) if !missing => {
                camcraft_app::pick::face_origin(&state.world, &sel.entity, sel.feature.clone())
                    .ok()
                    .map(|t| (sel.entity.clone(), sel.feature.clone(), t))
            }
            _ => None,
        };
        let row = ui
            .horizontal(|ui| {
                // #1688: the remove button comes first, so it stays in reach
                // however long the label is or however narrow the panel.
                let x = ui.small_button("×").on_hover_text("remove this pick");
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
                        crate::vertex_pick::queue_origin_at_face(state, entity, feat.clone(), false);
                    }
                    if t.plane_z.is_some()
                        && ui
                            .small_button("Set origin (Z only)")
                            .on_hover_text("keep the origin's X and Y; move Z to this flat face")
                            .clicked()
                    {
                        crate::vertex_pick::queue_origin_at_face(state, entity, feat.clone(), true);
                    }
                }
                // The label takes the width left and is shortened to fit
                // (#1688); hovering a shortened label shows it whole.
                let text = if missing {
                    RichText::new(&label).color(text_inks(ui, &state.world).missing)
                } else {
                    RichText::new(&label)
                };
                let resp = ui.add(egui::Label::new(text).truncate());
                // #1507: the label, the remove button and the row's other
                // buttons all light the pick: "which one am I removing?".
                if resp.hovered() || x.hovered() {
                    state.geo_hover = Some(sel.clone());
                }
                x.clicked()
            });
        // #1519: the viewport pointer over this pick's face, chain or vertex
        // marks its row (an outline in the hover ink); gone with the pointer.
        if state.hover.as_ref().is_some_and(|h| h.entity == sel.entity && h.feature.as_ref() == Some(&sel.feature)) {
            let ink = pal.selection_stroke;
            let r = pal.radius_widget.min(clamp::RADIUS);
            let w = clamp::range(pal.highlight_stroke_px, clamp::STROKE);
            ui.painter().rect_stroke(row.response.rect, r, egui::Stroke::new(w, ink), egui::StrokeKind::Inside);
            state.geo_row_marked.push(sel.clone());
        }
        if row.response.hovered() || ui.rect_contains_pointer(row.response.rect) {
            state.geo_hover = Some(sel);
        }
        let remove = row.inner;
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

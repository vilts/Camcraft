//! #2397: the tool shape editor's Profile section (#93) - a milling tool's
//! holder as a spun (revolved) half-outline - Parametric (the cylinder its
//! numbers state) or Custom (a table of points with optional arcs), the
//! live refusal by name, and Apply profile, which queues the one command.
//! #3930: the holder library left this window: the tool library window's
//! assembly group picks the holder (`tool_holder_picker`). #2007: the tool has one
//! preview - the to-scale one beside the fields (`tool_preview`), which
//! draws the draft of this tab through `tool_preview_inputs`. The shape
//! window draws the flat side view itself (`profile_view`). #93 slice 2:
//! Draw cutter switches the tab to the tool's own cutter, neck and shank
//! (`tool_cutter_page`). Turning tools are not spun profiles: for them
//! the editor stays the Body section alone (owner, #93).
//! Provenance: our own pane over `app::tool_body_view` and
//! `app::spun`. No vendor material consulted.

use egui::{Align2, Color32, FontId, Pos2, Sense, Shape, Stroke, Ui, Vec2};

use camcraft_app::spun::{SpunPoint, SpunProfile};
use camcraft_app::tool_body_view;
use camcraft_app::tools::Tool;
use camcraft_app::units::{self, Unit};

use crate::state::{State, ToolColumn, is_spun_kind};

/// The side view's size, logical px.
const VIEW: Vec2 = Vec2::new(260.0, 230.0);
const NOTE: Color32 = Color32::from_rgb(0x9E, 0xA8, 0xB8);
const HOLDER: Color32 = Color32::from_rgb(230, 160, 60);
pub(super) const BAD: Color32 = Color32::from_rgb(230, 90, 80);
/// #2007: the points table's height. It is also its least height, so a
/// table low on a scrolled page does not shrink to a few rows.
const TABLE_H: f32 = 150.0;

/// #2397: the tool shape editor's body - the Body / Profile tabs for a
/// milling tool, the Body section alone for a turning tool. #3930: the
/// holder pick is the tool library window's (its assembly group).
pub(super) fn editor(ui: &mut Ui, state: &mut State, exists: bool, u: Unit) {
    let milling = is_spun_kind(state.tool_ui.tool_kind());
    if milling {
        state.sync_tool_profile_ui();
        // #2007: the tab row wraps in a narrow pane.
        ui.horizontal_wrapped(|ui| {
            let c = state.tool_profile_ui.column;
            if ui.selectable_label(c == ToolColumn::Body, "Body").on_hover_text("import a holder or assembly mesh").clicked() {
                state.tool_profile_ui.column = ToolColumn::Body;
            }
            if ui
                .selectable_label(c == ToolColumn::Profile, "Profile")
                .on_hover_text("draw the holder as a revolved outline")
                .clicked()
            {
                state.tool_profile_ui.column = ToolColumn::Profile;
            }
            // #93 slice 2: which outline the Profile tab draws - on the
            // tab row, so the holder view keeps its height.
            if state.tool_profile_ui.column == ToolColumn::Profile {
                ui.separator();
                let cutter = state.tool_cutter_ui.shown;
                if ui.selectable_label(!cutter, "Draw holder").on_hover_text("the holder as a revolved outline").clicked() {
                    state.tool_cutter_ui.shown = false;
                }
                if ui
                    .selectable_label(cutter, "Draw cutter")
                    .on_hover_text("the tool's own cutter, neck and shank as a revolved outline")
                    .clicked()
                {
                    state.tool_cutter_ui.shown = true;
                }
            }
        });
        ui.separator();
    }
    if milling && state.tool_profile_ui.column == ToolColumn::Profile {
        profile_section(ui, state, exists, u);
    } else {
        super::tool_body_page::body_section(ui, state, exists, u);
    }
}

fn profile_section(ui: &mut Ui, state: &mut State, exists: bool, u: Unit) {
    if state.tool_cutter_ui.shown {
        super::tool_cutter_page::cutter_section(ui, state, exists, u);
        return;
    }
    if !exists {
        ui.strong("Holder profile");
        ui.label("Add the tool first, then draw its holder.");
        return;
    }
    state.sync_tool_profile_ui();
    // #2397: two columns - the outline controls on the left, the points
    // and the to-scale side view on the right. The window is wide enough
    // for both, so no row or button is clipped at the bottom.
    ui.horizontal_top(|ui| {
        ui.set_width(240.0);
        ui.vertical(|ui| profile_controls(ui, state));
        ui.separator();
        ui.vertical(|ui| profile_view(ui, state, u));
    });
}

/// The outline's controls: what landed, Parametric / Custom, Apply
/// profile, and the DXF / STEP doors.
fn profile_controls(ui: &mut Ui, state: &mut State) {
    ui.strong("Holder profile");
    let landed = state.editor_tool_body();
    // #2007: plain words - which holder the tool uses now.
    ui.label(match &landed {
        // #2016: a library holder is the entry, not a copy of it.
        Some(b) if b.library.is_some() => format!(
            "In use: library holder {}. Edit a point to make it a drawn outline.",
            b.library.as_ref().map_or("", |r| r.entry.as_str())
        ),
        Some(b) if b.spun.is_some() => "In use: a drawn holder outline".to_owned(),
        Some(b) => format!("In use: the imported holder {}. Apply profile replaces it.", b.source),
        None => "In use: the holder cylinder from the dimensions".to_owned(),
    });
    let custom = state.tool_profile_ui.custom;
    ui.horizontal(|ui| {
        if ui
            .selectable_label(!custom, "Parametric")
            .on_hover_text("the holder is a cylinder with the holder diameter and length, redrawn as they change")
            .clicked()
        {
            state.set_profile_custom(false);
        }
        if ui.selectable_label(custom, "Custom").on_hover_text("edit the outline point by point; changed dimensions leave it alone").clicked() {
            state.set_profile_custom(true);
        }
    });
    // #2007/#2397: the points table, the side view and the live refusal
    // moved into `profile_view`, drawn in the shape window.
    let state_custom = state.tool_profile_ui.custom;
    let can = state_custom || landed.is_some();
    if ui
        .add_enabled(can, egui::Button::new("Apply profile"))
        .on_hover_text(if state_custom {
            "use the drawn holder for this tool (one undo step)"
        } else {
            "remove the holder in use: the holder cylinder from the dimensions is checked again (one undo step)"
        })
        .clicked()
    {
        state.queue_apply_tool_profile();
    }
    ui.small("The holder is a turned shape: you edit one half of its outline. Parts that are not round are covered by a larger safe shape, never cut away.");
    let has_export = !state.tool_profile_ui.points.is_empty() || landed.is_some();
    // #2007: the three file buttons wrap in a narrow column.
    let (imp_dxf, exp_dxf, imp_step) = ui
        .horizontal_wrapped(|ui| {
            let imp_dxf = crate::browser_files::button(
                ui,
                state.in_browser(crate::browser_files::NEEDS_PICKER),
                "Import DXF…",
                "read an ASCII DXF with a CENTRELINE layer and one open half-profile into Custom. The centreline must be parallel to Y. DXF +Y is tool +Z; z = 0 is the lower end of the centreline, independent of draw direction",
            );
            let exp_dxf = if has_export {
                crate::browser_files::button(
                    ui,
                    state.in_browser(crate::browser_files::NO_EXPORT),
                    "Export DXF…",
                    "write the half-profile or holder body as ASCII DXF (mm, layers CENTRELINE and PROFILE)",
                )
            } else {
                ui.add_enabled(false, egui::Button::new("Export DXF…")).clicked()
            };
            let imp_step = crate::browser_files::button(
                ui,
                state.in_browser(crate::browser_files::NEEDS_PICKER),
                "Import STEP…",
                "convert an ISO 13399 aligned STEP holder into a conservative editable half-profile",
            );
            (imp_dxf, exp_dxf, imp_step)
        })
        .inner;
    if imp_dxf {
        state.import_profile_dxf_dialog();
    }
    if exp_dxf {
        state.export_profile_dxf_dialog();
    }
    if imp_step {
        state.import_step_profile_dialog();
    }
}

/// The points table and the flat side view, with the live refusal.
fn profile_view(ui: &mut Ui, state: &mut State, u: Unit) {
    points_table(ui, state, u);
    let tool = state.tool_ui.to_tool();
    let points = state.tool_profile_ui.points.clone();
    let check = if points.is_empty() { Err("No holder: the tool has none.".to_owned()) } else { tool_body_view::spun_preview(&tool, &points) };
    let cutter = camcraft_app::tool_cutter::shown(&tool, state.editor_tool_cutter().as_ref()).map(|c| c.outline()).unwrap_or_default();
    let holder: Vec<(f64, f64)> = check.as_ref().map_or_else(|_| points.iter().map(|p| (p.z, p.r)).collect(), SpunProfile::outline);
    side_view(ui, &tool, (&cutter, true), (&holder, check.is_ok()), u);
    match &check {
        Ok(s) => {
            state.retire_profile_refusal();
            let top = s.points().last().map_or(0.0, |p| p.z);
            let foot = s.points().first().map_or(0.0, |p| p.z);
            let wide = s.max_radius();
            let mm = u.suffix();
            ui.label(format!(
                "holder Ø {} × {} {mm}, from {} to {} {mm} above the tip",
                units::len(2.0 * wide, u, 1),
                units::len(top - foot, u, 1),
                units::len(foot, u, 1),
                units::len(top, u, 1),
            ));
        }
        Err(e) => {
            ui.colored_label(BAD, e);
        }
    }
}

/// The points, tip-up: z, r, arc radius (0 = line), insert after,
/// remove. Read-only in Parametric.
fn points_table(ui: &mut Ui, state: &mut State, u: Unit) {
    let custom = state.tool_profile_ui.custom;
    let (insert, remove) = points_grid(ui, u, &mut state.tool_profile_ui.points, custom, "tool_profile_points");
    if let Some(i) = insert {
        state.insert_profile_point(i);
    }
    if let Some(i) = remove {
        state.remove_profile_point(i);
    }
}

/// The editable points grid both drawn outlines share: returns the row
/// to insert after and the row to remove, when clicked.
pub(super) fn points_grid(ui: &mut Ui, u: Unit, points: &mut [SpunPoint], editable: bool, id: &str) -> (Option<usize>, Option<usize>) {
    let n = points.len();
    let mut insert = None;
    let mut remove = None;
    // #2007: the least height keeps the rows in view when the table sits
    // low in the scrolled page - without it egui shrinks a nested scroll
    // area to the space left in the view, about two rows.
    egui::ScrollArea::vertical().max_height(TABLE_H).min_scrolled_height(TABLE_H).id_salt(id).show(ui, |ui| {
        ui.add_enabled_ui(editable, |ui| {
            // #2007: no 40 px least column width - with it the six columns
            // overran the 260 px column and pushed "×" out of a narrow pane.
            egui::Grid::new((id, "grid")).num_columns(6).min_col_width(0.0).spacing([4.0, 2.0]).show(ui, |ui| {
                let heads = [
                    ("#".to_owned(), "the point number, from the tip up"),
                    (format!("height {}", u.suffix()), "the height of the point above the tool tip"),
                    (format!("radius {}", u.suffix()), "the distance of the point from the tool axis"),
                    ("arc".to_owned(), "the arc radius of this point; 0 is a straight line"),
                    (String::new(), ""),
                    (String::new(), ""),
                ];
                for (h, tip) in heads {
                    if tip.is_empty() {
                        ui.label(h);
                    } else {
                        ui.label(h).on_hover_text(tip).on_disabled_hover_text(tip);
                    }
                }
                ui.end_row();
                for (i, p) in points.iter_mut().enumerate() {
                    ui.label(format!("{}", i + 1));
                    len_cell(ui, u, &mut p.z);
                    len_cell(ui, u, &mut p.r);
                    len_cell(ui, u, &mut p.arc);
                    if i + 1 < n && ui.small_button("+").on_hover_text("insert a point after this one").clicked() {
                        insert = Some(i);
                    } else if i + 1 >= n {
                        ui.label("");
                    }
                    if ui.small_button("×").on_hover_text("remove this point").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        });
    });
    (insert, remove)
}

/// A compact unit-lens length cell (the `fields::num` law, narrower).
pub(super) fn len_cell(ui: &mut Ui, u: Unit, v: &mut f64) {
    let w = [54.0, ui.spacing().interact_size.y];
    if u == Unit::Millimeters {
        ui.add_sized(w, egui::DragValue::new(v).speed(0.1));
    } else {
        let mut d = units::to_display(*v, u);
        if ui.add_sized(w, egui::DragValue::new(&mut d).speed(0.1 / 25.4)).changed() {
            *v = units::from_display(d, u);
        }
    }
}

/// The flat side view: the cutter, neck and shank outline and the
/// holder outline, each with whether it validates (filled when it
/// does, a red outline when not), to scale, tip at the bottom, with
/// the key dimensions. #2397: the shape window's own view - #2007's
/// one to-scale preview stands in the property panel.
pub(super) fn side_view(ui: &mut Ui, tool: &Tool, cutter: (&[(f64, f64)], bool), holder: (&[(f64, f64)], bool), u: Unit) {
    const PAD: f32 = 16.0;
    let (resp, painter) = ui.allocate_painter(VIEW, Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, 4.0, Color32::from_rgb(24, 26, 31));
    let ((cutter, cutter_ok), (holder, ok)) = (cutter, holder);
    let all = || cutter.iter().chain(holder.iter()).filter(|q| q.0.is_finite() && q.1.is_finite());
    let r_max = all().fold(0.0f64, |a, q| a.max(q.1));
    let z_max = all().fold(0.0f64, |a, q| a.max(q.0));
    if r_max <= 0.0 || z_max <= 0.0 {
        painter.text(rect.center(), Align2::CENTER_CENTER, "enter dimensions", FontId::proportional(11.0), NOTE);
        return;
    }
    // The tool sits left of centre; the right margin holds the labels.
    let axis_x = rect.min.x + rect.width() * 0.38;
    let base_y = rect.max.y - PAD;
    let scale = ((rect.width() * 0.34) / r_max as f32).min((rect.height() - 2.0 * PAD) / z_max as f32).max(f32::EPSILON);
    let at = |z: f64, r: f64| Pos2::new(axis_x + r as f32 * scale, base_y - z as f32 * scale);
    painter.line_segment([Pos2::new(axis_x, rect.min.y + 4.0), Pos2::new(axis_x, rect.max.y - 4.0)], Stroke::new(1.0, Color32::from_rgb(70, 74, 84)));
    let zc = tool.flute_len.max(0.0);
    let slices = |o: &[(f64, f64)], fill: &dyn Fn(f64) -> Color32| {
        for w in o.windows(2) {
            let ((z0, r0), (z1, r1)) = (w[0], w[1]);
            if z1 > z0 && [z0, z1, r0, r1].iter().all(|v| v.is_finite()) {
                painter.add(Shape::convex_polygon(vec![at(z0, -r0), at(z0, r0), at(z1, r1), at(z1, -r1)], fill((z0 + z1) * 0.5), Stroke::NONE));
            }
        }
    };
    if cutter_ok {
        slices(cutter, &|zm| if zm < zc { Color32::from_rgb(168, 178, 192) } else { Color32::from_rgb(122, 130, 142) });
    }
    if ok {
        slices(holder, &|_| Color32::from_rgba_unmultiplied(230, 160, 60, 90));
    }
    let outline = |o: &[(f64, f64)], ink: Color32| {
        let mut pts: Vec<Pos2> = o.iter().map(|&(z, r)| at(z, r)).collect();
        pts.extend(o.iter().filter(|q| q.1 > 0.0).rev().map(|&(z, r)| at(z, -r)));
        if pts.len() >= 2 {
            painter.add(Shape::closed_line(pts, Stroke::new(1.0, ink)));
        }
    };
    outline(cutter, if cutter_ok { Color32::from_rgb(214, 220, 228) } else { BAD });
    outline(holder, if ok { HOLDER } else { BAD });
    // Heights: flute top, the holder's foot (stickout) and top (gauge).
    let small = FontId::proportional(10.0);
    let tick = |z: f64, what: &str| {
        if z.is_finite() && z > 0.0 {
            let y = base_y - z as f32 * scale;
            painter.line_segment([Pos2::new(axis_x + 4.0, y), Pos2::new(rect.max.x - 4.0, y)], Stroke::new(0.5, Color32::from_rgb(80, 86, 98)));
            painter.text(Pos2::new(rect.max.x - 4.0, y - 1.0), Align2::RIGHT_BOTTOM, format!("{what} {}", units::len(z, u, 1)), small.clone(), NOTE);
        }
    };
    tick(zc, "flutes");
    if let (Some(foot), Some(top)) = (holder.first().map(|q| q.0), holder.last().map(|q| q.0)) {
        tick(foot, "stickout");
        tick(top, "gauge");
    } else {
        tick(zc + tool.shank_len.max(0.0), "shank");
    }
    painter.text(Pos2::new(axis_x, rect.max.y - 2.0), Align2::CENTER_BOTTOM, format!("Ø {}", units::len(tool.diameter, u, 2)), small.clone(), NOTE);
    painter.text(
        rect.min + Vec2::new(6.0, 4.0),
        Align2::LEFT_TOP,
        format!("shank Ø {} {}", units::len(tool.shank_d, u, 2), u.suffix()),
        small,
        NOTE,
    );
}


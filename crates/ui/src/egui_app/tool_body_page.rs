//! The tool editor's Body section (#371): Import body… / Clear body, the
//! placement (axis preset or picked face normal, tip offset), Pick tip /
//! Pick axis on a small orbitable 3D preview of the mesh in its own
//! frame, and Apply body - which queues the one `SetToolBody`. The
//! preview is painted with egui (flat-shaded, depth-sorted triangles),
//! so it needs no GPU and runs under the test harness. Picks and orbit
//! are ephemeral; only Apply body / Clear body reach the history.
//! Provenance: our own pane over `app::tool_body_view` (orthographic
//! projection and ray casting from general graphics practice); no vendor
//! material consulted.

use egui::{Color32, ComboBox, Mesh, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

use camcraft_app::geometry::Triangle;
use camcraft_app::tool_body_view::{self, AXIS_CHOICES};
use camcraft_app::units::{self, Unit};

use super::fields::{field, num};
use crate::state::{BodyPickMode, State};

/// The preview's side, logical px.
const SIDE: Vec2 = Vec2::new(260.0, 200.0);
/// Past this many facets the preview paints every n-th one (picking
/// still reads them all) - a vendor STEP can carry 10^5+.
const MAX_PAINTED: usize = 30_000;
/// The default view: a three-quarter look from +X-Y, above.
const YAW0: f32 = -0.9;
const PITCH0: f32 = 0.45;

/// The envelope bands of the draft's placed body, for the profile
/// preview: `(above, len, r)` per band, or `None` with no usable draft.
/// Re-mirrors the world's body first (a no-op unless it changed), so
/// the preview follows Apply profile, undo and reload whichever tab
/// shows - not only once the Body tab has been drawn.
pub(super) fn envelope(state: &mut State) -> Option<Vec<(f64, f64, f64)>> {
    state.sync_tool_body_ui();
    match state.tool_body_preview()? {
        Ok(b) => Some(b.profile()?.bands().iter().map(|q| (q.above, q.len, q.r)).collect()),
        Err(_) => None,
    }
}

/// The section, the editor's third column beside the profile preview. A new (unsaved) tool
/// takes no body until it exists - the command names an existing tool.
pub(super) fn body_section(ui: &mut Ui, state: &mut State, exists: bool, u: Unit) {
    ui.strong("Tool body");
    if !exists {
        ui.label("Add the tool first, then import its holder or assembly.");
        return;
    }
    state.sync_tool_body_ui();
    let landed = state.editor_tool_body().map(|b| b.source);
    let draft_src = state.tool_body_ui.source.clone();
    let has_draft = !state.tool_body_ui.facets.is_empty();
    ui.label(match (&landed, has_draft) {
        (Some(src), _) if *src == draft_src => format!("body: {src}"),
        (_, true) => format!("draft: {draft_src} (not applied)"),
        _ if state.tool_body_ui.lathe.is_some() => "none - the holder is not judged".to_owned(),
        _ => "none - the holder cylinder is judged".to_owned(),
    });
    // #361: a boring bar chooses its frame; a lathe steel's is the
    // turning tool frame.
    super::tool_body_lathe_page::frame_row(ui, state);
    let (import, clear) = ui
        .horizontal(|ui| {
            let import = crate::browser_files::button(
                ui,
                state.in_browser(crate::browser_files::NEEDS_PICKER),
                "Import body…",
                "read a STEP, STL or OBJ holder/assembly; place it, then Apply body",
            );
            let clear = ui
                .add_enabled(landed.is_some() || has_draft, egui::Button::new("Clear body"))
                .on_hover_text("remove the body - the holder cylinder is judged again (undoable)")
                .clicked();
            (import, clear)
        })
        .inner;
    if import {
        state.import_tool_body_dialog();
    }
    if clear {
        state.clear_tool_body();
    }
    if state.tool_body_ui.facets.is_empty() {
        return;
    }
    if state.tool_body_ui.lathe.is_some() {
        super::tool_body_lathe_page::section(ui, state, u);
        super::tool_body_assembly_page::section(ui, state);
        return;
    }
    let old_axis = state.tool_body_ui.axis;
    let old_offset = state.tool_body_ui.offset;
    field(ui, "Tool axis (model direction, tip to spindle)");
    axis_combo(ui, &mut state.tool_body_ui.axis);
    field(ui, "Tip offset (model origin above the tip)");
    num(ui, u, &mut state.tool_body_ui.offset);
    if state.tool_body_ui.axis != old_axis || (state.tool_body_ui.offset - old_offset).abs() > 1e-9 {
        state.tool_body_ui.align_note = None;
    }
    if let Some(ref note) = state.tool_body_ui.align_note {
        ui.label(note);
    }
    let pick = state.tool_body_ui.pick;
    ui.horizontal(|ui| {
        if super::pick_button::pick_button(ui, pick == BodyPickMode::Tip, "Pick tip")
            .on_hover_text("click the body in the preview: the nearest corner becomes the tool tip")
            .clicked()
        {
            state.tool_body_ui.pick = if pick == BodyPickMode::Tip { BodyPickMode::Off } else { BodyPickMode::Tip };
        }
        if super::pick_button::pick_button(ui, pick == BodyPickMode::Axis, "Pick axis")
            .on_hover_text("click a face that looks toward the spindle: its normal becomes the tool axis")
            .clicked()
        {
            state.tool_body_ui.pick = if pick == BodyPickMode::Axis { BodyPickMode::Off } else { BodyPickMode::Axis };
        }
        if ui.button("Fit").on_hover_text("reset the preview's orbit and zoom (or double-click it)").clicked() {
            state.fit_body_preview();
        }
    });
    preview_3d(ui, state);
    let note = match state.tool_body_preview() {
        Some(Ok(b)) => match b.profile() {
            Some(p) => format!(
                "placed: foot {} to top {} {} above the tip",
                units::len(p.bottom(), u, 1),
                units::len(p.top(), u, 1),
                u.suffix()
            ),
            None => String::new(),
        },
        Some(Err(e)) => format!("placement: {e}"),
        None => String::new(),
    };
    ui.label(note);
    if ui.button("Apply body").on_hover_text("attach the body with this placement (one undo step)").clicked() {
        state.queue_apply_tool_body();
    }
    super::tool_body_assembly_page::section(ui, state);
}

fn axis_combo(ui: &mut Ui, axis: &mut [f64; 3]) {
    let text = tool_body_view::axis_label(*axis)
        .map_or_else(|| format!("picked ({:.2}, {:.2}, {:.2})", axis[0], axis[1], axis[2]), str::to_owned);
    ComboBox::from_id_salt("tool_body_axis").selected_text(text).show_ui(ui, |ui| {
        for (name, d) in AXIS_CHOICES {
            if ui.selectable_label(tool_body_view::axis_label(*axis) == Some(name), name).clicked() {
                *axis = d;
            }
        }
    });
}

/// The orthographic view: a center, a scale, and the right/up/forward
/// basis (forward points into the screen).
struct View {
    c: [f64; 3],
    s: f64,
    r: [f64; 3],
    up: [f64; 3],
    f: [f64; 3],
    mid: Pos2,
    reach: f64,
}

impl View {
    fn new(tris: &[Triangle], rect: Rect, d: &crate::state::ToolBodyUi) -> Option<Self> {
        let (yaw, pitch) = (d.yaw, d.pitch);
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for t in tris {
            for p in [t.a, t.b, t.c] {
                for (i, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                    if v.is_finite() {
                        lo[i] = lo[i].min(v);
                        hi[i] = hi[i].max(v);
                    }
                }
            }
        }
        // The model origin is always in view: the axis and tip live there.
        for i in 0..3 {
            lo[i] = lo[i].min(0.0);
            hi[i] = hi[i].max(0.0);
        }
        let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
        let reach = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt() * 0.5;
        if !(reach.is_finite() && reach > 1e-9) {
            return None;
        }
        let (y, p) = (f64::from(YAW0 + yaw), f64::from((PITCH0 + pitch).clamp(-1.5, 1.5)));
        let f = [-(p.cos() * y.cos()), -(p.cos() * y.sin()), -p.sin()];
        let r = norm([f[1], -f[0], 0.0]).unwrap_or([1.0, 0.0, 0.0]);
        let up = cross(r, f);
        let zoom = f64::from(crate::state::clamp_zoom(d.zoom, 0.0)).exp2();
        let s = f64::from(rect.width().min(rect.height())) * 0.45 / reach * zoom;
        let pan = if d.pan.iter().all(|v| v.is_finite()) { d.pan } else { [0.0; 2] };
        let c = [0, 1, 2].map(|i| c[i] + r[i] * pan[0] + up[i] * pan[1]);
        Some(Self { c, s, r, up, f, mid: rect.center(), reach })
    }

    fn screen(&self, q: [f64; 3]) -> Pos2 {
        let d = [q[0] - self.c[0], q[1] - self.c[1], q[2] - self.c[2]];
        Pos2::new(
            self.mid.x + (dot(d, self.r) * self.s) as f32,
            self.mid.y - (dot(d, self.up) * self.s) as f32,
        )
    }

    fn depth(&self, q: [f64; 3]) -> f64 {
        dot([q[0] - self.c[0], q[1] - self.c[1], q[2] - self.c[2]], self.f)
    }

    /// Screen point `at` as view units (right, up) off the view centre.
    fn offset(&self, at: Pos2) -> [f64; 2] {
        [f64::from(at.x - self.mid.x) / self.s, -f64::from(at.y - self.mid.y) / self.s]
    }

    /// The ray under screen point `at`: from in front of the body, into it.
    fn ray(&self, at: Pos2) -> ([f64; 3], [f64; 3]) {
        let (dx, dy) = (f64::from(at.x - self.mid.x) / self.s, -f64::from(at.y - self.mid.y) / self.s);
        let back = 4.0 * self.reach;
        let o = [0, 1, 2].map(|i| self.c[i] + self.r[i] * dx + self.up[i] * dy - self.f[i] * back);
        (o, self.f)
    }
}

pub(super) fn preview_3d(ui: &mut Ui, state: &mut State) {
    let (resp, painter) = ui.allocate_painter(SIDE, Sense::click_and_drag());
    let rect = resp.rect;
    painter.rect_filled(rect, 4.0, Color32::from_rgb(24, 26, 31));
    let d = &state.tool_body_ui;
    let Some(view) = View::new(&d.facets, rect, d) else {
        return;
    };
    // Painter's algorithm: far facets first, shaded by how squarely
    // they face the eye.
    let stride = d.facets.len().div_ceil(MAX_PAINTED).max(1);
    let mut order: Vec<(f64, &Triangle)> = d
        .facets
        .iter()
        .step_by(stride)
        .map(|t| (view.depth(centroid(t)), t))
        .filter(|(z, _)| z.is_finite())
        .collect();
    order.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut mesh = Mesh::default();
    for (_, t) in order {
        let [a, b, c] = [t.a, t.b, t.c].map(|p| [p.x, p.y, p.z]);
        let lit = norm(cross(sub(b, a), sub(c, a))).map_or(0.3, |n| dot(n, view.f).abs());
        let g = (60.0 + 150.0 * lit).clamp(0.0, 255.0) as u8;
        let col = Color32::from_rgb(g, g, g.saturating_add(12));
        let base = mesh.vertices.len() as u32;
        for q in [a, b, c] {
            mesh.colored_vertex(view.screen(q), col);
        }
        mesh.add_triangle(base, base + 1, base + 2);
    }
    painter.add(Shape::mesh(mesh));
    // The placement over it: the axis from the tip up (a lathe draft:
    // the tip with its radial and spindle directions), the tip, and
    // the model origin.
    if d.lathe.is_some() {
        super::tool_body_lathe_page::overlay(&painter, d, &|q| view.screen(q), view.reach);
    } else if let Some(a) = norm(d.axis) {
        let tip = [-d.offset * a[0], -d.offset * a[1], -d.offset * a[2]];
        let top = [0, 1, 2].map(|i| tip[i] + a[i] * view.reach * 2.0);
        painter.line_segment([view.screen(tip), view.screen(top)], Stroke::new(1.5, Color32::from_rgb(90, 200, 230)));
        painter.circle_filled(view.screen(tip), 4.0, Color32::from_rgb(240, 150, 40));
    }
    painter.circle_stroke(view.screen([0.0; 3]), 3.0, Stroke::new(1.0, Color32::from_rgb(160, 160, 170)));
    let mode = d.pick;
    if mode != BodyPickMode::Off {
        painter.text(
            rect.min + Vec2::new(6.0, 6.0),
            egui::Align2::LEFT_TOP,
            super::tool_body_lathe_page::pick_hint(mode),
            egui::FontId::proportional(10.0),
            Color32::from_rgb(240, 200, 120),
        );
    }
    // Zoom: the wheel or a pinch (Ctrl+wheel too) while hovered, about
    // the cursor; a double-click fits. View state only.
    if resp.hovered() {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
        let by = scroll * 0.004 + pinch.max(1e-3).log2();
        if by.abs() > 1e-6 {
            let about = resp.hover_pos().map_or([0.0; 2], |p| view.offset(p));
            state.zoom_body_preview(by, about);
        }
    }
    if resp.double_clicked() {
        state.fit_body_preview();
    }
    if resp.dragged() && mode == BodyPickMode::Off {
        let dv = resp.drag_delta();
        state.tool_body_ui.yaw -= dv.x * 0.01;
        state.tool_body_ui.pitch = (state.tool_body_ui.pitch + dv.y * 0.01).clamp(-1.9, 1.0);
    }
    if resp.clicked()
        && mode != BodyPickMode::Off
        && let Some(at) = resp.interact_pointer_pos()
    {
        let (o, dir) = view.ray(at);
        match tool_body_view::pick(&state.tool_body_ui.facets, o, dir) {
            Some(hit) => state.apply_body_pick(hit),
            None => state.status = "pick: no facet under the cursor".into(),
        }
    }
}

fn centroid(t: &Triangle) -> [f64; 3] {
    [(t.a.x + t.b.x + t.c.x) / 3.0, (t.a.y + t.b.y + t.c.y) / 3.0, (t.a.z + t.b.z + t.c.z) / 3.0]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = dot(a, a).sqrt();
    (n.is_finite() && n > 1e-12).then(|| [a[0] / n, a[1] / n, a[2] / n])
}

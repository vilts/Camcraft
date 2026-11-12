//! M-a: the graduated egui shell — the dock, the View3D-texture law and
//! the frame every shell draws, moved from the M1.4.1 spike binary into the
//! lib (docs/spike-egui-go-no-go.md, migration step M-a). The law is the
//! spike's, unchanged; what changed is where it lives and that the panes
//! are stubs that name their migration step.
//!
//! Since #31 this file holds only what any shell shares - `shell_layout`,
//! the dock tabs, the viewport panel, the simulation readouts and the
//! headless proof - and compiles for every target, wasm32 included. The
//! native window (winit's loop, the surface, GPU bring-up, the per-frame
//! draw) lives in `shell_window`, built only off wasm32.
//! Provenance: our own code, graduated from our own spike, specced from the
//! egui/egui-wgpu/egui_dock docs and vendored sources; no HSMWorks material
//! involved.

use egui_dock::{DockArea, DockState, Style as DockStyle, TabViewer};

// The unified-wgpu guarantee from the S0 resolution test: every wgpu type in
// this file names egui_wgpu's re-export, so Renderer and View3D cannot drift
// apart onto different wgpu majors.
use egui_wgpu::wgpu;

use crate::interact;
use crate::state::{DragMode, State};

use super::icons;
use super::layout;
use super::machine_editor;
use super::machines_pane;
use super::unsaved_prompt;
use super::pages;
use super::panes;
use super::post_refusal;
use super::theme;
use super::viewcube;

pub(crate) const VIEW3D_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Ribbon strip height: the cell box plus the caption row and the pad
/// (`theme::ribbon_h` = `cell_h + 26`), imgui's `height()` formula restated
/// as one derivation off the token - 61 at the dark defaults. The panel
/// sizes it live from `theme::current()`; this const is the test fixture's
/// number (a theme that grows the cells grows the ribbon with it).
#[cfg(test)]
pub(crate) const RIBBON_H: f32 = theme::ribbon_h(&theme::Tokens::DARK);
/// Status strip height (imgui's single-line bottom bar).
const STATUS_H: f32 = 22.0;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Tab {
    Viewport,
    Browser,
    Properties,
    Machines,
}

impl Tab {
    pub(crate) fn title(&self) -> &'static str {
        match self {
            Tab::Viewport => "Viewport",
            Tab::Browser => "Browser",
            Tab::Properties => "Properties",
            Tab::Machines => "Machines",
        }
    }
}

/// The default dock lives with the layout law (layout.rs); re-exported
/// here because the shell and its frame tests build it by this name.
// #31: only the native window and the tests build it today.
#[cfg_attr(target_arch = "wasm32", allow(unused_imports))]
pub(crate) use super::layout::build_dock;


/// The whole shell for one egui pass: keyboard shortcuts, the ribbon and
/// status strips, and the three-leaf dock. Shared by the live frame (draw)
/// and the frame tests (tests.rs) so the two can never disagree about
/// layout; the borrow order is deliberate - each pane reborrows `state` in
/// sequence, the dock tabs only after the strips end.
pub(crate) fn shell_layout(
    ui: &mut egui::Ui,
    state: &mut State,
    dock: &mut DockState<Tab>,
    tex: egui::TextureId,
    icons: Option<&icons::IconSet>,
    viewport_points: &mut [f32; 2],
    left_press: &mut Option<[f32; 2]>,
) {
    // The document's theme lands here, on the frame edge (M6.1 C): the
    // world singleton is the truth (command law - undo, the picker, and
    // a project load all move it between frames), and resolution lives
    // on this side of the seam (layering). The per-frame cost is a
    // clone and an equality test; the parse and the style swap happen
    // only on the change edge, before `current()` is read below.
    state.sync_theme(ui.ctx());
    // Ephemeral hover clears every frame before anything recomputes it (the
    // imgui App::draw law); the viewport's update_hover and the property-page
    // chip hovers then own it until the pointer moves on.
    state.geo_hover = None;
    panes::shortcuts(ui, state);
    // The live theme, read once per pass: the strips and every pane below
    // share one token table, so the widget path and the painted ink cannot
    // disagree on a retheme (M6.1 A). `ribbon_h` tracks the cell box.
    let pal = theme::current();
    // The View door's checkmarks read the dock as it stands; a toggle the
    // door files lands right after the ribbon, before the dock lays out.
    layout::settle(state, dock);
    egui::Panel::top("ribbon")
        .frame(egui::Frame::NONE)
        .exact_size(theme::ribbon_h(&pal))
        .show(ui, |ui| {
            panes::ribbon(ui, &mut *state, &pal, icons);
        });
    layout::settle(state, dock);
    egui::Panel::bottom("status")
        .frame(egui::Frame::NONE)
        .exact_size(STATUS_H)
        .show(ui, |ui| {
            panes::status(ui, &mut *state, &pal, icons);
        });
    // #667: the Setups page pick sweep runs here, once a frame, whatever
    // the dock shows. Viewport clicks route inside the dock (egui_dock
    // draws leaves breadth-first, so in the default layout the viewport
    // draws before the properties leaf), so the sweep brackets it:
    // a properties pane that will not draw this frame - closed, or behind
    // another tab in its leaf - disarms every pick BEFORE the dock, and a
    // drawn pane's rows mark their setups during it, the rest disarmed
    // AFTER it. A pick left armed at a frame's end therefore had its rows
    // on screen that frame, and the next frame's click sees the result:
    // no frame where a hidden draft takes a click.
    if !layout::is_shown(dock, &Tab::Properties) {
        crate::axis_pick::disarm_all(state);
        // #673: the locate draft's form is on that pane too - dropped
        // here, before the dock routes a viewport release to it.
        crate::locate_pick::disarm(state);
    }
    state.locate_form_drawn = false;
    egui::CentralPanel::default().show(ui, |ui| {
        DockArea::new(&mut *dock)
            .style(DockStyle::from_egui(ui.style().as_ref()))
            // No per-area close-all: it is the X that took whole panes
            // away by accident. Tabs keep their own X where closable.
            .show_leaf_close_all_buttons(false)
            .show_inside(ui, &mut ShellTabs {
                state: &mut *state,
                tex,
                icons,
                viewport_points,
                left_press,
            });
    });
    // A locate draft whose form the dock did not draw is dropped (#566).
    crate::locate_pick::sweep(state);
    crate::axis_pick::disarm_hidden(state); // #664/#667: undrawn rows' picks stop
    // Last, so it floats over the dock: the M1.4 graphical tool editor.
    // An in-app egui::Window - this is the shell's only kind of extra
    // surface; the single-OS-window law stands. Closed by default
    // (`state.tool_editor` is None on a fresh State), so no panel below
    // it ever moves in the snapshot until something opens it.
    pages::tool_editor_window(ui.ctx(), state);
    // M1.7d: the G-code viewer - same window law as the tool editor (an
    // in-app egui::Window over the dock; the single-OS-window law
    // stands). None on a fresh State, closed by ✖, reopened by the next
    // successful post, so the snapshot never moves until a post happens.
    super::gcode_view::gcode_window(ui.ctx(), state);
    // #265: a refused post's reason, kept on screen past the status
    // line's next write. None on a fresh State, so no snapshot moves.
    post_refusal::window(ui.ctx(), state);
    // M3.3 stage C: the machine editor - same window law as both above
    // (in-app egui::Window over the dock, None on a fresh State, so
    // snapshots never move until the Setup pane opens it).
    machine_editor::window(ui.ctx(), state);
    // #157: the Debug info window - same window law (closed on a fresh
    // State, opened from the status strip's bug button).
    super::debug_info::window(ui.ctx(), state);
    // #339 slice 6: the fixture library dialogs (None on a fresh State).
    super::fixture_library_dialog::window(ui.ctx(), state);
    // #107: the unsaved-changes prompt, last so it sits over everything.
    unsaved_prompt::window(ui.ctx(), state);
}

/// Feed this pass's CPU-texture deltas (the font atlas first of all) to the
/// renderer. 0.36's TexturesDelta asserts on drop unless drained - consume,
/// don't peek (the S1 panic this replaced was exactly that).
/// egui-wgpu 0.36 binds one buffer slice per mesh, including meshes with
/// no vertices or indices. wgpu 30 rejects those zero-length slices.
pub(crate) fn renderable_paint_job(job: &egui::ClippedPrimitive) -> bool {
    match &job.primitive {
        egui::epaint::Primitive::Mesh(mesh) =>
            !mesh.vertices.is_empty() && !mesh.indices.is_empty(),
        egui::epaint::Primitive::Callback(_) => true,
    }
}


struct ShellTabs<'a> {
    state: &'a mut State,
    tex: egui::TextureId,
    icons: Option<&'a icons::IconSet>,
    viewport_points: &'a mut [f32; 2],
    left_press: &'a mut Option<[f32; 2]>,
}

impl TabViewer for ShellTabs<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        // Title-derived: stable across frames, collision-free while the
        // dockspace holds one tab per title (true for the whole migration).
        egui::Id::new(tab.title())
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title().into()
    }

    /// The viewport is permanent: no X, no middle-click close, no
    /// context-menu close, and a float holding it cannot close either.
    /// The browser and the properties pane may close - the ribbon's
    /// View door reopens them (layout.rs).
    fn is_closeable(&self, tab: &Self::Tab) -> bool {
        layout::closeable(tab)
    }

    /// The viewport never floats: it stays in the main dock, so the
    /// main surface always has a root to reopen panels against.
    fn allowed_in_windows(&self, tab: &mut Self::Tab) -> bool {
        layout::closeable(tab)
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            Tab::Viewport => {
                viewport_panel(
                    ui,
                    self.state,
                    self.tex,
                    self.viewport_points,
                    self.left_press,
                );
            }
            Tab::Browser => {
                // Same seam, same table: every reader of the live theme
                // calls `current()` (M6.1 C makes it document state).
                let pal = theme::current();
                panes::browser(ui, self.state, &pal, self.icons);
            }
            Tab::Properties => {
                let pal = theme::current();
                pages::properties(ui, self.state, &pal);
            }
            Tab::Machines => {
                let pal = theme::current();
                machines_pane::draw(ui, self.state, &pal);
            }
        }
    }
}

/// The viewport tab: the 3D image, the pointer law, and the screen-space
/// overlays. M-e moved the law to `crate::interact`, where the retired
/// imgui shell had it first: a short LMB press picks (Ctrl adds, the
/// gesture table), an LMB drag past the click-travel threshold orbits, MMB
/// orbits, RMB pans, wheel dollies toward the cursor. egui's press/drag
/// bookkeeping differs from winit's raw stream, so the gestures are read
/// off `ui.input` directly, the same CLICK_TRAVEL_PX law applied by hand;
/// the spike's simplified mapping is retired.
fn viewport_panel(
    ui: &mut egui::Ui,
    state: &mut State,
    tex: egui::TextureId,
    viewport_points: &mut [f32; 2],
    left_press: &mut Option<[f32; 2]>,
) {
    // M1.7c move simulation: the timeline is rebuilt from the world on
    // every document change (`cached_script` below, keyed on
    // `doc_version`), which is why a document switch needs no reset -
    // the playhead simply indexes the new program. The transport
    // bar takes its strip BEFORE the image so `avail` measures what the 3D
    // view actually gets. Like the view cube and the hover hint, all of it
    // draws only while the ribbon cell is armed: a snapshot with no
    // simulation running sees zero churn. The playhead is immediate-mode
    // view state (AGENTS.md's explicit exemption), never a command.
    let mut sim_marker: Option<([f64; 3], [f64; 3])> = None;
    if state.sim.is_some() {
        let sc = cached_script(state);
        let total = sc.total.max(0.0);
        let unit = camcraft_app::units::of(&state.world);
        if let Some(sim) = state.sim.as_mut() {
            // Clamp a stalled frame's dt: a hiccup never teleports the
            // tool through material.
            sim.advance(f64::from(ui.input(|i| i.stable_dt).clamp(0.0, 0.1)), total);
        }
        let mut stop = false;
        ui.horizontal(|ui| {
            let Some(sim) = state.sim.as_mut() else {
                return;
            };
            let at_end = total > 0.0 && !sim.playing && sim.t >= total;
            let label = if sim.playing { "Pause" } else { "Play" };
            if ui
                .button(label)
                .on_hover_text("Start or stop the tool at the playhead")
                .clicked()
            {
                if at_end {
                    // Pressing Play on a finished run rewinds; the
                    // transport has no separate back-to-start button.
                    sim.t = 0.0;
                    sim.playing = true;
                } else {
                    sim.playing = !sim.playing;
                }
            }
            if ui.button("Stop").on_hover_text("Leave simulation").clicked() {
                stop = true;
            }
            if total > 0.0 {
                ui.add(egui::Slider::new(&mut sim.t, 0.0..=total).show_value(false));
                egui::ComboBox::from_id_salt("sim_speed")
                    .selected_text(format!("{:.0}x", sim.speed))
                    .show_ui(ui, |ui| {
                        for sp in [1.0, 2.0, 4.0, 8.0] {
                            ui.selectable_value(&mut sim.speed, sp, format!("{sp:.0}x"));
                        }
                    });
                // Readout AFTER the widgets: the scrub sets t this frame.
                let t = sim.t;
                ui.label(sim_readout(&sc, t, unit));
            } else {
                ui.label(if sc.truncated {
                    "timeline truncated - too many moves to time"
                } else {
                    "nothing to run"
                });
            }
        });
        if stop {
            state.sim = None;
        } else if !sc.is_empty()
            && let Some(sim) = &state.sim
        {
            let f = sc.at(sim.t);
            sim_marker = Some(([f.pos.x, f.pos.y, f.pos.z], f.axis));
        }
        ui.separator();
    }
    // M2.3 stage B: the Deviate verdict bar, parallel to the transport -
    // it takes its strip before the image, and draws only while the
    // sheet is armed (a snapshot with no verdict sees zero churn). The
    // sentence is re-derived from the frozen verdict every frame through
    // the one pure string (actions::deviate_readout - the status line
    // says the same thing at arming time, from the same function). The
    // buttons fly the camera to the worst rest and gouge cells: a
    // located verdict you cannot look at is only half located. Ephemeral
    // like every other camera move - no command.
    if let Some(ps) = &state.program_sim {
        let mut fly: Option<[f64; 3]> = None;
        let mut export = false;
        ui.horizontal(|ui| {
            ui.label(crate::actions::deviate_readout(&ps.dev, state.units()));
            if ps.dev.rest_cells > 0
                && let Some(q) = ps.dev.max_rest_at
                && ui.button("Frame rest").on_hover_text("Look at the worst uncut cell").clicked()
            {
                fly = Some([q.x, q.y, q.z]);
            }
            if ps.dev.gouge_cells > 0
                && let Some(q) = ps.dev.max_gouge_at
                && ui.button("Frame gouge").on_hover_text("Look at the deepest overcut cell").clicked()
            {
                fly = Some([q.x, q.y, q.z]);
            }
            // M7.2 D: what the program leaves, out as a mesh.
            if ui
                .button("Export STL")
                .on_hover_text("Write the machined stock as an STL mesh (extracted from the traced field)")
                .clicked()
            {
                export = true;
            }
        });
        if let Some(p) = fly {
            crate::actions::frame_at(state, p);
        }
        if export {
            state.pending.push_back(crate::state::CommandKind::ExportStockDialog);
        }
        ui.separator();
    }
    let avail = ui.available_size();
    if avail.x > 1.0 && avail.y > 1.0 {
        // The 3D extent deferral (last frame's layout sizes next frame's
        // target); `interact` hit-tests pointers against viewport_rect,
        // set below - no separate size channel since M-g.
        *viewport_points = [avail.x.max(1.0), avail.y.max(1.0)];
    }
    let resp = ui.add(
        egui::Image::from_texture(egui::load::SizedTexture::new(tex, avail))
            .max_size(avail)
            .sense(egui::Sense::click_and_drag()),
    );
    // The pane's rect is the projection screen, so hit-tests take the same
    // global pointer coords imgui's viewport_rect fed the raw-event path.
    let ([rx0, ry0], [rx1, ry1]) = (parr(resp.rect.min), parr(resp.rect.max));
    state.viewport_rect = ([rx0, ry0], [rx1, ry1]);
    let hot = resp.hovered();
    state.viewport_hot = hot;
    let (rw, rh) = (rx1 - rx0, ry1 - ry0);
    // The M1.5 corner furniture, laid out from the pane rect alone: the
    // pointer law consults it before it arms any gesture, the overlay
    // pass draws it last, and both calls see the same rects.
    let hud = viewcube::Hud::new(resp.rect);

    // One wheel notch must dolly by one line in both shells: egui bakes
    // `line_scroll_speed` pixels into smooth_scroll_delta, so undo it here
    // (the imgui shell sees raw mouse_wheel = notches).
    let line_px = ui.ctx().options(|o| o.input_options.line_scroll_speed).max(1.0);
    let (pos, mods, mid_press, right_press, left_pressed, left_released, other_released, motion, wheel) =
        ui.input(|a| {
            let p = &a.pointer;
            (
                p.interact_pos().or_else(|| p.hover_pos()),
                interact::Mods {
                    ctrl: a.modifiers.ctrl,
                    shift: a.modifiers.shift,
                    alt: a.modifiers.alt,
                },
                p.button_pressed(egui::PointerButton::Middle),
                p.button_pressed(egui::PointerButton::Secondary),
                p.button_pressed(egui::PointerButton::Primary),
                p.button_released(egui::PointerButton::Primary),
                p.button_released(egui::PointerButton::Middle)
                    || p.button_released(egui::PointerButton::Secondary),
                // Frame-to-frame pointer delta. egui's `motion()` only
                // fills from raw OS motion events (never from PointerMoved
                // in a kittest harness), while `delta()` tracks every
                // position change in both the window and the harness - the
                // per-frame motion both shells want.
                p.delta(),
                if a.smooth_scroll_delta.y != 0.0 {
                    Some(a.smooth_scroll_delta.y / line_px)
                } else {
                    None
                },
            )
        });

    // Presses - only over the pane, like the imgui hot-window gate. A press
    // is remembered, not acted on: the release decides pick vs orbit. The
    // corner furniture answers first (M1.5): a press inside it acts at
    // press time and is never remembered, so the release-pick below sees
    // no press to release and the motion pass sees nothing to promote -
    // the whole gesture is absorbed, exactly like a ribbon button's.
    if hot && let Some(p) = pos {
        let over_hud = hud.as_ref().is_some_and(|h| h.contains(p));
        if let Some(h) = &hud
            && over_hud
            && left_pressed
        {
            viewcube::click(state, h, p);
        }
        if !over_hud && interact::inside_viewport(state, parr(p)) {
            if mid_press {
                state.drag = Some(DragMode::Orbit);
            }
            if right_press {
                state.drag = Some(DragMode::Pan);
            }
            if left_pressed {
                *left_press = Some(parr(p));
            }
        }
        if let (Some(steps), Some(p)) = (wheel, pos)
            && rw > 1.0
            && rh > 1.0
        {
            let [x, y] = parr(p);
            let ndc = [(x - rx0) / rw * 2.0 - 1.0, 1.0 - (y - ry0) / rh * 2.0];
            state.cam.dolly_cursor(ndc, (rw / rh) as f64, steps);
        }
    }

    // Releases. A short press released inside the pane is a pick; a same-
    // frame press+release (fast click) is handled here, not at the press.
    if left_released {
        let press = left_press.take();
        if state.drag == Some(DragMode::Orbit) {
            state.drag = None;
        }
        if let (Some(q), Some(p)) = (press, pos)
            && (p - egui::pos2(q[0], q[1])).length() <= interact::CLICK_TRAVEL_PX
            && interact::inside_viewport(state, parr(p))
        {
            interact::pick_at(state, parr(p), mods);
        }
    }
    if other_released && state.drag.is_some() {
        state.drag = None;
    }

    // Motion: drags move the camera; a held-but-not-yet-dragging left
    // button promotes to orbit past the travel threshold. Hover recomputes
    // every frame (imgui checks inside draw, so a parked cursor keeps its
    // feature outline); interact gates it on drag/hot/inside itself.
    if motion != egui::Vec2::ZERO {
        match state.drag {
            Some(DragMode::Orbit) => state.cam.orbit(motion.x, motion.y),
            Some(DragMode::Pan) => state.cam.pan(motion.x, motion.y),
            None => {
                if let (Some(q), Some(p)) = (*left_press, pos)
                    && p.distance(egui::pos2(q[0], q[1])) > interact::CLICK_TRAVEL_PX
                {
                    state.drag = Some(DragMode::Orbit);
                    state.cam.orbit(motion.x, motion.y);
                }
            }
        }
    }
    if let Some(p) = pos {
        interact::update_hover(state, parr(p), left_press.is_some());
    }

    // Screen-space overlays on the image: axis/anchor/gizmo labels and the
    // hovered feature's outline, projected through the same camera math.
    if rw > 1.0 && rh > 1.0 {
        let aspect = (rw / rh) as f64;
        // 0.36 has no Style::text_style lookup - text_styles is the map.
        let font = ui
            .style()
            .text_styles
            .get(&egui::TextStyle::Small)
            .cloned()
            .unwrap_or_else(|| egui::FontId::proportional(11.0));
        let to_px = |uv: [f32; 2]| egui::pos2(rx0 + uv[0] * rw, ry0 + uv[1] * rh);
        let rgb8 = |rgb: [f32; 3]| {
            egui::Color32::from_rgb(
                (rgb[0] * 255.0).clamp(0.0, 255.0) as u8,
                (rgb[1] * 255.0).clamp(0.0, 255.0) as u8,
                (rgb[2] * 255.0).clamp(0.0, 255.0) as u8,
            )
        };
        // #195: the origin triads, chunky - a dark outline, then the axis
        // colour - over the 1 px GPU lines (a wgpu line has no width).
        let shade = egui::Color32::from_black_alpha(170);
        let segs = interact::gizmo_segments(state, aspect);
        for (a, b, _) in &segs {
            ui.painter().line_segment([to_px(*a), to_px(*b)], egui::Stroke::new(interact::GIZMO_OUTLINE_PX, shade));
        }
        for (a, b, rgb) in &segs {
            ui.painter().line_segment([to_px(*a), to_px(*b)], egui::Stroke::new(interact::GIZMO_STROKE_PX, rgb8(*rgb)));
        }
        let axis_font = egui::FontId::proportional((font.size * 1.4).max(14.0));
        for (uv, rgb, text) in interact::overlay_labels(state, aspect) {
            let at = to_px(uv);
            if interact::is_axis_label(&text) {
                // Bigger, with a shadow, so a letter reads on any ground.
                ui.painter().text(at + egui::vec2(1.0, 1.0), egui::Align2::LEFT_TOP, &text, axis_font.clone(), shade);
                ui.painter().text(at, egui::Align2::LEFT_TOP, text, axis_font.clone(), rgb8(rgb));
            } else {
                ui.painter().text(at, egui::Align2::LEFT_TOP, text, font.clone(), rgb8(rgb));
            }
        }
        // #709: picked points, violet and never occluded; the hover over them.
        for (a, b) in crate::vertex_pick::picked_point_marks(state, aspect) {
            ui.painter().line_segment([to_px(a), to_px(b)], egui::Stroke::new(2.0, rgb8(super::ground_ink::overlay(super::ground_ink::Overlay::Pick))));
        }
        for (a, b) in interact::hover_features(state, aspect) {
            ui.painter().line_segment([to_px(a), to_px(b)], egui::Stroke::new(1.0, rgb8(super::ground_ink::overlay(super::ground_ink::Overlay::Hover))));
        }
        // The machined mesh's hovered facet (#25): the hover ink's yellow,
        // a veil over the class colour and its outline.
        if let Some(tri) = crate::machined::hover_facet_uv(state, aspect) {
            let ink = egui::Color32::from_rgb(255, 217, 102);
            ui.painter().add(egui::Shape::convex_polygon(
                tri.iter().map(|uv| to_px(*uv)).collect(),
                egui::Color32::from_rgba_unmultiplied(255, 217, 102, 90),
                egui::Stroke::new(1.5, ink),
            ));
        }
        // #819: fixture plate grids - hole crosses, dowel diamonds, and
        // the names along row A and column 1.
        let plates = crate::plate_viz::plate_marks(state, aspect);
        for (a, b, dowel) in &plates.segs {
            let ink = if *dowel { egui::Color32::from_rgb(120, 200, 255) } else { egui::Color32::from_rgb(230, 190, 90) };
            ui.painter().line_segment([to_px(*a), to_px(*b)], egui::Stroke::new(1.5, ink));
        }
        for (uv, text) in plates.labels {
            ui.painter().text(to_px(uv), egui::Align2::LEFT_BOTTOM, text, font.clone(), egui::Color32::from_rgb(230, 190, 90));
        }
        // The Boundary group's marks (M11.5 UX): role-coloured picked
        // edges, the edge a click would take, chain ends, machined side.
        for m in crate::boundary_viz::boundary_marks(state, aspect) {
            ui.painter().line_segment(
                [
                    egui::pos2(rx0 + m.a[0] * rw, ry0 + m.a[1] * rh),
                    egui::pos2(rx0 + m.b[0] * rw, ry0 + m.b[1] * rh),
                ],
                egui::Stroke::new(m.width, super::colorvision::ink(m.rgb)),
            );
        }
        // The tool marker, last of the scene-space furniture but under the
        // chrome: a white ring at the tool point with three stubs along the
        // axes it actually moves on, projected through the same camera math
        // as the labels. Stubs keep screen-space length (a zoomed-out view
        // must not lose the marker) and drop when they project away, so a
        // behind-the-camera point never draws garbage.
        if let Some((p, axis)) = sim_marker {
            let mvp = state.cam.mvp(aspect);
            if let Some(u) = interact::project_uv(&mvp, p) {
                let c = egui::pos2(rx0 + u[0] * rw, ry0 + u[1] * rh);
                for (d, rgb) in [
                    ([4.0, 0.0, 0.0], (235u8, 90, 90)),
                    ([0.0, 4.0, 0.0], (90, 210, 110)),
                    ([0.0, 0.0, 4.0], (100, 140, 255)),
                ] {
                    let q = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
                    if let Some(v) = interact::project_uv(&mvp, q) {
                        let dir = egui::pos2(rx0 + v[0] * rw, ry0 + v[1] * rh) - c;
                        let len = dir.length();
                        if len > 0.5 {
                            let k = len.min(12.0) / len;
                            ui.painter().line_segment(
                                [c, c + dir * k],
                                egui::Stroke::new(
                                    1.5,
                                    egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2),
                                ),
                            );
                        }
                    }
                }
                // The tool axis (M5.1 stage G): a white stub up the
                // spindle, so a leaning head LEANS in the viewport and
                // the mid-turn law has a visible referent. It points
                // tip-to-spindle, standing in where the tool's real
                // body would be; on a prismatic path it is the Z stub
                // a hair longer and reads as before.
                let tip = [
                    p[0] + axis[0] * 10.0,
                    p[1] + axis[1] * 10.0,
                    p[2] + axis[2] * 10.0,
                ];
                if let Some(v) = interact::project_uv(&mvp, tip) {
                    let dir = egui::pos2(rx0 + v[0] * rw, ry0 + v[1] * rh) - c;
                    let len = dir.length();
                    if len > 0.5 {
                        let k = len.min(26.0) / len;
                        ui.painter().line_segment(
                            [c, c + dir * k],
                            egui::Stroke::new(2.0, super::ground_ink::cube().marker[0]),
                        );
                    }
                }
                ui.painter().circle_stroke(
                    c,
                    5.0,
                    egui::Stroke::new(1.5, super::ground_ink::cube().marker[1]),
                );
                ui.painter()
                    .circle_filled(c, 2.0, super::ground_ink::cube().marker[2]);
            }
        }
        // The G-code viewer's sync cursor: the move that line posts,
        // drawn amber over the paths - the segment arriving at the
        // scroll line's position, plus a ring at its end. Same display
        // law as the tool marker: reads view state, never the world,
        // and drops points that project behind the camera.
        if let Some((a, b)) = gcode_segment(state) {
            let mvp = state.cam.mvp(aspect);
            let pa = interact::project_uv(&mvp, a)
                .map(|u| egui::pos2(rx0 + u[0] * rw, ry0 + u[1] * rh));
            let pb = interact::project_uv(&mvp, b)
                .map(|u| egui::pos2(rx0 + u[0] * rw, ry0 + u[1] * rh));
            let amber = egui::Color32::from_rgb(255, 170, 60);
            if let (Some(pa), Some(pb)) = (pa, pb) {
                ui.painter()
                    .line_segment([pa, pb], egui::Stroke::new(2.5, amber));
            }
            if let Some(pb) = pb {
                ui.painter().circle_stroke(pb, 4.0, egui::Stroke::new(1.5, amber));
            }
        }
        // The Deviate verdict, located: a ring at the worst rest cell
        // (the sheet's amber) and the worst gouge (its red) - the two
        // numbers that carry the sentence in the verdict bar, drawn
        // where they happened. Same display law as every other overlay:
        // reads the frozen verdict, never the world, and drops points
        // that project behind the camera.
        if let Some(ps) = &state.program_sim {
            let mvp = state.cam.mvp(aspect);
            // M13.4: the rings wear the colour-vision palette (Off's
            // rest/gouge round to the 250,184,51 / 230,46,36 they were).
            let pal = super::colorvision::palette_of(&state.world);
            for (q, col) in [
                (ps.dev.max_rest_at, super::colorvision::ink(pal.rest)),
                (ps.dev.max_gouge_at, super::colorvision::ink(pal.gouge)),
            ] {
                let Some(q) = q else { continue };
                if let Some(u) = interact::project_uv(&mvp, [q.x, q.y, q.z]) {
                    let c = egui::pos2(rx0 + u[0] * rw, ry0 + u[1] * rh);
                    ui.painter().circle_stroke(c, 6.0, egui::Stroke::new(2.0, col));
                    ui.painter().circle_filled(c, 1.5, col);
                }
            }
        }
        // Empty document: the viewport is the largest surface on screen,
        // and on a fresh document it is a bare ground with an origin
        // triad. One dim line names the door that fills it, so the shell
        // never shows an unexplained void (the browser's empty groups say
        // the same kind of thing one pane over).
        let empty = {
            use camcraft_app::model;
            model::model_entities(&state.world).is_empty()
                && !model::named_entities(&state.world)
                    .iter()
                    .any(|(_, e)| model::is_operation(&state.world, e))
        };
        if empty {
            // Bottom-centre, clear of the origin triad and of the corner
            // control legend (bottom-right): the hint names the door that
            // fills the void without ever sitting on the scene's marks.
            ui.painter().text(
                egui::pos2(rx0 + rw * 0.5, (ry0 + rh - 56.0).max(ry0 + 24.0)),
                egui::Align2::CENTER_CENTER,
                "No model yet - File > Model imports an STL, STEP or OBJ file",
                egui::FontId::proportional(font.size * 1.2),
                egui::Color32::from_white_alpha(150),
            );
        }
        // Corner hint: the control legend, or the live mode while dragging.
        let hint = match state.drag {
            Some(DragMode::Orbit) => "orbit",
            Some(DragMode::Pan) => "pan",
            None => "LMB pick / drag-orbit · MMB orbit · RMB pan · wheel · F frame",
        };
        if hot || state.drag.is_some() {
            ui.painter().text(
                resp.rect.max - egui::vec2(12.0, 12.0),
                egui::Align2::RIGHT_BOTTOM,
                hint,
                font.clone(),
                super::ground_ink::cube().hint,
            );
        }
        // The corner furniture paints last of all: opaque over the scene,
        // and it lights the feature a click would command.
        if let Some(h) = &hud {
            let hp = if state.drag.is_none() {
                pos.filter(|p| hot && h.contains(*p))
            } else {
                None
            };
            viewcube::draw(ui, h, &state.cam, state, hp, font);
        }
    }
    // The furniture asks for the pointer's hand, not the crosshair.
    if hot && hud.as_ref().is_some_and(|h| pos.is_some_and(|p| h.contains(p))) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

/// The transport bar's right-hand text: `{op} · {verb}[ · A/B/C] ·
/// {t}/{total} s`, the verb the current segment reports (RAPID, the live
/// feed through the units lens, DWELL n s, END at the finish, STANDBY
/// before the first move), and - since M5.1 stage G - the commanded
/// letters whenever the head is off home. Pure and `pub(crate)` so the
/// tests pin the string law without a window.
pub(crate) fn sim_readout(
    sc: &camcraft_app::sim::Script,
    t: f64,
    unit: camcraft_app::units::Unit,
) -> String {
    let f = sc.at(t);
    let verb = match f.readout {
        camcraft_app::sim::Readout::Rapid => "RAPID".to_owned(),
        camcraft_app::sim::Readout::Feed(v) => {
            format!("F {} {}", camcraft_app::units::feed(v, unit), unit.feed_suffix())
        }
        camcraft_app::sim::Readout::Dwell(x) => format!("DWELL {x:.1} s"),
        camcraft_app::sim::Readout::Idle if f.done => "END".to_owned(),
        camcraft_app::sim::Readout::Idle => "STANDBY".to_owned(),
    };
    let op = f
        .op
        .and_then(|i| sc.ops.get(i))
        .map_or("(none)", |s| s.as_str());
    // The head's letters (M5.1 stage G), stated only while it is off
    // home: a prismatic program's readout is the exact string it
    // always was (the transport-law tests pin it), and the moment
    // the timeline carries a tilt, the numbers the post wrote and
    // the mid-turn law judged become readable mid-swing.
    let pose = if camcraft_app::cl::is_vertical(&f.axis) {
        String::new()
    } else {
        let [a, b, c] = camcraft_app::cl::axis_to_angles(f.axis);
        format!(" · A{a:.0} B{b:.0} C{c:.0}")
    };
    format!("{op} · {verb}{pose} · {t:.1}/{:.1} s", sc.total.max(0.0))
}

/// The timeline for this document version: `sim_script` once per
/// `doc_version` bump, shared by the transport and the traced
/// playback, so no frame re-walks (or re-clones) every op's path.
pub(crate) fn cached_script(state: &mut State) -> std::sync::Arc<camcraft_app::sim::Script> {
    if let Some((v, sc)) = &state.sim_script_cache
        && *v == state.doc_version
    {
        return std::sync::Arc::clone(sc);
    }
    let sc = std::sync::Arc::new(sim_script(state));
    state.sim_script_cache = Some((state.doc_version, std::sync::Arc::clone(&sc)));
    sc
}

/// Rebuild the timeline from the live world (`cached_script` is the
/// per-frame door): the same
/// tree-order walk the post takes (hidden paths included - hidden is
/// display, not suppression; the law stands on `State::has_toolpaths`),
/// so the transport clock and the posted program count the same moves.
pub(crate) fn sim_script(state: &State) -> camcraft_app::sim::Script {
    use camcraft_app::{model, posting, setup, sim};
    let tr = sim::Traverse::from_machine(&posting::machine_value(&state.world));
    let paths: Vec<(String, camcraft_app::cl::Toolpath)> = setup::operation_order(&state.world)
        .into_iter()
        .filter_map(|(n, e)| model::toolpath_of(&state.world, &e).map(|tp| (n, tp.path)))
        .collect();
    sim::script(paths.iter().map(|(n, t)| (n.as_str(), t)), &tr)
}

/// The move the G-code viewer's cursor points at, as the segment that
/// arrives at it: the last recorded position at or before the scroll
/// line, drawn from the one before that (so the highlight is the path's
/// own polyline, not a floating point - the first move's segment
/// collapses to its endpoint ring). None with no viewer up, the window
/// closed, or a map that holds no position yet. Pure, like the readout
/// - the viewport pass draws what this resolves, nothing more.
pub(crate) fn gcode_segment(state: &State) -> Option<([f64; 3], [f64; 3])> {
    let view = state.post_view.as_ref()?;
    if !view.open {
        return None;
    }
    let cursor = view.line.min(view.line_pos.len().saturating_sub(1));
    let mut to = None;
    let mut from = None;
    for (i, p) in view.line_pos.iter().enumerate() {
        if i > cursor {
            break;
        }
        if let Some(p) = p {
            from = to;
            to = Some(*p);
        }
    }
    let to = to?;
    Some((from.unwrap_or(to), to))
}

/// egui Pos2 -> the `[f32; 2]` convention interact.rs speaks (0.36 has no
/// `to_array` on Pos2).
fn parr(p: egui::Pos2) -> [f32; 2] {
    [p.x, p.y]
}

/// Headless proof (the M-a gate, graduated from the spike's S2 gate): no
/// window, no surface - `headless_gpu::device()` (software adapter first,
/// else hardware; the same choice as tests/gpu_render.rs), View3D on the shell's Rgba8Unorm offscreen
/// format, the SEEDED document rendered (block + stock + region + cached
/// path through set_overlay), and the readback showing a solid footprint of
/// changed pixels. The second half of the proof is the world itself:
/// Contour1 carries a non-empty cached toolpath, i.e. seed -> commands ->
/// generate worked. Native only: it blocks on the adapter and on the
/// readback, which a browser cannot (#31).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod headless {
    use camcraft_app::model;

    use crate::state::{CommandKind, State};
    use crate::viewport::{Camera, View3D};

    use super::VIEW3D_FORMAT;
    use super::wgpu;
    use super::super::world;

    pub(crate) fn run() -> Result<(), String> {
        let mut state = State::new();
        world::seed(&mut state);
        state.pending.push_back(CommandKind::FrameAll);
        state.drain_actions();
        let op_ok = model::find_by_name(&state.world, "Contour1")
            .and_then(|e| model::toolpath_of(&state.world, &e))
            .map_or(0, |tp| tp.path.moves.len());
        if op_ok == 0 {
            return Err(format!(
                "seed produced no cached toolpath for Contour1 (status: {})",
                state.status
            ));
        }

        // Software adapter first, else the hardware one (#615: macOS's
        // Metal has no software fallback).
        let gpu = crate::headless_gpu::device()?;
        println!("headless: {} adapter {}", gpu.kind.label(), gpu.name);
        let (device, queue) = (gpu.device, gpu.queue);

        let mut view = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            View3D::new(&device, VIEW3D_FORMAT, 320, 240)
        }))
        .map_err(|_| "View3D pipeline panicked on validation".to_string())?;
        let scene = world::collect(&state);
        view.set_document(&device, &queue, &scene.parts);
        view.set_overlay(&device, &queue, &scene.overlay);

        // Same vantage the gpu_render smoke tests use on equivalent parts.
        let cam = Camera {
            target: [0.0, 0.0, 20.0],
            yaw: 0.9,
            pitch: 0.5,
            dist: 160.0,
            ..Camera::default()
        };
        let (w, h) = view.extent();
        let mvp = cam.mvp(f64::from(w.max(1)) / f64::from(h.max(1)));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui-shell-headless"),
        });
        view.render(&queue, &mut encoder, &mvp, cam.eye_f32(), state.display);
        queue.submit([encoder.finish()]);

        let (rgba, w, h) = view
            .snapshot_rgba(&device, &queue)
            .ok_or_else(|| "no readback path".to_string())?;
        // The corner pixel is untouched clear color; anything differing from
        // it by the gpu_render epsilon is rasterized geometry (the format-
        // agnostic assertion - no assumptions about shading brightness).
        let bg: [u8; 4] = rgba
            .first_chunk::<4>()
            .copied()
            .ok_or_else(|| "empty readback".to_string())?;
        let changed = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| {
                p.iter().zip(bg).any(|(a, b)| {
                    i32::from(*a) - i32::from(b) > 6 || i32::from(b) - i32::from(*a) > 6
                })
            })
            .count();
        let total = (w as usize * h as usize).max(1);
        if changed * 20 < total {
            // less than 5% of the frame: the seeded stock did not rasterize
            return Err(format!("readback looks empty ({changed}/{total} changed px)"));
        }
        println!(
            "headless snapshot: {w}x{h}, {}% pixels changed; Contour1 cached {op_ok} moves",
            changed * 100 / total
        );
        Ok(())
    }
}

//! M-c: the property pages, graduated from the spike (S3) verbatim - the
//! dispatcher on the shell's ui target plus the five 2D operation pages, the
//! height law, and the setup pages (stock / work offsets / tool / model).
//!
//! These pages graduated from the spike verbatim; the imgui panels they
//! mirrored (operations.rs/setup.rs, plus the property-page code in
//! app.rs) retired at M-g, so the law they shared now lives in
//! `crate::operations` (height wording, retypes, warning folds - called
//! through, never restated) and here. Same fields, same notes and echoes,
//! same command shapes - every change rides `state.pending` /
//! the command methods, never an ad-hoc field poke (AGENTS.md, "Everything
//! Is a Command"). The M-c pages arrived as one scroll (parity first); the
//! named post-M-g carry-over landed here: the operation page is now the
//! specced named sections (SECTIONS - geometry, heights, passes, leads and
//! links, feeds) as a tab bar (`op_tabs`), each field keeping its
//! within-section order, the open tab transient pane state that queues
//! nothing (M9.1 B stacked them into one pane; user feedback brought the
//! tabs back). Two
//! deliberate translations, both from the go/no-go
//! report (S4):
//!
//! * `InputDouble` becomes `DragValue` (divergence D5): egui has no
//!   InputDouble, and DragValue edits + commits with no custom state -
//!   click to type, drag to step. The typed-entry and drag paths hit the
//!   same field the imgui widget did; `tests.rs` proves both.
//! * The warning wall renders through `crate::operations::fold_warnings`
//!   (the M1.5.4 display law, reused, not restated).
//!
//! One string-level fix rides here: imgui's `##id` anchors ("Apply##stk")
//! showed only their head, but egui 0.36 has no `##` convention - it would
//! paint the anchor literally. Every such label is now plain ("Apply"),
//! which is what imgui displayed and therefore what parity means; egui's
//! auto-ids are unique per allocation, so the anchors were never needed.
//! (The spike's page code carried them; this step's column-model fix for
//! the browser was the same class of graduation-time repair.)
//!
//! Since the user's 2026-11-06 review the operation dialog lives in its
//! own modules on the `layout` property grid: `op_page` (header, status
//! banner, tabs, action bar), `geo_page`, `params_page` and its
//! kind-family pages, `heights_page`, `lead_page` / `approach_page`, and
//! the shared widgets in `fields` (re-exported below for the setup pages).
//!
//! The kind is not a page field (M9.1): the operation page has no Type
//! combo - the button that made the operation decided what it is, and the
//! browser row's right-click is the only retype verb. Every surface that
//! retypes (the quick-adds, the menu's command) rides `crate::operations`'
//! one law, so they cannot disagree about what a retype preserves.
//!
//! Provenance: our own port, specced from imgui operations.rs/setup.rs
//! (our code) and the egui vendored sources; no HSMWorks material involved.

use egui::{
    Align2, Color32, ComboBox, DragValue, FontId, Id, Pos2, RichText, ScrollArea,
    Sense, Shape, Stroke, TextEdit, Ui, Vec2, Window,
};

use camcraft_app::geometry::{quat_from_axis_angle, quat_from_euler, quat_mul, quat_to_euler};
use camcraft_app::machine;
use camcraft_app::model;
use camcraft_app::operation::SrcRef;
use camcraft_app::posting;
use camcraft_app::schema::PostRef;
use camcraft_app::stock::StockShape;
use camcraft_app::tools;
use camcraft_app::units::{self, Unit};

use crate::actions;
use crate::interact;
use crate::state::{CommandKind, ErrorScope, FixtureUi, ModelUi, State, UiTarget, quat_yaw_deg};

use super::layout::hint;
use super::panes::kind_name;
use super::theme::Tokens;

// The value widgets graduated to `fields` with the property grid (user
// review 2026-11-06); the setup pages below and the sibling modules keep
// reading them through this path.
pub(crate) use super::fields::{combo, combo_owned, field, num, num_feed, num_plain};

/// Tool kind labels, same order as `ToolKind` (mirrors the tool page).
const TOOL_KINDS: [&str; 13] = [
    "End mill",
    "Ball nose",
    "Bull nose",
    "Drill",
    "Tap",
    "Bore",
    "Turning insert",
    "Grooving blade",
    "Threading insert",
    "Form gauge",
    "Chamfer mill",
    "Touch probe",
    // #670: the kerf is its diameter; it turns no spindle.
    "Plasma torch",
];

/// The note gray the model page's notes use ([0.62,0.66,0.72]).
const NOTE: Color32 = Color32::from_rgb(0x9E, 0xA8, 0xB8);

// --------------------------------------------------------------- properties

/// The dock's properties pane: dispatch on the shell's ui target, exactly
/// like operations.rs's contents() dispatcher. The operation page owns
/// its scroll (its action bar stays fixed under it); every other page
/// scrolls whole.
/// The Setups page's pick rows mark themselves drawn; the shell's
/// per-frame sweep (#664, #667) disarms the rest after the dock.
pub fn properties(ui: &mut Ui, state: &mut State, pal: &Tokens) {
    let t = state.pane_target();
    // Only an open Boundary group draws its viewport marks (M11.5 UX):
    // any other page this frame hides them and stops the collecting.
    state.boundary_view.shown = false;
    if !matches!(t, UiTarget::Operation(_)) {
        state.boundary_pick = None;
    }
    if let UiTarget::Operation(n) = &t {
        super::op_page::operation_page(ui, state, n.as_str(), pal);
        return;
    }
    let has_close = matches!(&t, UiTarget::Tool(_) | UiTarget::Stock | UiTarget::Wcs | UiTarget::Model(_) | UiTarget::SetupNode(..));
    let content_height = (ui.available_height() - if has_close { 40.0 } else { 0.0 }).max(80.0);
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(content_height)
        .show(ui, |ui| match t {
            UiTarget::None => super::op_page::no_operation_page(ui),
            UiTarget::Operation(_) => {}
            UiTarget::Tool(n) => tool_page(ui, state, &n),
            UiTarget::Stock => {
                state.sync_setup_ui();
                ui.heading("Stock");
                ui.separator();
                stock_page(ui, state);
                setup_error(ui, state);
            }
            UiTarget::Wcs => {
                state.sync_setup_ui();
                // #342: one WCS per setup; the word is its post label.
                let active = camcraft_app::setup::active(&state.world);
                ui.heading(camcraft_app::setup::wcs_label(&state.world, active.as_deref()));
                ui.separator();
                wcs_page(ui, state);
                // #343: the WCS node's dialog names its post word too.
                if let Some(name) = active.as_deref() {
                    ui.separator();
                    super::setup_wcs_pick::picker(ui, state, name);
                }
                setup_error(ui, state);
            }
            UiTarget::Model(n) => {
                state.sync_setup_ui();
                model_page(ui, state, n);
                setup_error(ui, state);
            }
            // #343: a setup's own node pages.
            UiTarget::SetupNode(name, page) => {
                state.sync_setup_ui();
                super::setup_pages::page(ui, state, &name, page);
                setup_error(ui, state);
            }
            UiTarget::Setups => {
                state.sync_setup_ui();
                ui.heading("Setups");
                ui.separator();
                setups_page(ui, state);
                setup_error(ui, state);
            }
        });
    if has_close {
        ui.separator();
        if ui.button("Close").clicked() {
            state.close_property_editor();
        }
    }
}

fn setup_error(ui: &mut Ui, state: &State) {
    if let Some(msg) = state.error_for(ErrorScope::Setup).map(str::to_owned) {
        super::error_text::error_text(ui, super::colorvision::text_inks(ui, &state.world).error, &msg);
    }
}
// --------------------------------------------------------------- stock page

fn stock_page(ui: &mut Ui, state: &mut State) {
    let u = state.units();
    // The document's unit lens, straight off the world singleton - the
    // click queues SetUnits; the numbers below re-render from the world
    // next pass, so there is no mirror to keep in step.
    ui.horizontal(|ui| {
        ui.label("Units");
        for cand in [Unit::Millimeters, Unit::Inches] {
            if ui.radio(u == cand, cand.label()).clicked() && u != cand {
                state.pending.push_back(CommandKind::SetUnits(cand));
            }
        }
    });
    ui.add_space(3.0);
    ui.horizontal(|ui| {
        for (i, name) in ["Box", "Cylinder", "From model"].iter().enumerate() {
            if ui
                .radio_value(&mut state.stock_ui.shape, i as i32, *name)
                .changed()
            {}
        }
    });
    // #197: the models' size, right above the numbers it informs.
    match super::stock_info::part_size_lines(&state.world, u) {
        Some([size, extents]) => {
            ui.label(size).on_hover_text("The models' bounding box - what Fit to Models wraps, plus the fit margin");
            ui.label(RichText::new(extents).weak());
        }
        None => {
            ui.label(RichText::new("No models yet - load one to see its size.").weak());
        }
    }
    ui.add_space(3.0);
    let shape = state.stock_ui.shape;
    if shape == 1 {
        field(ui, "Diameter");
        num(ui, u, &mut state.stock_ui.diameter);
        field(ui, "Height");
        num(ui, u, &mut state.stock_ui.height);
    } else if shape == 2 {
        // Stock from a model: parts only, as the op Source picker.
        let models: Vec<String> = model::model_entities(&state.world)
            .into_iter()
            .filter(|(_, e)| !model::is_fixture(&state.world, e))
            .map(|(n, _)| n)
            .collect();
        field(ui, "Source");
        if models.is_empty() {
            ui.label("No models to take stock from.");
        } else {
            let mut idx = models
                .iter()
                .position(|m| *m == state.stock_ui.source)
                .unwrap_or(models.len() - 1);
            let before = idx;
            combo_owned(ui, "stk_src", &mut idx, &models);
            if idx != before {
                state.stock_ui.source = models[idx.min(models.len() - 1)].clone();
            }
        }
        super::raw_stock_row::raw_stock_row(ui, state);
    } else {
        field(ui, "Length (X)");
        num(ui, u, &mut state.stock_ui.dx);
        field(ui, "Width (Y)");
        num(ui, u, &mut state.stock_ui.dy);
        field(ui, "Height (Z)");
        num(ui, u, &mut state.stock_ui.dz);
    }
    ui.add_space(6.0);
    field(ui, "X center");
    num(ui, u, &mut state.stock_ui.x);
    field(ui, "Y center");
    num(ui, u, &mut state.stock_ui.y);
    field(ui, "Z bottom");
    num(ui, u, &mut state.stock_ui.z);
    field(ui, "Rotation (yaw deg)");
    num_plain(ui, &mut state.stock_ui.yaw_deg);
    ui.add_space(6.0);
    // #198: a box can be fitted by a separate allowance on each side.
    let per_side = state.stock_ui.shape == 0 && {
        ui.checkbox(&mut state.stock_ui.fit_per_side, "Per side")
            .on_hover_text("Fit to Models pads each face of the box by its own allowance");
        state.stock_ui.fit_per_side
    };
    if per_side {
        for (i, name) in ["-X", "+X", "-Y", "+Y", "Bottom", "Top"].iter().enumerate() {
            field(ui, &format!("Fit margin {name}"));
            num(ui, u, &mut state.stock_ui.fit_sides[i]);
        }
    } else {
        field(ui, "Fit margin");
        num(ui, u, &mut state.stock_ui.fit_margin);
    }
    let sides = state.stock_ui.fit_sides;
    let shape = state.stock_ui.shape;
    let (dx, dy, dz) = (state.stock_ui.dx, state.stock_ui.dy, state.stock_ui.dz);
    let (diameter, height) = (state.stock_ui.diameter, state.stock_ui.height);
    let source = state.stock_ui.source.clone();
    let pos = [state.stock_ui.x, state.stock_ui.y, state.stock_ui.z];
    let yaw = state.stock_ui.yaw_deg;
    let margin = state.stock_ui.fit_margin;
    // The stock pane edits the ACTIVE setup's stock entity (M3.2 A):
    // with no setups in the document this resolves to the old global
    // "Stock", so the legacy behavior is the degenerate case, not a
    // branch. sync_setup_ui already mirrored this entity's numbers.
    let active = camcraft_app::setup::active(&state.world);
    let stock_name = camcraft_app::setup::stock_entity_name(&state.world, active.as_deref());
    let has_stock = model::find_by_name(&state.world, &stock_name)
        .is_some_and(|e| model::is_stock(&state.world, &e));
    if let Some(s) = active.as_deref() {
        ui.label(format!("Stock of setup {s}."));
    }
    ui.horizontal(|ui| {
        if ui.button("Apply").clicked() {
            let s = if shape == 1 {
                StockShape::Cylinder { diameter, height }
            } else if shape == 2 {
                StockShape::FromModel {
                    source: source.clone(),
                }
            } else {
                StockShape::Box { dx, dy, dz }
            };
            state.pending.push_back(CommandKind::SetStock {
                shape: s,
                pos,
                yaw_deg: yaw,
            });
        }
        if ui.button("Fit to Models").clicked() {
            let cmd = if per_side {
                CommandKind::StockFitSides { sides }
            } else {
                CommandKind::StockFit { margin, shape }
            };
            state.pending.push_back(cmd);
        }
        if has_stock && ui.button("Delete").clicked() {
            state
                .pending
                .push_back(CommandKind::DeleteByName(stock_name.clone()));
        }
    });
}

/// The document's post script (M3.3 A/B): setup furniture, read live
/// off the world singleton like the machine section above. The label
/// is exactly what the post resolves - one truth, so a pane can never
/// advertise a script the gate did not run. The combo offers the five
/// shipped posts BY NAME (an index would silently re-point the
/// document when the registry grows or reorders); loading a `.cps` of
/// your own and returning to the default are commands too - undo puts
/// the previous post back.
pub(super) fn post_section(ui: &mut Ui, state: &mut State) {
    let current = posting::post_of(&state.world);
    ui.label("Post");
    ui.label(posting::post_label(&state.world));
    post_offer_banner(ui, state);
    ui.horizontal(|ui| {
        // The registry by name; the combo OFFERS only the dialects
        // that serve the mounted machine's kind (M13.5: a lathe lists
        // its turning post, a mill-turn lists both), while the
        // selected text still names whatever the document chose.
        let kind = camcraft_app::machine::kind_of(&state.world);
        let names: Vec<&str> = posting::BUILTINS.iter().map(|(n, _)| *n).collect();
        let sel: Option<usize> = match &current {
            PostRef::Builtin => Some(0),
            PostRef::Named(n) => names.iter().position(|m| *m == n.as_str()),
            PostRef::File(_) => None,
        };
        let text = match sel {
            Some(i) => names[i].to_owned(),
            None => "custom".to_owned(),
        };
        let mut pick: Option<String> = None;
        egui::ComboBox::from_id_salt("post_registry")
            .selected_text(text)
            .show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    if !posting::post_serves(n, kind) && sel != Some(i) {
                        continue;
                    }
                    if ui.selectable_label(sel == Some(i), *n).clicked() {
                        pick = Some((*n).to_owned());
                    }
                }
            });
        if let Some(name) = pick {
            state.pending.push_back(CommandKind::SelectPostNamed(name));
        }
        if ui.button("Load post (.cps)").clicked() {
            state.pending.push_back(CommandKind::SelectPostDialog);
        }
        if matches!(current, PostRef::File(_)) && ui.button("Built-in").clicked() {
            state.pending.push_back(CommandKind::SelectPostBuiltin);
        }
    });
    // #110: post the program straight through - every operation keeps
    // its stop or trip home, the post leaves them all out. A command.
    let skip = posting::post_options(&state.world).skip_stops;
    let mut want = skip;
    ui.checkbox(&mut want, "Skip pauses and homes").on_hover_text(
        "post without any operation's optional stop, program stop or trip home - the operations keep their choices",
    );
    if want != skip {
        state.pending.push_back(CommandKind::SetSkipStops(want));
    }
    // #111: one combined program, or one standalone file per operation
    // (a folder at post time). A command, like the switch above.
    let per_op = posting::post_options(&state.world).per_op;
    let mut pick = per_op;
    ui.horizontal(|ui| {
        ui.label("Output");
        egui::ComboBox::from_id_salt("post_output")
            .selected_text(if per_op { "One file per operation" } else { "One program" })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut pick, false, "One program");
                ui.selectable_value(&mut pick, true, "One file per operation");
            })
            .response
            .on_hover_text("one file per operation writes each operation as its own standalone program into a folder, so it can be rerun on its own");
    });
    if pick != per_op {
        state.pending.push_back(CommandKind::SetPerOpOutput(pick));
    }
    super::post_options::threading_picker(ui, state);
    super::post_options::smoothing_switch(ui, state);
    super::post_options::end_units_picker(ui, state);
}

/// The post offer (M13.5 B): when the mounted machine's kind is not
/// served by the document's registry post, one warning line names
/// what the current post serves and the registry's default for the
/// mount, with the switch a button away - the SAME `SelectPost`
/// command the combo queues, so undo puts the previous post back.
/// Never a switch on the mount itself: the banner offers, the user
/// chooses. The law (`posting::post_offer`) lives in app; this only
/// draws it.
fn post_offer_banner(ui: &mut Ui, state: &mut State) {
    let Some((serves, offer)) = posting::post_offer(&state.world) else {
        return;
    };
    let Some(name) = posting::post_name(&offer).map(str::to_owned) else {
        return;
    };
    let ink = super::colorvision::text_inks(ui, &state.world).warn;
    super::layout::status_banner(
        ui,
        "post_offer",
        &format!("the current post serves a {serves}; switch to {name}?"),
        ink,
        &[],
    );
    if ui.button("Switch post").clicked() {
        state.pending.push_back(match offer {
            PostRef::Builtin => CommandKind::SelectPostBuiltin,
            PostRef::Named(n) => CommandKind::SelectPostNamed(n),
            // A file post is never the registry's default (post_offer
            // only ever names one), so there is nothing to queue.
            PostRef::File(_) => return,
        });
    }
}

/// The document's machine (M2.3 C, group completed by M3.3 stage C):
/// setup furniture read live off the world singleton - no mirror to go
/// stale. The overview grid is exactly what the file stated; a blank is
/// "not stated", never an invented ceiling. The library combo picks a shipped
/// machine BY NAME, "Load other" reaches any `.mch` through the file
/// law, and "Edit" opens the draft editor; all three land on the
/// same `LoadMachine` command, so every change here is undoable
/// furniture, not a field poke. The kind line follows the DECLARED
/// kind where the machine states one, the axis inventory otherwise.
pub(super) fn machine_section(ui: &mut Ui, state: &mut State) {
    let (path, m) = machine::of(&state.world);
    ui.label("Machine");
    let names: Vec<&str> = posting::MACHINES.iter().map(|(n, _)| *n).collect();
    ui.horizontal(|ui| {
        ui.label("Library");
        let sel: Option<usize> = names.iter().position(|n| path == posting::builtin_path(n));
        let text = match sel {
            Some(i) => names[i].to_owned(),
            None => "other".to_owned(),
        };
        let mut pick: Option<String> = None;
        egui::ComboBox::from_id_salt("machine_library")
            .selected_text(text)
            .show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    if ui.selectable_label(sel == Some(i), *n).clicked() {
                        pick = Some((*n).to_owned());
                    }
                }
            });
        if let Some(name) = pick {
            state
                .pending
                .push_back(CommandKind::LoadBuiltinMachine(name));
        }
    });
    match &m {
        Some(m) => {
            let source = if path.is_empty() {
                "not stated".to_owned()
            } else {
                match names.iter().find(|n| path == posting::builtin_path(n)) {
                    Some(n) => format!("Built-in: {n}"),
                    None => path.clone(),
                }
            };
            super::machine_grid::machine_grid(ui, "machine_overview", m, state.units(), &[("Source", source)]);
            ui.horizontal(|ui| {
                if ui.button("Load other").clicked() {
                    state.pending.push_back(CommandKind::LoadMachineDialog);
                }
                if ui.button("Edit").clicked() {
                    state.pending.push_back(CommandKind::EditMachineDialog);
                }
                if ui.button("Clear").clicked() {
                    state.pending.push_back(CommandKind::ClearMachine);
                }
            });
        }
        None => {
            ui.label("No machine loaded - the checker states, it cannot judge.");
            if ui.button("Load machine (.mch)").clicked() {
                state.pending.push_back(CommandKind::LoadMachineDialog);
            }
        }
    }
}

// ------------------------------------------------------------------ wcs page

fn wcs_page(ui: &mut Ui, state: &mut State) {
    let u = state.units();
    field(ui, "X origin");
    num(ui, u, &mut state.wcs_ui.x);
    field(ui, "Y origin");
    num(ui, u, &mut state.wcs_ui.y);
    field(ui, "Z origin");
    num(ui, u, &mut state.wcs_ui.z);
    field(ui, "Tilt about X (roll)");
    num_plain(ui, &mut state.wcs_ui.roll_deg);
    field(ui, "Tilt about Y (pitch)");
    num_plain(ui, &mut state.wcs_ui.pitch_deg);
    field(ui, "Turn about Z (yaw)");
    num_plain(ui, &mut state.wcs_ui.yaw_deg);
    ui.add_space(6.0);
    ui.label("Quarter-turns about world axes (apply immediately):");
    const TURNS: [(&str, usize, f64); 6] = [
        ("X+", 0, 1.0),
        ("X-", 0, -1.0),
        ("Y+", 1, 1.0),
        ("Y-", 1, -1.0),
        ("Z+", 2, 1.0),
        ("Z-", 2, -1.0),
    ];
    for row in [0usize, 3] {
        ui.horizontal(|ui| {
            for (label, axis, sign) in TURNS[row..row + 3].iter() {
                if ui.button(format!("Turn {label}")).clicked() {
                    turn_wcs(state, *axis, *sign);
                }
            }
        });
    }
    ui.horizontal(|ui| {
        if ui.button("Level").clicked() {
            state.wcs_ui.roll_deg = 0.0;
            state.wcs_ui.pitch_deg = 0.0;
            state.wcs_ui.yaw_deg = 0.0;
            apply_wcs_drafts(state);
        }
        if ui.button("Set Origin").clicked() {
            apply_wcs_drafts(state);
        }
    });

    ui.add_space(6.0);
    // Box-point anchoring (feedback 5, revised): the reference box is an
    // explicit choice - stock or model - and the nine points of the chosen
    // level can be clicked in the viewport as markers instead of hunting
    // through the button grid. The buttons and the markers share
    // `interact::anchor_points`, so what you click, what highlights, and
    // what places are the same points by construction. This section is the
    // imgui pane's origin picker (setup.rs, de272ca/067f3aa), restored:
    // `interact.rs` kept the markers and the viewport pick law through the
    // M-g retirement, but the widgets here died un-ported, leaving the pick
    // mode unreachable (the crosses only render while `pick_anchor` is
    // set). Behavior specced from published CAM setup practice; the grid
    // and its math are ours.
    const SOURCES: [&str; 2] = ["Stock box", "Model bounds"];
    field(ui, "Anchor on");
    ComboBox::from_id_salt("wcs_src")
        .selected_text(SOURCES[state.wcs_ui.anchor_source.min(1)])
        .show_ui(ui, |ui| {
            for (i, s) in SOURCES.iter().enumerate() {
                ui.selectable_value(&mut state.wcs_ui.anchor_source, i, *s);
            }
        });
    match interact::anchor_reference(state) {
        Some((_, what)) => {
            let chosen = SOURCES[state.wcs_ui.anchor_source.min(1)];
            if !chosen.eq_ignore_ascii_case(what) {
                ui.label(format!("No {chosen} defined - anchoring on {what}."));
            }
            ui.horizontal(|ui| {
                for (i, name) in ["Bottom", "Middle", "Top"].iter().enumerate() {
                    ui.radio_value(&mut state.wcs_ui.anchor_z, i, *name);
                }
            });
            ui.checkbox(&mut state.wcs_ui.pick_anchor, "Pick markers in viewport");
            if state.wcs_ui.pick_anchor {
                ui.label("Click a yellow marker in the viewport to set the origin.");
            }
            let pts = interact::anchor_points(state);
            for row in 0..3usize {
                ui.horizontal(|ui| {
                    for col in 0..3usize {
                        let k = row * 3 + col;
                        let code = pts.get(k).map_or("?", |(_, name)| *name);
                        let label = if state.wcs_ui.anchor_xy == k {
                            format!("[{code}]")
                        } else {
                            code.to_owned()
                        };
                        if ui.button(label).clicked()
                            && let Some((pos, _)) = pts.get(k)
                        {
                            state.wcs_ui.anchor_xy = k;
                            state.wcs_ui.x = pos[0];
                            state.wcs_ui.y = pos[1];
                            state.wcs_ui.z = pos[2];
                            apply_wcs_drafts(state);
                        }
                    }
                });
            }
        }
        None => {
            ui.label("Import a model or set stock to anchor the origin on a point.");
        }
    }

    ui.add_space(6.0);
    // #342: the setup's one WCS, and the work offset its post writes. The
    // dropdown that picks it is drawn under this page (#343), so the note
    // names what this page shows, not where the choice lives.
    let wcs_ctx = camcraft_app::setup::active(&state.world);
    let label = camcraft_app::setup::wcs_label(&state.world, wcs_ctx.as_deref());
    let line = match camcraft_app::setup::wcs_entity(&state.world, wcs_ctx.as_deref())
        .and_then(|e| model::transform_of(&state.world, &e))
    {
        Some(loc) => format!(
            "{label}: {}, {}, {}  yaw {:.1}",
            units::len(loc.pos.x, u, 2),
            units::len(loc.pos.y, u, 2),
            units::len(loc.pos.z, u, 2),
            quat_yaw_deg(loc.rot)
        ),
        None => format!("{label}: not placed (at the origin)"),
    };
    ui.label(line);
    ui.label("Work offset: the G54-G59 slot this setup posts. It does not move the origin.");
}

fn apply_wcs_drafts(state: &mut State) {
    state.pending.push_back(CommandKind::SetWcs {
        pos: [state.wcs_ui.x, state.wcs_ui.y, state.wcs_ui.z],
        roll_deg: state.wcs_ui.roll_deg,
        pitch_deg: state.wcs_ui.pitch_deg,
        yaw_deg: state.wcs_ui.yaw_deg,
    });
}

fn turn_wcs(state: &mut State, axis: usize, sign: f64) {
    let q = quat_from_euler(
        state.wcs_ui.roll_deg.to_radians(),
        state.wcs_ui.pitch_deg.to_radians(),
        state.wcs_ui.yaw_deg.to_radians(),
    );
    let turn = quat_from_axis_angle(axis, sign * 90.0f64.to_radians());
    let (r, p, y) = quat_to_euler(quat_mul(turn, q));
    state.wcs_ui.roll_deg = r.to_degrees();
    state.wcs_ui.pitch_deg = p.to_degrees();
    state.wcs_ui.yaw_deg = y.to_degrees();
    apply_wcs_drafts(state);
}

/// Queue the Model page's placement draft as a `MoveModel` (M5.2 G) -
/// the `apply_wcs_drafts` shape, one command kind over.
fn apply_model_draft(state: &mut State) {
    state.pending.push_back(CommandKind::MoveModel {
        name: state.model_ui.name.clone(),
        pos: [state.model_ui.x, state.model_ui.y, state.model_ui.z],
        roll_deg: state.model_ui.roll_deg,
        pitch_deg: state.model_ui.pitch_deg,
        yaw_deg: state.model_ui.yaw_deg,
    });
}

/// Quarter-turn the drafted part about a WORLD axis and write: the
/// `turn_wcs` law (the turn premultiplies the draft's rotation), on
/// the page that owns a full placement editor.
fn turn_model(state: &mut State, axis: usize, sign: f64) {
    let q = quat_from_euler(
        state.model_ui.roll_deg.to_radians(),
        state.model_ui.pitch_deg.to_radians(),
        state.model_ui.yaw_deg.to_radians(),
    );
    let turn = quat_from_axis_angle(axis, sign * 90.0f64.to_radians());
    let (r, p, y) = quat_to_euler(quat_mul(turn, q));
    state.model_ui.roll_deg = r.to_degrees();
    state.model_ui.pitch_deg = p.to_degrees();
    state.model_ui.yaw_deg = y.to_degrees();
    apply_model_draft(state);
}

// --------------------------------------------------------------- setups page

/// The setups list (M3.2 A): the contexts a clamping creates. Clicking a
/// row activates it (undoable - it is the ActiveSetup command), New
/// creates the next named setup, optionally carrying the current one's
/// stock and placed offsets across (the shop's datum transfer, the
/// re-clamp law). The Stock and Work-offset pages always edit the ACTIVE
/// row's furniture, so this page manages context, never geometry.
fn setups_page(ui: &mut Ui, state: &mut State) {
    let names = camcraft_app::setup::names(&state.world);
    let active = camcraft_app::setup::active(&state.world);
    if names.is_empty() {
        ui.label(
            "No setups in this document: the one global stock and the \
             G54 WCS act as the first setup.",
        );
        ui.add_space(6.0);
        if ui.button("Create first setup").clicked() {
            state.pending.push_back(CommandKind::CreateSetup {
                name: None,
                datum_from: None,
            });
        }
        ui.separator();
        // Workholding and the tool list of the implicit first setup live
        // under its Fixtures and Tool list nodes in the browser (#1197),
        // the same place as in a document with setups.
        ui.label(super::setup_pages::WORKHOLDING_POINTER);
        sheet_footer(ui, state);
        return;
    }
    for name in &names {
        let is_active = active.as_deref() == Some(name.as_str());
        ui.horizontal(|ui| {
            if ui.selectable_label(is_active, name).clicked() && !is_active {
                state
                    .pending
                    .push_back(CommandKind::ActivateSetup(Some(name.clone())));
            }
            if is_active {
                ui.label("(active)");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Delete").clicked() {
                    state
                        .pending
                        .push_back(CommandKind::DeleteSetup(name.clone()));
                }
                // Alignment, work offset, workholding and tools live on
                // the setup's own page: one click from here (#1104).
                if ui.small_button("Open page").clicked() {
                    // Activate it too, as the browser's Stock row does: the
                    // fixture and tool pages edit only the active setup.
                    super::browser_pane::focus(state, name);
                    actions::target(
                        state,
                        UiTarget::SetupNode(name.clone(), crate::state::SetupPage::Settings),
                    );
                }
            });
        });
        if let Some(def) = camcraft_app::setup::def_of(&state.world, name) {
            let placed = if camcraft_app::setup::wcs_entity(&state.world, Some(name.as_str())).is_some() {
                "placed"
            } else {
                "not placed"
            };
            let mut line = format!("    stock {}, WCS {placed}", def.stock);
            if !def.datum_from.is_empty() {
                line.push_str(&format!(", datum from {}", def.datum_from));
            }
            ui.label(line);
        }
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.setups_ui.datum, "Carry stock and WCS over");
        if ui.button("New setup").clicked() {
            let datum_from = if state.setups_ui.datum {
                active.clone()
            } else {
                None
            };
            state.pending.push_back(CommandKind::CreateSetup {
                name: None,
                datum_from,
            });
            state.setups_ui.datum = false;
        }
    });
    ui.separator();
    sheet_footer(ui, state);
    ui.label(
        "Each setup's alignment, work offset, fixtures and tool list are on its own          Setup page in the browser (Open page).",
    );
}

/// The tool list of one setup context (M3.2 stage C): what goes into
/// the spindle here, in change order, with the shop's time law behind
/// the estimate. Read-only by law - tools live in the Tool Library and
/// operations pick them; this page only reports what that wiring
/// implies, so there is no draft and no command here.
pub(super) fn tool_list_section(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    let u = state.units();
    let rows = camcraft_app::tool_list::for_setup(&state.world, ctx);
    if rows.is_empty() {
        ui.label("Tool list: no generated operation in this setup yet.");
        return;
    }
    ui.label("Tool list for this setup (in change order):");
    for row in &rows {
        ui.label(format!(
            "    {}  {} op{}  {} change{}  cut {}  rapid {}  est {} min",
            row.tool,
            row.ops.len(),
            if row.ops.len() == 1 { "" } else { "s" },
            row.changes,
            if row.changes == 1 { "" } else { "s" },
            units::len(row.cut_mm, u, 2),
            units::len(row.rapid_mm, u, 2),
            camcraft_app::setup_sheet::minutes(row.minutes),
        ));
    }
    ui.label(format!(
        "    setup total: est {} min",
        camcraft_app::setup_sheet::minutes(camcraft_app::tool_list::setup_minutes(
            &state.world,
            ctx,
        )),
    ));
}

/// The setup-sheet export row (M3.2 stage D). Writing a sheet is a
/// read of the document plus a file write - like posting - so the
/// button queues the dialog arm and nothing touches the history.
pub(super) fn sheet_footer(ui: &mut Ui, state: &mut State) {
    ui.horizontal(|ui| {
        if ui.button("Export setup sheets…").clicked() {
            state.pending.push_back(CommandKind::SetupSheetDialog);
        }
        ui.label("(one page per setup, text)");
    });
}

/// The workholding section of the Setups page (M3.2 stage B): the
/// solids the collision law sees in this context. A fixture is a plain
/// model wearing the marker - the rows are that model, placed by the
/// same `MoveFixture` any part uses - and the machine table is the one
/// row nobody can edit: it is derived prose, what the `.mch` says the
/// table is, never an entity. Import rides the model worker (no parse
/// in a command constructor) and tags on landing.
pub(super) fn fixtures_section(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    let u = state.units();
    let fixtures = camcraft_app::fixture::tagged(&state.world, ctx);
    let table = camcraft_app::fixture::table_box(&state.world, ctx);
    // A draft whose row vanished (deleted or untagged, here or via
    // undo) retires with it - the pane never edits a ghost.
    if !state.fixture_ui.name.is_empty()
        && !fixtures.iter().any(|(n, _)| n == &state.fixture_ui.name)
    {
        state.fixture_ui = FixtureUi::default();
    }
    state.retire_vise_opening_drafts();
    state.retire_plate_place_drafts();
    state.retire_mod_vise_drafts();
    state.retire_accessory_size_drafts();
    ui.label(if fixtures.is_empty() && table.is_none() {
        "No workholding modeled: the collision checks see stock and part only."
    } else {
        "Workholding in this setup's collision checks:"
    });
    for (name, entity) in fixtures {
        let loc = model::transform_of(&state.world, &entity).unwrap_or_default();
        let fixed = camcraft_app::fixture::def_of(&state.world, &entity).is_some_and(|d| d.fixed);
        super::fixture_rows::row(ui, state, &name, &loc, fixed);
    }
    if let Some(t) = table {
        ui.label(format!(
            "Machine/Table  span {} x {}  top Z {}  (from the machine file)",
            units::len(t.max.x - t.min.x, u, 1),
            units::len(t.max.y - t.min.y, u, 1),
            units::len(t.max.z, u, 2),
        ));
    }
    ui.horizontal_wrapped(|ui| {
        if ui.button("Import fixture…").clicked() {
            state.pending.push_back(CommandKind::ImportFixtureDialog);
        }
        // #788: a generic parametric vise, placed in one undoable step.
        if ui.button("Add built-in vise…").clicked() {
            state.pending.push_back(CommandKind::BuiltinViseDialog(ctx.unwrap_or_default().to_owned()));
        }
        // #789: parallels, blocks, strap clamps, soft jaws.
        if ui.button("Add accessory…").clicked() {
            state.pending.push_back(CommandKind::AccessoryDialog(ctx.unwrap_or_default().to_owned()));
        }
        // #819: a parametric grid plate, fixtures placed on its holes.
        if ui.button("Add fixture plate…").clicked() {
            state.pending.push_back(CommandKind::FixturePlateDialog(ctx.unwrap_or_default().to_owned()));
        }
        // The other way in: a model already imported - select its row
        // in the browser and mark it. Offered only for exactly one
        // selected model, so the button never guesses its target.
        let sel: Vec<String> = model::selected_names(&state.world)
            .into_iter()
            .filter(|n| {
                model::find_by_name(&state.world, n)
                    .is_some_and(|e| model::is_model(&state.world, &e))
            })
            .collect();
        if sel.len() == 1 && ui.button(format!("Mark {} as fixture", sel[0])).clicked() {
            state.pending.push_back(CommandKind::TagFixture {
                name: sel[0].clone(),
                setup: ctx.map(str::to_owned),
            });
        }
    });
}

// ----------------------------------------------------------------- tool page

/// The dock's tool surface is the LIBRARY (M1.4): what a tool in the
/// document is, in plain text, plus the verbs that point at it. The field
/// list moved to `tool_editor_window` - deliberately NO field widgets
/// live here, so every labeled field resolves in exactly one surface
/// (the a11y-query law the tests ride on).
fn tool_page(ui: &mut Ui, state: &mut State, name: &str) {
    let u = state.units();
    if !name.is_empty() && state.tool_ui.name != name {
        state.tool_ui.name = name.to_owned();
    }
    state.sync_tool_ui();
    // sync_tool_ui maintains `exists` from the world; the pane trusts it.
    let exists = state.tool_ui.exists;
    let nm = state.tool_ui.name.clone();
    if exists {
        ui.heading(format!("Tool: {nm}"));
        ui.separator();
        // The summary: the WORLD's numbers, not the draft's - what the
        // document holds right now, one line per fact.
        if let Some(t) =
            model::find_by_name(&state.world, &nm).and_then(|e| model::tool_of(&state.world, &e))
        {
            ui.label(format!(
                "{}  Ø{}  corner r {}",
                kind_name(t.kind),
                units::len(t.diameter, u, 2),
                units::len(t.tip_corner_r(), u, 2)
            ));
            ui.label(format!(
                "cut {}  shank Ø{}×{}  holder Ø{}×{}",
                units::len(t.flute_len, u, 1),
                units::len(t.shank_d, u, 1),
                units::len(t.shank_len, u, 1),
                units::len(t.holder_d, u, 1),
                units::len(t.holder_len, u, 1)
            ));
            ui.label(format!(
                "{} rpm  {} {}  {}",
                t.rpm,
                units::feed(t.feed, u),
                u.feed_suffix(),
                t.material_group
            ));
        }
        ui.add_space(4.0);
        let (edit, del) = ui
            .horizontal(|ui| (ui.button("Edit").clicked(), ui.button("Delete").clicked()))
            .inner;
        if del {
            state.pending.push_back(CommandKind::DeleteTool(nm.clone()));
        }
        if edit {
            actions::open_tool_editor(state, &nm);
        }
    } else {
        ui.heading("Tool library");
        ui.label("Pick a tool in the browser, or open the editor to create one.");
    }
    // Wrapped: the pane is narrow, and a clipped button is a hidden one.
    let (new, export, import) = ui
        .horizontal_wrapped(|ui| {
            let new = ui.button("New tool…").clicked();
            let export = ui
                .button("Export library…")
                .on_hover_text("write the document's tools to a CamCraft tool library file")
                .clicked();
            let import = ui
                .button("Import library…")
                .on_hover_text("import the tools of a CamCraft tool library file (one undo step; the mode beside it says what happens to a name the document has)")
                .clicked();
            tool_import_mode_combo(ui, &mut state.tool_import_mode);
            (new, export, import)
        })
        .inner;
    // #889: whether an import takes each tool's holder body from the file.
    let mut holders = state.import_tool_holders;
    if ui
        .checkbox(&mut holders, "Import tool holders")
        .on_hover_text("take each tool's holder body from the library file; unticked, tools arrive with their holder cylinder only")
        .changed()
    {
        state.set_import_tool_holders(holders);
    }
    if new {
        actions::open_tool_editor(state, "");
    }
    if export {
        state.export_tool_library_dialog();
    }
    if import {
        state.import_tool_library_dialog();
    }
    if let Some(msg) = state.error_for(ErrorScope::Tool).map(str::to_owned) {
        super::error_text::error_text(ui, super::colorvision::text_inks(ui, &state.world).error, &msg);
    }
    ui.separator();
    ui.heading("Catalog");
    for (cname, ct) in tools::catalog() {
        let taken = model::find_by_name(&state.world, cname).is_some();
        let clicked = ui
            .horizontal(|ui| {
                ui.label(format!(
                    "{}  Ø{} {}",
                    cname,
                    ct.diameter,
                    kind_name(ct.kind)
                ));
                if taken {
                    ui.label("(in document)");
                    false
                } else {
                    ui.button("Add").clicked()
                }
            })
            .inner;
        if clicked {
            state.pending.push_back(CommandKind::AddTool {
                name: cname.to_owned(),
                tool: ct,
            });
        }
    }
}

/// M1.4: the graphical tool editor - its own window, fields on the left,
/// a live to-scale side view on the right. An in-app `egui::Window`: the
/// shell has exactly one OS window and that law stands (RECOMMENDED.md).
/// Ephemeral subject like every pane: `state.tool_editor` says whether and
/// what it shows (None closed, Some(name) editing, Some("") a blank
/// draft); opening queues nothing, and the ghost law in
/// `State::drain_actions` closes it when the tool it names stops existing.
/// The numbers edit the shared `state.tool_ui` draft; only Apply/Add here
/// reach the world, through the same commands the dock's field list used
/// to carry. Delete stays a library verb (the dock owns it), which also
/// keeps every button label unique across the two surfaces.
pub(crate) fn tool_editor_window(ctx: &egui::Context, state: &mut State) {
    let u = state.units();
    let Some(_subject) = state.tool_editor.clone() else {
        return;
    };
    state.sync_tool_ui();
    let exists = state.tool_ui.exists;
    let title = if exists {
        format!("Tool: {}", state.tool_ui.name)
    } else {
        "New tool".to_owned()
    };
    let mut open = true;
    let mut close_clicked = false;
    Window::new(title)
        .id(Id::new("tool-editor"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        // #371 widened the window by the Body column (~270 px); its
        // default anchor moved left by as much, so it stays centred.
        .default_pos([
            ctx.viewport_rect().center().x - 520.0,
            ctx.viewport_rect().center().y - 210.0,
        ])
        .show(ctx, |ui| {
            // One snapshot of the draft drives the picture, so fields and
            // preview cannot disagree mid-frame; the DragValues below still
            // edit the live draft, and the next frame's snapshot follows.
            let preview_tool = state.tool_ui.to_tool();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(220.0);
                    if !exists {
                        // Typing the name is not an event; it rides the
                        // Add command at the bottom, same law as ever.
                        field(ui, "Name");
                        ui.add(
                            TextEdit::singleline(&mut state.tool_ui.name)
                                .desired_width(f32::INFINITY),
                        );
                    }
                    field(ui, "Type");
                    let before = state.tool_ui.kind;
                    let mut k = state.tool_ui.kind.min(TOOL_KINDS.len() - 1);
                    combo(ui, "tool_kind", &mut k, &TOOL_KINDS);
                    state.tool_ui.kind = k.min(TOOL_KINDS.len() - 1);
                    if state.tool_ui.kind != before {
                        // A draft inherited from a mill carries every
                        // specialty field inert (0.0); moving to a kind
                        // that READS them seeds the standard value, so
                        // flipping the combo alone never leaves the Add
                        // button refusing on a half-built tool.
                        if state.tool_ui.kind == 3
                            && (state.tool_ui.tip_angle.is_nan() || state.tool_ui.tip_angle <= 60.0)
                        {
                            state.tool_ui.tip_angle = 118.0;
                        }
                        if state.tool_ui.kind == 4
                            && (state.tool_ui.pitch.is_nan() || state.tool_ui.pitch <= 0.0)
                        {
                            state.tool_ui.pitch = 1.0;
                        }
                        if state.tool_ui.kind == 6
                            && (state.tool_ui.nose_radius.is_nan() || state.tool_ui.nose_radius <= 0.0)
                        {
                            state.tool_ui.nose_radius = 0.8;
                        }
                        // The boring bar's nose (M5.2 F): the same
                        // corner circle, read by the turning kind now -
                        // seed it the way the insert's row is, so a
                        // combo flip alone never leaves the bore op
                        // refusing at the kernel's point-tip door.
                        if state.tool_ui.kind == 5
                            && (state.tool_ui.nose_radius.is_nan() || state.tool_ui.nose_radius <= 0.0)
                        {
                            state.tool_ui.nose_radius = 0.4;
                        }
                        if state.tool_ui.kind == 7
                            && (state.tool_ui.width.is_nan() || state.tool_ui.width <= 0.0)
                        {
                            state.tool_ui.width = 2.0;
                        }
                        // The threading V (M5.2 D): crest width 0.866 x
                        // the seed op's pitch 2.0, and the 60-degree
                        // metric/unified flank angle - the two numbers
                        // the generator fits against the thread it cuts.
                        if state.tool_ui.kind == 8 {
                            if state.tool_ui.width.is_nan() || state.tool_ui.width <= 0.0 {
                                state.tool_ui.width = 1.732;
                            }
                            if state.tool_ui.tip_angle.is_nan()
                                || !(0.0..180.0).contains(&state.tool_ui.tip_angle)
                            {
                                state.tool_ui.tip_angle = 60.0;
                            }
                        }
                        // The form gauge (M5.2 E2): the house row's
                        // 4-wide band on a 6-radius crown - wide
                        // enough to state a valley, narrow enough to
                        // stand inside twice its own arc.
                        if state.tool_ui.kind == 9 {
                            if state.tool_ui.width.is_nan() || state.tool_ui.width <= 0.0 {
                                state.tool_ui.width = 4.0;
                            }
                            if state.tool_ui.nose_radius.is_nan()
                                || state.tool_ui.nose_radius <= 0.0
                            {
                                state.tool_ui.nose_radius = 6.0;
                            }
                        }
                        // The chamfer mill (M2.4 B): the shop's 90 -
                        // the supplement of the 45-degree face the
                        // catalog's chamfer seed stands at.
                        if state.tool_ui.kind == 10
                            && (state.tool_ui.tip_angle.is_nan()
                                || !(0.0..180.0).contains(&state.tool_ui.tip_angle))
                        {
                            state.tool_ui.tip_angle = 90.0;
                        }
                        // The torch (#670): no spindle, no corner - a
                        // kerf. Leaving it, a spinning kind needs an rpm.
                        if state.tool_ui.tool_kind() == tools::ToolKind::Torch {
                            state.tool_ui.rpm = 0.0;
                            state.tool_ui.corner_r = 0.0;
                        } else if before == 12 && (state.tool_ui.rpm.is_nan() || state.tool_ui.rpm <= 0.0) {
                            state.tool_ui.rpm = 10000.0;
                        }
                    }
                    let torch = state.tool_ui.tool_kind() == tools::ToolKind::Torch;
                    if torch {
                        // The kerf width IS the torch's diameter (#669 /
                        // #670): every offset reads radius = kerf / 2.
                        field(ui, "Kerf width");
                        num(ui, u, &mut state.tool_ui.diameter);
                        ui.label("the cut's width - about 1.5x the nozzle orifice; confirm it with a test cut");
                    } else {
                        field(ui, "Diameter");
                        num(ui, u, &mut state.tool_ui.diameter);
                    }
                    // Kind-read fields (the inert-unless-the-kind-reads-them
                    // law): a point angle means nothing on an end mill, a
                    // pitch means nothing on a drill - show each only where
                    // it feeds a generator or the collision profile.
                    if state.tool_ui.kind == 3 {
                        field(ui, "Point angle (deg)");
                        num_plain(ui, &mut state.tool_ui.tip_angle);
                        ui.label("cone tip, 60 to 180; standard twist drill is 118");
                    }
                    if state.tool_ui.kind == 4 {
                        field(ui, "Thread pitch");
                        num(ui, u, &mut state.tool_ui.pitch);
                        ui.label("distance advanced per turn - M6 is 1.0, M8 is 1.25");
                    }
                    if state.tool_ui.kind == 6 {
                        field(ui, "Insert nose radius");
                        num(ui, u, &mut state.tool_ui.nose_radius);
                        ui.label("the corner circle ground into the tip - it IS the cutting ball");
                    }
                    if state.tool_ui.kind == 5 {
                        field(ui, "Bar nose radius");
                        num(ui, u, &mut state.tool_ui.nose_radius);
                        ui.label("the ground tip circle the bar rides the hole wall on (M5.2 F) - inert on drilling cycles, which size by the slide");
                    }
                    // #630: which point of the nose the lathe posts write.
                    super::tool_orientation_row::orientation_row(ui, state);
                    // #891: a lathe steel's surface speed, feed per rev, clamp.
                    super::tool_turning_row::turning_rows(ui, state);
                    if state.tool_ui.kind == 7 {
                        field(ui, "Blade width");
                        num(ui, u, &mut state.tool_ui.width);
                        ui.label("the cutting edge along the axis - the kerf's width IS the tool");
                    }
                    if state.tool_ui.kind == 8 {
                        field(ui, "Blade width");
                        num(ui, u, &mut state.tool_ui.width);
                        ui.label("the V's crest width across the axis - 0.866 x the pitch it threads, and it must fit inside that pitch");
                        field(ui, "Form angle (deg)");
                        num_plain(ui, &mut state.tool_ui.tip_angle);
                        ui.label("the included angle of the V's flanks - 60 for the metric and unified forms, 55 for Whitworth");
                    }
                    if state.tool_ui.kind == 9 {
                        field(ui, "Band width");
                        num(ui, u, &mut state.tool_ui.width);
                        ui.label("the crown's ground edge across the axis - it must stand inside twice the crown radius");
                        field(ui, "Crown radius");
                        num(ui, u, &mut state.tool_ui.nose_radius);
                        ui.label("the arc of the ground edge in the work's meridian - the sagitta the verdict bends by");
                    }
                    if state.tool_ui.kind == 10 {
                        field(ui, "Cone angle (deg)");
                        num_plain(ui, &mut state.tool_ui.tip_angle);
                        ui.label("the included angle of the taper - 180 minus twice the chamfer's slope, so 90 cuts a 45-degree face");
                    }
                    if state.tool_ui.tool_kind() == tools::ToolKind::BallNose {
                        // #263: a ball's corner IS its ball, half the
                        // diameter (`Tool::tip_corner_r`) - shown, never
                        // edited, since no reader takes any other number.
                        field(ui, "Corner radius");
                        ui.label(format!("{} {}", units::len(state.tool_ui.diameter * 0.5, u, 3), u.suffix()));
                        ui.label("half the diameter - a ball nose's whole tip is its ball");
                    } else if !matches!(state.tool_ui.tool_kind(), tools::ToolKind::Tap | tools::ToolKind::Torch) {
                        field(ui, "Corner radius");
                        num(ui, u, &mut state.tool_ui.corner_r);
                    }
                    field(ui, "Flute length");
                    num(ui, u, &mut state.tool_ui.flute_len);
                    field(ui, "Shank diameter");
                    num(ui, u, &mut state.tool_ui.shank_d);
                    field(ui, "Shank length");
                    num(ui, u, &mut state.tool_ui.shank_len);
                    field(ui, "Holder diameter");
                    num(ui, u, &mut state.tool_ui.holder_d);
                    field(ui, "Holder length");
                    num(ui, u, &mut state.tool_ui.holder_len);
                    field(ui, "Material group");
                    ui.add(
                        TextEdit::singleline(&mut state.tool_ui.material_group)
                            .desired_width(140.0),
                    );
                    if !torch {
                        field(ui, "Spindle rpm");
                        num_plain(ui, &mut state.tool_ui.rpm);
                    }
                    field(ui, &format!("Feed ({})", u.feed_suffix()));
                    num_feed(ui, u, &mut state.tool_ui.feed);
                    // M11.3: an entry may plunge into solid only with a
                    // centre-cutting tool; otherwise it helixes, ramps,
                    // or the operation is refused by name.
                    ui.checkbox(&mut state.tool_ui.center_cutting, "Centre cutting (may plunge into solid)");
                    // #221: the machine's tool-table number - the post's
                    // T and H words. 0 leaves it to the list position.
                    field(ui, "Tool number (0 = list position)");
                    ui.add(egui::DragValue::new(&mut state.tool_ui.number).range(0..=camcraft_app::tool_numbers::MAX));
                });
                ui.vertical(|ui| {
                    // #371: the placed body's envelope rides the profile.
                    let body = if exists { super::tool_body_page::envelope(state) } else { None };
                    draw_tool_preview(ui, &preview_tool, u, body.as_deref());
                });
                ui.vertical(|ui| {
                    ui.set_width(260.0);
                    // #93: Body / Profile tabs for a milling tool.
                    super::tool_profile_page::third_column(ui, state, exists, u);
                });
            });
            ui.separator();
            let name_now = state.tool_ui.name.clone();
            let block = if name_now.trim().is_empty() {
                "Name the tool first.".to_owned()
            } else if !exists && model::find_by_name(&state.world, &name_now).is_some() {
                "That name is already in the document.".to_owned()
            } else {
                String::new()
            };
            if exists {
                if ui.button("Apply").clicked() {
                    // #93 slice 3: a landed drawn cutter rides the edit.
                    let apply = state.apply_tool_numbers(&name_now);
                    state.pending.push_back(apply);
                    push_number_if_changed(state, &name_now);
                }
            } else {
                let add = ui.add_enabled(block.is_empty(), egui::Button::new("Add tool"));
                if add.clicked() {
                    state.pending.push_back(CommandKind::AddTool {
                        name: name_now.trim().to_owned(),
                        tool: state.tool_ui.to_tool(),
                    });
                    push_number_if_changed(state, name_now.trim());
                }
                if !block.is_empty() {
                    ui.label(block);
                }
            }
            if let Some(msg) = state.error_for(ErrorScope::Tool).map(str::to_owned) {
                super::error_text::error_text(ui, super::colorvision::text_inks(ui, &state.world).error, &msg);
            }
            close_clicked = ui.button("Close").clicked();
        });
    if !open || close_clicked {
        state.close_tool_editor_window();
    }
}

/// #221: queue a `SetToolNumber` when the editor's number differs from the
/// tool's own (a new tool has none). Rides the same Apply / Add click as
/// the geometry, but as its own undoable command.
fn push_number_if_changed(state: &mut State, name: &str) {
    let want = (state.tool_ui.number > 0).then_some(state.tool_ui.number);
    let have = model::find_by_name(&state.world, name).and_then(|e| model::tool_number_of(&state.world, &e));
    if want != have {
        state.pending.push_back(CommandKind::SetToolNumber { name: name.to_owned(), number: want });
    }
}

/// The live preview: the tool drawn to scale from `tools::Tool::profile`,
/// the same band stack the collision checker sweeps, so what you see is
/// what `verify` would collide.
///
/// Pure state→paint: nothing here queues a command; it renders the draft
/// the fields beside it are editing. Tip at the bottom (z = 0 upward),
/// equal aspect both axes - "to scale" means to scale. A half-typed draft
/// renders a stub and a word, never a NaN.
fn draw_tool_preview(ui: &mut Ui, t: &tools::Tool, u: Unit, body: Option<&[(f64, f64, f64)]>) {
    const SIDE: f32 = 260.0;
    const PAD: f32 = 18.0;
    let (resp, painter) = ui.allocate_painter(Vec2::splat(SIDE), Sense::hover());
    let rect = resp.rect;
    let axis_x = rect.center().x;
    painter.rect_filled(rect, 4.0, Color32::from_rgb(24, 26, 31));
    let outline = t.profile();
    // #371: a body's bands (above, len, r) widen the frame to hold them.
    let bands = body.unwrap_or_default();
    let r_max = outline.iter().map(|q| q.1).chain(bands.iter().map(|b| b.2)).fold(0.0f64, f64::max);
    let z_max = outline.iter().map(|q| q.0).chain(bands.iter().map(|b| b.0 + b.1)).fold(0.0f64, f64::max);
    if r_max <= 0.0 || z_max <= 0.0 || !r_max.is_finite() || !z_max.is_finite() {
        painter.line_segment(
            [
                Pos2::new(axis_x, rect.min.y + PAD),
                Pos2::new(axis_x, rect.max.y - PAD),
            ],
            Stroke::new(1.0, NOTE),
        );
        painter.text(
            Pos2::new(axis_x, rect.center().y),
            Align2::CENTER_CENTER,
            "enter dimensions",
            FontId::proportional(11.0),
            NOTE,
        );
        return;
    }
    let base_y = rect.max.y - PAD;
    let scale = (((rect.width() - 2.0 * PAD) / (r_max as f32 * 2.0))
        .min((rect.height() - 2.0 * PAD) / z_max as f32))
    .max(f32::EPSILON);
    let at = |z: f64, r: f64| -> Pos2 {
        Pos2::new(axis_x + (r as f32 * scale), base_y - (z as f32 * scale))
    };
    // The axis first, so everything steel-colored lies over it.
    painter.line_segment(
        [
            Pos2::new(axis_x, rect.min.y + 6.0),
            Pos2::new(axis_x, rect.max.y - 6.0),
        ],
        Stroke::new(1.0, Color32::from_rgb(70, 74, 84)),
    );
    let zc = t.flute_len.max(0.0);
    let zs = zc + t.shank_len.max(0.0);
    let steel = [
        Color32::from_rgb(168, 178, 192), // cutting band
        Color32::from_rgb(122, 130, 142), // shank
        Color32::from_rgb(86, 92, 104),   // holder
    ];
    // Band fills, one quad per outline pair: each z-monotone symmetric
    // slice is convex, so it fills honestly - the whole stepped silhouette
    // is not (concave at every band step) and egui fills convex only.
    // Radius steps (z0 == z1) carry no area; the slice above paints them.
    for w in outline.windows(2) {
        let (z0, r0) = w[0];
        let (z1, r1) = w[1];
        if !(z0.is_finite() && z1.is_finite() && r0.is_finite() && r1.is_finite()) || z1 <= z0 {
            continue;
        }
        let zm = (z0 + z1) * 0.5;
        let band = (usize::from(zm >= zc) + usize::from(zm >= zs)).min(2);
        painter.add(Shape::convex_polygon(
            vec![at(z0, -r0), at(z0, r0), at(z1, r1), at(z1, -r1)],
            steel[band],
            Stroke::NONE,
        ));
    }
    // #371: the modelled body's revolved envelope, translucent amber over
    // the cylinders it replaces in the judging laws.
    for &(above, len, r) in bands {
        if !(above.is_finite() && len.is_finite() && r.is_finite()) || len <= 0.0 || r <= 0.0 {
            continue;
        }
        painter.rect(
            egui::Rect::from_two_pos(at(above, -r), at(above + len, r)),
            0.0,
            Color32::from_rgba_unmultiplied(230, 160, 60, 70),
            Stroke::new(1.0, Color32::from_rgb(230, 160, 60)),
            egui::StrokeKind::Inside,
        );
    }
    // The outline as one closed polyline (a stroke has no convexity rule):
    // right side up, then the mirrored left side back down. Axis points
    // (r = 0) appear on the right only - the mirror skips them so the tip
    // and top close with a single straight segment.
    let mut pts: Vec<Pos2> = Vec::with_capacity(outline.len() * 2);
    for &(z, r) in &outline {
        pts.push(at(z, r));
    }
    for &(z, r) in outline.iter().filter(|q| q.1 > 0.0).rev() {
        pts.push(at(z, -r));
    }
    painter.add(Shape::closed_line(
        pts,
        Stroke::new(1.0, Color32::from_rgb(214, 220, 228)),
    ));
    painter.text(
        Pos2::new(axis_x, rect.max.y - 4.0),
        Align2::CENTER_BOTTOM,
        format!("Ø {}", units::len(t.diameter, u, 2)),
        FontId::proportional(10.0),
        NOTE,
    );
    painter.text(
        Pos2::new(rect.min.x + 6.0, rect.min.y + 6.0),
        Align2::LEFT_TOP,
        format!("{} {} overall", units::len(z_max, u, 1), u.suffix()),
        FontId::proportional(10.0),
        NOTE,
    );
}

// ---------------------------------------------------------------- model page

fn model_page(ui: &mut Ui, state: &mut State, name: String) {
    let u = state.units();
    ui.heading(format!("Model: {name}"));
    let Some(entity) = model::find_by_name(&state.world, &name) else {
        ui.label("It is gone from the document.");
        return;
    };
    let tris = model::facets_of(&state.world, &entity).map_or(0, |f| f.len());
    ui.label(format!("{tris} triangle(s)"));
    // #864: a model imported coarser than the importer promises says so,
    // with the error it carries and what that does to the checks.
    if let Some(chord) = camcraft_app::model_fidelity::chord_of(&state.world, &entity) {
        ui.label(camcraft_app::model_fidelity::summary(&state.world, chord));
        hint(
            ui,
            "This model was too large to import at full detail. Gouge checks against it are \
             stricter to make up for that; when it is too coarse to check at all, its operations \
             are refused at the post. Raise the STEP facet budget below and import the file \
             again to clear this.",
        );
    }
    super::step_budget_row::row(ui, state, &entity);
    if let Some(a) = model::entity_world_aabb(&state.world, &entity) {
        ui.label(format!(
            "{} x {} x {} {}",
            units::len(a.max.x - a.min.x, u, 1),
            units::len(a.max.y - a.min.y, u, 1),
            units::len(a.max.z - a.min.z, u, 1),
            u.suffix()
        ));
    }
    // Placement (M5.2 G): the M3.2 flip grew its two missing angles.
    // The draft mirrors the world whenever the page moves to another
    // model; typing edits the draft, Apply (or a turn button) writes
    // through `MoveModel` - the numbers on screen and the geometry in
    // the viewport are the same Location, one write apart.
    if state.model_ui.name != name {
        let loc = model::transform_of(&state.world, &entity).unwrap_or_default();
        let (roll, pitch, yaw) = quat_to_euler(loc.rot);
        state.model_ui = ModelUi {
            name: name.clone(),
            x: loc.pos.x,
            y: loc.pos.y,
            z: loc.pos.z,
            roll_deg: roll.to_degrees(),
            pitch_deg: pitch.to_degrees(),
            yaw_deg: yaw.to_degrees(),
        };
    }
    ui.add_space(6.0);
    field(ui, "X");
    num(ui, u, &mut state.model_ui.x);
    field(ui, "Y");
    num(ui, u, &mut state.model_ui.y);
    field(ui, "Z");
    num(ui, u, &mut state.model_ui.z);
    field(ui, "Tilt about X (roll)");
    num_plain(ui, &mut state.model_ui.roll_deg);
    field(ui, "Tilt about Y (pitch)");
    num_plain(ui, &mut state.model_ui.pitch_deg);
    field(ui, "Turn about Z (yaw)");
    num_plain(ui, &mut state.model_ui.yaw_deg);
    ui.label("Quarter-turns about world axes (apply immediately):");
    const TURNS: [(&str, usize, f64); 6] = [
        ("X+", 0, 1.0),
        ("X-", 0, -1.0),
        ("Y+", 1, 1.0),
        ("Y-", 1, -1.0),
        ("Z+", 2, 1.0),
        ("Z-", 2, -1.0),
    ];
    for row in [0usize, 3] {
        ui.horizontal(|ui| {
            for (label, axis, sign) in TURNS[row..row + 3].iter() {
                if ui.button(format!("Turn {label}")).clicked() {
                    turn_model(state, *axis, *sign);
                }
            }
        });
    }
    ui.horizontal(|ui| {
        if ui.button("Apply").clicked() {
            apply_model_draft(state);
        }
    });
    // Pockets and slots (#511): recognition on a click, Create per row.
    super::pockets_page::section(ui, state, &name);
    // #561: the part into a fixture (or a fixture onto the part) by mates.
    super::locate_form::model_row(ui, state, &name);
    // Mesh repair (M13.11): weld and fill on the user's say-so, never on
    // import. The button queues `RepairModel`; the report lands in the
    // status line and stays here while this model is on the page.
    ui.add_space(6.0);
    ui.label("Mesh repair");
    field(ui, "Weld tolerance");
    num(ui, u, &mut state.repair_ui.weld_tol);
    field(ui, "Largest hole to fill (edges)");
    ui.add(DragValue::new(&mut state.repair_ui.max_hole_edges).range(3..=100_000));
    hint(
        ui,
        "Welds vertices closer than the tolerance, drops degenerate facets, then closes every \
         boundary loop up to the stated size by ear clipping in its plane. Non-manifold or \
         non-planar holes are refused by name; undo restores the facets.",
    );
    if ui.button("Repair mesh").clicked() {
        state.pending.push_back(CommandKind::RepairModel {
            name: name.clone(),
            weld_tol: state.repair_ui.weld_tol,
            max_hole_edges: state.repair_ui.max_hole_edges,
        });
    }
    if let Some((who, line)) = &state.repair_ui.last
        && *who == name
    {
        ui.label(format!("Last repair: {line}"));
    }
    let users: Vec<String> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_operation(&state.world, e))
        .filter_map(|(n, e)| {
            model::operation_of(&state.world, &e).and_then(|op| match &op.src {
                SrcRef::Entity(s) if *s == name => Some(n),
                _ => None,
            })
        })
        .collect();
    if users.is_empty() {
        ui.label("No operation uses it as a source.");
    } else {
        ui.label(format!("Source for: {}", users.join(", ")));
    }
    if model::stock_def_of(&state.world, &entity)
        .is_some_and(|s| matches!(s, StockShape::FromModel { source } if source == name))
    {
        ui.label("It is also the stock solid (Setup > Stock).");
    }
    ui.label("Selected now: Del removes it (undo restores).");
}

/// #899: the Import library mode picker (view state; the import is the command).
fn tool_import_mode_combo(ui: &mut egui::Ui, mode: &mut camcraft_app::commands::ImportMode) {
    use camcraft_app::commands::ImportMode;
    egui::ComboBox::from_id_salt("tool_import_mode")
        .selected_text(match mode {
            ImportMode::Add => "Add (rename duplicates)",
            ImportMode::Update => "Update same-named",
            ImportMode::Replace => "Replace library",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(mode, ImportMode::Add, "Add (rename duplicates)")
                .on_hover_text("an identical tool is skipped, a different tool of a taken name comes in as \"Name (2)\"");
            ui.selectable_value(mode, ImportMode::Update, "Update same-named")
                .on_hover_text("a same-named tool takes the file's definition in place (number kept; operations using it go Stale); new names are added");
            ui.selectable_value(mode, ImportMode::Replace, "Replace library")
                .on_hover_text("update, and tools the file lacks are removed - refused if an operation uses one");
        });
}

#[cfg(test)]
mod turn_tests {
    //! The turn/Level buttons are commands of their own now: the world -
    //! and with it the viewport gizmo - moves on the press. Moved verbatim
    //! from the retired imgui setup.rs at M-g; the code under test is
    //! pages.rs's own `turn_wcs`/`apply_wcs_drafts`.

    use super::*;
    use camcraft_app::geometry::{Point3, transform_point};

    fn place(state: &mut State, pos: [f64; 3], yaw_deg: f64) {
        state.pending.push_back(CommandKind::SetWcs {
            pos,
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg,
        });
        state.drain_actions();
    }

    fn location(state: &State, slot: &str) -> camcraft_app::schema::Location {
        let entity = model::find_by_name(&state.world, slot).expect("offset entity");
        model::transform_of(&state.world, &entity).expect("location")
    }

    #[test]
    fn a_turn_moves_the_world_not_just_the_drafts() {
        let mut state = State::new();
        place(&mut state, [10.0, 0.0, 0.0], 0.0);
        state.sync_setup_ui();
        turn_wcs(&mut state, 0, 1.0); // X+ quarter about the world X
        state.drain_actions();
        let loc = location(&state, "G54");
        assert_eq!([loc.pos.x, loc.pos.y, loc.pos.z], [10.0, 0.0, 0.0]);
        // Right-hand +90 about X takes +Z to -Y.
        let p = transform_point(Point3::new(0.0, 0.0, 1.0), loc.pos, loc.rot);
        assert!(
            [(p.x - 10.0), (p.y + 1.0), p.z]
                .iter()
                .all(|v| v.abs() < 1e-9),
            "{p:?}"
        );
    }

    #[test]
    fn four_turns_about_one_axis_return_to_the_start() {
        let mut state = State::new();
        place(&mut state, [0.0, 0.0, 0.0], 30.0);
        state.sync_setup_ui();
        for _ in 0..4 {
            turn_wcs(&mut state, 2, 1.0);
        }
        state.drain_actions();
        let loc = location(&state, "G54");
        // Same rotation as the 30-degree start: quaternion equal up to sign.
        let start = quat_from_euler(0.0, 0.0, 30.0f64.to_radians());
        let dot: f64 = start.iter().zip(loc.rot).map(|(a, b)| a * b).sum();
        assert!(
            (dot.abs() - 1.0).abs() < 1e-9,
            "closed the circle: {:?}",
            loc.rot
        );
    }

    #[test]
    fn turning_an_unplaced_wcs_applies_the_typed_position() {
        let mut state = State::new();
        state.wcs_ui.x = 5.0;
        turn_wcs(&mut state, 1, -1.0); // Y- quarter
        state.drain_actions();
        let loc = location(&state, "G54");
        assert_eq!([loc.pos.x, loc.pos.y, loc.pos.z], [5.0, 0.0, 0.0]);
        // Right-hand -90 about Y takes +X to +Z: the drafts, not just a
        // rotation, rode along.
        let p = transform_point(Point3::new(1.0, 0.0, 0.0), loc.pos, loc.rot);
        assert!(
            [(p.x - 5.0), p.y, (p.z - 1.0)]
                .iter()
                .all(|v| v.abs() < 1e-9),
            "{p:?}"
        );
    }

    /// M1.4: opening the editor is pure view state - the named path
    /// re-fetches the draft from the world (every field starts at the
    /// document's numbers, the law that lets the live preview be honest),
    /// the blank path starts a real draft numbered past what's taken, and
    /// neither queues a command.
    #[test]
    fn open_tool_editor_syncs_the_draft_from_the_world() {
        let mut state = State::new();
        let (name, tool) = tools::catalog()
            .into_iter()
            .find(|(n, _)| *n == "EM-0600-4FL")
            .expect("test fixture: the catalog has it");
        state.pending.push_back(CommandKind::AddTool {
            name: name.to_owned(),
            tool: tool.clone(),
        });
        state.drain_actions();
        actions::open_tool_editor(&mut state, name);
        assert_eq!(state.tool_editor.as_deref(), Some(name));
        assert!(state.pending.is_empty(), "opening an editor queues nothing");
        assert!(state.tool_ui.exists);
        assert_eq!(state.tool_ui.name, name);
        assert_eq!(state.tool_ui.diameter, tool.diameter);
        assert_eq!(state.tool_ui.shank_d, tool.shank_d);
        assert_eq!(state.tool_ui.holder_len, tool.holder_len);
        // The blank path: "Tool2" - numbered past the one tool the
        // document now holds, with the starter's plausible numbers.
        actions::open_tool_editor(&mut state, "");
        assert_eq!(state.tool_editor.as_deref(), Some(""));
        assert_eq!(state.tool_ui.name, "Tool2");
        assert!(!state.tool_ui.exists);
        assert!(state.pending.is_empty());
    }
}

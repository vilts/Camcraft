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

use egui::{DragValue, Id, ScrollArea, Ui, Window};

use camcraft_app::geometry::{quat_from_axis_angle, quat_from_euler, quat_mul, quat_to_euler};
use camcraft_app::model;
use camcraft_app::operation::SrcRef;
use camcraft_app::posting;
use camcraft_app::schema::PostRef;
use camcraft_app::stock::StockShape;
use camcraft_app::units;

use crate::actions;
use crate::state::{CommandKind, ErrorScope, ModelUi, State, UiTarget};

use super::layout::hint;
use super::theme::Tokens;

// The value widgets graduated to `fields` with the property grid (user
// review 2026-11-06); the setup pages below and the sibling modules keep
// reading them through this path.
pub(crate) use super::fields::{field, num, num_plain};

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
                super::stock_page::stock_page(ui, state);
                setup_error(ui, state);
            }
            UiTarget::Wcs => {
                state.sync_setup_ui();
                // #342: one WCS per setup; the word is its post label.
                let active = camcraft_app::setup::active(&state.world);
                ui.heading(camcraft_app::setup::wcs_label(&state.world, active.as_deref()));
                ui.separator();
                super::wcs_page::wcs_page(ui, state);
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
    // #902: the unit the program is written in - the document's own.
    super::post_options::units_row(ui, state, "Program units");
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
        if crate::browser_files::button(ui, state.browser_refusal_of(&CommandKind::SelectPostDialog), "Load post (.cps)", "") {
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
    super::post_options::explain_switch(ui, state);
    // #258: the program's number and comment, each one undoable command.
    super::program_info::program_rows(ui, state);
    super::post_options::threading_picker(ui, state);
    super::post_options::smoothing_switch(ui, state);
    super::post_options::end_units_picker(ui, state);
    super::post_options::between_setups_picker(ui, state);
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

// The WCS page lives in `wcs_page` (#739 split it out at the 400-line law).

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

/// The setup-sheet export row (M3.2 stage D). Writing a sheet is a
/// read of the document plus a file write - like posting - so the
/// button queues the dialog arm and nothing touches the history.
pub(super) fn sheet_footer(ui: &mut Ui, state: &mut State) {
    ui.horizontal(|ui| {
        if crate::browser_files::button(ui, state.browser_refusal_of(&CommandKind::SetupSheetDialog), "Export setup sheets…", "") {
            state.pending.push_back(CommandKind::SetupSheetDialog);
        }
        ui.label("(one page per setup, text)");
    });
}

pub(super) use super::fixtures_page::fixtures_section;

// ----------------------------------------------------------------- tool page

fn tool_page(ui: &mut Ui, state: &mut State, name: &str) {
    super::tool_settings::page(ui, state, name);
}

/// The creation and explicit tool editor share the Properties form.
pub(crate) fn tool_editor_window(ctx: &egui::Context, state: &mut State) {
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
            super::tool_settings::contents(ui, state, true);
            close_clicked = ui.button("Close").clicked();
        });
    if !open || close_clicked {
        state.close_tool_editor_window();
    }
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
        ui.label(camcraft_app::mesh_open::summary(&state.world, &entity, chord));
        // #1610: an incomplete model's line already says what to do.
        if camcraft_app::mesh_open::hole_of(&state.world, &entity).is_none() {
            hint(
            ui,
            "This model was too large to import at full detail. Gouge checks against it are \
             stricter to make up for that; when it is too coarse to check at all, its operations \
             are refused at the post. Raise the STEP facet budget below and import the file \
             again to clear this.",
            );
        }
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

#[cfg(test)]
#[path = "tap_hand_tests.rs"]
mod tap_hand_tests;

#[cfg(test)]
mod turn_tests {
    //! The page tests that stayed with pages.rs; the WCS turn tests moved
    //! with `turn_wcs` to `wcs_page_tests` (#739).

    use camcraft_app::tools;
    use super::*;

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

//! The operation page's frame (reorganised on the user's 2026-11-06
//! review: "interface is hard to read, badly organised"): a compact
//! label | control header for Tool, Source and Setup; ONE coloured status
//! banner that expands to the warning list on click; the section tab bar;
//! the open tab's content in its own scroll; and a fixed action bar at
//! the bottom, outside the scroll - Generate primary, Apply, Clear
//! toolpath and Close secondary, Delete operation apart and marked destructive.
//! Every change still rides `state.pending` / the command methods; tab
//! and banner state are view state, never commands.
//!
//! Provenance: our own port of the M-c operation page (no vendor
//! material, strings or layouts consulted).

use egui::{RichText, ScrollArea, Ui};

use camcraft_app::model;
use camcraft_app::operation::{self, OpStatus};
use camcraft_app::schema::Operation;
use camcraft_app::verify::Warning;

use crate::operations::{fold_warnings, unresolved_note};
use crate::start_exit_pick::{self, StartExitAct, act_id, armed_id};
use crate::state::{CommandKind, ErrorScope, State};
use crate::shortcut_keys::chord;

use super::colorvision::text_inks;
mod header;
use super::layout::{Action, ActionKind, BarInks, action_bar, group, prop_grid, status_banner_located};
use super::op_tabs::{SEC_FEEDS, SEC_GEO, SEC_LEADS, SEC_MACHINE, SEC_PASSES};
use super::theme::Tokens;

/// The hover text of a refusal line that names its spot (#261 part 2);
/// the structural test finds the link by it.
pub(crate) const REFUSAL_LINK_HINT: &str = "show where the operation was refused in the viewport";

/// Room the action bar (and the line above it) keeps under the scroll.
const BAR_RESERVE: f32 = 120.0;

/// The empty state of the properties pane when nothing is targeted and
/// nothing is selected (M9.1): the kind comes from the button you
/// pressed, so this page points at the doors that press it.
pub(super) fn no_operation_page(ui: &mut Ui) {
    ui.add_space(8.0);
    ui.heading("No operation selected");
    ui.add_space(4.0);
    ui.label("Select an operation in the browser to edit its parameters, or create a new one.");
    ui.add_space(10.0);
    ui.strong("Get started");
    ui.label("1. Set up stock, WCS, and tools in the Setup section.");
    ui.label("2. Create an operation from the ribbon (2D, 3D, Drill, or Multi-axis).");
    ui.label("3. Pick geometry, adjust parameters, then Generate and Simulate.");
    ui.add_space(8.0);
    ui.small("Tip: keep the Browser and Properties panes open while you work.");
}

pub(super) fn operation_page(ui: &mut Ui, state: &mut State, name: &str, pal: &Tokens) {
    state.sync_ops_ui();
    let target = name.to_owned();
    let live: Option<Operation> = model::find_by_name(&state.world, &target)
        .as_ref()
        .filter(|e| model::is_operation(&state.world, e))
        .and_then(|e| model::operation_of(&state.world, e));
    let label = model::find_by_name(&state.world, &target)
        .map(|e| model::operation_label(&state.world, &target, &e))
        .unwrap_or_else(|| target.clone());
    let exists = live.is_some();
    // The heights' context is the operation's declared setup, never the
    // active cursor: what the page echoes is what that operation's
    // generate will see.
    let live_setup = live.as_ref().and_then(|o| o.setup.clone());
    // Everything but the action bar scrolls; the scroll's own viewport is
    // also what bounds the page's width (the dock body may scroll
    // sideways), so the bar takes the same width under it.
    let max_h = (ui.available_height() - BAR_RESERVE).max(80.0);
    let out = ScrollArea::vertical().id_salt("op_page_scroll").max_height(max_h).auto_shrink([false, false]).show(ui, |ui| {
        // egui draws the vertical scrollbar over the viewport's right
        // edge. A control flush right in the control column - the box of
        // a check_row is 18 px - then sits under the bar and the bar
        // takes clicks meant for it. Stop the content short of that zone.
        super::layout::keep_scroll_gutter(ui, 120.0);
        ui.horizontal(|ui| {
            ui.heading(format!("Operation: {label}"));
            if exists && ui.small_button("Rename").clicked() {
                state.rename_draft = Some((target.clone(), label.clone()));
            }
        });
        if let Some(op) = &live {
            ui.label(format!("Type: {}", op.params.kind()));
        }
        let mut new_label = None;
        let mut cancel_rename = false;
        if let Some((key, draft)) = state.rename_draft.as_mut() && key == &target {
            ui.label("Name");
            let response = ui.text_edit_singleline(draft);
            ui.horizontal(|ui| {
                if ui.button("Apply name").clicked()
                    || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    new_label = Some(draft.clone());
                }
                cancel_rename = ui.button("Cancel").clicked();
            });
        }
        if let Some(new_label) = new_label {
            state.pending.push_back(CommandKind::RenameOperation { name: target.clone(), label: new_label });
            state.rename_draft = None;
        } else if cancel_rename {
            state.rename_draft = None;
        }
        crate::frame_cost::time("ui/properties.op.header", || prop_grid(ui, "op_header", |ui| header::header_rows(ui, state, &target, live.as_ref(), exists)));
        if exists {
            crate::frame_cost::time("ui/properties.op.status", || status(ui, state, &target, pal));
            needs_review_banner(ui, state, &target);
        }
        ui.separator();
        // The named sections (M1.4's tabs): a kind shows only the sections
        // it has fields for. The open tab is transient pane state
        // (`State::op_tab`), never a command, never saved.
        let kind = state.op_ui.params.kind();
        let tab = super::op_tabs::tab_bar(ui, kind, &mut state.op_tab, pal);
        if tab != SEC_LEADS {
            start_exit_pick::disarm(state);
        }
        if let Some(role) = state.op_ui.start_exit_pick.filter(|_| start_exit_pick::collecting(state)) {
            ui.label(
                RichText::new(format!("Picking {} - click on the part or stock in the viewport", role.label()))
                    .strong(),
            );
        } else {
            super::geo_page::pick_mode_line(ui, state);
        }
        if tab == SEC_GEO {
            crate::frame_cost::time("ui/properties.op.picks", || super::geo_page::pick_block(ui, state, &target, live_setup.as_deref(), pal));
        }
        // The single-concept tabs ride one titled group; Heights draws
        // one group per height and Lead in / Lead out its own groups.
        let title = match tab {
            SEC_PASSES => Some("Cutting passes"),
            SEC_FEEDS => Some("Feed rates"),
            SEC_MACHINE => Some("Bar feeder"),
            _ => None,
        };
        ui.ctx().data_mut(|d| {
            d.insert_temp(armed_id(), state.op_ui.start_exit_pick);
            d.insert_temp(act_id(), None::<StartExitAct>);
        });
        let fields = crate::frame_cost::span("ui/properties.op.fields");
        match title {
            Some(t) => {
                group(ui, t, None, |ui| super::params_page::params_fields(ui, state, tab));
            }
            None => super::params_page::params_fields(ui, state, tab),
        }
        if let Some(act) = ui.ctx().data(|d| d.get_temp::<Option<StartExitAct>>(act_id())).flatten() {
            match act {
                StartExitAct::Toggle(role) => start_exit_pick::toggle(state, role),
                StartExitAct::ClearIf(role) => {
                    if state.op_ui.start_exit_pick == Some(role) {
                        start_exit_pick::disarm(state);
                    }
                }
            }
            ui.ctx().request_repaint();
        }
        drop(fields);
        // #3324: a kind that does not take a start or exit pick says so
        // by name, on the tab where the contour's rows sit (else Geometry).
        let picks_tab = if super::op_tabs::has_section(kind, SEC_LEADS) { SEC_LEADS } else { SEC_GEO };
        if tab == picks_tab {
            let u = state.units();
            // #3333: the sinker kinds type their start and exit points here.
            super::sinker_pick_rows::sinker_pick_groups(ui, u, &mut state.op_ui.params);
            // #3328: the hole kinds type their first and last hole here.
            super::start_exit_rows::hole_order_groups(ui, u, &mut state.op_ui.params);
            super::start_exit_rows::not_taken_groups(ui, kind);
            // #3331: a turning kind's picks, typed in the lathe (Z, X) plane.
            super::turning_start_exit_rows::meridian_groups(ui, state.units(), &mut state.op_ui.params);
        }
        if tab == SEC_FEEDS {
            // M13.2: material, edge, flutes and the suggestion.
            super::feeds_page::cutting_group(ui, state, &target);
        }
        if tab == SEC_GEO {
            // The Boundary group (M11.5): how far the area reaches, under
            // the picks that say where it is.
            crate::frame_cost::time("ui/properties.op.boundary", || super::boundary::boundary_group(ui, state));
        }
        if let Some(note) = crate::frame_cost::time("ui/properties.op.unresolved", || {
            unresolved_note(&state.world, live_setup.as_deref(), Some(&state.op_ui.src()), &state.op_ui.params)
        }) {
            ui.colored_label(text_inks(ui, &state.world).warn, note);
        }
    });
    ui.set_max_width((out.inner_rect.width() - 12.0).max(120.0));
    crate::frame_cost::time("ui/properties.op.actions", || actions(ui, state, &target, live.as_ref(), exists, pal));
}

/// The status banner's line and details, from the world (pure; the
/// tests read it). M10.1 C: the tool law's verdict outranks the stamp.
pub(super) fn status_text(state: &State, target: &str) -> (String, super::layout::Tone, Vec<String>) {
    use super::layout::Tone;
    let ent = model::find_by_name(&state.world, target);
    let (moves, warnings, notes): (usize, Vec<Warning>, Vec<String>) = ent
        .as_ref()
        // #2201: read in place - a clone of the whole path each frame.
        .and_then(|e| model::with_toolpath(&state.world, e, |tp| (tp.path.len(), tp.warnings.clone(), tp.path.notes.clone())))
        .unwrap_or((0, Vec::new(), Vec::new()));
    let blocked = ent.as_ref().and_then(|e| operation::tool_gate::tool_problem_of(&state.world, e));
    let st = ent.as_ref().map(|e| operation::status_of(&state.world, e));
    let mut details: Vec<String> = fold_warnings(&warnings);
    details.extend(super::approach_page::entry_status_lines(&notes));
    // #339 slice 4: who the fixture warnings strike, or that no single
    // fixture reproduces them - said, never guessed.
    details.extend(state.struck_lines(target));
    let n = details.len();
    let tail = match n {
        0 => String::new(),
        1 => " · 1 warning".to_owned(),
        n => format!(" · {n} warnings"),
    };
    let (line, tone) = match (blocked, st) {
        (Some(why), _) => (format!("Blocked · {why} · {moves} moves cached"), Tone::Error),
        (None, Some(OpStatus::Never) | None) => ("Status: never generated".to_owned(), Tone::Quiet),
        (None, Some(OpStatus::Ok)) => (format!("OK · {moves} moves{tail}"), if n > 0 { Tone::Warn } else { Tone::Ok }),
        (None, Some(OpStatus::Stale)) => {
            // M3.1 C: say which input moved, not just that one did.
            let reason = ent
                .as_ref()
                .and_then(|e| operation::stale_reason(&state.world, e))
                .unwrap_or_else(|| "inputs moved since generation".to_owned());
            (format!("Stale · {reason} · {moves} moves cached{tail}"), Tone::Warn)
        }
    };
    (line, tone, details)
}

fn status(ui: &mut Ui, state: &mut State, target: &str, pal: &Tokens) {
    use super::layout::Tone;
    let (line, tone, details) = status_text(state, target);
    // #261: a warning line that names a move frames it on click (the
    // fold lines lead `details`, aligned with `warning_focus`).
    let focus: Vec<Option<[f64; 3]>> = model::find_by_name(&state.world, target)
        .and_then(|e| model::with_toolpath(&state.world, &e, crate::path_lines::warning_focus))
        .unwrap_or_default();
    let links: Vec<bool> = focus.iter().map(Option::is_some).collect();
    let inks = text_inks(ui, &state.world);
    let ink = match tone {
        Tone::Ok => pal.accent,
        Tone::Warn => inks.warn,
        Tone::Error => inks.error,
        Tone::Quiet => pal.text_dim,
    };
    if let Some(p) = status_banner_located(ui, "op_status", &line, ink, &details, &links)
        .and_then(|i| focus.get(i).copied().flatten())
    {
        // View state, like every camera move: no command (AGENTS.md).
        crate::actions::frame_at(state, p);
    }
}

/// Migration warning banner for un-carriable parameters dropped on open (#1517).
fn needs_review_banner(ui: &mut Ui, state: &mut State, target: &str) {
    let notes = model::find_by_name(&state.world, target)
        .map(|e| model::operation_meta_of(&state.world, &e).needs_review)
        .unwrap_or_default();
    if notes.is_empty() {
        return;
    }
    let warn = text_inks(ui, &state.world).warn;
    ui.group(|ui| {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(warn, format!("Needs review: {}", notes.join("; ")));
            if ui
                .button("Mark reviewed")
                .on_hover_text("Clear the migration review warning so this operation can post")
                .clicked()
            {
                state.pending.push_back(CommandKind::MarkOperationReviewed(target.to_owned()));
            }
        });
    });
}

/// The fixed bottom bar, with the one refusal line above it.
fn actions(ui: &mut Ui, state: &mut State, target: &str, live: Option<&Operation>, exists: bool, pal: &Tokens) {
    let unchanged = live.is_some_and(|o| o.params == state.op_ui.params && o.area == state.op_ui.area);
    // Apply never reads as a dead button: the hint says why it is dark,
    // or which edited field the door will refuse (crate::feedback).
    let apply_hint = crate::feedback::apply_hint(live.map(|o| &o.params), &state.op_ui.params, unchanged);
    let has_path =
        exists && model::find_by_name(&state.world, target).is_some_and(|e| model::has_toolpath(&state.world, &e));
    let inks = text_inks(ui, &state.world);
    if let Some(msg) = state.error_for(ErrorScope::Operation).map(str::to_owned) {
        // #261 part 2: a refusal that names its spot is a link that
        // frames it (view state, like every camera move - no command).
        match state.refusal_focus() {
            Some(p) => {
                let text = egui::RichText::new(&msg).color(inks.error).underline();
                let r = super::error_text::error_rich(ui, text, &msg, true);
                if r.on_hover_text(REFUSAL_LINK_HINT).clicked() {
                    crate::actions::frame_at(state, p);
                }
            }
            None => {
                super::error_text::error_text(ui, inks.error, &msg);
            }
        }
    } else if !unchanged && let Some(why) = &apply_hint {
        // Enabled, but the door will refuse: a safety line, not help.
        ui.colored_label(inks.warn, why);
    }
    // Agie-only: an inspection source on a setup that mounts the Agie machine.
    let live_inspection = live.filter(|op| operation::has_agie_inspection_source(op)
        && super::agie_inspection::agie_mounted(&state.world, op.setup.as_deref()));
    if live_inspection.is_some_and(|op| matches!(&op.params, operation::OpParams::WireContour(w) if w.condition.is_empty())) {
        super::layout::hint(ui, "Nominal inspection only. No machining condition is assigned.");
    }
    let commit_hint = format!("Commit the edited parameters ({} undoes)", chord("Z"));
    let clear_hint = format!("Remove the generated path and keep the operation ({} restores it)", chord("Z"));
    let delete_hint = format!("Delete the operation and its toolpath ({} restores both)", chord("Z"));
    let inspect = live_inspection
        .map(|_| CommandKind::ExportAgieInspection(target.to_owned()));
    let inspection_refusal = inspect.as_ref().and_then(|cmd| state.browser_refusal_of(cmd)).or_else(|| {
        (!unchanged || (inspect.is_some() && live.is_some_and(|op| op.src != state.op_ui.src())))
            .then_some("Apply the changes before exporting")
    });
    let bar = [
        Action { label: "Generate", kind: ActionKind::Primary, enabled: true, hint: Some("Compute the toolpath (applies pending edits first)"), why_not: None },
        Action {
            label: "Apply",
            kind: ActionKind::Secondary,
            enabled: !unchanged,
            hint: apply_hint.as_deref().or(Some(commit_hint.as_str())),
            why_not: apply_hint.as_deref(),
        },
        Action {
            label: "Clear toolpath",
            kind: ActionKind::Secondary,
            enabled: has_path,
            hint: Some(clear_hint.as_str()),
            why_not: Some("Nothing generated yet - there is no toolpath to clear"),
        },
        Action {
            label: "Close",
            kind: ActionKind::Secondary,
            enabled: true,
            hint: Some("Close this editor without deleting the operation"),
            why_not: None,
        },
        Action {
            label: "Delete operation",
            kind: ActionKind::Destructive,
            enabled: exists,
            hint: Some(delete_hint.as_str()),
            why_not: Some("The operation is gone"),
        },
        Action {
            label: "Export Agie inspection setup",
            kind: ActionKind::Secondary,
            enabled: inspection_refusal.is_none(),
            hint: Some("Nominal inspection only. Physical stock XY is unknown. The native piece uses zero length, width and working steps."),
            why_not: inspection_refusal,
        },
    ];
    let visible = if inspect.is_some() { &bar[..] } else { &bar[..5] };
    let bar_inks = BarInks { accent: pal.accent, on_accent: super::contrast::on_fill(pal.accent), danger: inks.error };
    match action_bar(ui, visible, &bar_inks) {
        Some(0) => {
            start_exit_pick::disarm(state);
            state.generate_operation(target);
        }
        Some(1) => {
            start_exit_pick::disarm(state);
            state.apply_operation_params(target);
        }
        Some(2) => state.pending.push_back(CommandKind::ClearToolpath(target.to_owned())),
        Some(3) => state.close_operation_editor(target),
        Some(4) => state.pending.push_back(CommandKind::DeleteOperation(target.to_owned())),
        Some(5) => state.pending.extend(inspect),
        _ => {}
    }
}

#[cfg(test)]
#[path = "agie_inspection_tests.rs"]
mod agie_inspection_tests;

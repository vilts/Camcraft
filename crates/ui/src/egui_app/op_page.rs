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

use egui::{ScrollArea, Ui};

use camcraft_app::model;
use camcraft_app::operation::{self, OpStatus};
use camcraft_app::schema::Operation;
use camcraft_app::verify::Warning;

use crate::operations::{fold_warnings, unresolved_note};
use crate::state::{CommandKind, ErrorScope, State};

use super::colorvision::text_inks;
use super::fields::combo_owned;
use super::layout::{Action, ActionKind, BarInks, action_bar, group, prop_grid, status_banner_located};
use super::op_tabs::{SEC_FEEDS, SEC_GEO, SEC_MACHINE, SEC_PASSES};
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
    ui.heading("Operations");
    ui.separator();
    ui.label("No operation to show.");
    ui.label("Click one in the browser, or pick a kind from the ribbon's family doors - the new operation opens its own page straight away.");
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
        prop_grid(ui, "op_header", |ui| header_rows(ui, state, &target, live.as_ref(), exists));
        if exists {
            status(ui, state, &target, pal);
        }
        ui.separator();
        // The named sections (M1.4's tabs): a kind shows only the sections
        // it has fields for. The open tab is transient pane state
        // (`State::op_tab`), never a command, never saved.
        let kind = state.op_ui.params.kind();
        let tab = super::op_tabs::tab_bar(ui, kind, &mut state.op_tab, pal);
        if tab == SEC_GEO {
            super::geo_page::pick_block(ui, state, &target, live_setup.as_deref());
        }
        // The single-concept tabs ride one titled group; Heights draws
        // one group per height and Lead in / Lead out its own groups.
        let title = match tab {
            SEC_PASSES => Some("Cutting passes"),
            SEC_FEEDS => Some("Feed rates"),
            SEC_MACHINE => Some("Bar feeder"),
            _ => None,
        };
        match title {
            Some(t) => {
                group(ui, t, None, |ui| super::params_page::params_fields(ui, state, tab));
            }
            None => super::params_page::params_fields(ui, state, tab),
        }
        if tab == SEC_FEEDS {
            // M13.2: material, edge, flutes and the suggestion.
            super::feeds_page::cutting_group(ui, state, &target);
        }
        if tab == SEC_GEO {
            // The Boundary group (M11.5): how far the area reaches, under
            // the picks that say where it is.
            super::boundary::boundary_group(ui, state);
        }
        if let Some(note) =
            unresolved_note(&state.world, live_setup.as_deref(), Some(&state.op_ui.src()), &state.op_ui.params)
        {
            ui.colored_label(text_inks(ui, &state.world).warn, note);
        }
    });
    ui.set_max_width((out.inner_rect.width() - 12.0).max(120.0));
    actions(ui, state, &target, live.as_ref(), exists, pal);
}

/// Tool, Source and Setup as label | control rows of the header grid.
fn header_rows(ui: &mut Ui, state: &mut State, target: &str, live: Option<&Operation>, exists: bool) {
    let mut tools = state.tool_names();
    if !state.op_ui.tool.is_empty() && !tools.contains(&state.op_ui.tool) {
        tools.push(state.op_ui.tool.clone());
    } else if state.op_ui.tool.is_empty()
        && let Some(first) = tools.first()
    {
        state.op_ui.tool = first.clone();
    }
    ui.label("Tool");
    if tools.is_empty() {
        ui.label("none").on_hover_text("no tools in the document - the Tools page has a catalog");
    } else {
        let mut t_idx = tools.iter().position(|t| *t == state.op_ui.tool).unwrap_or(0);
        let before = t_idx;
        ui.horizontal(|ui| { super::tool_icons::named_glyph(ui, &state.world, &state.op_ui.tool); combo_owned(ui, "op_tool", &mut t_idx, &tools); });
        if t_idx != before
            && let Some(t) = tools.get(t_idx)
        {
            state.op_ui.tool = t.clone();
            if exists {
                state.pending.push_back(CommandKind::EditOperationLinks {
                    name: target.to_owned(),
                    tool: Some(state.op_ui.tool.clone()),
                    src: None,
                });
            }
        }
    }
    ui.end_row();
    coolant_row(ui, state, target, live);

    // Source: the stock or one of the user's parts - never a work
    // offset (G54...), a setup or a fixture (user review 2026-11-06).
    let models: Vec<String> = model::model_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| !model::is_fixture(&state.world, e))
        .map(|(n, _)| n)
        .collect();
    ui.label("Source");
    let mut src_items: Vec<String> = vec!["Stock".to_owned()];
    src_items.extend(models.iter().cloned());
    let mut s_idx: usize = if state.op_ui.src_kind != 0 && !state.op_ui.src_name.is_empty() {
        match models.iter().position(|m| *m == state.op_ui.src_name) {
            Some(p) => 1 + p,
            None => {
                state.op_ui.src_kind = 0;
                state.op_ui.src_name.clear();
                0
            }
        }
    } else {
        0
    };
    if state.op_ui.src_kind == 2 {
        s_idx = 0;
    }
    let before = s_idx;
    combo_owned(ui, "op_src", &mut s_idx, &src_items);
    if s_idx != before {
        match s_idx.checked_sub(1).and_then(|i| models.get(i)) {
            None => {
                state.op_ui.src_kind = 0;
                state.op_ui.src_name.clear();
            }
            Some(m) => {
                state.op_ui.src_kind = 1;
                state.op_ui.src_name = m.clone();
            }
        }
        if exists {
            state.pending.push_back(CommandKind::EditOperationLinks {
                name: target.to_owned(),
                tool: None,
                src: Some(state.op_ui.src()),
            });
        }
    }
    ui.end_row();

    // Setup membership (M3.2 A): moving is never a stamp edit -
    // SetOperationSetup keeps the stamp and lets the resolved-Z digest
    // say whether the op went Stale.
    ui.label("Setup");
    let setups = camcraft_app::setup::names(&state.world);
    if setups.is_empty() {
        ui.label("global (no setups)").on_hover_text("this document has no setups yet - the global stock and offsets apply");
    } else {
        let cur = live.and_then(|o| o.setup.clone()).or_else(|| setups.first().cloned());
        let mut s_idx = setups.iter().position(|n| Some(n) == cur.as_ref()).unwrap_or(0);
        let before = s_idx;
        combo_owned(ui, "op_setup", &mut s_idx, &setups);
        if s_idx != before
            && let Some(n) = setups.get(s_idx)
        {
            state.pending.push_back(CommandKind::SetOperationSetup { name: target.to_owned(), setup: Some(n.clone()) });
        }
    }
    ui.end_row();
}

/// The Coolant row (#112), under the tool it feeds: the fluid the post
/// switches on for this operation. A committed operation only - a draft
/// takes the default (Flood) at creation and offers the row once it
/// exists, so the row never edits a value no command would carry.
fn coolant_row(ui: &mut Ui, state: &mut State, target: &str, live: Option<&Operation>) {
    let Some(op) = live else { return };
    use camcraft_app::operation::CoolantMode;
    ui.label("Coolant").on_hover_text(
        "the cutting fluid the post switches on for this operation - the machine's own code for it; a choice the machine states no code for blocks the post by name",
    );
    let labels: Vec<String> = CoolantMode::ALL.iter().map(|m| m.label().to_owned()).collect();
    let mut idx = CoolantMode::ALL.iter().position(|m| *m == op.coolant).unwrap_or(1);
    let before = idx;
    combo_owned(ui, "op_coolant", &mut idx, &labels);
    if idx != before
        && let Some(m) = CoolantMode::ALL.get(idx)
    {
        state.pending.push_back(CommandKind::EditOperationCoolant { name: target.to_owned(), coolant: *m });
    }
    ui.end_row();
}

/// The status banner's line and details, from the world (pure; the
/// tests read it). M10.1 C: the tool law's verdict outranks the stamp.
pub(super) fn status_text(state: &State, target: &str) -> (String, super::layout::Tone, Vec<String>) {
    use super::layout::Tone;
    let ent = model::find_by_name(&state.world, target);
    let (moves, warnings, notes): (usize, Vec<Warning>, Vec<String>) = ent
        .as_ref()
        .and_then(|e| model::toolpath_of(&state.world, e))
        .map(|tp| (tp.path.len(), tp.warnings, tp.path.notes))
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
        .and_then(|e| model::toolpath_of(&state.world, &e))
        .map(|tp| crate::path_lines::warning_focus(&tp))
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

/// The fixed bottom bar, with the one refusal line above it.
fn actions(ui: &mut Ui, state: &mut State, target: &str, live: Option<&Operation>, exists: bool, pal: &Tokens) {
    let unchanged = live.is_some_and(|o| o.params == state.op_ui.params && o.area == state.op_ui.area);
    // Apply never reads as a dead button: the hint says why it is dark,
    // or which edited field the door will refuse (crate::feedback).
    let apply_hint = crate::feedback::apply_hint(live.map(|o| &o.params), &state.op_ui.params, unchanged);
    let has_path =
        exists && model::find_by_name(&state.world, target).and_then(|e| model::toolpath_of(&state.world, &e)).is_some();
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
    let bar = [
        Action { label: "Generate", kind: ActionKind::Primary, enabled: true, hint: Some("Compute the toolpath (applies pending edits first)"), why_not: None },
        Action {
            label: "Apply",
            kind: ActionKind::Secondary,
            enabled: !unchanged,
            hint: apply_hint.as_deref().or(Some("Commit the edited parameters (Ctrl+Z undoes)")),
            why_not: apply_hint.as_deref(),
        },
        Action {
            label: "Clear toolpath",
            kind: ActionKind::Secondary,
            enabled: has_path,
            hint: Some("Remove the generated path and keep the operation (Ctrl+Z restores it)"),
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
            hint: Some("Delete the operation and its toolpath (Ctrl+Z restores both)"),
            why_not: Some("The operation is gone"),
        },
    ];
    let bar_inks = BarInks { accent: pal.accent, on_accent: super::contrast::on_fill(pal.accent), danger: inks.error };
    match action_bar(ui, &bar, &bar_inks) {
        Some(0) => state.generate_operation(target),
        Some(1) => state.apply_operation_params(target),
        Some(2) => state.pending.push_back(CommandKind::ClearToolpath(target.to_owned())),
        Some(3) => state.close_operation_editor(target),
        Some(4) => state.pending.push_back(CommandKind::DeleteOperation(target.to_owned())),
        _ => {}
    }
}

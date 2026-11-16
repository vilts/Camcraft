//! The browser's operation menu (split from `panes`, #343): a right-click
//! on an operation row - or on a setup's Operations row, for Paste - opens
//! the verbs and the retype list; every row is one undoable command.
//! Provenance: our own port of the imgui tree menu (our code); no HSMWorks
//! material consulted.

use egui::{Rect, Sense, Ui, Vec2};

use camcraft_app::model;
use camcraft_app::operation;

use crate::actions;
use crate::state::{CommandKind, State, TreeMenuTarget, UiTarget};

use super::browser_tree::Leaf;
use super::tree_entry;
use super::panes::{DROP_GAP, DROP_W};
use super::theme::Tokens;

/// The tree menu's verb rows, keyed past every kind ordinal.
const MENU_CLEAR: usize = usize::MAX;
const MENU_DELETE: usize = usize::MAX - 1;
const MENU_RENAME: usize = usize::MAX - 2;
const MENU_COPY: usize = usize::MAX - 3;
const MENU_PASTE: usize = usize::MAX - 4;
/// #110: suppress / unsuppress, and the after-action row that steps
/// through `AfterOperation::ALL`.
const MENU_SUPPRESS: usize = usize::MAX - 5;
const MENU_AFTER: usize = usize::MAX - 6;

/// The after-action row's text: what happens now; a click steps on.
fn after_row_label(a: operation::AfterOperation) -> &'static str {
    match a {
        operation::AfterOperation::None => "After: continue (click to change)",
        operation::AfterOperation::OptionalStop => "After: optional stop (M1)",
        operation::AfterOperation::ProgramStop => "After: program stop (M0)",
        operation::AfterOperation::GoHome => "After: go home",
    }
}

/// The operation's retype menu (M9.1): what replaced the page's Type
/// combo. Grouping is the retired combo's law restated once -
/// `Section::ORDER` headers, `KindInfo::admits` hiding a section none
/// of whose kinds the machine can run (M13.5 B: the lathe's Drilling
/// header fronts the axial Drill alone, every other row of it dark),
/// and the current kind's section staying listed even when the
/// machine says no, so a mid-edit machine swap never erases the
/// operation's own name. A kind row needs a second click before it queues
/// `ChangeOperationKind` - one undo unit, the numbers carried by the
/// one retype law below; a dark row is queued too, because the refusal
/// belongs at the command door (`family_gate`), where it names itself
/// on the status line - never a silent dead pixel. Hand-drawn like the
/// ribbon's dropdowns (the panel is geometry, not a Widget): an Area
/// at Foreground order, so a press on a row belongs to the row, never
/// to the tree behind it; any other primary press closes it.
pub(super) fn tree_menu(ui: &mut Ui, state: &mut State, pal: &Tokens, rect: Rect, leaves: &[Leaf]) {
    let Some(target) = state.tree_menu.clone() else {
        return;
    };
    // #339 slice 4: model rows and a setup's Fixtures node carry the
    // fixture verbs, drawn by their own menu.
    if matches!(target, TreeMenuTarget::Model { .. } | TreeMenuTarget::Fixtures(_)) {
        super::fixture_menu::fixture_menu(ui, state, pal, rect, leaves, &target);
        return;
    }
    // A setup's Operations row anchors Paste: the active setup's (the
    // menu's click activated it), else - for the frame its activation is
    // still queued - the first one.
    let active = camcraft_app::setup::active(&state.world).unwrap_or_default();
    let row_ix = leaves.iter().position(|leaf| match (&target, leaf) {
        (TreeMenuTarget::Operation(name), Leaf::Op(row)) => name == row,
        (TreeMenuTarget::Operations, Leaf::SetupOps(setup)) => *setup == active,
        (TreeMenuTarget::Entry(name), Leaf::Entry(row)) => name == row,
        _ => false,
    }).or_else(|| match target {
        TreeMenuTarget::Operations => leaves.iter().position(|leaf| matches!(leaf, Leaf::SetupOps(_))),
        TreeMenuTarget::Operation(_)
        | TreeMenuTarget::Entry(_)
        | TreeMenuTarget::Model { .. }
        | TreeMenuTarget::Fixtures(_) => None,
    });
    let Some(ix) = row_ix else {
        state.tree_menu = None;
        state.retype_confirm = None;
        return;
    };
    // #155: a Pause / Home entry's own menu (gone - the menu closes).
    let entry = match &target {
        TreeMenuTarget::Entry(n) => {
            let Some(e) = model::find_by_name(&state.world, n)
                .and_then(|e| camcraft_app::program_entry::entry_of(&state.world, &e))
            else {
                state.tree_menu = None;
                state.retype_confirm = None;
                return;
            };
            Some((n.clone(), e))
        }
        _ => None,
    };
    let name = match &target {
        TreeMenuTarget::Operation(name) => {
            if model::find_by_name(&state.world, name)
                .and_then(|entity| model::operation_of(&state.world, &entity))
                .is_none()
            {
                state.tree_menu = None;
                state.retype_confirm = None;
                return;
            }
            Some(name.as_str())
        }
        TreeMenuTarget::Operations
        | TreeMenuTarget::Entry(_)
        | TreeMenuTarget::Model { .. }
        | TreeMenuTarget::Fixtures(_) => None,
    };
    let here = name.and_then(|name| model::find_by_name(&state.world, name))
        .and_then(|entity| model::operation_of(&state.world, &entity))
        .map(|op| op.params.kind().ordinal());
    // #110: the op's suppression and after-action, for the menu rows.
    let flow = name.and_then(|name| model::find_by_name(&state.world, name))
        .and_then(|entity| model::operation_of(&state.world, &entity))
        .map(|op| (op.suppressed, op.after));
    // #388: an op's rows (its retype) read its own setup's machine, the
    // one the retype gate judges; a new op joins the active setup.
    let machine = match name.and_then(|name| model::find_by_name(&state.world, name))
        .and_then(|entity| model::operation_of(&state.world, &entity))
    {
        Some(op) => camcraft_app::machine::kind_of_setup(&state.world, op.setup.as_deref()),
        None => camcraft_app::machine::kind_of(&state.world),
    };
    // The rows: section headers and kinds, in registry order, read
    // from the same taxonomy as the ribbon's cells - never a list.
    let mut lines: Vec<(&'static str, bool, Option<usize>)> = Vec::new();
    // The row's verbs first: clearing the generated path (keeps the
    // operation) and deleting the operation are two different asks,
    // and the menu names both - each one undoable command.
    lines.push(("Operation", false, None));
    if name.is_some() {
        lines.push(("Rename operation", true, Some(MENU_RENAME)));
        lines.push(("Copy operation", true, Some(MENU_COPY)));
    }
    lines.push(("Paste operation", state.operation_clipboard.is_some(), Some(MENU_PASTE)));
    if name.is_some() {
        lines.push(("Clear toolpath", true, Some(MENU_CLEAR)));
        lines.push(("Delete operation (undoable)", true, Some(MENU_DELETE)));
        // #110: beside the verbs, so they stay on screen however long
        // the kind list below runs.
        let suppressed = flow.is_some_and(|(s, _)| s);
        lines.push((if suppressed { "Unsuppress operation" } else { "Suppress operation" }, true, Some(MENU_SUPPRESS)));
        // One row that steps through the after-actions (a click is one
        // undoable edit), so the menu grows by two rows, not six.
        lines.push((after_row_label(flow.map_or(operation::AfterOperation::None, |(_, a)| a)), true, Some(MENU_AFTER)));
    }
    // #155: a Pause / Home entry after this row (a setup's: at its end),
    // last of the verbs so every row above keeps its place.
    lines.extend(tree_entry::add_lines());
    let here_section = here.and_then(|index| operation::KindInfo::ALL.get(index).map(|k| k.section));
    for s in operation::Section::ORDER.into_iter().filter(|_| name.is_some()) {
        // The current kind's own section keeps every row (dark where
        // the machine says no); any other section lists only what the
        // door admits, and leaves the menu when that is nothing.
        let rows = if here_section == Some(s) {
            s.kinds()
        } else {
            s.kinds_for(machine)
        };
        if rows.is_empty() {
            continue;
        }
        lines.push((s.label(), false, None));
        for i in rows {
            if let Some(k) = operation::KindInfo::ALL.get(i) {
                lines.push((k.label, k.admits(machine), Some(i)));
            }
        }
    }
    if let Some((_, e)) = &entry {
        lines = tree_entry::entry_lines(e);
    }
    let anchor = egui::Pos2::new(
        rect.left(),
        rect.top() + (ix + 1) as f32 * pal.row_height + DROP_GAP,
    );
    let panel = Rect::from_min_size(
        anchor,
        Vec2::new(DROP_W, lines.len() as f32 * pal.row_height),
    );
    let mut chosen: Option<usize> = None;
    egui::Area::new(ui.id().with(("tree-menu", ix)))
        .fixed_pos(anchor)
        .order(egui::Order::Foreground)
        .movable(false)
        .show(ui.ctx(), |ui| {
            let p = ui.painter();
            p.rect_filled(panel, pal.radius_menu, pal.ink);
            p.rect_stroke(
                panel,
                pal.radius_menu,
                egui::Stroke::new(1.0, pal.border),
                egui::StrokeKind::Outside,
            );
            for (k, (label, runnable, item)) in lines.iter().enumerate() {
                let r = Rect::from_min_size(
                    egui::Pos2::new(anchor.x, anchor.y + k as f32 * pal.row_height),
                    Vec2::new(DROP_W, pal.row_height),
                );
                let is_current = item.is_some_and(|i| Some(i) == here);
                if is_current {
                    p.rect_filled(r, 0.0, pal.selection_bg);
                }
                let sense = if item.is_some() { Sense::click() } else { Sense::hover() };
                let resp = ui.interact(r, ui.id().with(("tree-menu-row", k)), sense);
                if item.is_some() && resp.hovered() && !is_current {
                    p.rect_filled(r, 0.0, pal.field_hover);
                }
                let armed = name.is_some_and(|name| state.retype_confirm.as_ref()
                    .is_some_and(|(pending_name, pending_kind)|
                        pending_name == name && *item == Some(*pending_kind)));
                let text = if is_current {
                    format!("{label} (current)")
                } else if armed {
                    format!("Confirm change to {label}")
                } else {
                    (*label).to_owned()
                };
                let (kind, live) = if item.is_some() {
                    (egui::WidgetType::Button, *runnable && !is_current)
                } else {
                    (egui::WidgetType::Label, true)
                };
                resp.widget_info(|| egui::WidgetInfo::labeled(kind, live, &text));
                p.text(
                    r.left_center() + Vec2::new(8.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    &text,
                    egui::FontId::proportional(pal.font_row),
                    match item {
                        None => pal.text_dim,
                        Some(_) if !runnable || is_current => pal.text_faint,
                        Some(_) => pal.text,
                    },
                );
                if item.is_some()
                    && resp.clicked()
                    && (*runnable || *item != Some(MENU_PASTE))
                {
                    chosen = *item;
                }
            }
        });
    if let Some(i) = chosen {
        if let Some((n, e)) = &entry {
            tree_entry::act(state, i, n, e);
        } else if tree_entry::add(state, i, name) {
            // queued: the add rows of an operation or a setup's list
        } else if i == MENU_PASTE {
            actions::paste_operation(state);
        } else if let Some(name) = name {
            if i == MENU_RENAME {
                let label = model::find_by_name(&state.world, name)
                    .map(|e| model::operation_label(&state.world, name, &e))
                    .unwrap_or_else(|| name.to_owned());
                state.rename_draft = Some((name.to_owned(), label));
                actions::target(state, UiTarget::Operation(name.to_owned()));
            } else if i == MENU_COPY {
                actions::copy_operation(state, name);
            } else if i == MENU_CLEAR {
                state.pending.push_back(CommandKind::ClearToolpath(name.to_owned()));
            } else if i == MENU_DELETE {
                state.pending.push_back(CommandKind::DeleteOperation(name.to_owned()));
            } else if i == MENU_SUPPRESS {
                let suppressed = !flow.is_some_and(|(s, _)| s);
                state.pending.push_back(CommandKind::SuppressOperation { name: name.to_owned(), suppressed });
            } else if i == MENU_AFTER {
                let all = operation::AfterOperation::ALL;
                let now = flow.map_or(operation::AfterOperation::None, |(_, a)| a);
                let k = all.iter().position(|a| *a == now).unwrap_or(0);
                let after = all.get((k + 1) % all.len()).copied().unwrap_or_default();
                state.pending.push_back(CommandKind::EditOperationAfter { name: name.to_owned(), after });
            } else if Some(i) != here {
                let choice = (name.to_owned(), i);
                if state.retype_confirm.as_ref() == Some(&choice) {
                    state.pending.push_back(CommandKind::ChangeOperationKind {
                        name: name.to_owned(), kind: i,
                    });
                } else {
                    state.retype_confirm = Some(choice);
                    return;
                }
            }
        }
        state.tree_menu = None;
        state.retype_confirm = None;
    } else if ui.input(|i| {
        i.pointer.button_pressed(egui::PointerButton::Primary)
            && i.pointer.hover_pos().is_none_or(|pos| !panel.contains(pos))
    }) {
        // The dropdowns' close law: a primary press that is nobody's
        // row is the menu's - it never stacks and never stalks a
        // document the user has moved on from.
        state.tree_menu = None;
        state.retype_confirm = None;
    }
}

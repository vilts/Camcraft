//! The browser pane (#343, split from `panes`): the global view toggles,
//! then the painted tree with its click law. A setup node's radio queues
//! `ActivateSetup` (undoable); its row opens the setup's settings. Every
//! node UNDER a setup opens that setup's page - and when the setup is not
//! the active one it is activated first (the same undoable command), so
//! the Stock, WCS, Machine, Post and Fixtures editors - which edit the
//! active setup's furniture and mount - never edit a setup the clicked
//! node does not belong to.
//! Provenance: our own port of the imgui browser (our code) and the
//! maintainer's per-node behaviour (#343); no HSMWorks material consulted.

use egui::{Rect, Sense, Ui, Vec2};

use camcraft_app::{model, setup};

use crate::actions;
use crate::state::{CommandKind, SetupPage, State, TreeMenuTarget, UiTarget};
use crate::viewport::ProjectionMode;

use super::browser_tree::{Leaf, browser_rows};
use super::icons::IconSet;
use super::painted;
use super::theme::{Tokens, clamp};

/// The browser pane.
pub fn browser(ui: &mut Ui, state: &mut State, pal: &Tokens, icons: Option<&IconSet>) {
    // One compact chrome row: the view
    // switches behind a door, and the two whole-tree gestures beside
    // it. The tree starts right under it instead of under five rows
    // of checkboxes.
    egui::Frame::NONE
        .fill(pal.strip_fill())
        .inner_margin(egui::Margin::symmetric(
            clamp::range(pal.tree_pad_x, clamp::SPACING).round() as i8,
            clamp::range(pal.tree_pad_y, clamp::SPACING).round() as i8,
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let _ = egui::containers::menu::MenuButton::new("View")
                    .ui(ui, |ui| view_menu(ui, state));
                ui.separator();
                if ui
                    .button("Expand all")
                    .on_hover_text("Open the Setup, Models and Tools sections")
                    .clicked()
                {
                    state.browser_expanded = [true; 3];
                }
                if ui
                    .button("Collapse all")
                    .on_hover_text("Close the Setup, Models and Tools sections")
                    .clicked()
                {
                    state.browser_expanded = [false; 3];
                }
            });
        });
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(2.0);
    let (rows, leaves) = browser_rows(state);
    // The top button row can widen `ScrollArea`'s virtual width past the
    // visible pane; clamp to the clip edge so eye toggles stay unclipped and
    // hit-testable at their AccessKit centers.
    let vis_w = (ui.clip_rect().right() - ui.cursor().min.x).max(0.0);
    let rect = Rect::from_min_size(
        ui.cursor().min,
        Vec2::new(ui.available_width().min(vis_w), rows.len() as f32 * pal.row_height),
    );
    let hit = painted::tree(ui, rect, &rows, pal, icons);
    drag_ops(ui, state, pal, rect, &leaves, hit.drag_started);
    if let Some(ix) = hit.radio
        && let Some(Leaf::Setup(name)) = leaves.get(ix)
    {
        focus(state, name);
    }
    if let Some(leaf) = hit.eye.and_then(|ix| leaves.get(ix)) {
        eye(state, leaf);
    }
    if let Some(leaf) = hit.row.and_then(|ix| leaves.get(ix)).cloned() {
        click(state, leaf);
    }
    // Operation rows offer the operation verbs; a setup's Operations row
    // offers Paste even when it has no operation rows yet (activating
    // the setup, so the menu anchors on the row that was clicked). Model
    // rows offer Use / Stop using as fixture, a setup's Fixtures node
    // Import fixture… (#339 slice 4).
    if let Some(ix) = hit.menu {
        let target = match leaves.get(ix) {
            Some(Leaf::Op(name)) => Some(TreeMenuTarget::Operation(name.clone())),
            Some(Leaf::Entry(name)) => Some(TreeMenuTarget::Entry(name.clone())),
            Some(Leaf::SetupOps(setup)) => {
                focus(state, setup);
                Some(TreeMenuTarget::Operations)
            }
            // #339 slice 4: the fixture verbs.
            Some(Leaf::Model(name)) => Some(TreeMenuTarget::Model { name: name.clone(), row: ix }),
            Some(Leaf::Tool(name)) => Some(TreeMenuTarget::Tool { name: name.clone(), row: ix }),
            Some(Leaf::Page(setup, SetupPage::Fixtures)) => {
                focus(state, setup);
                Some(TreeMenuTarget::Fixtures(setup.clone()))
            }
            _ => None,
        };
        state.tree_menu = target.filter(|target| state.tree_menu.as_ref() != Some(target));
        state.retype_confirm = None;
    }
    super::tree_menu::tree_menu(ui, state, pal, rect, &leaves);
    ui.allocate_rect(rect, Sense::hover());
}

/// Make `setup` the active setup (one undoable `ActivateSetup`) unless it
/// already is, or it is the implicit setup of a setup-free document.
pub(super) fn focus(state: &mut State, name: &str) {
    if name.is_empty() || setup::active(&state.world).as_deref() == Some(name) {
        return;
    }
    state.pending.push_back(CommandKind::ActivateSetup(Some(name.to_owned())));
}

/// The View door's contents: the same switches the inline block used to
/// show, now one click behind the header.
fn view_menu(ui: &mut Ui, state: &mut State) {
    if ui.button("Tool library…").clicked() {
        actions::target(state, UiTarget::Tool(String::new()));
        ui.close();
    }
    ui.separator();
    ui.checkbox(&mut state.show_world_axes, "Show world origin axes");
    // #645: the #727 gizmo names get the switch the review asked for.
    // Same local-copy law as "Show all toolpaths": the flip goes
    // through a State method, which takes the user's choice out to
    // the view file with it.
    let mut show_names = state.show_gizmo_names;
    if ui.checkbox(&mut show_names, "Show origin names").changed() {
        state.set_gizmo_names(show_names);
    }
    let mut show_all_paths = state.show_all_paths;
    if ui.checkbox(&mut show_all_paths, "Show all toolpaths").changed() {
        if show_all_paths {
            state.show_all_toolpaths();
        } else {
            state.hide_all_toolpaths();
        }
    }
    // #1567: the machine's work envelope. Enabled only with a machine
    // mounted; the tooltip says why nothing is drawn when its travel
    // figures are missing. View state, never a command.
    let mounted = crate::work_envelope::machine_mounted(state);
    let tip = if mounted {
        crate::work_envelope::tooltip(state)
    } else {
        "Mount a machine on the active setup to show its work envelope.".to_owned()
    };
    ui.add_enabled(mounted, egui::Checkbox::new(&mut state.show_work_envelope, "Show work envelope"))
        .on_hover_text(tip)
        .on_disabled_hover_text("Mount a machine on the active setup to show its work envelope.");
    // #2632: the operator-side arrow of the envelope. View state too.
    let envelope_on = mounted && state.show_work_envelope;
    ui.add_enabled(envelope_on, egui::Checkbox::new(&mut state.show_operator_side, crate::operator_side::SWITCH))
        .on_hover_text(crate::operator_side::TIP)
        .on_disabled_hover_text(crate::operator_side::TIP_OFF);
    // The display mode moved to the viewport's Display menu (#2818).
    // Projection is view state the same way (#434): orthographic keeps
    // a side view straight - parallel edges, no converging - which is
    // what CAM eyeballing wants most of the time.
    ui.horizontal(|ui| {
        ui.label("Projection");
        ui.radio_value(&mut state.cam.projection, ProjectionMode::Perspective, "Perspective");
        ui.radio_value(&mut state.cam.projection, ProjectionMode::Orthographic, "Ortho");
    });
}

fn ctx(name: &str) -> Option<&str> {
    (!name.is_empty()).then_some(name)
}

pub(super) fn eye(state: &mut State, leaf: &Leaf) {
    match leaf {
        Leaf::Model(n) | Leaf::Sketch(_, n) => State::toggle_visibility(&mut state.hidden_models, n),
        Leaf::Stock(s) => {
            let name = setup::stock_entity_name(&state.world, ctx(s));
            State::toggle_visibility(&mut state.hidden_stocks, &name);
        }
        Leaf::Wcs(s) => {
            let name = setup::wcs_entity_name(&state.world, ctx(s));
            State::toggle_visibility(&mut state.hidden_offsets, &name);
        }
        Leaf::Op(n) => State::toggle_visibility(&mut state.hidden_paths, n),
        Leaf::Setup(n) => state.toggle_setup_display(n),
        _ => {}
    }
}

pub(super) fn click(state: &mut State, leaf: Leaf) {
    match leaf {
        Leaf::Section => {}
        Leaf::Parent(section) => {
            let expanded = &mut state.browser_expanded[section.index()];
            *expanded = !*expanded;
        }
        // Target-only, like the Tool rows: browsing setups never arms Del.
        Leaf::Setups => actions::target(state, UiTarget::Setups),
        Leaf::Setup(name) => actions::target(state, UiTarget::SetupNode(name, SetupPage::Settings)),
        Leaf::Stock(s) => {
            focus(state, &s);
            actions::target(state, UiTarget::Stock);
            let name = setup::stock_entity_name(&state.world, ctx(&s));
            if model::find_by_name(&state.world, &name).is_some() {
                state.pending.push_back(CommandKind::Select(Some(name)));
            }
        }
        Leaf::Wcs(s) => {
            focus(state, &s);
            actions::target(state, UiTarget::Wcs);
            let name = setup::wcs_entity_name(&state.world, ctx(&s));
            if model::find_by_name(&state.world, &name).is_some() {
                state.pending.push_back(CommandKind::Select(Some(name)));
            }
        }
        Leaf::Page(s, page) => {
            focus(state, &s);
            actions::target(state, UiTarget::SetupNode(s, page));
        }
        Leaf::SetupOps(s) => focus(state, &s),
        Leaf::Model(n) => {
            actions::target(state, UiTarget::Model(n.clone()));
            state.pending.push_back(CommandKind::Select(Some(n)));
        }
        // #1863: a profile opens its Stock page (its placement lives
        // there) and is selected, so Del deletes it (undoable).
        Leaf::Sketch(s, n) => {
            focus(state, &s);
            actions::target(state, UiTarget::Stock);
            state.pending.push_back(CommandKind::Select(Some(n)));
        }
        // Target only: no Select, so browsing tools never arms Del.
        Leaf::Tool(n) => actions::target(state, UiTarget::Tool(n)),
        Leaf::Op(n) => {
            actions::target(state, UiTarget::Operation(n.clone()));
            state.pending.push_back(CommandKind::Select(Some(n)));
        }
        // #155: selected, so Del deletes it (undoable); no page of its own.
        Leaf::Entry(n) => state.pending.push_back(CommandKind::Select(Some(n))),
    }
}

/// Drag an operation row onto another of the SAME setup: the insertion
/// line and the `ReorderOperation` command use the same target row.
fn drag_ops(ui: &mut Ui, state: &mut State, pal: &Tokens, rect: Rect, leaves: &[Leaf], started: Option<usize>) {
    if let Some(ix) = started
        && let Some(Leaf::Op(name) | Leaf::Entry(name)) = leaves.get(ix)
    {
        state.dragging_op = Some(name.clone());
    }
    let Some(source) = state.dragging_op.clone() else {
        return;
    };
    let (released, pointer) = ui.input(|i| (i.pointer.any_released(), i.pointer.hover_pos()));
    let drop = pointer.filter(|p| rect.contains(*p)).and_then(|p| {
        let ix = ((p.y - rect.top()) / pal.row_height) as usize;
        let (Leaf::Op(target) | Leaf::Entry(target)) = leaves.get(ix)? else { return None };
        // #155: entries drag and take drops like operations.
        let setup_of = |name: &str| {
            model::find_by_name(&state.world, name)
                .and_then(|e| camcraft_app::program_entry::effective_setup(&state.world, &e))
        };
        let after = p.y > rect.top() + (ix as f32 + 0.5) * pal.row_height;
        (source != *target && setup_of(&source) == setup_of(target)).then_some((ix, target.clone(), after))
    });
    if let Some((ix, target, after)) = drop {
        let y = rect.top() + (ix + usize::from(after)) as f32 * pal.row_height;
        ui.painter().hline(
            rect.x_range(),
            y,
            egui::Stroke::new(clamp::range(pal.highlight_stroke_px, clamp::STROKE), pal.accent),
        );
        if released {
            state.pending.push_back(CommandKind::ReorderOperation { name: source, target, after });
        }
    }
    if released {
        state.dragging_op = None;
    }
}
// force rebuild

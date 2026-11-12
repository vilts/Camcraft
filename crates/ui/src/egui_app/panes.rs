//! The egui shell's pane layer, graduated from the spike's panels.rs -
//! every pane a faithful re-implementation of its imgui original (the
//! retired ribbon.rs/browser.rs): same strings, same command shapes, same
//! enabled rules. The strips are painted (super::painted) because that is
//! the law the spike proved; the property pages moved to `pages` with M-c,
//! leaving this module the keyboard layer and the two painted panes. Since
//! M-g the bundled Tabler glyphs ride both strips as egui textures
//! (super::icons), with the painted vector marks as their no-GPU twins -
//! the retired imgui icons module folded into this stack, not replaced.
//!
//! Layering holds (AGENTS.md): world access is `camcraft_app`'s public API
//! only, and every change goes through `state.pending` / the command
//! methods, never an ad-hoc field poke. The whole-pane law
//! (docs/spike-egui-go-no-go.md) forbids porting a pane by halves.
//!
//! Since M-f the surfaces here stopped forking their own command logic: the
//! ribbon cells, browser rows and keyboard shortcuts all fire through the
//! shared `crate::actions` layer - the same one the imgui ribbon/browser/menu
//! call - so each effect lives exactly once across both shells. `Cmd::Paths`
//! stays a direct field flip (view state, mirroring imgui ribbon.rs) and the
//! browser pushes its `Select` alongside `actions::target` (the tree law).

use egui::{Key, Label, Rect, RichText, Sense, Ui, Vec2};

use camcraft_app::model;
use camcraft_app::operation;

use crate::actions;
use crate::machine_modes;
use crate::build_info;
use crate::state::{CommandKind, Sim, State, UiTarget};

use super::icons::{Icon, IconSet};
use super::op_icons::op_icon;
use super::layout;
use super::painted;
use super::theme::Tokens;

/// The operation kinds in panel order - the app's kind registry labels
/// (M5.0.2), one list for the whole surface. Shared with pages.rs: the
/// Type combo there and the ribbon tips here quote the same table.
pub(crate) const TYPES: [&str; camcraft_app::operation::KIND_COUNT] =
    camcraft_app::operation::ALL_LABELS;

// ---------------------------------------------------------------- shortcuts

/// The shell's keyboard layer: history keys always, destructive and file
/// keys only while no text field owns focus (imgui's io.WantTextInput
/// guard - a dialog must never spawn mid-type). Every arm fires through
/// `crate::actions`, the same entry the ribbon uses.
pub fn shortcuts(ui: &mut Ui, state: &mut State) {
    let typing = ui.memory(|m| m.focused().is_some());
    let (modifiers, n, o, s, p, q, z, y, del, f, c, v) = ui.input(|i| {
        (
            i.modifiers,
            i.key_pressed(Key::N),
            i.key_pressed(Key::O),
            i.key_pressed(Key::S),
            i.key_pressed(Key::P),
            i.key_pressed(Key::Q),
            i.key_pressed(Key::Z),
            i.key_pressed(Key::Y),
            i.key_pressed(Key::Delete),
            i.key_pressed(Key::F),
            i.key_pressed(Key::C),
            i.key_pressed(Key::V),
        )
    });
    let cmd = modifiers.command;
    if cmd && z && !modifiers.shift && !modifiers.alt {
        actions::undo(state);
    }
    if cmd && y {
        // Ctrl+Y and Ctrl+Shift+Y both redo (imgui law).
        actions::redo(state);
    }
    if cmd && !typing {
        if c && let Some(name) = state.selected_operation() {
            actions::copy_operation(state, &name);
        }
        if v {
            actions::paste_operation(state);
        }
        // The File tips' keys (ribbon.rs strings): N new, O a model,
        // Shift+O a project, S save, Shift+S save as.
        if n {
            actions::new_document(state);
        }
        if o && !modifiers.shift {
            actions::open_model_dialog(state);
        }
        if o && modifiers.shift {
            actions::open_project(state);
        }
        if s {
            if modifiers.shift {
                actions::save_project_as(state);
            } else {
                actions::save_project(state);
            }
        }
        if p {
            // M1.7b: posting joins the file lane - the same dialog law
            // as Save (the gate's refusals arrive through the queue arm).
            actions::post_program(state);
        }
        if q {
            // The window close button (CloseRequested) always quit; since
            // M-g it has a keyboard sibling. imgui's only quit surface was
            // its File menu, which retired with the imgui shell - Ctrl+Q
            // keeps that capability on the shared law (Quit -> request_quit
            // -> exit_requested -> el.exit()).
            actions::quit(state);
        }
    }
    if del && !typing {
        actions::delete_selected(state);
    }
    if f && !typing && !cmd && !modifiers.alt {
        actions::frame_all(state);
    }
    if p && !typing && !cmd && !modifiers.alt {
        // #434: bare P flips perspective/ortho - a viewport-layer key
        // like F (Ctrl+P is the post lane above).
        actions::toggle_projection(state);
    }
}

// -------------------------------------------------------------------- ribbon

/// One ribbon cell: command identity plus the state it reads for enabled,
/// toggled, label and tip. The table is the whole group layout - command
/// for command since M6.2 divided the Create row into the catalog's
/// family sections; the only hand-typed group left is the anchors below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Cmd {
    New,
    Model,
    Project,
    Save,
    SaveAs,
    Post,
    /// The setups door (#322): opens the Setups page, the one place a
    /// document's first setup is created - a fresh document has no
    /// other way there.
    Setups,
    Stock,
    Origin,
    Tools,
    /// The machine door (the ribbon law): one cell that opens the
    /// workspaces of `machine_modes::MACHINE_MODES` and wears the
    /// current one's plain caption. Switching mounts the kind's
    /// built-in reference through `LoadBuiltinMachine`, so the change
    /// rides undo like every other setup move.
    Machine,
    Create(usize),
    /// The dropdown button of a folded section (M6.2): it carries the
    /// section's label, never a faceless chevron - the cell's whole
    /// caption is that label plus the combo's own arrow glyph.
    More(operation::Section),
    Undo,
    Redo,
    Run,
    Delete,
    Frame,
    Paths,
    Simulate,
    Deviate,
    /// The View door: one cell whose list toggles the dock's closable
    /// panels and resets the layout (layout.rs). View state, no command.
    View,
    /// The Theme door (#441): the user's look - the built-in and
    /// discovered themes plus the colour-vision remap - right of the
    /// View door. Opening it is view state; a pick queues `SelectTheme`
    /// / `SelectColorVision`, kept as a user preference (#877).
    Theme,
}

/// The four anchor groups: file work opens the row, setup follows, and
/// the run/judge groups close it. The middle is the registry's (below):
/// the old hand-typed eight-cell Create group was M1.4.3's debt, home
/// to kinds 0..=7 while the table grew - the registry counts the
/// kinds, this comment never must.
const FILE: (&str, &[Cmd]) =
    ("File", &[Cmd::New, Cmd::Model, Cmd::Project, Cmd::Save, Cmd::SaveAs, Cmd::Post]);
const SETUP: (&str, &[Cmd]) =
    ("Setup", &[Cmd::Setups, Cmd::Stock, Cmd::Origin, Cmd::Tools, Cmd::Machine]);

/// The bar's whole op-shortcut budget (the ribbon law): at most this
/// many Create cells stand inline, and they are the user's pins. The
/// rest of the catalog lives behind the section doors at every width.
const MAX_PINS: usize = 4;
const MODIFY: (&str, &[Cmd]) = ("Modify", &[Cmd::Undo, Cmd::Redo, Cmd::Run, Cmd::Delete]);
const INSPECT: (&str, &[Cmd]) =
    ("Inspect", &[Cmd::Frame, Cmd::Paths, Cmd::Simulate, Cmd::Deviate]);
const VIEW: (&str, &[Cmd]) = ("View", &[Cmd::View, Cmd::Theme]);

/// One ribbon group before layout: its caption and every cell it would
/// show in a row that never ran out of width. `section` marks the
/// registry-seeded Create groups - only those fold into a dropdown.
#[derive(Clone, Debug)]
pub(super) struct Group {
    pub title: &'static str,
    pub cells: Vec<Cmd>,
    pub section: Option<operation::Section>,
}

/// The whole ribbon as data (M6.2's home law): the anchors, the user's
/// pinned kinds, then every catalog section the document's machine can
/// run. Hiding, not darkening - the mill document's table carries no
/// Turning cells to grey out; `KindInfo::admits` is the single word
/// this filter and the command door share (M13.5 B: per kind, so the
/// lathe's Drilling door fronts the one axial Drill cell). The
/// registry's `section` column is the only seed, and the row order is
/// the product order, so a section's cells list the kinds in table
/// sequence.
#[must_use]
pub(super) fn ribbon_plan(
    machine: camcraft_app::machine::Kind,
    pinned: &[usize],
) -> Vec<Group> {
    let mut plan = vec![
        Group { title: FILE.0, cells: FILE.1.to_vec(), section: None },
        Group { title: SETUP.0, cells: SETUP.1.to_vec(), section: None },
    ];
    // Pinned kinds first among the Create groups, in pin order - and
    // only those the machine can run, so the hiding law has one more
    // surface saying the same thing (a pin to other work survives in
    // the list unread, waiting for the mill-turn document that shows
    // it again). These are the bar's only inline op cells, and their
    // number is its budget: the promise of a pin is that it stands
    // where the user put it, so the fold law starts Pinned inline,
    // unfolded, at most MAX_PINS deep.
    let pins: Vec<usize> = pinned
        .iter()
        .copied()
        .filter(|i| {
            operation::KindInfo::ALL
                .get(*i)
                .is_some_and(|k| k.admits(machine))
        })
        // The bar's op budget (the ribbon law): the first MAX_PINS
        // admitted pins stand inline. A document whose list outgrew
        // the cap - pinned on an earlier build, or one that ignored
        // it - still renders, it just shows the first four; the rest
        // wait behind their section doors, listed and reachable.
        .take(MAX_PINS)
        .collect();
    if !pins.is_empty() {
        plan.push(Group {
            title: "Pinned",
            cells: pins.iter().copied().map(Cmd::Create).collect(),
            section: None,
        });
    }
    plan.extend(operation::shown_for(machine).into_iter().filter_map(|s| {
        // One home per kind (the ribbon law's corollary): a pinned
        // kind stands on the bar and leaves its section's list - the
        // door holds "everything of this family not already in
        // reach", never a duplicate of what the user can see.
        let cells: Vec<Cmd> = s
            .kinds_for(machine)
            .into_iter()
            .filter(|i| !pins.contains(i))
            .map(Cmd::Create)
            .collect();
        (!cells.is_empty()).then_some(Group {
            title: s.label(),
            cells,
            section: Some(s),
        })
    }));
    plan.push(Group { title: MODIFY.0, cells: MODIFY.1.to_vec(), section: None });
    plan.push(Group { title: INSPECT.0, cells: INSPECT.1.to_vec(), section: None });
    plan.push(Group { title: VIEW.0, cells: VIEW.1.to_vec(), section: None });
    plan
}

// The cell box is tokens, not constants (M6.1 A): `Tokens::cell_w`/
// `cell_h` - icon over caption, ribbon.rs's cell: glyph + 2 + one line,
// so the PNG and the painted fallback occupy the same box; the 11.5 px
// label needs 35, and shell.rs's ribbon height follows the token
// (`theme::ribbon_h`). GAP/PAD_H stay literals: the ribbon's own
// rhythm, not a theme knob.
const GAP: f32 = 4.0;
const PAD_H: f32 = 8.0;

fn label_of(c: Cmd) -> &'static str {
    match c {
        Cmd::New => "New",
        Cmd::Model => "Model",
        Cmd::Project => "Open",
        Cmd::Save => "Save",
        Cmd::SaveAs => "Save As",
        Cmd::Post => "Post",
        Cmd::Setups => "Setups",
        Cmd::Stock => "Stock",
        Cmd::Origin => "Origin",
        Cmd::Tools => "Tools",
        Cmd::Machine => "Machine",
        Cmd::Create(_) => "op", // formatted by the group row; ribbon shows TYPES
        Cmd::Undo => "Undo",
        Cmd::Redo => "Redo",
        Cmd::Run => "Run",
        Cmd::Delete => "Delete",
        Cmd::Frame => "Frame",
        Cmd::Paths => "Paths",
        Cmd::Simulate => "Simulate",
        Cmd::Deviate => "Deviate",
        Cmd::View => "Panels",
        Cmd::Theme => "Theme",
        Cmd::More(s) => s.label(),
    }
}

/// The section door's short ribbon word: a cell is ~43 px wide at 1280, so
/// "Milling 2D" would not fit; the full name stays in the tip and the
/// dropdown. Plain English, deliberate - never a cut-off one (#948).
fn door_label(s: operation::Section) -> &'static str {
    match s {
        operation::Section::Mill2D => "Mill 2D",
        operation::Section::Mill3D => "Mill 3D",
        operation::Section::Drilling => "Drill",
        operation::Section::MultiAxis => "Multi",
        other => other.label(),
    }
}

pub(super) fn text_of(c: Cmd, state: &State) -> String {
    match c {
        // The overflow button wears the section's name plus an
        // ellipsis - "Milling 3D…" reads as "the rest of this is in
        // here", which is exactly the law the dropdown keeps (M6.2:
        // a labeled door, never a faceless chevron). The machine door
        // wears the same invitation: the ellipsis says "a list opens".
        Cmd::More(s) => format!("{}…", door_label(s)),
        Cmd::Machine => {
            format!("{}…", machine_modes::caption(camcraft_app::machine::kind_of(&state.world)))
        }
        Cmd::View | Cmd::Theme => format!("{}…", label_of(c)),
        // The ribbon caption is registry data (M5.0.2) - imgui's strings,
        // same order as the Create cells below. The list here is built
        // from the table, so an out-of-range index means a table bug:
        // stop in debug, name it and stay on screen in release.
        Cmd::Create(i) => match camcraft_app::operation::KindInfo::ALL.get(i) {
            Some(k) => k.caption.to_owned(),
            None => {
                debug_assert!(false, "Create({i}) past the kind table");
                diag_warn!("ribbon: unknown op kind {i}");
                "op".to_owned()
            }
        },
        other => label_of(other).to_owned(),
    }
}

/// The tip table, `tips_match_the_imgui_table`'s guard. The strings
/// were imgui's verbatim (ribbon.rs:140-222) until M6.2 restructured
/// the ribbon; the promise retired that day (RECOMMENDED.md) and the
/// table became the product's own - the test still pins it row for
/// row, because a tip is a promise and promises want a guard, not
/// because the wording is borrowed. Every advertised key is live:
/// `shortcuts` implements them all, and the pin gesture names itself.
pub(super) fn tip_of(c: Cmd, state: &State) -> String {
    match c {
        Cmd::New => "New document\tCtrl+N".into(),
        Cmd::Model => "Open a mesh model\tCtrl+O".into(),
        Cmd::Project => "Open a project\tCtrl+Shift+O".into(),
        Cmd::Save => "Save the project\tCtrl+S".into(),
        Cmd::SaveAs => "Save the project under a new name\tCtrl+Shift+S".into(),
        Cmd::Post => "Post NC program (validation gate), then review it\tCtrl+P".into(),
        Cmd::Setups => "Setups: create, activate and delete the document's setups".into(),
        Cmd::Stock => "Stock definition".into(),
        Cmd::Origin => "Work offset (WCS)".into(),
        Cmd::Tools => "Tool library".into(),
        // The cell's caption already says which workspace is running;
        // the tip says what a click does, and how to undo it.
        Cmd::Machine => format!(
            "Switch the workspace: {} (mounts the kind's built-in machine, undoable)",
            machine_modes::modes_phrase()
        ),
        Cmd::Create(i) => {
            // The tool-less swap first (imgui's law, ribbon.rs:138):
            // an empty tool list is the deeper trouble - nothing
            // lights until a tool exists, whatever the machine is.
            // Then the family truth (M5.2 stage A): since M6.2 the
            // plan already hides what the machine cannot run, so the
            // darkening branch is defense in depth for any surface
            // that renders a plan the door would refuse - it says so
            // and names the fix, the same pair CreateOperation
            // enforces at the command. The pin hint rides every live
            // tip: the gesture a ribbon cell answers to beyond the
            // click, so the tip table is where it belongs.
            if state.tool_names().is_empty() {
                "No tool yet - add one (Setup > Tools)".into()
            } else {
                let machine = camcraft_app::machine::kind_of(&state.world);
                let dark = camcraft_app::operation::KindInfo::ALL
                    .get(i)
                    .is_some_and(|k| !k.admits(machine));
                if dark {
                    let f = camcraft_app::operation::KindInfo::ALL[i].family();
                    let (fword, mword) = (f.work(), f.machine());
                    // #670: plasma work has no mill-turn to fall back on.
                    let or_mt = if f == camcraft_app::operation::Family::Plasma { "" } else { " or a mill-turn" };
                    format!(
                        "The document's machine is a {} - {fword} operations do not drive it; mount a {mword}{or_mt} (Setup > Machine)",
                        machine.word()
                    )
                } else if !has_stock(state) {
                    NO_STOCK_TIP.into()
                } else if let Some(reason) = tool_law_note(i, state) {
                    reason
                } else if i == 0 {
                    "Add a Face mill operation\tRight-click to pin/unpin".into()
                } else {
                    format!("Add a {} operation\tRight-click to pin/unpin", TYPES[i])
                }
            }
        }
        Cmd::More(s) => format!("The rest of the {} kinds", s.label()),
        Cmd::Undo => "Undo\tCtrl+Z".into(),
        Cmd::Redo => "Redo\tCtrl+Y".into(),
        Cmd::Run => "Generate the selected operation".into(),
        Cmd::Delete => "Delete the selection\tDel".into(),
        Cmd::Frame => "Frame all\tF".into(),
        Cmd::Paths => if state.show_all_paths {
            "Hide every toolpath".into()
        } else {
            "Show every cached toolpath".into()
        },
        Cmd::Simulate => "Simulate tool moves (transport bar)".into(),
        Cmd::Deviate => {
            "Judge the program's stock against the model (deviation map)".into()
        }
        Cmd::View => "Show or hide the browser, properties and machines panes, or reset the layout".into(),
        Cmd::Theme => "Pick the document's theme and colour-vision mode (undoable)".into(),
    }
}

pub(super) fn enabled(c: Cmd, state: &State) -> bool {
    match c {
        Cmd::Undo => state.history.can_undo(),
        Cmd::Redo => state.history.can_redo(),
        Cmd::Run => state.selected_operation().is_some(),
        Cmd::Delete => state.has_selection(),
        // The family gate by pairing (M3.2 E, M5.2 A) is the command
        // door's law; since M6.2 the ribbon's plan already hides
        // what the machine cannot run, so this half stands as defense
        // in depth - the live gate is the tool list: a kind with no
        // tool cannot cut whatever its section says.
        Cmd::Create(i) => {
            let machine = camcraft_app::machine::kind_of(&state.world);
            let kind_admits = camcraft_app::operation::KindInfo::ALL
                .get(i)
                .is_none_or(|k| k.admits(machine));
            // The tool law (M10.1 B): darken, never hide - the cell
            // stays to explain itself, the fix is a library click away.
            // #216: every ribbon-made op machines the active setup's stock
            // (`SrcRef::Stock`) - without one the door refuses, so the
            // cell darkens and says so instead of failing on the click.
            kind_admits && !state.tool_names().is_empty() && has_stock(state) && tool_law_note(i, state).is_none()
        }
        // A dropdown button is live whenever it is on the ribbon; the
        // kinds behind it carry their own gates.
        Cmd::More(_) => true,
        Cmd::Simulate => state.has_toolpaths(),
        // The same gate as Simulate: a program with no generated move
        // has nothing to judge - the cell stays dark until there is.
        Cmd::Deviate => state.has_toolpaths(),
        _ => true,
    }
}

/// The refusal a gated Create answer - a cell on the bar or a row
/// behind a door - speaks when it is clicked anyway: named, loud on
/// the status line, pointing at the fix. It reads the same two truths
/// `enabled` reads, in the same order of depth (no tool first, then
/// the machine's family), phrased as `actions::add_operation` phrases
/// its own refusal. Silence on a click reads as a dead button - the
/// law `State::apply_operation_params` already keeps for the pane -
/// so a gate that can be clicked must be able to answer. `None` when
/// the command is actually live, so no caller overwrites a status an
/// enabled command already owns.
fn gate_note(c: Cmd, state: &State) -> Option<String> {
    let Cmd::Create(i) = c else { return None };
    if enabled(c, state) {
        return None;
    }
    if state.tool_names().is_empty() {
        return Some(String::from(
            "cannot add an operation: no tool in the document - add one (Setup > Tools)",
        ));
    }
    let machine = camcraft_app::machine::kind_of(&state.world);
    let dark = camcraft_app::operation::KindInfo::ALL
        .get(i)
        .filter(|k| !k.admits(machine));
    match dark {
        Some(k) => {
            let f = k.family();
            let (fword, mword) = (f.work(), f.machine());
            let or_mt = if f == camcraft_app::operation::Family::Plasma { "" } else { " or a mill-turn" };
            Some(format!(
                "cannot add a {} operation: {fword} operations do not drive the document's {} - mount a {mword}{or_mt} (Setup > Machine)",
                k.label,
                machine.word(),
            ))
        }
        None if !has_stock(state) => Some(String::from(
            "cannot add an operation: no stock in the setup - set up stock first (Setup > Stock)",
        )),
        None => tool_law_note(i, state).map(|r| {
            let label = camcraft_app::operation::KindInfo::ALL.get(i).map_or("", |k| k.label);
            format!("cannot add a {label} operation: {r}")
        }),
    }
}

/// Where a refused Create click leads (#191): the place that fixes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GateRoute {
    /// No tool, or none that fits the kind: the Tool library page.
    Tools,
    /// The mounted machine cannot run the kind's family: the machine
    /// door's workspace list.
    Machine,
    /// The active setup has no stock (#216): the Stock page.
    Stock,
}

/// The dark Create cell's tip when the setup has no stock (#216).
const NO_STOCK_TIP: &str = "No stock yet - set up stock first (Setup > Stock)";

/// Does the active setup have stock? Every ribbon-made operation
/// machines it (`SrcRef::Stock`), so without it `CreateOperation`
/// refuses - the same resolution the command door makes.
fn has_stock(state: &State) -> bool {
    let active = camcraft_app::setup::active(&state.world);
    let name = camcraft_app::setup::stock_entity_name(&state.world, active.as_deref());
    model::find_by_name(&state.world, &name).is_some_and(|e| model::is_stock(&state.world, &e))
}

/// The fix for a dark Create cell, when it has one ([`gate_note`]'s
/// cases, in its order).
pub(super) fn gate_route(c: Cmd, state: &State) -> Option<GateRoute> {
    let Cmd::Create(i) = c else { return None };
    if enabled(c, state) {
        return None;
    }
    if state.tool_names().is_empty() {
        return Some(GateRoute::Tools);
    }
    let machine = camcraft_app::machine::kind_of(&state.world);
    if camcraft_app::operation::KindInfo::ALL.get(i).is_some_and(|k| !k.admits(machine)) {
        return Some(GateRoute::Machine);
    }
    if !has_stock(state) {
        return Some(GateRoute::Stock);
    }
    tool_law_note(i, state).map(|_| GateRoute::Tools)
}

/// A dark Create cell was clicked anyway (#191): say why on the status
/// line, then take the user to the fix rather than leave them at the
/// error - the Tool library, or the machine door opened on its list.
/// View state only; nothing is created or queued.
pub(super) fn follow_gate(c: Cmd, state: &mut State) {
    let Some(why) = gate_note(c, state) else { return };
    match gate_route(c, state) {
        Some(GateRoute::Tools) => {
            actions::target(state, UiTarget::Tool(String::new()));
            state.status = format!("{why} - opened the tool library");
        }
        Some(GateRoute::Stock) => {
            actions::target(state, UiTarget::Stock);
            state.status = format!("{why} - opened the Stock page");
        }
        Some(GateRoute::Machine) => {
            state.ribbon_more = None;
            state.ribbon_view_open = false;
            state.ribbon_theme_open = false;
            state.ribbon_recent_open = None;
            state.ribbon_mode_open = true;
            state.status = format!("{why} - pick a workspace");
        }
        None => state.status = why,
    }
}

/// The tool law's darkening (M10.1 B): the app's own refusal sentence
/// for creating kind `i` against the active setup's tools, or `None`
/// when a fitting tool is there. The predicate lives in app (the same
/// one `CreateOperation` enforces); the ribbon only renders it.
fn tool_law_note(i: usize, state: &State) -> Option<String> {
    camcraft_app::operation::KindInfo::ALL
        .get(i)
        .and_then(|k| camcraft_app::operation::tool_gate::create_refusal(&state.world, k))
}

pub(super) fn toggled(c: Cmd, state: &State) -> bool {
    match c {
        Cmd::Paths => state.show_all_paths,
        Cmd::Simulate => state.sim.is_some(),
        Cmd::Deviate => state.program_sim.is_some(),
        // The open dropdown's button stands toggled while it is open
        // (M6.2): the accent fill says "this door is the one you
        // opened".
        Cmd::More(s) => state.ribbon_more == Some(s),
        Cmd::Machine => state.ribbon_mode_open,
        Cmd::View => state.ribbon_view_open,
        Cmd::Theme => state.ribbon_theme_open,
        Cmd::Model | Cmd::Project => {
            state.ribbon_recent_open.is_some() && state.ribbon_recent_open == super::recent_drop::kind_of(c)
        }
        _ => false,
    }
}

/// Command to bundled glyph: the same table the retired imgui icons.rs
/// store keyed (and ribbon.rs's `Item.icon` chose per cell), so the ribbon,
/// the browser's `spec.icon` slots and the store name one set of pixels.
pub(super) fn icon_of(c: Cmd, state: &State) -> Option<Icon> {
    match c {
        Cmd::New => Some(Icon::New),
        Cmd::Model => Some(Icon::OpenModel),
        Cmd::Project => Some(Icon::OpenProject),
        Cmd::Save => Some(Icon::Save),
        Cmd::SaveAs => Some(Icon::Save),
        Cmd::Post => Some(Icon::Post),
        // A setup wears the setup sheet (#661), the tree's Setups glyph;
        // it never twins Origin's beside it.
        Cmd::Setups => Some(Icon::Setup),
        Cmd::Stock => Some(Icon::Stock),
        Cmd::Origin => Some(Icon::WorkOffset),
        Cmd::Tools => Some(Icon::Tools),
        // Icons stay a UI table (asset names do not belong in the app),
        // but an exhaustive match on the kind, not on the index: a new
        // registry row with no icon here stops at the compiler, never
        // silently borrows the last glyph (M5.0.2 - the old `_ =>` arm
        // was one of the three silent-drift catch-alls).
        Cmd::Create(i) => match camcraft_app::operation::KindInfo::ALL.get(i) {
            Some(k) => Some(op_icon(k.ty)),
            None => {
                debug_assert!(false, "Create({i}) past the kind table");
                diag_warn!("ribbon: unknown op kind {i}");
                Some(Icon::Generate)
            }
        },
        // A section door wears its family's signature glyph (#207: with
        // no pins, glyph-less doors left every Create cell of a fresh
        // install text-only - users read it as missing icons). No new
        // asset: an op glyph that says what the family makes, distinct
        // from the machine door's beside it; the caption still names
        // the section.
        Cmd::More(s) => Some(match s {
            operation::Section::Mill2D => Icon::Pocket,
            operation::Section::Mill3D => Icon::Finish3D,
            operation::Section::Drilling => Icon::Drill,
            operation::Section::MultiAxis => Icon::MultiAxisRough,
            // #661: turning has glyphs of its own now - the facing cut.
            operation::Section::Turning => Icon::Facing,
            // #670: the plasma section cuts 2D profiles - the contour's
            // glyph (the machine door's own choice, no new asset).
            operation::Section::Plasma => Icon::Contour,
        }),
        // The machine door wears its workspace's glyph (#207), from the
        // existing set: the face mill for Milling, the turned bar for
        // Turning, the rotary bar for Mill-turning.
        Cmd::Machine => Some(match camcraft_app::machine::kind_of(&state.world) {
            camcraft_app::machine::Kind::Lathe => Icon::RotaryContour,
            camcraft_app::machine::Kind::MillTurn => Icon::RotaryParallel,
            camcraft_app::machine::Kind::Mill => Icon::Face,
            // #530: the wire cuts a contour; the sinker burns a pocket.
            camcraft_app::machine::Kind::WireEdm => Icon::Contour,
            camcraft_app::machine::Kind::SinkerEdm => Icon::Pocket,
            // #669: a plasma table cuts 2D profiles - the contour glyph.
            camcraft_app::machine::Kind::Plasma => Icon::Contour,
        }),
        Cmd::Undo => Some(Icon::Undo),
        Cmd::Redo => Some(Icon::Redo),
        Cmd::Run => Some(Icon::Generate),
        Cmd::Delete => Some(Icon::Delete),
        Cmd::Frame => Some(Icon::FrameAll),
        Cmd::Paths => Some(Icon::Toolpaths),
        Cmd::Simulate => Some(Icon::Simulate),
        // The stock glyph, reused (RECOMMENDED.md's no-new-asset law):
        // what the cell shows IS the judged stock.
        Cmd::Deviate => Some(Icon::Stock),
        // The machine door's no-new-asset law: the caption names the
        // door, the ellipsis says a list opens.
        // The Theme door follows the View door: a list door captioned by
        // its own word (no fitting glyph in the bundled set).
        Cmd::View | Cmd::Theme => None,
    }
}

/// The ribbon's one command path: every cell fires the shared actions
/// layer (`crate::actions`), so the effect lives there exactly once and
/// menu, tree and shortcut cannot fork a variant of it (actions.rs's
/// rule). `Cmd::Paths` stays a direct field flip - view state, not a
/// command, exactly imgui's run_toggle_paths (ribbon.rs:98).
fn run_cmd(c: Cmd, state: &mut State) {
    match c {
        Cmd::New => actions::new_document(state),
        // #662: with recent files on hand the button opens its list
        // (recent_drop.rs); with none it is the dialog, as before.
        Cmd::Model => super::recent_drop::press(state, camcraft_app::recent::RecentKind::Model),
        Cmd::Project => super::recent_drop::press(state, camcraft_app::recent::RecentKind::Project),
        Cmd::Save => actions::save_project(state),
        Cmd::SaveAs => actions::save_project_as(state),
        Cmd::Post => actions::post_program(state),
        Cmd::Setups => actions::target(state, UiTarget::Setups),
        Cmd::Stock => actions::target(state, UiTarget::Stock),
        Cmd::Origin => actions::target(state, UiTarget::Wcs),
        Cmd::Tools => actions::target(state, UiTarget::Tool(String::new())),
        Cmd::Create(i) => actions::add_operation(state, i),
        Cmd::Undo => actions::undo(state),
        Cmd::Redo => actions::redo(state),
        Cmd::Run => actions::generate_selected(state),
        Cmd::Delete => actions::delete_selected(state),
        Cmd::Frame => actions::frame_all(state),
        Cmd::Paths => state.toggle_all_toolpaths(),
        Cmd::Simulate => {
            // View state, like Paths: the cell arms or disarms the
            // transport. The playhead inside is immediate-mode by
            // AGENTS.md's explicit exemption - no command to route.
            state.sim = if state.sim.is_some() { None } else { Some(Sim::new()) };
        }
        Cmd::Deviate => {
            // View state like Simulate, with one difference the actions
            // layer holds: arming RUNS the program sim (a world read,
            // never a world write), so the effect lives in actions.
            actions::toggle_deviate(state);
        }
        // Immediate-mode view state (the Paths law): which door stands
        // open is a frame fact, not a document fact - nothing to undo,
        // and clicking the open door again closes it. One dropdown at
        // a time: opening either closes the other.
        Cmd::More(s) => {
            state.ribbon_more = (state.ribbon_more != Some(s)).then_some(s);
            state.ribbon_recent_open = None;
            state.ribbon_mode_open = false;
            state.ribbon_view_open = false;
            state.ribbon_theme_open = false;
        }
        Cmd::Machine => {
            state.ribbon_mode_open = !state.ribbon_mode_open;
            if state.ribbon_mode_open {
                state.ribbon_recent_open = None;
                state.ribbon_more = None;
                state.ribbon_view_open = false;
                state.ribbon_theme_open = false;
            }
        }
        // Same door law; the dock itself is the shell's (layout.rs).
        Cmd::View => {
            state.ribbon_view_open = !state.ribbon_view_open;
            if state.ribbon_view_open {
                state.ribbon_recent_open = None;
                state.ribbon_more = None;
                state.ribbon_mode_open = false;
                state.ribbon_theme_open = false;
            }
        }
        Cmd::Theme => {
            state.ribbon_theme_open = !state.ribbon_theme_open;
            if state.ribbon_theme_open {
                state.ribbon_recent_open = None;
                state.ribbon_more = None;
                state.ribbon_mode_open = false;
                state.ribbon_view_open = false;
            }
        }
    }
}

/// The pin gesture (M6.2): toggle one kind's presence in the document's
/// pinned list. The pins are saved with the project, so the toggle is a
/// `SetRibbonPins` command on the history (issue #143): Ctrl+Z puts the
/// previous pins back and the document counts as changed. The status
/// line says what happened, because a silent toggle is a half-answer.
pub(super) fn toggle_pin(state: &mut State, i: usize) {
    let Some(k) = operation::KindInfo::ALL.get(i) else {
        debug_assert!(false, "toggle_pin({i}) past the kind table");
        diag_warn!("ribbon: unknown op kind {i}");
        return;
    };
    let mut pins = camcraft_app::ribbon::pinned_of(&state.world);
    let verb = match pins.iter().position(|p| *p == i) {
        Some(at) => {
            pins.remove(at);
            "unpinned"
        }
        // The bar's op budget: a fifth pin is refused by name, not
        // silently dropped (Prime Directive - the gesture asked for
        // something the law declines, so the law says so).
        None if pins.len() >= MAX_PINS => {
            state.status = format!(
                "the ribbon keeps {MAX_PINS} pins - unpin one first (right-click a pinned cell)"
            );
            return;
        }
        None => {
            pins.push(i);
            "pinned"
        }
    };
    match camcraft_app::commands::SetRibbonPins::new(&state.world, &pins) {
        Ok(command) => {
            state.history.execute(&state.world, command);
            state.status = format!("{verb} {} on the ribbon", k.caption);
        }
        // Unreachable from the toggle (it always changes the list), but a
        // refusal is named, never swallowed.
        Err(err) => state.status = format!("ribbon pin not changed: {err}"),
    }
}

/// One laid-out ribbon cell: which command, where, and its id salt.
/// A flat index salt (was `(group, index)`) - the value only has to be
/// stable across frames for hover bookkeeping, and the flat list is
/// what the gesture tests can reach.
pub(super) struct Cell {
    pub cmd: Cmd,
    pub rect: Rect,
    pub salt: usize,
}

/// One folded section's dropdown, pure geometry (M6.2): the More button
/// that owns it and its hidden kinds' row rects, all in GLOBAL
/// coordinates - so the layout math is testable (and the reach of every
/// kind provable) without a frame, and the draw only relocates it into
/// its Area. Rows are in registry order, the same order the section
/// drew before it folded.
pub(super) struct Drop {
    pub section: operation::Section,
    pub button: Rect,
    pub rows: Vec<(Cmd, Rect)>,
}

/// The dropdown's own rhythm (ribbon constants law): wide enough for
/// the longest registry label, rows one theme row tall.
/// What one layout pass answers: cells to draw, group captions (mid x,
/// title), divider x's, and the doors with their dropdown rows.
pub(super) type RibbonGeom = (Vec<Cell>, Vec<(f32, f32, &'static str)>, Vec<f32>, Vec<Drop>);

/// The machine door's dropdown rows, pure geometry (the section doors'
/// `Drop` law, worn by the switcher): the modes in table order, one
/// theme row tall each, `width` wide, hung under the door button in the
/// strip's global frame. The tests click the switch through exactly
/// these rects - one source of truth for where each mode lives.
pub(super) fn machine_rows(
    button: Rect,
    row_h: f32,
    width: f32,
) -> Vec<(camcraft_app::machine::Kind, Rect)> {
    machine_modes::MACHINE_MODES
        .iter()
        .enumerate()
        .map(|(k, (kind, _, _))| {
            (
                *kind,
                Rect::from_min_size(
                    egui::Pos2::new(
                        button.left(),
                        button.bottom() + DROP_GAP + k as f32 * row_h,
                    ),
                    Vec2::new(width, row_h),
                ),
            )
        })
        .collect()
}

/// The machine dropdown's width: as wide as its longest row (text is
/// never cut), and never narrower than `DROP_W`. `longest_px` is the
/// measured width of that row's text.
pub(super) fn machine_menu_width(longest_px: f32) -> f32 {
    (longest_px + 2.0 * DROP_TEXT_PAD).max(DROP_W)
}
const DROP_TEXT_PAD: f32 = 8.0;
pub(super) const DROP_W: f32 = 200.0;
pub(super) const DROP_GAP: f32 = 2.0;

/// The ribbon's layout math, pure: a plan (ribbon_plan's groups) and the
/// strip give back cell rects, group captions (centre x, title), the
/// divider x's between groups, and one `Drop` per folded section.
/// `ribbon` draws exactly this, and the tests click through it - one
/// source of truth for where each command lives. FLOOR_W/GAP/PAD_H are
/// ribbon.rs's; the cell box is the theme's (Tokens::cell_w/cell_h).
///
/// The fold law (the ribbon law, retired M6.2's width law): a
/// section's kinds live behind their labeled door AT EVERY WIDTH -
/// the bar's inline op cells are exactly the pinned ones, at most
/// MAX_PINS of them. A crowd that only folds when squeezed is still a
/// crowd, and the user asked for this one directly. Cells share the
/// strip's width - a flow, not imgui's fixed pitch - and anchors
/// never fold; if the anchors and doors cannot fit, the row overflows
/// on purpose (clamped at FLOOR_W) rather than rendering a narrower
/// lie. Every section's door owns its full list; the layout draws
/// the door where the run of cells would have stood.
pub(super) fn ribbon_cells(strip: Rect, pal: &Tokens, plan: &[Group]) -> RibbonGeom {
    const FLOOR_W: f32 = 40.0;
    const BUILD_BADGE_W: f32 = 310.0;
    // Each group's inline run and folded tail: sections start fully
    // folded, anchors fully inline - the split IS the law, no loop
    // decides it.
    let groups: Vec<(&Group, Vec<Cmd>, Vec<Cmd>)> = plan
        .iter()
        .map(|g| match g.section {
            Some(_) => (g, Vec::new(), g.cells.clone()),
            None => (g, g.cells.clone(), Vec::new()),
        })
        .collect();
    let n_groups = groups.len();
    let n_cells: usize = groups
        .iter()
        .map(|(_, shown, hidden)| shown.len() + usize::from(!hidden.is_empty()))
        .sum();
    let overhead = 2.0 * PAD_H
        + (n_cells - n_groups) as f32 * GAP
        + (n_groups - 1) as f32 * (GAP + 2.0);
    // Reserve the largest build label that fits after the minimum cell row.
    // The three sizes carry full details, version plus source, or version.
    let minimum = overhead + n_cells as f32 * FLOOR_W;
    let badge_w = [BUILD_BADGE_W, 165.0, 75.0]
        .into_iter()
        .find(|w| strip.width() >= minimum + *w)
        .unwrap_or(0.0);
    let cell_w = ((strip.width() - overhead - badge_w) / n_cells.max(1) as f32)
        .clamp(FLOOR_W, pal.cell_w);
    // PAD_V (ribbon.rs): cells start 4 below the strip top.
    let cell_top = strip.top() + 4.0;
    let mut cells = Vec::new();
    let mut captions = Vec::new();
    let mut dividers = Vec::new();
    let mut drops = Vec::new();
    let mut x = strip.left() + PAD_H;
    let mut salt = 0usize;
    for (gi, (group, shown, hidden)) in groups.iter().enumerate() {
        let start = x;
        let mut draw = |cmd: Cmd, x: &mut f32, cells: &mut Vec<Cell>| {
            cells.push(Cell {
                cmd,
                rect: Rect::from_min_size(
                    egui::Pos2::new(*x, cell_top),
                    Vec2::new(cell_w, pal.cell_h),
                ),
                salt: {
                    salt += 1;
                    salt - 1
                },
            });
            *x += cell_w + GAP;
        };
        for c in shown {
            draw(*c, &mut x, &mut cells);
        }
        // Only section groups ever carry a hidden tail (the fold law
        // starts them folded and folds nothing else), so the `section`
        // test is a type guard, not a second opinion.
        if let (Some(section), true) = (group.section, !hidden.is_empty()) {
            // The section's door stands at the end of its inline run,
            // and owns the dropdown of everything behind it.
            let button = Rect::from_min_size(
                egui::Pos2::new(x, cell_top),
                Vec2::new(cell_w, pal.cell_h),
            );
            let row_h = pal.row_height;
            let rows = hidden
                .iter()
                .enumerate()
                .map(|(k, c)| {
                    (
                        *c,
                        Rect::from_min_size(
                            egui::Pos2::new(
                                button.left(),
                                button.bottom() + DROP_GAP + k as f32 * row_h,
                            ),
                            Vec2::new(DROP_W, row_h),
                        ),
                    )
                })
                .collect();
            drops.push(Drop {
                section,
                button,
                rows,
            });
            cells.push(Cell {
                cmd: Cmd::More(section),
                rect: button,
                salt: {
                    salt += 1;
                    salt - 1
                },
            });
            x += cell_w + GAP;
        }
        x -= GAP;
        captions.push(((start + x) * 0.5, (x - start).max(0.0), group.title));
        if gi + 1 < n_groups {
            x += GAP + 2.0;
            dividers.push(x - (GAP + 2.0) / 2.0 - 1.0);
        }
    }
    (cells, captions, dividers, drops)
}

/// The ribbon strip: painted cells over a fixed-height top panel. Group
/// captions and hairline dividers match imgui's layout math, the cells
/// themselves are painted::cell - all from `ribbon_cells`'s geometry.
/// Since M-g each cell carries its bundled glyph (the retired imgui
/// ribbon's Item.icon table), or the painted fallback headless.
pub fn ribbon(
    ui: &mut Ui,
    state: &mut State,
    pal: &Tokens,
    icons: Option<&IconSet>,
) {
    let rect = Rect::from_min_size(ui.cursor().min, ui.available_size());
    painted::strip_bg(ui.painter(), rect, pal);
    let base = ui.id().with("ribbon");
    // The plan is read fresh every frame from the two facts it is a
    // function of - the document's machine and its pins (M6.2). No
    // cached shape: a machine mount or an unpin cannot leave a stale
    // group standing.
    let machine = camcraft_app::machine::kind_of(&state.world);
    let plan = ribbon_plan(machine, &camcraft_app::ribbon::pinned_of(&state.world));
    let (cells, captions, dividers, drops) = ribbon_cells(rect, pal, &plan);
    // Self-heal first: a folded-away door (window widened, pin moved a
    // cell inline, document swapped) must not leave `ribbon_more`
    // naming a button that no longer exists.
    if let Some(s) = state.ribbon_more
        && !cells.iter().any(|cell| cell.cmd == Cmd::More(s))
    {
        state.ribbon_more = None;
    }
    let before = state.ribbon_more;
    for cell in &cells {
        let c = cell.cmd;
        let resp = painted::cell(
            ui,
            base.with(cell.salt),
            cell.rect,
            &text_of(c, state),
            pal,
            enabled(c, state),
            toggled(c, state),
            icon_of(c, state).and_then(|i| icons.and_then(|s| s.texture(i))),
        );
        // The pin gesture lives at the cell, not in a command: the
        // right-click that answers it names itself in every live tip
        // (tip_of), and its law - layout rides the document, undo
        // replays cuts - is ROADMAP M6.2's, recorded in RECOMMENDED.md.
        // Read before `on_hover_text` moves the response into the
        // tooltip wiring.
        let fired = resp.clicked() && enabled(c, state);
        // A dark Create cell that gets clicked is not a dead pixel:
        // the refusal speaks (gate_note), the same answer the
        // dropdown rows give. Hover already taught it; the click
        // proves it.
        let gated = resp.clicked() && !fired && matches!(c, Cmd::Create(_));
        let pin = resp.secondary_clicked();
        resp.on_hover_text(tip_of(c, state));
        if fired {
            run_cmd(c, state);
        }
        if gated {
            follow_gate(c, state);
        }
        if let (Cmd::Create(i), true) = (c, pin) {
            toggle_pin(state, i);
        }
    }
    // Group captions (#948): under each group's cells when they fit the
    // group's span. A single-door group (Milling 2D, Drilling ...) is one
    // narrow cell that already names its section, so its caption is
    // skipped rather than run under the neighbour's.
    for (mid, width, title) in captions {
        painted::caption(ui, mid, width, title, rect, pal, rect.top() + 4.0 + pal.cell_h);
    }
    for dx in dividers {
        painted::divider(ui.painter(), dx, rect, pal);
    }
    // The open door's dropdown: one Area at foreground order (egui's
    // popup layer), anchored under its button, its rows the section's
    // hidden kinds in registry
    // order. Geometry arrives global (ribbon_cells); the Area-local
    // rect is the global one minus the panel origin - one pair of
    // numbers, no second layout.
    if let Some(s) = state.ribbon_more {
        // The self-heal above proved a door is on the ribbon; finding
        // none means that proof broke, so the state drops itself -
        // named, printed, never a silent unwrap.
        match drops.iter().find(|d| d.section == s) {
            None => {
                debug_assert!(false, "ribbon: open door has no dropdown");
                diag_warn!("ribbon: closing a dropdown whose button is gone");
                state.ribbon_more = None;
            }
            Some(drop) => {
                // Every rect here is global: ribbon_cells computed the
                // button and row rects in the strip's global frame, and
                // egui paints and hit-tests this ui in global coords
                // too. The Area adds only the layer - Foreground order,
                // so a press on a row belongs to the row, never to the
                // cell it happens to cover.
                let panel = Rect::from_min_size(
                    drop.button.left_bottom() + Vec2::new(0.0, DROP_GAP),
                    Vec2::new(DROP_W, drop.rows.len() as f32 * pal.row_height),
                );
                let mut hit: Option<Cmd> = None;
                let mut gated_row: Option<Cmd> = None;
                let mut pinned_row: Option<usize> = None;
                let rows = &drop.rows;
                egui::Area::new(base.with(("drop", drop.section.label())))
                    .fixed_pos(panel.min)
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
                        for (k, (cmd, r)) in rows.iter().enumerate() {
                            let resp = ui.interact(*r, base.with(("drop-row", k)), Sense::click());
                            if resp.hovered() {
                                p.rect_filled(*r, 0.0, pal.field_hover);
                            }
                            let en = enabled(*cmd, state);
                            let label = match *cmd {
                                Cmd::Create(i) => TYPES.get(i).copied().unwrap_or("op"),
                                other => label_of(other),
                            };
                            p.text(
                                r.left_center() + Vec2::new(8.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                label,
                                egui::FontId::proportional(pal.font_row),
                                if en { pal.text } else { pal.text_faint },
                            );
                            // Read the click flags before
                            // `on_hover_text` moves the response -
                            // the cell law again, one line lower. The
                            // hint on a row is the bar's tip for the
                            // same command: what it creates, or why
                            // it cannot yet.
                            let clicked = resp.clicked();
                            let secondary = resp.secondary_clicked();
                            resp.on_hover_text(tip_of(*cmd, state));
                            if secondary
                                && let Cmd::Create(i) = *cmd
                            {
                                pinned_row = Some(i);
                            }
                            if clicked {
                                if en {
                                    hit = Some(*cmd);
                                } else {
                                    gated_row = Some(*cmd);
                                }
                            }
                        }
                    });
                if let Some(i) = pinned_row {
                    toggle_pin(state, i);
                }
                if let Some(c) = hit {
                    run_cmd(c, state);
                    state.ribbon_more = None;
                } else if let Some(c) = gated_row {
                    // The dark row answered: the refusal named on the
                    // status line, the door closed on the way out. A
                    // click into silence is what made the folded
                    // ribbon read as dead - this branch is why it
                    // never is.
                    state.ribbon_more = None;
                    follow_gate(c, state);
                } else if state.ribbon_more == before {
                    // Primary press anywhere outside the door and its
                    // rows closes it (egui's popup law, hand-drawn
                    // because the panel is geometry, not a Widget). A
                    // press that already toggled a door this frame
                    // (`state.ribbon_more != before`) is the new
                    // door's own business.
                    let outside = ui.input(|i| {
                        i.pointer.button_pressed(egui::PointerButton::Primary)
                            && i.pointer.interact_pos().is_none_or(|p| {
                                !drop.button.expand(DROP_GAP).contains(p)
                                    && !drop.rows.iter().any(|(_, r)| r.contains(p))
                            })
                    });
                    if outside {
                        state.ribbon_more = None;
                    }
                }
            }
        }
    }
    // The machine door's dropdown (the ribbon law): the workspaces
    // of the mode table, the current one marked and inert, the others
    // mounting their kind's library reference - through the queue,
    // so the switch rides undo. Same hand-drawn popup law as the
    // section doors: foreground order, outside press closes it.
    if state.ribbon_mode_open {
        match cells.iter().find(|c| c.cmd == Cmd::Machine) {
            None => {
                // The anchor law says the door stands on the bar every
                // frame; finding none means that broke - named, not
                // silent, and the state drops itself.
                debug_assert!(false, "ribbon: machine door missing from the bar");
                diag_warn!("ribbon: closing a mode list whose button is gone");
                state.ribbon_mode_open = false;
            }
            Some(cell) => {
                let button = cell.rect;
                // The menu is as wide as its longest row, the current
                // row's "(current)" tail included.
                let longest = ui.ctx().fonts_mut(|f| {
                    machine_modes::MACHINE_MODES
                        .iter()
                        .map(|(k, _, _)| {
                            let text = format!("{}  (current)", machine_modes::mode_label(*k));
                            f.layout_no_wrap(text, egui::FontId::proportional(pal.font_row), pal.text).size().x
                        })
                        .fold(0.0_f32, f32::max)
                });
                let width = machine_menu_width(longest);
                let rows = machine_rows(button, pal.row_height, width);
                let machine = camcraft_app::machine::kind_of(&state.world);
                let mut pick: Option<camcraft_app::machine::Kind> = None;
                let mut chose = false;
                let panel = Rect::from_min_size(
                    button.left_bottom() + Vec2::new(0.0, DROP_GAP),
                    Vec2::new(width, rows.len() as f32 * pal.row_height),
                );
                egui::Area::new(base.with("machine-drop"))
                    .fixed_pos(panel.min)
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
                        for (k, (kind, r)) in rows.iter().enumerate() {
                            let resp =
                                ui.interact(*r, base.with(("mode-row", k)), Sense::click());
                            if resp.hovered() {
                                p.rect_filled(*r, 0.0, pal.field_hover);
                            }
                            let current = *kind == machine;
                            let label = if current {
                                format!("{}  (current)", machine_modes::mode_label(*kind))
                            } else {
                                machine_modes::mode_label(*kind)
                            };
                            p.text(
                                r.left_center() + Vec2::new(8.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                label,
                                egui::FontId::proportional(pal.font_row),
                                if current { pal.text_faint } else { pal.text },
                            );
                            // A press on the current row is a choice
                            // too - it closes the list, it just
                            // mounts nothing.
                            if resp.clicked() {
                                chose = true;
                                if !current {
                                    pick = Some(*kind);
                                }
                            }
                        }
                    });
                if chose {
                    if let Some(k) = pick {
                        machine_modes::set_machine_mode(state, k);
                    }
                    state.ribbon_mode_open = false;
                } else {
                    let outside = ui.input(|i| {
                        i.pointer.button_pressed(egui::PointerButton::Primary)
                            && i.pointer.interact_pos().is_none_or(|p| {
                                !button.expand(DROP_GAP).contains(p)
                                    && !rows.iter().any(|(_, r)| r.contains(p))
                            })
                    });
                    if outside {
                        state.ribbon_mode_open = false;
                    }
                }
            }
        }
    }
    // The View door's dropdown (layout.rs): the panel toggles and the
    // layout reset, same popup law as the doors above.
    if state.ribbon_view_open {
        match cells.iter().find(|c| c.cmd == Cmd::View) {
            None => {
                debug_assert!(false, "ribbon: view door missing from the bar");
                diag_warn!("ribbon: closing a view list whose button is gone");
                state.ribbon_view_open = false;
            }
            Some(cell) => {
                let rows = view_rows_of(cell.rect, rect, pal);
                layout::view_drop(ui, state, pal, base.with("view-drop"), cell.rect, &rows);
            }
        }
    }
    // The Theme door's dropdown (#441): theme_drop.rs, same popup law.
    if state.ribbon_theme_open {
        match cells.iter().find(|c| c.cmd == Cmd::Theme) {
            None => {
                debug_assert!(false, "ribbon: theme door missing from the bar");
                diag_warn!("ribbon: closing a theme list whose button is gone");
                state.ribbon_theme_open = false;
            }
            Some(cell) => {
                let rows = super::theme_drop::rows_of(state, cell.rect, rect, pal);
                super::theme_drop::show(ui, state, pal, base.with("theme-drop"), cell.rect, &rows);
            }
        }
    }
    // The recent-files lists under Open and Model (#662): recent_drop.rs.
    super::recent_drop::draw(ui, state, pal, base.with("recent-drop"), &cells, rect);
    // The source/build label is the rightmost toolbar item. At small window
    // widths it contracts to version plus source, then version alone; the
    // full text remains available on hover.
    let badge_left = cells.last().map_or(rect.left(), |cell| cell.rect.right()) + 8.0;
    let badge_rect = Rect::from_min_max(
        egui::Pos2::new(badge_left, rect.top()),
        egui::Pos2::new(rect.right() - 8.0, rect.bottom()),
    );
    let lines = if badge_rect.width() >= 285.0 {
        Some(build_info::toolbar_lines(false))
    } else if badge_rect.width() >= 145.0 {
        Some(build_info::toolbar_lines(true))
    } else if badge_rect.width() >= 50.0 {
        Some((build_info::toolbar_version(), String::new()))
    } else {
        None
    };
    if let Some((top, bottom)) = lines {
        let (full_top, full_bottom) = build_info::toolbar_lines(false);
        ui.interact(badge_rect, base.with("build-info"), Sense::hover())
            .on_hover_text(format!("{full_top}\n{full_bottom}"));
        let painter = ui.painter().with_clip_rect(badge_rect);
        let y = badge_rect.center().y;
        painter.text(
            egui::Pos2::new(badge_rect.right(), y - 8.0),
            egui::Align2::RIGHT_CENTER,
            top,
            egui::FontId::proportional(pal.font_row),
            pal.text_faint,
        );
        if !bottom.is_empty() {
            painter.text(
                egui::Pos2::new(badge_rect.right(), y + 8.0),
                egui::Align2::RIGHT_CENTER,
                bottom,
                egui::FontId::proportional(pal.font_row),
                pal.text_faint,
            );
        }
    }
    ui.allocate_rect(rect, Sense::hover());
}

/// The View door's rows under `button` in `strip` - the one geometry the
/// draw and the tests share.
pub(super) fn view_rows_of(
    button: Rect,
    strip: Rect,
    pal: &Tokens,
) -> Vec<(layout::ViewRow, Rect)> {
    layout::view_rows(button, strip, pal.row_height, DROP_W, DROP_GAP)
}

/// The status strip (bottom panel): the shell's one-line feedback, dimmer,
/// with the chrome the painted ground gives it. Since M-d this is the
/// file-op outcome line too ("saved …", "open failed: …" - State::run
/// writes them all), while command refusals read twice by design: here for
/// everyone, and on their own pane through `error_for` (M-c). Startup is a
/// plain empty document; an untouched status reads "ready".
pub fn status(ui: &mut Ui, state: &mut State, pal: &Tokens, icons: Option<&IconSet>) {
    let rect = Rect::from_min_size(ui.cursor().min, ui.available_size());
    painted::strip_bg(ui.painter(), rect, pal);
    // Owned so the button below can reborrow `state` mutably while the
    // label still shows the text (disjoint fields - cheap either way).
    let text = if state.status.is_empty() {
        "ready".to_owned()
    } else {
        state.status.clone()
    };
    // M3.1 C: the batch fix rides the bar whenever a revision has left
    // operations Stale - the tree marks them, the page says why, this is
    // the one click that brings the job current.
    let stale = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| {
            model::is_operation(&state.world, e)
                && operation::status_of(&state.world, e) == operation::OpStatus::Stale
        })
        .count();
    // #192: a refusal on the status line must not read like any other
    // status - error ink, bold, a "!" marker, on an error-tinted band.
    let alert = super::status_alert::is_alert(state);
    let line = if alert {
        let ink = super::colorvision::text_inks(ui, &state.world).error;
        ui.painter().rect_filled(rect, 0.0, ink.gamma_multiply(0.16));
        RichText::new(format!("! {text}")).color(ink).strong()
    } else {
        // #215: routine status in the body text ink, not egui's "weak"
        // grey - that read as near-invisible on the dark strip.
        RichText::new(text.as_str()).color(ui.visuals().text_color())
    };
    ui.horizontal(|ui| {
        // The right-hand widgets are laid out first so a long status line
        // can never push them off the strip (#157: the Debug info door
        // must stay reachable); the line takes what is left, truncated,
        // with the full text on hover and in "Copy message".
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // #157: the Debug info door, rightmost - the glyph when the
            // icon set is up, its name when it is not (headless frames
            // and a failed decode degrade to text, never to nothing).
            if super::debug_info::button(ui, icons).clicked() {
                state.debug_info_open = !state.debug_info_open;
            }
            // #183: the live MCP server says it is listening.
            if let Some(note) = crate::mcp_host::badge(state) {
                ui.label(RichText::new(note).color(pal.text_faint).small());
            }
            if stale > 0 {
                let button = ui.button(format!("Regenerate all stale ({stale})"));
                if button.clicked() {
                    state.pending.push_back(CommandKind::RegenerateAll);
                }
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(Label::new(line).selectable(true).truncate())
                    .on_hover_text(text.as_str())
                    .context_menu(|ui| {
                        if ui.button("Copy message").clicked() {
                            ui.ctx().copy_text(text.clone());
                            ui.close();
                        }
                    });
            });
        });
    });
    ui.allocate_rect(rect, Sense::hover());
}

// ------------------------------------------------------------------ browser

// #343: the browser tree moved to its own modules (the 400-line law) -
// rows in `browser_tree`, the pane and its click law in `browser_pane`,
// the operation menu in `tree_menu`. Re-exported so callers keep the path.
pub(crate) use super::browser_tree::kind_name;
#[cfg(test)]
pub(crate) use super::browser_tree::{Leaf, browser_rows, model_row_marks};
pub use super::browser_pane::browser;

// --------------------------------------------------------------- parameters


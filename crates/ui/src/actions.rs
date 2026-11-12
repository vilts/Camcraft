//! One entry point per command: what every surface (ribbon, tree,
//! shortcuts) fires, so no surface forks command-queue logic of its own.
//! The menu surface retired with the imgui shell at M-g; two menu-only
//! helpers (`new_operation`, `delete_operation`) left with it - the
//! ribbon's Create/Delete cells already cover both effects through this
//! same module.
//!
//! Provenance: our own module, the standard "actions layer" pattern of CAM
//! shells. The rule it exists to enforce: a surface never pushes a
//! `CommandKind` directly and never writes world state - it calls one of
//! these. Enabling/disabling stays with the surface (it knows what the
//! user can act on), but the effect lives here exactly once.

use camcraft_app::operation::SrcRef;

use crate::operations::quick_params;
use crate::state::{CommandKind, State, UiTarget};
use crate::viewport::ProjectionMode;

/// File menu: ask for a mesh to import (the dialog itself runs in
/// `State::run`, never mid-frame).
pub(crate) fn open_model_dialog(state: &mut State) {
    state.pending.push_back(CommandKind::OpenModelDialog);
}

pub(crate) fn new_document(state: &mut State) {
    state.pending.push_back(CommandKind::NewDocument);
}

pub(crate) fn open_project(state: &mut State) {
    state.pending.push_back(CommandKind::OpenProjectDialog);
}

pub(crate) fn save_project(state: &mut State) {
    state.pending.push_back(CommandKind::SaveProjectDialog);
}

pub(crate) fn save_project_as(state: &mut State) {
    state.pending.push_back(CommandKind::SaveProjectAsDialog);
}

/// Post every operation to one NC program (M1.7b). The dialog arm picks
/// the path; the gate itself - generated, fresh, warning-free or refused
/// by name - lives once in the app layer (camcraft_app::posting).
pub(crate) fn post_program(state: &mut State) {
    state.pending.push_back(CommandKind::PostProgramDialog);
}

/// Close the app through the same queue as everything else (the window
/// button and, since M-g, Ctrl+Q both land here; the imgui File menu that
/// used to was retired with that shell).
pub(crate) fn quit(state: &mut State) {
    state.pending.push_back(CommandKind::Quit);
}

/// The unsaved-changes prompt's answer (#107), through the same queue.
pub(crate) fn resolve_unsaved(state: &mut State, choice: crate::unsaved::UnsavedChoice) {
    state.pending.push_back(CommandKind::ResolveUnsaved(choice));
}

pub(crate) fn undo(state: &mut State) {
    state.pending.push_back(CommandKind::Undo);
}

pub(crate) fn redo(state: &mut State) {
    state.pending.push_back(CommandKind::Redo);
}

pub(crate) fn delete_selected(state: &mut State) {
    state.pending.push_back(CommandKind::DeleteSelected);
}

pub(crate) fn copy_operation(state: &mut State, name: &str) {
    state.copy_operation(name);
}

pub(crate) fn paste_operation(state: &mut State) {
    state.paste_operation();
}

/// View: reframe the camera on the whole document (ephemeral, like the
/// camera itself - no command).
pub(crate) fn frame_all(state: &mut State) {
    state.pending.push_back(CommandKind::FrameAll);
}

/// View: look at one world point, keeping orientation and zoom (the
/// Deviate bar's jump - a located verdict you cannot fly to is only
/// half located; ephemeral like every other camera move, no command).
pub(crate) fn frame_at(state: &mut State, p: [f64; 3]) {
    state.cam.target = p;
}

/// Ribbon Inspect > Deviate (M2.3 stage B): arm or disarm the program
/// material sim. Arming runs the whole program - ops in posting order,
/// `stock_sim::simulate_document` walking `posting::program_ops`, so
/// what gets judged is exactly what would post - and stores the
/// verdict as view state (AGENTS.md's exemption, like `Sim`'s playhead:
/// never a command, never saved, retired by a document switch). The
/// simulation reads the world and writes nothing to it; a refusal
/// leaves the cell dark and the status line says why.
pub(crate) fn toggle_deviate(state: &mut State) {
    state.field_view = None; // either way, the old traced stock retires
    if state.program_sim.is_some() {
        state.program_sim = None;
        state.status = String::from("deviate off");
        return;
    }
    match camcraft_app::stock_sim::simulate_document(&state.world) {
        Ok(sim) => {
            let u = state.units();
            state.status = deviate_readout(&sim.dev, u);
            // The machine's vote joins the verdict when a machine is
            // mounted (stage C) - the clause law lives in `machine_vote`.
            if let Some(vote) = machine_vote(&state.world) {
                state.status += &vote;
            }
            state.program_sim = Some(sim);
            state.doc_version += 1; // the sheet is a new GPU part
        }
        Err(err) => {
            state.program_sim = None;
            state.status = format!("deviate refused: {err}");
        }
    }
}

/// M7.2 B3b + C: the traced viewport stock's field. With Deviate armed
/// and the transport down it is the whole program, walked once per
/// arming; with the transport up it is the playback field cut to the
/// playhead (judged when Deviate is armed too), rebuilt on any document
/// edit. Asked by the GPU-backed shell only (a headless run never pays
/// for a picture it cannot show); a refusal is kept and said on the
/// status line - the sheet stands, named.
pub(crate) fn ensure_field_view(state: &mut State) {
    let playing = state.sim.is_some();
    let judged = state.program_sim.is_some();
    let mode = (playing || judged).then_some((playing, judged, if playing { state.doc_version } else { 0 }));
    if mode != state.field_mode {
        state.field_mode = mode;
        state.field_view = None;
        state.field_play = None;
    }
    if mode.is_none() || matches!(state.field_view, Some(Err(_))) {
        return;
    }
    if !playing {
        if state.field_view.is_none() {
            walk_whole_program(state);
        }
        return;
    }
    if let Err(err) = follow_playhead(state, judged) {
        state.status += &format!(" | traced playback refused: {err} - the sheet stands");
        state.field_play = None;
        state.field_view = Some(Err(err));
    }
}

fn walk_whole_program(state: &mut State) {
    match camcraft_app::stock_view::field_view(&state.world) {
        Ok(fv) => {
            if let Some(note) = &fv.target_note {
                state.status += &format!(" | traced stock unjudged: {note}");
            }
            state.field_view = Some(Ok(std::sync::Arc::new(fv)));
            // The viewer's mesh rides the same view (M13.1 owed): one
            // extraction here, read every frame after.
            crate::machined::prime(state);
        }
        Err(err) => {
            state.status += &format!(" | traced stock refused: {err} - the sheet stands");
            state.field_view = Some(Err(err));
        }
    }
}

/// Point the playback worker at the transport's playhead and show its
/// newest picture. Never waits: the worker cuts (and packs) off this
/// thread, and until its first publication the sheet stands.
fn follow_playhead(state: &mut State, judged: bool) -> Result<(), String> {
    use camcraft_app::stock_play::{progress, PlaySeed};
    use camcraft_app::stock_play_worker::PlayWorker;
    use camcraft_app::stock_view::DEFAULT_TEXELS;
    if state.field_play.is_none() {
        let seed = PlaySeed::gather(&state.world, judged, DEFAULT_TEXELS)?;
        state.field_play = Some(PlayWorker::spawn(seed)?);
    }
    let sc = crate::egui_app::shell::cached_script(state);
    let t = state.sim.as_ref().map_or(0.0, |s| s.t);
    let play = state.field_play.as_ref().ok_or("playback was not built")?;
    let k = match progress(&sc, t) {
        None => 0,
        Some(p) => play
            .index()
            .index_at(&p)
            .ok_or_else(|| format!("{} is on the timeline but not in the posting plan", p.op))?,
    };
    play.request(k);
    if let Some(frame) = play.take() {
        state.field_view = Some(Ok(frame?.view));
    }
    Ok(())
}

/// The machine's vote as a status clause (M2.3 C): `None` when no
/// machine is mounted - the unstated-limit law in document form, so the
/// line never claims a judgment that was made. `check_document` walks
/// the same `program_ops` gate the sim just walked, so the vote is
/// always about exactly what was deviated. It informs; stage D is what
/// makes it gate. Pure, so the ui tests keep its shape without a window.
#[must_use]
pub fn machine_vote(world: &flecs_ecs::prelude::World) -> Option<String> {
    // #388: each machine judges the ops its own setups run; the vote
    // sums them (one machine - the common case - reads as it always did).
    let checks = camcraft_app::posting::machine_checks(world);
    if checks.is_empty() {
        return None;
    }
    let (mut problems, mut more) = (0usize, 0usize);
    let mut notes: Vec<String> = Vec::new();
    for (_, check) in checks {
        match check {
            Ok(c) => {
                problems += c.problems.len();
                more += c.more;
                notes.extend(c.notes);
            }
            Err(err) => return Some(format!("; machine: {err}")),
        }
    }
    // #669: what the machine's kind put outside the check is said, never
    // silently read as "within limits" (a plasma table's Z and rpm).
    let skipped = if notes.is_empty() { String::new() } else { format!(" - not judged: {}", notes.join("; ")) };
    Some(if problems == 0 {
        format!("; machine: within limits{skipped}")
    } else {
        format!("; machine: {problems} limit problem(s){}{skipped}", if more > 0 { format!(" (+{more})") } else { String::new() })
    })
}

/// The Deviate verdict in one line: the clean program says so; a
/// judged-worse program names both truths with their counts, volumes,
/// worst depth and located XY + sim Z - the sentence a user acts on,
/// and the test-pinned string law (`sim_readout`'s sibling). Pure, so
/// the ui tests keep its shape without a window.
#[must_use]
pub fn deviate_readout(dev: &camcraft_app::stock_sim::Deviation, unit: camcraft_app::units::Unit) -> String {
    use camcraft_app::geometry::Point3;
    use camcraft_app::units::{len, vol};
    if dev.is_clean() {
        return String::from("deviate: clean - the program meets the model everywhere");
    }
    let at = |p: Option<Point3>| -> String {
        p.map_or_else(
            || "?".to_owned(),
            |q| format!(" at ({}, {}, z {})", len(q.x, unit, 2), len(q.y, unit, 2), len(q.z, unit, 2)),
        )
    };
    let mut s = String::from("deviate:");
    if dev.rest_cells > 0 {
        s += &format!(
            " REST {} cells ({} {}, worst {} {}{})",
            dev.rest_cells,
            vol(dev.rest_volume, unit, 2),
            unit.vol_suffix(),
            len(dev.max_rest, unit, 2),
            unit.suffix(),
            at(dev.max_rest_at)
        );
    }
    if dev.gouge_cells > 0 {
        s += &format!(
            " GOUGE {} cells ({} {}, worst {} {}{})",
            dev.gouge_cells,
            vol(dev.gouge_volume, unit, 2),
            unit.vol_suffix(),
            len(dev.max_gouge, unit, 2),
            unit.suffix(),
            at(dev.max_gouge_at)
        );
    }
    s
}

/// View: look at the current target along a world direction - the
/// view cube's face/edge/corner law resolving to `Camera::toward`
/// (ephemeral, like the camera itself - no command; `target` and `dist`
/// survive, so orienting never pans or zooms).
pub(crate) fn orient_view(state: &mut State, d: [f64; 3]) {
    state.cam = state.cam.toward(d);
}

/// View: the house isometric (yaw 0.9, pitch 0.5) - what `FrameAll`
/// lands on, spelled as an orientation for the nav bar.
pub(crate) fn view_iso(state: &mut State) {
    state.cam = state.cam.oriented(0.9, 0.5);
}

/// View: flip perspective and orthographic (parallel) projection
/// (#434) - a direct field flip on the camera like every other view
/// toggle (ephemeral, never a command, never saved); `target`, `dist`
/// and orientation survive, and the ortho half-height equals what
/// perspective sees at the target plane, so the framing never jumps.
pub(crate) fn toggle_projection(state: &mut State) {
    state.cam.projection = match state.cam.projection {
        ProjectionMode::Perspective => ProjectionMode::Orthographic,
        ProjectionMode::Orthographic => ProjectionMode::Perspective,
    };
    state.status = match state.cam.projection {
        ProjectionMode::Perspective => "perspective projection".to_owned(),
        ProjectionMode::Orthographic => "orthographic projection".to_owned(),
    };
}

/// Point the context panes at `t`. Pure view state: the panes read it
/// through `State::pane_target`, which validates it against the world each
/// frame. Surfaces that also change what the world highlights (tree nodes,
/// viewport picks) push a `Select` alongside; a bare retarget never does.
pub(crate) fn target(state: &mut State, t: UiTarget) {
    state.ui_target = t;
}

/// Open the tool editor window (M1.4) on `name` - the named tool through
/// the same sync law the panes use, so the fields (and the live preview)
/// start at the world's numbers; the empty name opens a numbered blank
/// draft as the creation form. View state like `target`: opening itself
/// queues nothing; closing is the window's ✕ or the ghost law in
/// `State::drain_actions`, and only the form's buttons ever push
/// Add/Change/Delete.
pub(crate) fn open_tool_editor(state: &mut State, name: &str) {
    if name.is_empty() {
        let draft = state.new_tool_draft();
        state.tool_ui = draft;
    } else {
        state.tool_ui.name = name.to_owned();
        state.sync_tool_ui();
    }
    state.tool_editor = Some(name.to_owned());
    // #371: a fresh body draft per opening; it mirrors the tool's body.
    state.tool_body_ui = crate::state::ToolBodyUi::default();
    state.tool_profile_ui = crate::state::ToolProfileUi::default();
    state.tool_cutter_ui = crate::state::ToolCutterUi::default();
}

/// Quick-add an operation of `kind` (index into the registry the ribbon's
/// Create cells follow): name it after the kind it is - "2D Pocket2" says
/// what geometry is coming, the way a pressed tool previews its work
/// (M9.1) - give it the newest tool that can cut the kind (the M10.1
/// tool law's table, asked of app), cut the stock with retyped default
/// params, then make it both selected and targeted so the operation's
/// own pane lands on the click. Without a tool the refusal is loud on
/// the status line - the ribbon disables the button too, but every
/// surface shares this one effect. With tools but none that fit, the
/// newest one is handed on so `CreateOperation` refuses by name.
pub(crate) fn add_operation(state: &mut State, kind: usize) {
    let fitting = camcraft_app::operation::KindInfo::ALL
        .get(kind)
        .map(|k| camcraft_app::operation::tool_gate::fitting_tools(&state.world, k))
        .unwrap_or_default();
    let Some(tool) = fitting.last().cloned().or_else(|| state.tool_names().pop()) else {
        state.status = String::from("cannot add an operation: no tool in the document");
        return;
    };
    let label = camcraft_app::operation::KindInfo::ALL
        .get(kind)
        .map_or("Operation", |k| k.label);
    let mut n = 1;
    let name = loop {
        let candidate = format!("{label}{n}");
        if camcraft_app::model::find_by_name(&state.world, &candidate).is_none() {
            break candidate;
        }
        n += 1;
    };
    let mut params = quick_params(kind);
    // M11.3: the seed helix radius follows the op's tool (0.4 x radius).
    if let Some(t) = camcraft_app::model::find_by_name(&state.world, &tool)
        .and_then(|e| camcraft_app::model::tool_of(&state.world, &e))
    {
        camcraft_app::operation::seed_helix_for_tool(&mut params, t.diameter);
    }
    state.pending.push_back(CommandKind::CreateOperation {
        name: name.clone(),
        tool,
        src: SrcRef::Stock,
        params,
    });
    state.pending.push_back(CommandKind::Select(Some(name.clone())));
    state.ui_target = UiTarget::Operation(name);
}

/// Generate the selected operation, if any - surfaces gate the menu item
/// on the same question. Routes through the same arm as the pane button,
/// so a visible dirty draft is applied before generating from here too.
pub(crate) fn generate_selected(state: &mut State) {
    if let Some(name) = state.selected_operation() {
        state.generate_operation(&name);
    }
}

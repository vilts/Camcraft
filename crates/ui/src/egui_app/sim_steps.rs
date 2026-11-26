//! The transport bar's operation steps (#2608): "Previous op" and
//! "Next op" buttons, and Page Up / Page Down for them. A step
//! moves the playhead to an operation's start (`OpStarts::next`,
//! `OpStarts::prev`) and pauses there, so the user can check that
//! operation. The playhead is immediate-mode view state (AGENTS.md), so a
//! step is no command. The stock follows the playhead as it does for a
//! scrub, so a step shows the same stock as playback to that position.
//! The starts are calculated one time for each timeline
//! (`cached_op_starts`, keyed on the script's `Arc`), not in each frame:
//! a long program has a million segments (#2238 frame-cost rule).
//! Provenance: our own transport bar; nothing from vendor binaries.

use std::sync::Arc;

use egui::{Key, Ui};

use camcraft_app::sim::{OpStarts, Script};

use crate::state::{Sim, State};

/// The button labels (and AccessKit names).
pub(crate) const PREV_OP: &str = "Previous op";
pub(crate) const NEXT_OP: &str = "Next op";

/// The keys of the two steps, as the tips name them.
pub(crate) const PREV_KEY: &str = "Page Up";
pub(crate) const NEXT_KEY: &str = "Page Down";

/// The tip of a step button: what it does, the operation it goes to,
/// and its key. Pure, so the tests pin the words.
#[must_use]
pub(crate) fn step_tip(next: bool, target: Option<&str>, held: bool) -> String {
    let key = if next { NEXT_KEY } else { PREV_KEY };
    if held {
        return "Available when the stock removal is calculated".to_owned();
    }
    match (next, target) {
        (true, Some(op)) => format!("Go to the start of the next operation: {op}\t{key}"),
        (false, Some(op)) => format!("Go to the start of {op}\t{key}"),
        (true, None) => "This is the last operation".to_owned(),
        (false, None) => "This is the start of the first operation".to_owned(),
    }
}

/// Draw the two step buttons and read their keys. `held` is true while
/// the stock removal is calculated before playback: the scrub is held,
/// and so are the steps. The keys work only while no text field has the
/// focus.
pub(crate) fn op_step_buttons(ui: &mut Ui, sim: &mut Sim, sc: &Script, starts: &OpStarts, held: bool) {
    let prev = starts.prev(sim.t);
    let next = starts.next(sim.t);
    let typing = ui.memory(|m| m.focused().is_some());
    let (key_prev, key_next) = if typing {
        (false, false)
    } else {
        ui.input(|i| (i.key_pressed(Key::PageUp), i.key_pressed(Key::PageDown)))
    };
    let mut go = None;
    for (is_next, target, label, key) in [(false, prev, PREV_OP, key_prev), (true, next, NEXT_OP, key_next)] {
        let name = target.and_then(|t| starts.op_at(t)).and_then(|op| sc.op_name(op));
        let live = !held && target.is_some();
        let tip = step_tip(is_next, name, held);
        let resp = ui.add_enabled(live, egui::Button::new(label));
        ui.ctx().accesskit_node_builder(resp.id, |b| b.set_description(tip.as_str()));
        let clicked = resp.on_hover_text(tip.as_str()).on_disabled_hover_text(tip.as_str()).clicked();
        if live && (clicked || key) {
            go = target;
        }
    }
    if let Some(t) = go {
        sim.t = t;
        sim.playing = false;
    }
}

/// The operation starts of the current timeline
/// (`shell::cached_script`). Kept in `State` with a `Weak` of the script
/// that they belong to: while `cached_script` gives the same `Arc` (a
/// still frame), no segment is read. A new timeline (a document change)
/// is walked one time. The `Weak` lets the old script go, and its
/// address is not reused while the `Weak` lives.
pub(crate) fn cached_op_starts(state: &mut State) -> Arc<OpStarts> {
    let sc = super::shell::cached_script(state);
    if let Some((script, starts)) = &state.sim_op_starts
        && std::ptr::eq(script.as_ptr(), Arc::as_ptr(&sc))
    {
        return Arc::clone(starts);
    }
    let starts = Arc::new(crate::frame_cost::time("ui/viewport.sim_op_starts", || OpStarts::of(&sc)));
    state.sim_op_starts = Some((Arc::downgrade(&sc), Arc::clone(&starts)));
    starts
}

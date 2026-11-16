//! The Thread operation page's own rows (#894, #985): the feature list -
//! holes, or bosses for an external thread - the thread's pitch (left to
//! a multi-form mill's, or stated for a single-form mill) and its hand,
//! by name; the pass control (#988) - the schedule, finishing and spring
//! passes, the fit allowance, the spindle and the cutter's hand. The
//! rows edit the page draft; Apply rides
//! `EditOperationParams`, which refuses a thread the mounted mill cannot
//! cut, so the edit is gated and undoable.
//!
//! Provenance: ISO metric thread terms; our own page law. No vendor
//! material consulted.

use egui::Ui;

use camcraft_app::cl::ThreadHand;
use camcraft_app::drilling::Hole;
use camcraft_app::machining2d::{PassSchedule, ThreadPasses};
use camcraft_app::units::{self, Unit};

use super::fields::{button, check_row, field, hint, int_range_row, list_row, num, prop_row, radio_pair, small_button};

/// The Geometry tab's rows: whether the thread is external, the feature
/// list and its detect button. Returns whether the button was clicked
/// (one recognize pass per click; the list it finds lands as a command).
/// Switching between holes and bosses empties the draft's list: a hole
/// is not a boss, and the other kind is read from the model again.
pub(super) fn geometry_rows(ui: &mut Ui, u: Unit, features: &mut Vec<Hole>, external: &mut bool) -> bool {
    let was = *external;
    check_row(ui, "External thread (on a boss)", external);
    if *external != was {
        features.clear();
    }
    let (title, none, button_label, note) = if *external {
        (
            "Bosses",
            "none yet - Detect bosses reads them from the model",
            "Detect bosses",
            "each boss's diameter is the thread's MAJOR diameter (the nominal size CAD draws); the stock around it must be machined away first",
        )
    } else {
        (
            "Holes",
            "none yet - Detect holes reads them from the model",
            "Detect holes",
            "each hole's diameter is the thread's MINOR diameter (the tap-drill size CAD draws)",
        )
    };
    field(ui, title);
    if features.is_empty() {
        hint(ui, none);
    }
    // One row per feature, each with its own remove button: detection
    // lists every round wall (a round base is a boss too), and the
    // thread is cut only on the ones left in the list.
    let mut drop = None;
    for (i, h) in features.iter().enumerate() {
        let foot = match (*external, h.through) {
            (true, _) => format!("shoulder {}", units::len(h.floor_z, u, 3)),
            (false, true) => "through".to_owned(),
            (false, false) => format!("floor {}", units::len(h.floor_z, u, 3)),
        };
        ui.horizontal(|ui| {
            list_row(
                ui,
                format!(
                    "{}  X {}  Y {}  Ø{}  {} {}  {foot}",
                    i + 1,
                    units::len(h.xy.x, u, 3),
                    units::len(h.xy.y, u, 3),
                    units::len(2.0 * h.radius, u, 3),
                    if *external { "top" } else { "entry" },
                    units::len(h.entry_z, u, 3),
                ),
            );
            if small_button(ui, format!("Remove {}", i + 1)).clicked() {
                drop = Some(i);
            }
        });
    }
    if let Some(i) = drop.filter(|i| *i < features.len()) {
        features.remove(i);
    }
    let clicked = button(ui, button_label).clicked();
    hint(ui, note);
    clicked
}

/// The infeed row's hint: the depth the last pass lands on.
pub(super) fn infeed_hint(external: bool) -> &'static str {
    if external {
        "equal radial passes, each a full helix; the last lands on the external thread depth (0.6134 x pitch)"
    } else {
        "equal radial passes, each a full helix; the last lands on the thread depth (0.5413 x pitch)"
    }
}

/// The Passes tab's thread rows: whether the pitch is stated, the pitch,
/// and the hand.
pub(super) fn thread_rows(ui: &mut Ui, u: Unit, pitch: &mut Option<f64>, hand: &mut ThreadHand) {
    let mut stated = pitch.is_some();
    check_row(ui, "State the pitch", &mut stated);
    match (stated, pitch.as_mut()) {
        (true, Some(p)) => {
            prop_row(ui, "Thread pitch", |ui| num(ui, u, p));
        }
        (true, None) => *pitch = Some(1.0),
        (false, _) => *pitch = None,
    }
    hint(ui, "unstated reads a multi-form thread mill's own pitch; a single-form mill needs the pitch stated, inside its range");
    // The row lays right to left: Left first reads "Right hand  Left hand".
    prop_row(ui, "Hand", |ui| {
        radio_pair(ui, hand, (ThreadHand::Left, "Left hand"), (ThreadHand::Right, "Right hand"));
    });
    hint(ui, "climb milling winds a right-hand thread up from the bottom of a hole, a left-hand one down from the top; on a boss the other way round");
}

/// The Passes tab's pass-control rows (#988): the schedule, a finishing
/// pass of its own depth, spring passes and the fit allowance.
pub(super) fn pass_rows(ui: &mut Ui, u: Unit, passes: &mut ThreadPasses) {
    prop_row(ui, "Pass depths", |ui| {
        radio_pair(
            ui,
            &mut passes.schedule,
            (PassSchedule::ConstantArea, "Decreasing"),
            (PassSchedule::Equal, "Equal"),
        );
    });
    hint(ui, "decreasing: every pass takes the same chip area - the infeed is the first, deepest pass");
    prop_row(ui, "Finishing pass depth", |ui| num(ui, u, &mut passes.finish));
    prop_row(ui, "Spring passes", |ui| {
        int_range_row(ui, &mut passes.spring, 0..=camcraft_app::machining2d::MAX_SPRING_PASSES);
    });
    prop_row(ui, "Depth allowance", |ui| num(ui, u, &mut passes.allowance));
    hint(ui, "0 finishes on the standard depth; a positive allowance leaves that much radially for a tighter fit (at most a quarter of the depth)");
}

/// The Feeds tab's spindle rows (#988): the spindle direction and the
/// cutter's hand. Climb follows the spindle; a cutter against its
/// spindle is refused by name at Apply.
pub(super) fn spindle_rows(ui: &mut Ui, passes: &mut ThreadPasses) {
    prop_row(ui, "Spindle", |ui| {
        radio_pair(
            ui,
            &mut passes.spindle_reverse,
            (true, "M4 (counter-clockwise)"),
            (false, "M3 (clockwise)"),
        );
    });
    check_row(ui, "Left-hand-cut thread mill", &mut passes.cutter_left);
    hint(ui, "a right-hand-cut mill runs M3, a left-hand-cut one M4; climb milling follows the spindle");
}

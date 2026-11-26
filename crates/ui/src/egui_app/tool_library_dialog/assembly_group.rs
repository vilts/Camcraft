//! #3895 (U4 of #3663): the assembly group of the tool library window's
//! Tool tab - the holder pick (`tool_holder_picker`), the gauge length,
//! the projection (the stick-out), their read-only total, what Apply
//! holder would refuse (by name, before the click), a holder taper that
//! is not the machine's `tooling.spindle_mount`, and a projection that a
//! tool length change re-seeded (#808 item 2). The values are the
//! window's own holder draft (`ToolEditorView::holder`); only Apply
//! holder lands them, as the one undoable `SetToolBody::library`.
//! Provenance: our own group over `app::holder_library`; no HSMWorks
//! material involved.

use egui::{Color32, Ui};

use camcraft_app::holder_library;
use camcraft_app::units::{self, Unit};

use crate::state::{State, is_spun_kind};

/// The group's heading, and its AccessKit name.
pub(crate) const HEADING: &str = "Holder and stick-out";
/// #3930: the folded body section's heading, and its AccessKit name.
pub(crate) const BODY: &str = "Body";

const WARN: Color32 = Color32::from_rgb(230, 170, 60);
const BAD: Color32 = Color32::from_rgb(230, 90, 80);

pub(super) fn draw(ui: &mut Ui, state: &mut State) {
    ui.add_space(6.0);
    ui.separator();
    ui.strong(HEADING);
    if !state.tool_ui.exists {
        ui.label("Add the tool first, then choose its holder.");
        return;
    }
    let u = state.units();
    if is_spun_kind(state.tool_ui.tool_kind()) {
        super::super::tool_holder_picker::holder_fields(ui, state, u);
        notes(ui, state, u);
        super::super::tool_holder_picker::apply_holder_button(ui, state);
    } else {
        ui.label("A turning tool takes no library holder: import its holder in the Body section.");
    }
    body(ui, state, u);
}

/// #3930: the folded Body section - the stand-in holder cylinder, then
/// the body import, its placement and its pieces. Each landing is the
/// command it was in the tool shape window (`SetToolBody`, its piece
/// commands), or the window's Apply for the two stand-in numbers.
fn body(ui: &mut Ui, state: &mut State, u: Unit) {
    super::super::layout::group_open(ui, BODY, None, false, |ui| {
        super::super::tool_settings_grid::holder(ui, state);
        super::super::tool_body_page::body_section(ui, state, true, u);
    });
}

/// The total, the re-seed note, the mount warning and the refusal.
fn notes(ui: &mut Ui, state: &State, u: Unit) {
    let d = &state.tool_profile_ui;
    let Some(entry) = d.library.clone() else {
        return;
    };
    let (gauge, projection, reseeded) = (d.gauge, d.projection, d.reseeded);
    let l = |v: f64| format!("{} {}", units::len(v, u, 2), u.suffix());
    super::super::layout::prop_row_hint(
        ui,
        "Total gauge length",
        "from the gauge line to the tool tip: the gauge length plus the projection",
        |ui| ui.label(l(gauge + projection)),
    );
    if reseeded {
        ui.colored_label(
            WARN,
            format!("The tool length changed: the projection follows the new stick-out ({}). Apply holder to store it.", l(projection)),
        );
    }
    let stored = state.editor_stored_tool();
    let resolved = stored.as_ref().and_then(|t| holder_library::resolve(&entry, t.shank_d).ok());
    if let Some(taper) = resolved.and_then(|e| e.taper)
        && let Some(mount) = camcraft_app::machine::document_of(&state.world).1.and_then(|m| m.spindle_mount)
        && !mount.eq_ignore_ascii_case(taper.name)
    {
        ui.colored_label(WARN, format!("The machine's spindle takes {mount}: this holder has a {} taper.", taper.name));
    }
    // The shank refusal stands in the picker; this names the rest of what
    // Apply holder would refuse (the lengths, the fit on the tool).
    let shank_ok = stored.as_ref().is_some_and(|t| {
        holder_library::resolve(&entry, t.shank_d).and_then(|e| e.fits_shank(t.shank_d)).is_ok()
    });
    // Both verdicts come from the cached preview model (built when its
    // inputs change). The refusal is the command's, on the stored tool;
    // the draft note judges the unapplied tool numbers only.
    let m = super::model::of(state);
    if shank_ok && let Some(err) = m.holder_err.clone() {
        ui.colored_label(BAD, format!("Apply holder refuses this: {err}"));
    }
    if let Some(note) = m.draft_err.clone() {
        ui.colored_label(
            WARN,
            format!("Draft note: the unapplied tool numbers do not take this holder ({note}). Apply holder checks the stored tool."),
        );
    }
}

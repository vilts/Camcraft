//! The raised-rotary rows of a setup's Machine page (#821): how the
//! rotary's centerline is raised by the stack it stands on (rotary
//! subplates and the plates under them), the `nose_to_axis` the
//! index-swing check judges with, and the setup's own measured figure -
//! stated or cleared through `SetupNoseToAxis` (one undoable command per
//! committed edit). Drawn only when the machine turns the work on a
//! rotary.
//! Provenance: our own page law over `camcraft_app::rotary_stack`; no
//! vendor material consulted.

use egui::{DragValue, Id, RichText, Ui};

use camcraft_app::rotary_stack::{self, Rise};
use camcraft_app::units::{self, Unit};

use crate::state::{CommandKind, State};

/// The heading the section draws (the structural twin reads it).
pub(crate) const HEADING: &str = "Rotary centerline";
/// The checkbox that states the setup's own measured figure.
pub(crate) const STATE_OWN: &str = "This setup states its own spindle nose to rotary centerline (measured on its stack)";
/// What the machine file's `nose_to_axis` is called on the page.
const NOSE: &str = "spindle nose to rotary centerline";

/// The section, for setup `ctx` (`None` - the implicit context).
pub(super) fn section(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    let Some(m) = camcraft_app::machine::of_setup(&state.world, ctx).1 else { return };
    let noses: Vec<(String, Option<f64>)> =
        m.axes.iter().filter(|a| a.rotary && a.turns_part == Some(true)).map(|a| (a.name.clone(), a.nose_to_axis)).collect();
    if noses.is_empty() {
        return;
    }
    let u = state.units();
    let shown = |v: f64| format!("{:.4} {}", units::to_display(v, u), u.suffix());
    ui.separator();
    ui.label(RichText::new(HEADING).strong());
    for (r, n) in &noses {
        ui.label(match n {
            Some(n) => format!("Machine file: {r} {NOSE} {}", shown(*n)),
            None => format!("Machine file: {r} states no {NOSE}"),
        });
    }
    let base = noses.iter().filter_map(|(_, n)| *n).fold(f64::INFINITY, f64::min);
    let rise = rotary_stack::rise(&state.world, ctx);
    let mut fresh = base.is_finite().then_some(base);
    match &rise {
        Rise::Table => {
            ui.label(RichText::new("No rotary subplate in this setup: judged on the machine's figure.").weak());
        }
        Rise::Stated(_) => {}
        Rise::Stack(s) => {
            ui.label(format!("Raised by {}: {}", shown(s.thickness), s.describe()));
            fresh = fresh.map(|b| b - s.thickness).filter(|v| *v > 0.0);
        }
        Rise::Refused(why) => {
            ui.colored_label(ui.visuals().error_fg_color, format!("Index swing refused: {why}"));
        }
    }
    let Some(name) = ctx else {
        ui.label(RichText::new(format!("Add a setup to state its measured {NOSE}.")).weak());
        return;
    };
    let stated = rotary_stack::stated(&state.world, ctx);
    let mut on = stated.is_some();
    if ui.checkbox(&mut on, STATE_OWN).changed() {
        let value = if on { Some(fresh.unwrap_or(254.0)) } else { None };
        state.pending.push_back(CommandKind::SetupNoseToAxis { setup: name.to_owned(), value });
    }
    if let Some(v) = stated
        && base.is_finite()
        && let Err(why) = rotary_stack::check_stated(&state.world, ctx, v, base)
    {
        ui.colored_label(ui.visuals().error_fg_color, format!("Index swing refused: {why}"));
    }
    if let Some(v) = stated {
        let id = Id::new(("setup_nose_to_axis", name));
        let mut d = ui.data(|m| m.get_temp::<f64>(id)).unwrap_or_else(|| units::to_display(v, u));
        let speed = if u == Unit::Millimeters { 0.1 } else { 0.1 / 25.4 };
        let resp = ui.add(DragValue::new(&mut d).speed(speed).suffix(format!(" {}", u.suffix())));
        if resp.dragged() || resp.has_focus() {
            ui.data_mut(|m| m.insert_temp(id, d));
        }
        if resp.drag_stopped() || resp.lost_focus() || (resp.changed() && !resp.dragged() && !resp.has_focus()) {
            ui.data_mut(|m| m.remove::<f64>(id));
            let mm = units::from_display(d, u);
            if (mm - v).abs() > 1e-9 {
                state.pending.push_back(CommandKind::SetupNoseToAxis { setup: name.to_owned(), value: Some(mm) });
            }
        }
    } else if let Some(f) = fresh {
        ui.label(RichText::new(format!("The index swing is judged with {NOSE} {}.", shown(f))).weak());
    }
}

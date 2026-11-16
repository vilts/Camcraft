//! The post's user options (issue #34): a lathe post's thread output -
//! "Threading: G76 cycle / G32 passes" (Fanuc Lathe) or "G76 cycle / G33
//! passes" (Tormach PathPilot Lathe, #612) - and the Tormach PathPilot's end
//! units (#596), each shown under the post picker while its post is the
//! document's, and per-operation smoothing (#616) for every mill post. A
//! pick queues the
//! undoable `SetPostOptions`; the widget only reads the singleton. The
//! document's unit (#902) - display and posted program - is one row
//! shown on both the Stock page and the Post section, queuing `SetUnits`.
//! Provenance: our own panel; the choice's meaning is the post's
//! (assets/posts/fanuc-lathe.cps, assets/posts/tormach-lathe.cps).

use camcraft_app::posting;
use camcraft_app::schema::{EndUnits, PostOptions, PostRef, ThreadingOutput};
use camcraft_app::units::Unit;
use egui::Ui;

use crate::state::{CommandKind, State};

/// The pass word a lathe post writes when it does not write the cycle:
/// G32 (Fanuc) or G33 (LinuxCNC / PathPilot); `None` for a post with no
/// threading option.
fn pass_word(post: &PostRef) -> Option<&'static str> {
    match post {
        PostRef::Named(n) if n == "Fanuc Lathe" => Some("G32"),
        PostRef::Named(n) if n == "Tormach PathPilot Lathe" => Some("G33"),
        _ => None,
    }
}

/// The threading row, when the document posts through a lathe post
/// that reads the option.
pub(super) fn threading_picker(ui: &mut Ui, state: &mut State) {
    let Some(pass) = pass_word(&posting::post_of(&state.world)) else {
        return;
    };
    let now = posting::post_options(&state.world);
    let passes = format!("{pass} passes");
    let label = |t: ThreadingOutput| match t {
        ThreadingOutput::G32Passes => passes.clone(),
        ThreadingOutput::G76Cycle => "G76 cycle".to_owned(),
    };
    let mut pick: Option<ThreadingOutput> = None;
    ui.horizontal(|ui| {
        ui.label("Threading");
        egui::ComboBox::from_id_salt("post_threading")
            .selected_text(label(now.threading))
            .show_ui(ui, |ui| {
                for t in [ThreadingOutput::G76Cycle, ThreadingOutput::G32Passes] {
                    if ui.selectable_label(now.threading == t, label(t)).clicked() && now.threading != t {
                        pick = Some(t);
                    }
                }
            })
            .response
            .on_hover_text(format!("How a thread posts: one G76 multi-pass cycle the control runs, or every pass as {pass} moves"));
    });
    if let Some(t) = pick {
        state.pending.push_back(CommandKind::SetPostOptions(PostOptions { threading: t, ..now }));
    }
}

/// Per-operation smoothing (#616): off by default (no smoothing words);
/// on, each operation states its finish or rough tolerance and the post
/// writes its control's mode. Not offered for the lathe post, which
/// writes none.
pub(super) fn smoothing_switch(ui: &mut Ui, state: &mut State) {
    if posting::post_of(&state.world) == PostRef::Named("Fanuc Lathe".into()) {
        return;
    }
    let now = posting::post_options(&state.world);
    let mut want = now.smoothing;
    ui.checkbox(&mut want, "Smoothing per operation").on_hover_text(
        "write the control's smoothing mode per operation: finishing ops ask their tolerance, roughing ops a looser one from the stock they leave (G64 P, G5.1, G187, CYCLE832, G62) - needs the control's smoothing option on some machines",
    );
    if want != now.smoothing {
        state.pending.push_back(CommandKind::SetPostOptions(PostOptions { smoothing: want, ..now }));
    }
}

/// The end-units row (#596), when the document posts through Tormach
/// PathPilot: the unit the control is left in after M30 (LinuxCNC's
/// program end does not reset G20 / G21). "Leave" is the default and
/// posts the program unchanged.
pub(super) fn end_units_picker(ui: &mut Ui, state: &mut State) {
    if posting::post_of(&state.world) != PostRef::Named("Tormach PathPilot".into()) {
        return;
    }
    let now = posting::post_options(&state.world);
    let label = |u: EndUnits| match u {
        EndUnits::Leave => "Program's unit",
        EndUnits::Inch => "Inch (G20)",
        EndUnits::Millimeters => "mm (G21)",
    };
    let mut pick: Option<EndUnits> = None;
    ui.horizontal(|ui| {
        ui.label("End in");
        egui::ComboBox::from_id_salt("post_end_units")
            .selected_text(label(now.end_units))
            .show_ui(ui, |ui| {
                for u in [EndUnits::Leave, EndUnits::Inch, EndUnits::Millimeters] {
                    if ui.selectable_label(now.end_units == u, label(u)).clicked() && now.end_units != u {
                        pick = Some(u);
                    }
                }
            })
            .response
            .on_hover_text("the unit the control is left in after the program: M30 does not reset G20/G21, so a mm program on an inch-run control leaves the DRO and MDI in mm unless the unit word is written before M30");
    });
    if let Some(u) = pick {
        state.pending.push_back(CommandKind::SetPostOptions(PostOptions { end_units: u, ..now }));
    }
}

/// #1532: what the program does where it changes setup - pause (the
/// default), an optional stop, run straight on, or one file per setup.
/// Queues the same `SetPostOptions` command as the pickers above.
pub(super) fn between_setups_picker(ui: &mut Ui, state: &mut State) {
    let now = posting::post_options(&state.world);
    let mut pick: Option<camcraft_app::schema::BetweenSetups> = None;
    ui.horizontal(|ui| {
        ui.label("Between setups");
        egui::ComboBox::from_id_salt("post_between_setups")
            .selected_text(now.between_setups.label())
            .show_ui(ui, |ui| {
                for b in camcraft_app::schema::BetweenSetups::ALL {
                    if ui.selectable_label(now.between_setups == b, b.label()).clicked() && now.between_setups != b {
                        pick = Some(b);
                    }
                }
            })
            .response
            .on_hover_text("where the program moves on to the next setup the part has to be re-clamped: pause there with a comment naming the next setup, stop only with the optional-stop switch on, run straight on (several vises on one table), or write each setup as its own program");
    });
    if let Some(b) = pick {
        state.pending.push_back(CommandKind::SetPostOptions(PostOptions { between_setups: b, ..now }));
    }
}

/// The document's unit (#902), straight off the world singleton: one
/// setting for every number shown and for the program every post
/// writes (G21 / G20, Heidenhain G71 / G70, 840D G710 / G700). A click
/// queues the undoable `SetUnits`; there is no mirror to keep in step.
pub(super) fn units_row(ui: &mut Ui, state: &mut State, label: &str) {
    let u = state.units();
    ui.horizontal(|ui| {
        ui.label(label);
        for cand in [Unit::Millimeters, Unit::Inches] {
            let r = ui.radio(u == cand, cand.label()).on_hover_text(
                "the document's unit: every number shown, and the unit every post writes the program in (G21 mm / G20 inch, or the control's own word) - the machine file does not choose it",
            );
            if r.clicked() && u != cand {
                state.pending.push_back(CommandKind::SetUnits(cand));
            }
        }
    });
}

//! The machine editor's wire EDM section (#535, part of #290): the
//! `edm` keys (guide planes, taper, wire diameters, AWT, tank and
//! flushing), the control's
//! words (`edm.codes`) and the condition table, each value drawn with
//! its "not stated" face - a checkbox decides whether a value
//! participates, as everywhere in the machine editor. Edits are on the
//! editor's DRAFT; Apply pushes `EditMachineApply` (undoable), whose
//! door refuses what cannot be meant by name (`edm_refusal`).
//!
//! Provenance: our own UI over our own machine model (#290 D4/D5,
//! `camcraft_post::machine::{edm, edm_codes}`); no vendor text.

use camcraft_app::machine::{Condition, Edm, Flushing, Tank, CONDITION_ROLES};
use camcraft_app::units::Unit;
use egui::{ComboBox, TextEdit, Ui};

use super::{fields, pages};

/// The control words a wire post may ask for (#533) - offered as rows
/// so each shows "not stated" until the user writes it from their
/// manual. Any other name already on the machine is listed after them.
pub(super) const KNOWN_CODES: [(&str, &str); 5] = [
    ("condition", "selects a condition; must hold {id}"),
    ("comp_off", "cancels the control's wire compensation"),
    ("thread", "auto wire threading (AWT)"),
    ("cut", "cuts the wire at the end of a section (AWT)"),
    ("tip_disposal", "disposes of the wire tip (AWT)"),
];

pub(super) fn section(ui: &mut Ui, u: Unit, m: &mut camcraft_app::machine::Machine) {
    let before = m.edm.clone();
    let mut e = before.clone().unwrap_or_default();
    pages::field(ui, "Wire keys");
    keys(ui, u, &mut e);
    pages::field(ui, "EDM codes (the control's words)");
    codes(ui, &mut m.edm_codes);
    pages::field(ui, "Conditions");
    conditions(ui, u, &mut e);
    // Untouched stays untouched: an absent section is not made present
    // by looking at it.
    if e != before.clone().unwrap_or_default() {
        m.edm = Some(e);
    }
}

fn labelled_text(ui: &mut Ui, label: &str, text: &mut String, width: f32) {
    let l = ui.label(label);
    ui.add(TextEdit::singleline(text).desired_width(width)).labelled_by(l.id);
}

/// A stated/unstated number row. Toggling on seeds `seed`, never 0.
fn opt_row(ui: &mut Ui, label: &str, v: &mut Option<f64>, seed: f64, show: impl FnOnce(&mut Ui, &mut f64, &str)) {
    ui.horizontal(|ui| {
        let mut stated = v.is_some();
        if ui.checkbox(&mut stated, label).changed() {
            *v = stated.then_some(seed);
        }
        match v {
            Some(x) => show(ui, x, &format!("{label} value")),
            None => {
                ui.label("not stated");
            }
        }
    });
}

fn keys(ui: &mut Ui, u: Unit, e: &mut Edm) {
    opt_row(ui, "lower guide Z", &mut e.lower_guide_z, 0.0, |ui, v, n| fields::num_named(ui, u, v, n));
    opt_row(ui, "upper guide Z", &mut e.upper_guide_z, 50.0, |ui, v, n| fields::num_named(ui, u, v, n));
    opt_row(ui, "max taper (deg)", &mut e.max_taper_deg, 10.0, fields::num_plain_named);
    ui.horizontal(|ui| {
        ui.label("automatic threading");
        let shown = match e.awt {
            None => "not stated",
            Some(true) => "yes",
            Some(false) => "no",
        };
        let cb = ComboBox::from_id_salt("edm_awt").selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut e.awt, None, "not stated");
            ui.selectable_value(&mut e.awt, Some(true), "yes");
            ui.selectable_value(&mut e.awt, Some(false), "no");
        });
        fields::named(cb.response, "Automatic wire threading");
    });
    dielectric(ui, e);
    ui.label("wire diameters the machine takes");
    if e.wire_diameters.is_empty() {
        ui.label("not stated");
    }
    let mut remove = None;
    for (i, d) in e.wire_diameters.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            fields::num_named(ui, u, d, &format!("Wire diameter {}", i + 1));
            if ui.button("✖").on_hover_text("remove this diameter").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        e.wire_diameters.remove(i);
    }
    if ui.button("add wire diameter").clicked() {
        e.wire_diameters.push(0.25);
    }
}

/// #1499: the tank and the flushing pressure the job runs with. "not
/// stated" leaves both to the operator at the control (the post writes
/// no word for them).
fn dielectric(ui: &mut Ui, e: &mut Edm) {
    ui.horizontal(|ui| {
        ui.label("tank");
        let shown = e.tank.map_or("not stated", Tank::word);
        let cb = ComboBox::from_id_salt("edm_tank").selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut e.tank, None, "not stated");
            for t in [Tank::Submerged, Tank::NonSubmerged] {
                ui.selectable_value(&mut e.tank, Some(t), t.word());
            }
        });
        fields::named(cb.response, "Tank");
    });
    ui.horizontal(|ui| {
        ui.label("flushing");
        let shown = e.flushing.map_or("not stated", Flushing::word);
        let cb = ComboBox::from_id_salt("edm_flushing").selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut e.flushing, None, "not stated");
            for f in [Flushing::High, Flushing::Low] {
                ui.selectable_value(&mut e.flushing, Some(f), f.word());
            }
        });
        fields::named(cb.response, "Flushing");
    });
    // #2686: the flushing of each skim pass. It needs the rough cut's
    // flushing (the machine law), so it is offered only with one stated
    // and goes with it when that is taken back.
    if e.flushing.is_none() {
        e.skim_flushing = None;
    }
    ui.add_enabled_ui(e.flushing.is_some(), |ui| {
        ui.horizontal(|ui| {
            ui.label("skim flushing");
            let shown = e.skim_flushing.map_or("not stated", Flushing::word);
            let cb = ComboBox::from_id_salt("edm_skim_flushing").selected_text(shown).show_ui(ui, |ui| {
                ui.selectable_value(&mut e.skim_flushing, None, "not stated");
                for f in [Flushing::High, Flushing::Low] {
                    ui.selectable_value(&mut e.skim_flushing, Some(f), f.word());
                }
            });
            fields::named(cb.response, "Skim flushing");
        });
    });
}

fn codes(ui: &mut Ui, codes: &mut std::collections::BTreeMap<String, String>) {
    for (name, what) in KNOWN_CODES {
        ui.horizontal(|ui| {
            let mut stated = codes.contains_key(name);
            if ui.checkbox(&mut stated, format!("code {name}")).on_hover_text(what).changed() {
                if stated {
                    codes.insert(name.to_owned(), String::new());
                } else {
                    codes.remove(name);
                }
            }
            match codes.get_mut(name) {
                Some(t) => {
                    labelled_text(ui, &format!("Code {name}"), t, 150.0);
                }
                None => {
                    ui.label("not stated");
                }
            }
        });
    }
    let extra: Vec<String> = codes.keys().filter(|k| !KNOWN_CODES.iter().any(|(n, _)| n == k)).cloned().collect();
    for k in extra {
        ui.horizontal(|ui| {
            if let Some(t) = codes.get_mut(&k) {
                labelled_text(ui, &format!("Code {k}"), t, 150.0);
            }
            if ui.button("✖").on_hover_text("remove this code").clicked() {
                codes.remove(&k);
            }
        });
    }
}

fn conditions(ui: &mut Ui, u: Unit, e: &mut Edm) {
    if e.conditions.is_empty() {
        ui.label("none stated - every wire operation refuses until one is");
    }
    let mut remove = None;
    for (i, c) in e.conditions.iter_mut().enumerate() {
        ui.horizontal_wrapped(|ui| {
            labelled_text(ui, &format!("Condition {} id", i + 1), &mut c.id, 70.0);
            labelled_text(ui, &format!("Condition {} name", i + 1), &mut c.name, 90.0);
            opt_row(ui, &format!("gap {}", i + 1), &mut c.gap, 0.02, |ui, v, n| fields::num_named(ui, u, v, n));
            opt_row(ui, &format!("wire {}", i + 1), &mut c.wire_diameter, 0.25, |ui, v, n| fields::num_named(ui, u, v, n));
            role(ui, i, c);
            if ui.button("✖").on_hover_text("remove this condition").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        e.conditions.remove(i);
    }
    if ui.button("add condition").clicked() {
        e.conditions.push(Condition::default());
    }
}

fn role(ui: &mut Ui, i: usize, c: &mut Condition) {
    let shown = if c.role.is_empty() { "role not stated" } else { c.role.as_str() };
    let cb = ComboBox::from_id_salt(("edm-role", i)).selected_text(shown).show_ui(ui, |ui| {
        if ui.selectable_label(c.role.is_empty(), "role not stated").clicked() {
            c.role.clear();
        }
        for r in CONDITION_ROLES {
            if ui.selectable_label(c.role == r, r).clicked() {
                c.role = r.to_owned();
            }
        }
    });
    fields::named(cb.response, &format!("Condition {} role", i + 1));
}

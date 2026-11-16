//! The Machine window's sinker EDM section (#548, part of #290): the
//! `edm` keys a sinker machine states - the largest orbit radius, the
//! electrode pickup, the orbit list (CamCraft's geometry and the
//! control's own word), the generator condition table (#531) and the
//! `edm.codes` roles the sinker post reads (#542, held on the machine as
//! `Machine::edm_codes`). Absent values read
//! "not stated", never zero. Every edit is a change to the machine
//! DRAFT; Apply rides the existing undoable `EditMachineApply`, whose
//! `LoadMachine` refuses a draft the loader would refuse (`Edm::is_sane`),
//! so this page names the cause beside the section and the
//! refusal is never opaque. Orbit-pattern ops and side burning are #544 / #545;
//! the electrode-from-faces button (#547) joins here when it lands.
//!
//! Provenance: our own UI over our own machine grammar (#290 D4, D5);
//! general die-sink EDM practice; no vendor text.

use std::collections::BTreeMap;

use egui::{CollapsingHeader, ComboBox, TextEdit, Ui};

use camcraft_app::machine::{
    condition_id_key, condition_refusal, Condition, Edm, Kind, Machine, OrbitWord, CONDITION_ROLES, ORBIT_KINDS,
    SINKER_CODES,
};
use camcraft_app::units::Unit;

use super::{fields, pages};

/// Draw the section for a sinker machine; any other kind shows nothing.
/// The draft's `edm` is only written when an edit changed it, so merely
/// opening the window never turns an absent section into an empty one.
pub(super) fn draw(ui: &mut Ui, u: Unit, m: &mut Machine) {
    if !matches!(m.kind.as_deref().and_then(Kind::from_word), Some(Kind::SinkerEdm)) {
        return;
    }
    let before = m.edm.clone().unwrap_or_default();
    let mut edm = before.clone();
    CollapsingHeader::new("Sinker EDM").id_salt("machine-edm").default_open(true).show(ui, |ui| {
        keys(ui, u, &mut edm);
        orbits(ui, &mut edm);
        conditions(ui, u, &mut edm);
        codes(ui, &mut m.edm_codes);
        for p in problems(&edm, &m.edm_codes) {
            ui.colored_label(ui.visuals().error_fg_color, format!("Apply will refuse: {p}"));
        }
    });
    if edm != before {
        m.edm = Some(edm);
    }
}

/// A stated / "not stated" number: the unstated-limits law as a
/// checkbox. Toggling on seeds `seed` (a value the field's law accepts).
/// What ticking a condition's gap seeds, mm: a value the loader accepts.
const GAP_SEED_MM: f64 = 0.01;
/// What ticking a condition's undersize seeds, mm (within (0, 1] mm).
const UNDERSIZE_SEED_MM: f64 = 0.1;
/// What ticking a condition's wear rate seeds: a ratio the wear law
/// (`wear_rate_ok`, [0, 1)) accepts. It is the user's to correct.
const WEAR_SEED: f64 = 0.1;

fn stated_num(ui: &mut Ui, u: Unit, label: &str, v: &mut Option<f64>, seed: f64) {
    stated_with(ui, label, v, seed, |ui, x, name| fields::num_named(ui, u, x, name));
}

/// A stated number with no unit (a ratio): the same tick-in row.
fn stated_plain(ui: &mut Ui, label: &str, v: &mut Option<f64>, seed: f64) {
    stated_with(ui, label, v, seed, fields::num_plain_named);
}

fn stated_with(ui: &mut Ui, label: &str, v: &mut Option<f64>, seed: f64, show: impl FnOnce(&mut Ui, &mut f64, &str)) {
    ui.horizontal(|ui| {
        let mut on = v.is_some();
        if ui.checkbox(&mut on, label).changed() {
            *v = on.then_some(seed);
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
    pages::field(ui, "Sinker keys");
    stated_num(ui, u, "Max orbit radius", &mut e.max_orbit_radius, 1.0);
    ui.horizontal(|ui| {
        let mut on = e.pickup.is_some();
        if ui.checkbox(&mut on, "Electrode pickup").changed() {
            e.pickup = on.then(String::new);
        }
        match &mut e.pickup {
            Some(p) => {
                let l = ui.label("pickup name");
                ui.add(TextEdit::singleline(p).desired_width(140.0).hint_text("the holder system's name")).labelled_by(l.id);
            }
            None => {
                ui.label("not stated");
            }
        }
    });
}

fn orbits(ui: &mut Ui, e: &mut Edm) {
    pages::field(ui, "Orbits the control offers");
    let mut remove = None;
    for (i, o) in e.orbits.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let mut k = ORBIT_KINDS.iter().position(|w| *w == o.kind).unwrap_or(0);
            let shown = ORBIT_KINDS.get(k).copied().unwrap_or("circular");
            let cb = ComboBox::from_id_salt(("edm-orbit-kind", i)).selected_text(shown).width(90.0).show_ui(ui, |ui| {
                for (j, w) in ORBIT_KINDS.iter().enumerate() {
                    ui.selectable_value(&mut k, j, *w);
                }
            });
            fields::named(cb.response, &format!("Orbit {} kind", i + 1));
            o.kind = ORBIT_KINDS.get(k).copied().unwrap_or("circular").to_owned();
            let l = ui.label(format!("Orbit {} word", i + 1));
            ui.add(TextEdit::singleline(&mut o.word).desired_width(80.0).hint_text("control word")).labelled_by(l.id);
            if ui.button(format!("Remove orbit {}", i + 1)).clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        e.orbits.remove(i);
    }
    if e.orbits.is_empty() {
        ui.label("no orbits stated");
    }
    if ui.button("Add orbit").clicked() {
        e.orbits.push(OrbitWord { kind: ORBIT_KINDS[0].to_owned(), word: String::new() });
    }
}

fn conditions(ui: &mut Ui, u: Unit, e: &mut Edm) {
    pages::field(ui, "Conditions (from the machine's manual)");
    let mut remove = None;
    for (i, c) in e.conditions.iter_mut().enumerate() {
        let n = i + 1;
        ui.horizontal_wrapped(|ui| {
            let l = ui.label(format!("Condition {n} id"));
            ui.add(TextEdit::singleline(&mut c.id).desired_width(50.0)).labelled_by(l.id);
            let l = ui.label(format!("Condition {n} name"));
            ui.add(TextEdit::singleline(&mut c.name).desired_width(90.0)).labelled_by(l.id);
            // Seeds the condition law accepts (#560: a gap > 0, an
            // undersize in (0, 1] mm); the user types the manual's.
            stated_num(ui, u, &format!("Condition {n} gap"), &mut c.gap, GAP_SEED_MM);
            let us = format!("Condition {n} undersize");
            stated_num(ui, u, &us, &mut c.undersize, UNDERSIZE_SEED_MM);
            let mut r = CONDITION_ROLES.iter().position(|w| *w == c.role).map_or(0, |p| p + 1);
            let shown = if r == 0 { "role not stated" } else { CONDITION_ROLES[r - 1] };
            let cb = ComboBox::from_id_salt(("edm-cond-role", i)).selected_text(shown).width(110.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut r, 0, "role not stated");
                for (j, w) in CONDITION_ROLES.iter().enumerate() {
                    ui.selectable_value(&mut r, j + 1, *w);
                }
            });
            fields::named(cb.response, &format!("Condition {n} role"));
            c.role = if r == 0 { String::new() } else { CONDITION_ROLES[r - 1].to_owned() };
            if ui.button(format!("Remove condition {n}")).clicked() {
                remove = Some(i);
            }
        });
        // A second line keeps the row inside the window (#1381): the wear
        // rate and the notes.
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            // The axial wear rate (#1381): a ratio, so no unit lens. A value
            // outside [0, 1) is named below and refused at Apply.
            stated_plain(ui, &format!("Condition {n} wear rate"), &mut c.wear, WEAR_SEED);
            let l = ui.label(format!("Condition {n} notes"));
            ui.add(TextEdit::singleline(&mut c.notes).desired_width(120.0).hint_text("free text")).labelled_by(l.id);
        });
    }
    if let Some(i) = remove {
        e.conditions.remove(i);
    }
    if e.conditions.is_empty() {
        ui.label("no conditions stated - a sinker op refuses by name until one is");
    }
    if ui.button("Add condition").clicked() {
        let mut n = e.conditions.len() + 1;
        while e.conditions.iter().any(|c| condition_id_key(&c.id) == condition_id_key(&format!("C{n}"))) {
            n += 1;
        }
        e.conditions.push(Condition { id: format!("C{n}"), ..Condition::default() });
    }
}

fn codes(ui: &mut Ui, codes: &mut BTreeMap<String, String>) {
    pages::field(ui, "Control codes (edm.codes)");
    for (role, placeholders) in SINKER_CODES {
        ui.horizontal(|ui| {
            let mut on = codes.contains_key(role);
            if ui.checkbox(&mut on, format!("Code {role}")).changed() {
                if on {
                    codes.insert(role.to_owned(), String::new());
                } else {
                    codes.remove(role);
                }
            }
            match codes.get_mut(role) {
                Some(t) => {
                    let l = ui.label(format!("{role} template"));
                    let hint = if placeholders.is_empty() { "the control's word".to_owned() } else { format!("fills {placeholders}") };
                    ui.add(TextEdit::singleline(t).desired_width(150.0).hint_text(hint)).labelled_by(l.id);
                }
                None => {
                    ui.label("not stated");
                }
            }
        });
    }
    let extra: Vec<&str> = codes.keys().map(String::as_str).filter(|k| !SINKER_CODES.iter().any(|(r, _)| r == k)).collect();
    if !extra.is_empty() {
        ui.label(format!("also states (kept as written): {}", extra.join(", ")));
    }
}

/// Why `LoadMachine` would refuse this section, in words - the same
/// laws as `Edm::is_sane` / the file loader, stated one by one. Each
/// condition is judged by the loader's own `condition_refusal` (id
/// spelling, a gap > 0, an undersize in (0, 1] mm), its ids compared as
/// the loader compares them (case and leading zeros ignored), and a
/// wire-only field on this sinker is named as the loader names it.
pub(super) fn problems(e: &Edm, codes: &BTreeMap<String, String>) -> Vec<String> {
    let mut out = Vec::new();
    if e.max_orbit_radius.is_some_and(|r| !(r.is_finite() && r > 0.0)) {
        out.push("max orbit radius must be positive".to_owned());
    }
    for (i, o) in e.orbits.iter().enumerate() {
        if o.word.trim().is_empty() {
            out.push(format!("orbit {} states no control word", i + 1));
        }
    }
    for (i, c) in e.conditions.iter().enumerate() {
        let n = i + 1;
        if c.id.trim().is_empty() {
            out.push(format!("condition {n} states no id"));
        } else if let Some(r) = condition_refusal(c, &e.wire_diameters) {
            out.push(format!("condition {n} {r}"));
        }
        if c.wire_diameter.is_some() {
            out.push(format!("condition {n} states a wire diameter - a wire field; this is a sinker machine"));
        }
        if !c.id.trim().is_empty()
            && let Some(d) = e.conditions.iter().skip(n).find(|d| condition_id_key(&d.id) == condition_id_key(&c.id))
        {
            if d.id == c.id {
                out.push(format!("condition id {:?} is stated twice", c.id));
            } else {
                out.push(format!("condition ids {:?} and {:?} are one selector (case and leading zeros are ignored)", c.id, d.id));
            }
        }
    }
    for (role, t) in codes {
        if t.trim().is_empty() {
            out.push(format!("code {role:?} is blank - type the control's word or untick it"));
        }
    }
    if let Some(p) = &e.pickup
        && p.trim().is_empty()
    {
        out.push("electrode pickup is blank - name it or untick it".to_owned());
    }
    out
}

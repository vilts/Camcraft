//! The "Add built-in vise…" dialog body (#788), drawn inside the fixture
//! library's modal: pick a size from the table, edit any figure to match
//! the shop's own vise, then either fit it to the setup's stock (fixed
//! jaw on the stock's +Y side, centred, floor under it, jaws opened to
//! the stock) or type where its frame goes. Place queues one undoable
//! `InsertVise` through the command queue. The 5-axis self-centering
//! sizes (#787) are a second row of the picker: centred on the stock,
//! with a clamping range and an optional dovetail lip instead of a
//! screw housing.
//! Provenance: our own layout over our own vise commands.

use egui::{RichText, Ui};

use camcraft_app::units::{Unit, len};
use camcraft_app::vise::{self, ViseStyle};
use camcraft_app::vise_fit::VisePreview;

use crate::state::{CommandKind, ViseDraft};

use super::fields::{num, num_plain};

/// What the body's buttons asked for.
pub(crate) enum ViseAct {
    Close,
    Place(Box<CommandKind>),
    /// Close the dialog and open the Stock page.
    OpenStock,
}

/// The Place button's command for this draft.
#[must_use]
pub(crate) fn place_command(d: &ViseDraft) -> CommandKind {
    CommandKind::InsertBuiltinVise {
        setup: (!d.setup.is_empty()).then(|| d.setup.clone()),
        params: d.params.clone(),
        fit_stock: fitting(d),
        pos: d.pos,
        yaw_deg: d.yaw_deg,
    }
}

/// Is Fit in force: ticked, and the setup has a stock body to fit to.
fn fitting(d: &ViseDraft) -> bool {
    d.fit_stock && !matches!(d.fit, Some(VisePreview::NoStock))
}

/// The plain lines under the Fit checkbox: what the vise grips, and where
/// it lands with a stock (all from `vise_fit::preview`, the command's own
/// derivation).
fn fit_lines(ui: &mut Ui, u: Unit, d: &ViseDraft) -> Option<ViseAct> {
    let mut act = None;
    match &d.fit {
        Some(VisePreview::NoStock) => {
            ui.label(
                "This setup has no stock yet. A vise grips the stock - the block the part is cut from -                  not the imported model. Set the stock on the Stock page to fit the vise to it,                  or place the vise by position below.",
            );
            if ui.button("Open the Stock page").clicked() {
                act = Some(ViseAct::OpenStock);
            }
        }
        Some(VisePreview::Fits { opening, at, centred }) if d.fit_stock => {
            ui.label(RichText::new("The vise grips the stock (set on the Stock page), not the imported model.").small());
            let l = |v: f64| format!("{} {}", len(v, u, 3), u.suffix());
            let (x, y, z) = (l(at.x), l(at.y), l(at.z));
            if *centred {
                ui.label(format!(
                    "Centred on the stock at X {x}, Y {y}; the jaws close on its {} Y width; the stock sits on the jaw floor (Z {z}).",
                    l(*opening)
                ));
            } else {
                ui.label(format!(
                    "Opens to {} on the stock's Y width; the fixed jaw sits against the stock's +Y side (Y {y}), centred in X ({x}); the stock sits on the jaw floor (Z {z}).",
                    l(*opening)
                ));
            }
        }
        Some(VisePreview::Refused(why)) if d.fit_stock => {
            ui.colored_label(ui.visuals().error_fg_color, format!("This vise cannot be fitted to the stock: {why}"));
        }
        _ => {
            ui.label(RichText::new("The vise grips the stock (set on the Stock page), not the imported model.").small());
        }
    }
    act
}

pub(crate) fn body(ui: &mut Ui, u: Unit, d: &mut ViseDraft) -> Option<ViseAct> {
    let mut act = None;
    ui.heading("Add built-in vise");
    ui.label(RichText::new("A generic vise built from these figures; edit them to match yours.").small());
    ui.add_space(4.0);
    let mut picked = None;
    for (title, self_centering) in [("Size (jaw width)", false), ("5-axis self-centering", true)] {
        ui.label(title);
        ui.horizontal_wrapped(|ui| {
            for (k, row) in vise::presets().enumerate() {
                if matches!(row.style, ViseStyle::SelfCentering(_)) == self_centering
                    && ui.selectable_label(d.size == k, row.label).clicked()
                {
                    picked = Some(k);
                }
            }
        });
    }
    if let Some(k) = picked {
        d.pick(k);
    }
    let p = &mut d.params;
    egui::Grid::new("builtin-vise-dims").num_columns(2).show(ui, |ui| {
        let mut rows: Vec<(&str, &mut f64)> = vec![
            ("Jaw width", &mut p.jaw_width),
            ("Jaw height", &mut p.jaw_height),
            ("Jaw thickness", &mut p.jaw_thickness),
            ("Jaw body depth", &mut p.jaw_body_depth),
            ("Max opening", &mut p.max_opening),
            ("Overall length", &mut p.length),
            ("Overall width", &mut p.width),
            ("Base height", &mut p.base_height),
        ];
        match &mut p.style {
            ViseStyle::Standard => {
                rows.push(("Housing length", &mut p.housing_length));
                rows.push(("Housing height", &mut p.housing_height));
            }
            ViseStyle::SelfCentering(sc) => {
                rows.push(("Min opening", &mut sc.min_opening));
                rows.push(("Dovetail lip height", &mut sc.lip_height));
                rows.push(("Dovetail lip depth", &mut sc.lip_depth));
            }
        }
        for (label, v) in rows {
            ui.label(label);
            num(ui, u, v);
            ui.end_row();
        }
    });
    let centred = matches!(d.params.style, ViseStyle::SelfCentering(_));
    let fit = if centred { "Fit to the stock (centred, jaws closed on it)" } else { "Fit to the stock (jaws open to it)" };
    if matches!(d.fit, Some(VisePreview::NoStock)) {
        let mut off = false;
        ui.add_enabled(false, egui::Checkbox::new(&mut off, fit))
            .on_disabled_hover_text("Fit to the stock needs a stock: this setup has none yet.");
    } else {
        ui.checkbox(&mut d.fit_stock, fit);
    }
    if let Some(a) = fit_lines(ui, u, d) {
        act = Some(a);
    }
    if !fitting(d) {
        egui::Grid::new("builtin-vise-place").num_columns(2).show(ui, |ui| {
            ui.label("Jaw opening");
            num(ui, u, &mut d.params.opening);
            ui.end_row();
            let axes = if centred { ["Centre X", "Centre Y", "Floor Z"] } else { ["Fixed jaw X", "Fixed jaw Y", "Floor Z"] };
            for (k, axis) in axes.iter().enumerate() {
                ui.label(*axis);
                num(ui, u, &mut d.pos[k]);
                ui.end_row();
            }
            ui.label("Turn about Z (yaw)");
            num_plain(ui, &mut d.yaw_deg);
            ui.end_row();
        });
    }
    if let Some(err) = &d.error {
        super::error_text::error_text(ui, ui.visuals().error_fg_color, err);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Place").clicked() {
            act = Some(ViseAct::Place(Box::new(place_command(d))));
        }
        if ui.button("Cancel").clicked() {
            act = Some(ViseAct::Close);
        }
    });
    act
}

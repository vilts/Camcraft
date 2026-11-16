//! The property-page form law (user review 2026-11-06: "crammed, hard to
//! understand, hard to read"): every page lays its fields out through
//! these helpers, so the whole operation dialog reads one way -
//!
//! - a property grid: label LEFT in a fixed-width column, control RIGHT
//!   on the same row, right-aligned and compact ([`prop_row`],
//!   [`check_row`], [`prop_grid`]);
//! - units as a suffix on the value, never in the label
//!   ([`value_with_unit`]);
//! - help as a hover tooltip on the label ([`hint`] attaches it to the
//!   row just drawn), never an inline paragraph;
//! - a field whose 0 means "derived" as an explicit Auto toggle
//!   ([`auto_value`]) - Auto on writes the model's 0, so the params and
//!   the commands stay exactly what they were;
//! - concept groups, collapsible, an optional feature's group enabled by
//!   a checkbox in its header ([`group`]);
//! - a one-line coloured status banner that expands on click
//!   ([`status_banner`]) and a bottom action bar ([`action_bar`]).
//!
//! Every helper is immediate-mode layout only: it edits the `&mut` it is
//! handed (the page's draft), never the world - the pages' commands stay
//! the one way a change lands (AGENTS.md, "Everything Is a Command").
//!
//! Provenance: our own layout code over egui's Grid / CollapsingState /
//! layouts (the vendored egui 0.36 docs); the label-left property grid
//! is the generic pattern of desktop property inspectors. No vendor
//! material, strings or layouts copied.

use egui::{Align, Button, Color32, DragValue, Id, Label, Layout, Response, RichText, Ui, Vec2};

/// The label column's width - one value, so every row of every page
/// lines up.
pub const LABEL_W: f32 = 120.0;
/// A compact control's width (combos, numbers).
pub const CTRL_W: f32 = 96.0;
/// Space between two groups.
pub const GROUP_GAP: f32 = 6.0;

/// Where the last row's label response waits for a [`hint`] this frame.
fn last_label_id() -> Id {
    Id::new("camcraft.form.last_label")
}

fn remember(ui: &Ui, r: &Response) {
    let frame = ui.ctx().cumulative_frame_nr();
    ui.ctx().data_mut(|d| d.insert_temp(last_label_id(), (frame, r.clone())));
}

fn label_cell(ui: &mut Ui, label: &str) -> Response {
    let h = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(Vec2::new(LABEL_W, h), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_width(LABEL_W);
        ui.add(Label::new(label).truncate())
    })
    .inner
}

/// The id of the label of the property row drawn last this frame, for a
/// control to be named by it (`Response::labelled_by`, #1119). `None` when
/// no row was drawn this frame.
pub fn row_label_id(ui: &Ui) -> Option<Id> {
    let frame = ui.ctx().cumulative_frame_nr();
    let last: Option<(u64, Response)> = ui.ctx().data(|d| d.get_temp(last_label_id()));
    last.and_then(|(f, r)| (f == frame).then_some(r.id))
}

/// One property row: `label` in the label column, `add`'s controls at
/// the right, on one line. The label's full text rides its hover (a
/// truncated label never hides its name).
pub fn prop_row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        let lab = label_cell(ui, label);
        remember(ui, &lab);
        ui.with_layout(Layout::right_to_left(Align::Center), add).inner
    })
    .inner
}

/// [`prop_row`] with its help as the label's hover tooltip.
pub fn prop_row_hint<R>(ui: &mut Ui, label: &str, help: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let out = prop_row(ui, label, add);
    hint(ui, help);
    out
}

/// A checkbox as a property row: the caption is the row's label, the
/// box sits in the control column. Returns the box's response (callers
/// read `.changed()`).
pub fn check_row(ui: &mut Ui, label: &str, v: &mut bool) -> Response {
    ui.horizontal(|ui| {
        // The caption toggles too, as a checkbox's own text would.
        let h = ui.spacing().interact_size.y;
        let lab = ui
            .allocate_ui_with_layout(Vec2::new(LABEL_W, h), Layout::left_to_right(Align::Center), |ui| {
                ui.set_min_width(LABEL_W);
                ui.add(Label::new(label).truncate())
            })
            .inner;
        remember(ui, &lab);
        let hit = ui.interact(lab.rect, lab.id.with("toggle"), egui::Sense::click());
        let mut r = ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.checkbox(v, "")).inner;
        if hit.clicked() {
            *v = !*v;
            r.mark_changed();
        }
        r
    })
    .inner
}

/// Attach `help` as the hover tooltip of the row drawn last this frame;
/// with no row drawn yet this frame, it shows as a small dimmed line.
pub fn hint(ui: &mut Ui, help: &str) {
    let frame = ui.ctx().cumulative_frame_nr();
    let last: Option<(u64, Response)> = ui.ctx().data(|d| d.get_temp(last_label_id()));
    match last {
        Some((f, r)) if f == frame => {
            r.on_hover_text(help);
        }
        _ => {
            ui.label(RichText::new(help).small().weak());
        }
    }
}

/// A dimmed caption under a row - the ONE consequence line a safety rule
/// may keep visible (e.g. what stands in when an entry cannot fit).
pub fn consequence(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(LABEL_W + ui.spacing().item_spacing.x);
        ui.add(Label::new(RichText::new(text).small().weak()).wrap());
    });
}

/// A two-column property grid (label | control) for pages that draw
/// their rows inside one `egui::Grid`: `add` calls `ui.end_row()` after
/// each row. The label column keeps [`LABEL_W`].
pub fn prop_grid<R>(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Grid::new(id)
        .num_columns(2)
        .min_col_width(LABEL_W)
        .spacing(Vec2::new(ui.spacing().item_spacing.x, 4.0))
        .striped(false)
        .show(ui, add)
        .inner
}

/// What a [`group`] call drew: whether its header checkbox flipped this
/// frame, and the body's answer when the body was open (and enabled).
pub struct GroupOut<R> {
    pub toggled: bool,
    pub inner: Option<R>,
}

/// A titled, collapsible concept group (open by default). With
/// `enabled`, the header carries the checkbox that switches the whole
/// optional feature on or off; an off group draws no body (nothing it
/// would hold applies).
pub fn group<R>(
    ui: &mut Ui,
    title: &str,
    enabled: Option<&mut bool>,
    add: impl FnOnce(&mut Ui) -> R,
) -> GroupOut<R> {
    group_open(ui, title, enabled, true, add)
}

/// [`group`] with its first-frame open state (`default_open`).
pub fn group_open<R>(
    ui: &mut Ui,
    title: &str,
    enabled: Option<&mut bool>,
    default_open: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> GroupOut<R> {
    let id = ui.make_persistent_id(("camcraft.group", title));
    let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, default_open);
    let mut toggled = false;
    let mut on = true;
    let header = state.show_header(ui, |ui| {
        if let Some(e) = enabled {
            toggled = ui.checkbox(e, "").on_hover_text(format!("use {title}")).changed();
            on = *e;
        }
        ui.label(RichText::new(title).strong());
    });
    let body = header.body(|ui| {
        if on {
            Some(egui::Frame::NONE.inner_margin(egui::Margin { left: 2, right: 0, top: 0, bottom: 2 }).show(ui, add).inner)
        } else {
            None
        }
    });
    ui.add_space(GROUP_GAP);
    GroupOut { toggled, inner: body.2.and_then(|b| b.inner) }
}

/// A number with its unit as a suffix inside the field; returns the
/// response (`changed()` on an edit).
pub fn value_with_unit(ui: &mut Ui, v: &mut f64, unit: &str, speed: f64) -> Response {
    let mut dv = DragValue::new(v).speed(speed);
    if !unit.is_empty() {
        dv = dv.suffix(format!(" {unit}"));
    }
    ui.add_sized(Vec2::new(CTRL_W, ui.spacing().interact_size.y), dv)
}

/// The toggle for a field whose 0 means "derived" (or "no limit"): the
/// box captioned `toggle` ("Auto", "No limit") on writes 0 - the model's
/// own sentinel - and shows `derived` dimmed in place of the number; off
/// seeds `seed` for the user to edit, drawn by `edit`. Returns whether
/// the value moved this frame.
pub fn auto_value(
    ui: &mut Ui,
    label: &str,
    toggle: &str,
    v: &mut f64,
    derived: &str,
    seed: f64,
    edit: impl FnOnce(&mut Ui, &mut f64),
) -> bool {
    let before = *v;
    prop_row(ui, label, |ui| {
        let mut auto = *v == 0.0;
        if ui.checkbox(&mut auto, toggle).changed() {
            *v = if auto { 0.0 } else { seed };
        }
        if auto {
            ui.add_enabled(false, Label::new(RichText::new(derived).weak()));
        } else {
            edit(ui, v);
        }
    });
    *v != before
}

/// [`auto_value`] for a field the model keeps as an `Option`: `None` is
/// the derived figure. Auto on writes `None`; off seeds `Some(seed)` for
/// `edit` to draw. Returns whether the value moved this frame.
pub fn auto_opt(
    ui: &mut Ui,
    label: &str,
    toggle: &str,
    v: &mut Option<f64>,
    derived: &str,
    seed: f64,
    edit: impl FnOnce(&mut Ui, &mut f64),
) -> bool {
    let before = *v;
    prop_row(ui, label, |ui| {
        let mut auto = v.is_none();
        if ui.checkbox(&mut auto, toggle).changed() {
            *v = if auto { None } else { Some(seed) };
        }
        match v {
            Some(x) => edit(ui, x),
            None => {
                ui.add_enabled(false, Label::new(RichText::new(derived).weak()));
            }
        }
    });
    *v != before
}

/// How a status banner reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Warn,
    Error,
    Quiet,
}

/// The one-line coloured status at the top of a page: `line` in its
/// tone's ink; with `details`, a click expands them as a compact list
/// (view state, never a command).
pub fn status_banner(ui: &mut Ui, id: &str, line: &str, ink: Color32, details: &[String]) {
    let _ = status_banner_located(ui, id, line, ink, details, &[]);
}

/// [`status_banner`] whose detail lines can be links (#261): a detail
/// `i` with `links[i]` true reads as a link (hover says so) and a click
/// on it returns `Some(i)` - the caller decides what it shows. A
/// missing `links` entry is plain text.
pub fn status_banner_located(ui: &mut Ui, id: &str, line: &str, ink: Color32, details: &[String], links: &[bool]) -> Option<usize> {
    let key = ui.make_persistent_id(("camcraft.banner", id));
    let mut open: bool = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(false);
    let arrow = if details.is_empty() {
        ""
    } else if open {
        "v "
    } else {
        "> "
    };
    let text = RichText::new(format!("{arrow}{line}")).color(ink).strong();
    let r = ui.add(Label::new(text).sense(egui::Sense::click()).truncate());
    if !details.is_empty() && r.clicked() {
        open = !open;
        ui.ctx().data_mut(|d| d.insert_temp(key, open));
    }
    if !details.is_empty() {
        r.on_hover_text(if open { "hide the details" } else { "show the details" });
    }
    let mut hit = None;
    if open {
        ui.indent(key, |ui| {
            for (i, d) in details.iter().enumerate() {
                if links.get(i).copied().unwrap_or(false) {
                    let r = ui.add(Label::new(RichText::new(d).small().underline()).sense(egui::Sense::click()).wrap());
                    if r.on_hover_text("show this move in the viewport").clicked() {
                        hit = Some(i);
                    }
                } else {
                    ui.add(Label::new(RichText::new(d).small()).wrap());
                }
            }
        });
    }
    hit
}

/// An action bar button's role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Primary,
    Secondary,
    Destructive,
}

/// One action bar button.
pub struct Action<'a> {
    pub label: &'a str,
    pub kind: ActionKind,
    pub enabled: bool,
    /// Hover text while enabled.
    pub hint: Option<&'a str>,
    /// Why it is disabled (hover text while disabled).
    pub why_not: Option<&'a str>,
}

/// The inks the bar paints with (theme tokens and the colour-vision
/// palette - never literals).
pub struct BarInks {
    pub accent: Color32,
    pub on_accent: Color32,
    pub danger: Color32,
}

/// The bottom action bar: the primary action filled with the accent,
/// the secondaries plain, the destructive ones pushed to the far right
/// in the danger ink. Returns the index of the clicked action.
pub fn action_bar(ui: &mut Ui, actions: &[Action<'_>], inks: &BarInks) -> Option<usize> {
    let mut clicked = None;
    ui.separator();
    let draw = |ui: &mut Ui, i: usize, a: &Action<'_>, clicked: &mut Option<usize>| {
        let btn = match a.kind {
            ActionKind::Primary => Button::new(RichText::new(a.label).strong().color(inks.on_accent)).fill(inks.accent),
            ActionKind::Secondary => Button::new(a.label),
            ActionKind::Destructive => {
                Button::new(RichText::new(a.label).color(inks.danger)).stroke(egui::Stroke::new(1.0, inks.danger))
            }
        };
        let mut r = ui.add_enabled(a.enabled, btn.min_size(Vec2::new(0.0, 24.0)));
        if let Some(h) = a.hint {
            r = r.on_hover_text(h);
        }
        if let Some(w) = a.why_not {
            r = r.on_disabled_hover_text(w);
        }
        if r.clicked() {
            *clicked = Some(i);
        }
    };
    // The working verbs wrap on a narrow pane; the destructive ones sit
    // apart on their own row, at the right.
    ui.horizontal_wrapped(|ui| {
        for (i, a) in actions.iter().enumerate().filter(|(_, a)| a.kind != ActionKind::Destructive) {
            draw(ui, i, a, &mut clicked);
        }
    });
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        for (i, a) in actions.iter().enumerate().filter(|(_, a)| a.kind == ActionKind::Destructive) {
            draw(ui, i, a, &mut clicked);
        }
    });
    clicked
}

//! #3897 (U6 of #3663): the Holders page of the tool library window - the
//! built-in holder library (`holder_library::library`) grouped by spindle
//! taper, the taper of the mounted machine first and marked. A row gives
//! the name, the taper, the clamp, the clamping range, the gauge length,
//! the largest diameter and the number of document tools that use the
//! holder (`app::tool_body::holder_users`, keyed on the entry name). A
//! click on a name opens the read-only holder view (`holder_view`).
//! Built-in holders are read-only: user holders and holder edits need the
//! user library store of #3645 (U7, #3898).
//! Frame cost: the library rows are built once for each thread; a frame
//! reads the document's holder references and formats the rows.
//! Provenance: our own list over our own holder library; no HSMWorks
//! material involved.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::rc::Rc;

use egui::{RichText, Ui};

use camcraft_app::holder_library::{self, Clamp, HolderEntry};
use camcraft_app::tool_body;
use camcraft_app::units::{self, Unit};

use crate::state::State;

/// The label of the row that shows every taper.
pub(crate) const ALL: &str = "All holders";
/// The name of the group of the straight-shank chucks (no taper).
pub(crate) const STRAIGHT: &str = "Straight shank";
/// The note that says what the page cannot do yet.
pub(crate) const READ_ONLY: &str =
    "Built-in holders are read-only. User holders and holder edits come with the user library (#3645).";
/// The column heads.
pub(crate) const HEADS: [&str; 7] = ["Name", "Taper", "Clamp", "Clamps Ø", "Gauge length", "Largest Ø", "Used by"];
const TAPER_W: f32 = 170.0;

/// One library holder as the list shows it.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub entry: HolderEntry,
    pub name: String,
    /// The taper's name, or "" for a straight-shank chuck.
    pub taper: &'static str,
}

thread_local! {
    static ROWS: OnceCell<Rc<Vec<Row>>> = const { OnceCell::new() };
}

/// The library's rows, in the library's order (built once).
pub(crate) fn rows() -> Rc<Vec<Row>> {
    ROWS.with(|r| {
        Rc::clone(r.get_or_init(|| {
            Rc::new(
                holder_library::library()
                    .into_iter()
                    .map(|e| Row { name: e.name(), taper: e.taper.map_or("", |t| t.name), entry: e })
                    .collect(),
            )
        }))
    })
}

/// The group name of a taper ("" is the straight-shank group).
pub(crate) fn group_name(taper: &str) -> &str {
    if taper.is_empty() { STRAIGHT } else { taper }
}

/// The taper the mounted machine's spindle takes, if the machine states one.
pub(crate) fn machine_taper(state: &State) -> Option<String> {
    camcraft_app::machine::document_of(&state.world).1.and_then(|m| m.spindle_mount)
}

/// The tapers in the list's order: the machine's first, then the library's.
pub(crate) fn tapers(rows: &[Row], machine: Option<&str>) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for r in rows {
        if !out.contains(&r.taper) {
            out.push(r.taper);
        }
    }
    if let Some(m) = machine
        && let Some(i) = out.iter().position(|t| t.eq_ignore_ascii_case(m))
    {
        let t = out.remove(i);
        out.insert(0, t);
    }
    out
}

/// The document tools that use each holder, by `tool_body::holder_key`.
pub(crate) fn users(state: &State) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for (tool, r) in tool_body::holder_users(&state.world) {
        out.entry(tool_body::holder_key(&r.entry)).or_default().push(tool);
    }
    out
}

/// The clamp as words: "ER32 collet", "Weldon side lock".
pub(crate) fn clamp_text(e: &HolderEntry) -> String {
    match e.clamp {
        Clamp::Er(er) => format!("{} collet", er.name),
        Clamp::Weldon { .. } => "Weldon side lock".to_owned(),
    }
}

/// The shank diameters the holder takes.
pub(crate) fn range_text(e: &HolderEntry, u: Unit) -> String {
    match e.clamp {
        Clamp::Er(er) => format!("{} – {}", units::len(er.min_bore, u, 2), units::len(er.max_bore, u, 2)),
        Clamp::Weldon { bore, .. } => units::len(bore, u, 3),
    }
}

/// The largest diameter of the holder: the flange's or the nose's.
pub(crate) fn largest_d(e: &HolderEntry) -> f64 {
    e.taper.map_or(0.0, |t| t.flange_d).max(e.body_r() * 2.0)
}

pub(super) fn draw(ui: &mut Ui, state: &mut State) {
    let rows = rows();
    let machine = machine_taper(state);
    let users = users(state);
    let Some(view) = state.tool_library.as_ref().map(|v| v.holders.clone()) else {
        return;
    };
    let mut pick_taper = None;
    let mut open = None;
    let mut filter = view.filter.clone();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(TAPER_W);
            pick_taper = taper_column(ui, &rows, machine.as_deref(), view.taper.as_deref());
        });
        ui.separator();
        ui.vertical(|ui| {
            let label = ui.label("Holder filter");
            ui.add(egui::TextEdit::singleline(&mut filter).desired_width(ui.available_width())).labelled_by(label.id);
            ui.weak(READ_ONLY);
            egui::ScrollArea::both().id_salt("tool_library_holders").auto_shrink([false, false]).show(ui, |ui| {
                open = table(ui, &rows, &users, machine.as_deref(), &view.taper, &filter, state.units());
            });
        });
    });
    if let Some(v) = state.tool_library.as_mut() {
        v.holders.filter = filter;
        if let Some(t) = pick_taper {
            v.holders.taper = t;
        }
    }
    if let Some(name) = open {
        state.open_holder_in_library(Some(&name));
    }
}

/// "All holders", then one row for each taper with its count, then the
/// machine's taper. Gives the new taper pick when the user clicks a row.
fn taper_column(ui: &mut Ui, rows: &[Row], machine: Option<&str>, picked: Option<&str>) -> Option<Option<String>> {
    let mut out = None;
    ui.label(RichText::new("SPINDLE TAPER").small().weak());
    let item = |ui: &mut Ui, sel: bool, text: &str, n: usize| {
        ui.horizontal(|ui| {
            let c = ui.selectable_label(sel, text).clicked();
            ui.weak(n.to_string());
            c
        })
        .inner
    };
    if item(ui, picked.is_none(), ALL, rows.len()) {
        out = Some(None);
    }
    for t in tapers(rows, machine) {
        let n = rows.iter().filter(|r| r.taper == t).count();
        let mark = machine.is_some_and(|m| m.eq_ignore_ascii_case(t));
        let text = if mark { format!("{} (machine)", group_name(t)) } else { group_name(t).to_owned() };
        if item(ui, picked == Some(t), &text, n) {
            out = Some(Some(t.to_owned()));
        }
    }
    ui.add_space(8.0);
    ui.label(RichText::new("MACHINE").small().weak());
    ui.label(match machine {
        Some(m) => format!("The mounted machine takes {m}. Its holders come first."),
        None => "The mounted machine states no spindle taper.".to_owned(),
    });
    out
}

/// The holder rows, grouped by taper; gives the name the user opened.
fn table(
    ui: &mut Ui,
    rows: &[Row],
    users: &HashMap<String, Vec<String>>,
    machine: Option<&str>,
    taper: &Option<String>,
    filter: &str,
    u: Unit,
) -> Option<String> {
    let f = filter.to_lowercase();
    let mut open = None;
    egui::Grid::new("tool_library_holder_rows").striped(true).show(ui, |ui| {
        for h in HEADS {
            ui.label(RichText::new(h).strong());
        }
        ui.end_row();
        for t in tapers(rows, machine) {
            if taper.as_deref().is_some_and(|p| p != t) {
                continue;
            }
            let shown: Vec<&Row> = rows.iter().filter(|r| r.taper == t && r.name.to_lowercase().contains(&f)).collect();
            if shown.is_empty() {
                continue;
            }
            let mark = if machine.is_some_and(|m| m.eq_ignore_ascii_case(t)) { "  (the machine's taper)" } else { "" };
            ui.label(RichText::new(format!("{}{mark}", group_name(t))).small().weak());
            ui.end_row();
            for r in shown {
                if ui.selectable_label(false, &r.name).on_hover_text("open this holder").clicked() {
                    open = Some(r.name.clone());
                }
                ui.label(group_name(r.taper));
                ui.label(clamp_text(&r.entry));
                ui.label(range_text(&r.entry, u));
                ui.label(units::len(r.entry.gauge_len, u, 2));
                ui.label(units::len(largest_d(&r.entry), u, 2));
                let n = users.get(&tool_body::holder_key(&r.name)).map_or(0, Vec::len);
                ui.label(if n == 1 { "1 tool".to_owned() } else { format!("{n} tools") });
                ui.end_row();
            }
        }
    });
    open
}

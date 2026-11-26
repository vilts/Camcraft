//! #3897 (U6 of #3663): the read-only holder view of the tool library
//! window, as the holder picture of the #3663 mock-up: the machine side
//! (the spindle taper, marked when it is the mounted machine's), the tool
//! side (the clamp and its clamping range), the shape (gauge length,
//! largest and nose diameter), the document tools that use the holder
//! (a click opens the tool in the editor), and the outline to scale with
//! its dimension lines. Nothing here is editable: a built-in holder is
//! fixed, and holder edits wait for the user library of #3645 (U7).
//! Frame cost (`crates/ui/AGENTS.md`): the outline's shapes and labels
//! are built when the holder, the drawing's rect, the unit or the
//! dimension toggle change; a still frame builds nothing (`builds`).
//! Provenance: our own view over `holder_library::HolderEntry::points`;
//! the dimension style follows `side_view` (general drawing practice).
//! No HSMWorks material involved.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, Vec2};

use camcraft_app::holder_library::HolderEntry;
use camcraft_app::tool_body;
use camcraft_app::units::{self, Unit};

use super::holders::{self, READ_ONLY};
use crate::egui_app::tool_model_view::BG;
use crate::state::State;

/// The button back to the holder list.
pub(crate) const BACK: &str = "Holder list";
/// The heading of the tools that use the holder.
pub(crate) const USED_BY: &str = "Used by";
/// The drawing's dimension toggle.
pub(crate) const DIMS: &str = "Dimensions";
const FACTS_W: f32 = 380.0;
const BODY: Color32 = Color32::from_rgb(86, 92, 104);
const EDGE: Color32 = Color32::from_rgb(214, 220, 228);
const DIM: Color32 = Color32::from_rgb(0x7F, 0xB4, 0xD8);
const NOTE: Color32 = Color32::from_rgb(0x9E, 0xA8, 0xB8);

/// What the user asked for this frame.
enum Ask {
    List,
    Tool(String),
}

pub(super) fn draw(ui: &mut Ui, state: &mut State) {
    let Some(view) = state.tool_library.as_ref().map(|v| v.holders.clone()) else {
        return;
    };
    let rows = holders::rows();
    let Some(row) = view.open.as_deref().and_then(|n| rows.iter().find(|r| r.name == n)) else {
        // A name the library does not hold: back to the list.
        state.open_holder_in_library(None);
        return;
    };
    let e = row.entry;
    let u = state.units();
    let machine = holders::machine_taper(state);
    let users = tool_body::tools_using_holder(&state.world, &row.name);
    let mut ask = None;
    let mut hide = view.hide_dims;
    ui.horizontal(|ui| {
        if ui.button(BACK).on_hover_text("back to the holders by taper").clicked() {
            ask = Some(Ask::List);
        }
        ui.heading(row.name.as_str());
        ui.weak(format!("holder · used by {}", count(users.len())));
    });
    ui.weak(READ_ONLY);
    ui.separator();
    // Bounded by the screen: a column as tall as the room left would let
    // the window grow each frame.
    let h = ui.available_height().min(ui.ctx().viewport_rect().height() * 0.7 - 140.0).max(160.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(FACTS_W);
            egui::ScrollArea::vertical().id_salt("tool_library_holder_facts").max_height(h).show(ui, |ui| {
                facts(ui, &e, machine.as_deref(), u);
                if let Some(t) = used_by(ui, state, &users) {
                    ask = Some(Ask::Tool(t));
                }
            });
        });
        ui.separator();
        ui.vertical(|ui| {
            let mut show = !hide;
            ui.horizontal(|ui| {
                ui.checkbox(&mut show, DIMS).on_hover_text("show the dimension lines of the drawing");
                ui.weak(format!("to scale · {}", u.suffix()));
            });
            hide = !show;
            let size = Vec2::new(ui.available_width().max(160.0), (h - 30.0).max(160.0));
            outline(ui, &row.name, &e, size, u, show);
        });
    });
    if let Some(v) = state.tool_library.as_mut() {
        v.holders.hide_dims = hide;
    }
    match ask {
        Some(Ask::List) => state.open_holder_in_library(None),
        Some(Ask::Tool(name)) => state.open_tool_in_library(&name),
        None => {}
    }
}

fn count(n: usize) -> String {
    if n == 1 { "1 tool".to_owned() } else { format!("{n} tools") }
}

/// The machine side, the tool side and the shape, as label rows.
fn facts(ui: &mut Ui, e: &HolderEntry, machine: Option<&str>, u: Unit) {
    let l = |v: f64| format!("{} {}", units::len(v, u, 2), u.suffix());
    let taper = e.taper.map_or(holders::STRAIGHT, |t| t.name);
    let taper_note = match (e.taper, machine) {
        (Some(t), Some(m)) if m.eq_ignore_ascii_case(t.name) => "the mounted machine's taper".to_owned(),
        (_, Some(m)) => format!("the mounted machine takes {m}"),
        (_, None) => "the machine states no taper".to_owned(),
    };
    let nose = e.body_r() * 2.0;
    let sections: [(&str, Vec<(&str, String)>); 3] = [
        ("MACHINE SIDE", vec![("Spindle taper", format!("{taper} ({taper_note})"))]),
        ("TOOL SIDE", vec![("Clamp", holders::clamp_text(e)), ("Clamping range", format!("Ø {} {}", holders::range_text(e, u), u.suffix()))]),
        (
            "SHAPE",
            vec![("Gauge length", l(e.gauge_len)), ("Largest diameter", l(holders::largest_d(e))), ("Nose diameter", l(nose))],
        ),
    ];
    for (head, rows) in sections {
        ui.add_space(4.0);
        ui.label(RichText::new(head).small().weak());
        egui::Grid::new(("holder_facts", head)).num_columns(2).show(ui, |ui| {
            for (k, v) in rows {
                ui.label(k);
                ui.label(v);
                ui.end_row();
            }
        });
    }
}

/// The tools that use the holder: a link for each, with its stick-out and
/// gauge length. Gives the tool the user opened.
fn used_by(ui: &mut Ui, state: &State, users: &[String]) -> Option<String> {
    ui.add_space(6.0);
    ui.label(RichText::new(format!("{USED_BY} {}", count(users.len()))).small().weak());
    if users.is_empty() {
        ui.weak("No tool of this document uses this holder.");
        return None;
    }
    let u = state.units();
    let refs = tool_body::holder_users(&state.world);
    let mut out = None;
    for name in users {
        ui.horizontal(|ui| {
            if ui.link(name.as_str()).on_hover_text("edit this tool in this window").clicked() {
                out = Some(name.clone());
            }
            if let Some((_, r)) = refs.iter().find(|(n, _)| n == name) {
                let l = |v: f64| units::len(v, u, 2);
                ui.weak(format!("stick-out {}, gauge {} {}", l(r.projection), l(r.gauge_len + r.projection), u.suffix()));
            }
        });
    }
    out
}

/// What the outline was built from.
#[derive(Clone, PartialEq)]
struct Key {
    name: String,
    rect: Rect,
    unit: Unit,
    dims: bool,
}

/// The kept drawing: its shapes and its labels.
struct Drawing {
    shapes: Vec<Shape>,
    labels: Vec<(Pos2, Align2, String)>,
}

thread_local! {
    static KEPT: RefCell<Option<(Key, Rc<Drawing>)>> = const { RefCell::new(None) };
    static BUILDS: Cell<u64> = const { Cell::new(0) };
}

/// The outline drawings built on this thread - a still frame adds none.
#[cfg(test)]
pub(crate) fn builds() -> u64 {
    BUILDS.with(Cell::get)
}

/// The holder to scale in a `size` area, nose down, gauge line up.
fn outline(ui: &mut Ui, name: &str, e: &HolderEntry, size: Vec2, u: Unit, dims: bool) {
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    painter.rect_filled(resp.rect, 4.0, BG);
    let key = Key { name: name.to_owned(), rect: resp.rect, unit: u, dims };
    let d = KEPT.with(|k| {
        let mut k = k.borrow_mut();
        if let Some((kk, d)) = k.as_ref()
            && *kk == key
        {
            return Rc::clone(d);
        }
        BUILDS.with(|b| b.set(b.get().saturating_add(1)));
        let d = Rc::new(build(e, resp.rect, u, dims));
        *k = Some((key, Rc::clone(&d)));
        d
    });
    painter.extend(d.shapes.iter().cloned());
    for (at, align, text) in &d.labels {
        painter.text(*at, *align, text, FontId::proportional(11.0), NOTE);
    }
}

fn build(e: &HolderEntry, rect: Rect, u: Unit, dims: bool) -> Drawing {
    let mut d = Drawing { shapes: Vec::new(), labels: Vec::new() };
    let Ok(points) = e.points(0.0, e.gauge_len) else {
        d.labels.push((rect.center(), Align2::CENTER_CENTER, "no outline".to_owned()));
        return d;
    };
    let pts: Vec<(f64, f64)> = points.iter().map(|p| (p.z, p.r)).filter(|(z, r)| z.is_finite() && r.is_finite()).collect();
    let r_max = pts.iter().map(|p| p.1).fold(0.0f64, f64::max);
    let z_max = pts.iter().map(|p| p.0).fold(0.0f64, f64::max);
    if r_max <= 0.0 || z_max <= 0.0 {
        return d;
    }
    let inner = rect.shrink2(Vec2::new(70.0, 34.0));
    let scale = (inner.width() / (2.0 * r_max as f32)).min(inner.height() / z_max as f32).max(f32::EPSILON);
    let (axis, base) = (inner.center().x, inner.bottom());
    let at = |z: f64, r: f64| Pos2::new(axis + r as f32 * scale, base - z as f32 * scale);
    d.shapes.push(Shape::line_segment([Pos2::new(axis, rect.top() + 4.0), Pos2::new(axis, rect.bottom() - 4.0)], Stroke::new(1.0, Color32::from_rgb(70, 74, 84))));
    for w in pts.windows(2) {
        let ((z0, r0), (z1, r1)) = (w[0], w[1]);
        if z1 > z0 {
            d.shapes.push(Shape::convex_polygon(vec![at(z0, -r0), at(z0, r0), at(z1, r1), at(z1, -r1)], BODY, Stroke::NONE));
        }
    }
    let mut edge: Vec<Pos2> = pts.iter().map(|&(z, r)| at(z, r)).collect();
    edge.extend(pts.iter().filter(|p| p.1 > 0.0).rev().map(|&(z, r)| at(z, -r)));
    d.shapes.push(Shape::closed_line(edge, Stroke::new(1.0, EDGE)));
    if dims {
        let len = |v: f64| units::len(v, u, 2);
        let stroke = Stroke::new(1.0, DIM);
        // The gauge length on the right, the largest diameter over the top,
        // the nose diameter under the nose.
        let x = at(0.0, r_max).x + 14.0;
        d.shapes.push(Shape::line_segment([Pos2::new(x, at(0.0, 0.0).y), Pos2::new(x, at(z_max, 0.0).y)], stroke));
        d.labels.push((Pos2::new(x + 4.0, at(z_max * 0.5, 0.0).y), Align2::LEFT_CENTER, format!("gauge length {}", len(e.gauge_len))));
        let y = at(z_max, 0.0).y - 10.0;
        d.shapes.push(Shape::line_segment([Pos2::new(at(0.0, -r_max).x, y), Pos2::new(at(0.0, r_max).x, y)], stroke));
        d.labels.push((Pos2::new(axis, y - 2.0), Align2::CENTER_BOTTOM, format!("Ø {}", len(holders::largest_d(e)))));
        let rn = e.body_r();
        let y = base + 10.0;
        d.shapes.push(Shape::line_segment([Pos2::new(at(0.0, -rn).x, y), Pos2::new(at(0.0, rn).x, y)], stroke));
        d.labels.push((Pos2::new(axis, y + 2.0), Align2::CENTER_TOP, format!("nose Ø {}", len(rn * 2.0))));
    }
    d
}

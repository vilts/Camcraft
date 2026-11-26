//! #3897 (U6 of #3663): the small drawing at the start of each row of the
//! tool list, as in the #3663 mock-up: the cutter and the shank to scale
//! in a fixed cell, the cutting band lighter than the shank. Display only.
//! Frame cost (`crates/ui/AGENTS.md`): the shapes of a drawing are built
//! when the tool value changes and kept by the tool's name; a still frame
//! only moves the kept shapes to the row (`builds` is the counter).
//! Provenance: our own drawing over `Tool::profile`, as the tool page's
//! side drawing (`tool_preview::paint`); no HSMWorks material involved.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use egui::{Color32, Pos2, Sense, Shape, Stroke, Ui, Vec2};

use camcraft_app::tools::Tool;

/// The cell of one drawing, logical px.
pub(crate) const CELL: Vec2 = Vec2::new(14.0, 30.0);
/// More kept drawings than this clears the store (a long catalog scroll).
const KEEP: usize = 512;

const CUT: Color32 = Color32::from_rgb(200, 160, 80);
const SHANK: Color32 = Color32::from_rgb(122, 130, 142);

/// One kept drawing: the tool it was built from and its shapes, placed
/// in a cell whose top-left corner is the origin.
struct Kept {
    tool: Tool,
    shapes: Rc<Vec<Shape>>,
}

thread_local! {
    static KEPT: RefCell<HashMap<String, Kept>> = RefCell::new(HashMap::new());
    static BUILDS: Cell<u64> = const { Cell::new(0) };
}

/// The drawings built on this thread - a still frame adds none.
#[cfg(test)]
pub(crate) fn builds() -> u64 {
    BUILDS.with(Cell::get)
}

/// The drawing of `tool` (row `name`) in a new cell of the row.
pub(super) fn draw(ui: &mut Ui, name: &str, tool: &Tool) {
    let (rect, _) = ui.allocate_exact_size(CELL, Sense::hover());
    let shapes = shapes_of(name, tool);
    let to = rect.min.to_vec2();
    ui.painter().extend(shapes.iter().cloned().map(|mut s| {
        s.translate(to);
        s
    }));
}

/// The kept shapes of `name`, built again only when its tool changed.
fn shapes_of(name: &str, tool: &Tool) -> Rc<Vec<Shape>> {
    KEPT.with(|k| {
        let mut k = k.borrow_mut();
        if let Some(kept) = k.get(name)
            && kept.tool == *tool
        {
            return Rc::clone(&kept.shapes);
        }
        BUILDS.with(|b| b.set(b.get().saturating_add(1)));
        if k.len() >= KEEP {
            k.clear();
        }
        let shapes = Rc::new(build(tool));
        k.insert(name.to_owned(), Kept { tool: tool.clone(), shapes: Rc::clone(&shapes) });
        shapes
    })
}

/// The cutter and the shank (no holder), tip down, to scale in the cell.
/// A tool without a size draws one thin line.
fn build(t: &Tool) -> Vec<Shape> {
    let outline = Tool { holder_len: 0.0, ..t.clone() }.profile();
    let ok = |&(z, r): &(f64, f64)| z.is_finite() && r.is_finite();
    let r_max = outline.iter().filter(|q| ok(q)).map(|q| q.1).fold(0.0f64, f64::max);
    let z_max = outline.iter().filter(|q| ok(q)).map(|q| q.0).fold(0.0f64, f64::max);
    let axis = CELL.x * 0.5;
    if r_max <= 0.0 || z_max <= 0.0 {
        return vec![Shape::line_segment([Pos2::new(axis, 2.0), Pos2::new(axis, CELL.y - 2.0)], Stroke::new(1.0, SHANK))];
    }
    let scale = ((CELL.x - 2.0) / (2.0 * r_max as f32)).min((CELL.y - 2.0) / z_max as f32).max(f32::EPSILON);
    let base = CELL.y - 1.0;
    let at = |z: f64, r: f64| Pos2::new(axis + r as f32 * scale, base - z as f32 * scale);
    let flute = t.flute_len.max(0.0);
    let mut out = Vec::new();
    // One convex slice for each outline pair, as the tool page's drawing.
    for w in outline.windows(2) {
        let ((z0, r0), (z1, r1)) = (w[0], w[1]);
        if !(ok(&w[0]) && ok(&w[1])) || z1 <= z0 {
            continue;
        }
        let fill = if (z0 + z1) * 0.5 < flute { CUT } else { SHANK };
        out.push(Shape::convex_polygon(vec![at(z0, -r0), at(z0, r0), at(z1, r1), at(z1, -r1)], fill, Stroke::NONE));
    }
    out
}

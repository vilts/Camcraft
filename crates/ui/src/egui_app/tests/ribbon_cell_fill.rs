//! #2752: a ribbon cell's fill holds its whole label in each theme, for a
//! toggled cell, a hovered cell and a toggled cell under the pointer. The
//! label ink follows the fill, as the glyph ink does. The box never enters
//! the group caption row. Structural checks of the painted shapes.
//! Provenance: our own tests over our own ribbon cell.

use egui::{Color32, Pos2, Rect, Shape, Vec2};
use egui_kittest::Harness;

use super::ribbon_rows::{each_theme, line_h};
use crate::egui_app::ribbon_cell::cell_box;
use crate::egui_app::{contrast, painted};
use crate::egui_app::theme::Tokens;

const CELL_W: f32 = 56.0;

struct Painted {
    cell: Rect,
    fill: Option<Rect>,
    label: Rect,
    ink: Color32,
}

fn flat(shape: &Shape, out: &mut Vec<Shape>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| flat(s, out)),
        other => out.push(other.clone()),
    }
}

fn paint(pal: Tokens, toggled: bool, hover: bool) -> Painted {
    let cell = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(CELL_W, pal.cell_h));
    let mut h = Harness::builder().with_size(Vec2::new(120.0, 120.0)).build_ui(move |ui| {
        let _ = painted::cell(ui, egui::Id::new("cell_2752"), cell, "Paths", &pal, true, toggled, None);
    });
    h.run_steps(2);
    if hover {
        h.hover_at(Pos2::new(cell.center().x, cell.top() + 4.0));
        h.run_steps(2);
    }
    let mut shapes = Vec::new();
    for c in &h.output().shapes {
        flat(&c.shape, &mut shapes);
    }
    let fill = shapes.iter().find_map(|s| match s {
        Shape::Rect(r) if (r.rect.width() - CELL_W).abs() < 0.01 => Some(r.rect),
        _ => None,
    });
    let (label, ink) = shapes
        .iter()
        .find_map(|s| match s {
            Shape::Text(t) if t.galley.text() == "Paths" => {
                Some((Rect::from_min_size(t.pos, t.galley.size()), t.fallback_color))
            }
            _ => None,
        })
        .expect("the cell paints its label");
    Painted { cell, fill, label, ink }
}

fn each_table() -> Vec<(&'static str, Tokens)> {
    let mut all = vec![("DARK", Tokens::DARK), ("LIGHT", Tokens::LIGHT)];
    all.extend(each_theme());
    all
}

#[test]
fn a_filled_cell_holds_its_whole_label_in_each_theme() {
    for (name, pal) in each_table() {
        for (toggled, hover) in [(true, false), (true, true), (false, true)] {
            let p = paint(pal, toggled, hover);
            let fill = p.fill.unwrap_or_else(|| panic!("{name} toggled={toggled} hover={hover}: no fill"));
            assert!((fill.top() - p.cell.top()).abs() < 0.01, "{name}: the fill starts at the cell top");
            assert!(
                fill.bottom() >= p.label.bottom() - 0.01,
                "{name} toggled={toggled} hover={hover}: fill ends at {} but the label at {}",
                fill.bottom(),
                p.label.bottom()
            );
            assert!(fill.left() <= p.label.left() && fill.right() >= p.label.right(), "{name}: label is wider than the fill");
        }
    }
}

#[test]
fn a_filled_cell_writes_its_label_in_the_on_fill_ink() {
    for (name, pal) in each_table() {
        let on = paint(pal, true, false);
        assert_eq!(on.ink, contrast::on_fill(pal.selection_bg), "{name}: toggled label ink");
        let over = paint(pal, true, true);
        assert_eq!(over.ink, on.ink, "{name}: hover does not change a toggled cell's label ink");
        let plain = paint(pal, false, false);
        assert!(plain.fill.is_none(), "{name}: a plain cell paints no fill");
        assert_eq!(plain.ink, pal.text, "{name}: a plain label keeps the text ink");
    }
}

#[test]
fn the_cell_box_stops_clear_of_the_caption_row() {
    for (name, pal) in each_table() {
        let cell = Rect::from_min_size(Pos2::ZERO, Vec2::new(CELL_W, pal.cell_h));
        let label_bottom = pal.glyph_px + 2.0 + line_h(pal.font_label);
        let bx = cell_box(cell, label_bottom, &pal);
        assert!(bx.bottom() >= cell.bottom(), "{name}: the box is never shorter than the cell");
        assert!(bx.bottom() >= label_bottom.min(cell.bottom() + pal.ribbon_caption_gap - 2.0), "{name}: label cut");
        let huge = cell_box(cell, 1000.0, &pal);
        assert!(huge.bottom() <= cell.bottom() + pal.ribbon_caption_gap - 2.0 + 0.01, "{name}: box enters the caption row");
    }
}

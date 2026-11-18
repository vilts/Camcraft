//! #661 section 4 wiring: every Tool library catalog row and every tool
//! picker item wears its kind's glyph. The rows and items draw through
//! `catalog_row_icon` and `picker_item_icon`, the pure choices pinned here,
//! so the test reads the same mapping the painted pixels come from.
//! Provenance: our own test; no HSMWorks or SolidCraft material involved.

use super::op_icon_table::every_tool_kind;
use super::*;
use crate::egui_app::icons::Icon;
use crate::egui_app::tool_icons::{catalog_row_icon, picker_item_icon, tool_icon};
use camcraft_app::tools::{self, Tool};

/// A catalog tool of each kind in the editor's kind numbering: the shipped
/// catalog's first tool with its kind swapped, so every field but the kind
/// is a real, valid tool.
fn tool_of_kind(kind: camcraft_app::tools::ToolKind) -> Tool {
    let (_, mut t) = tools::catalog().into_iter().next().expect("the shipped catalog is not empty");
    t.kind = kind;
    t
}

/// Every Tool library catalog row wears the glyph of its own kind, and the
/// shipped catalog's kinds are all in the kind list (no row falls outside it).
#[test]
fn every_catalog_row_wears_its_kind_glyph() {
    let kinds = every_tool_kind();
    for (name, ct) in tools::catalog() {
        assert!(kinds.contains(&ct.kind), "catalog row {name} has a kind the editor does not number");
        assert_eq!(catalog_row_icon(&ct), tool_icon(ct.kind), "catalog row {name} wears the wrong glyph");
    }
    for k in kinds {
        let row = tool_of_kind(k);
        let icon: Icon = catalog_row_icon(&row);
        assert_eq!(icon, tool_icon(k), "a catalog row of {k:?} wears {icon:?}");
    }
}

/// Every picker item of a document tool wears the glyph of that tool's kind,
/// driven from the kind list; an item naming no tool wears nothing.
#[test]
fn every_picker_item_wears_its_kind_glyph() {
    for k in every_tool_kind() {
        let tool = tool_of_kind(k);
        assert_eq!(picker_item_icon(Some(&tool)), Some(tool_icon(k)), "a picker item of {k:?} wears the wrong glyph");
    }
    assert_eq!(picker_item_icon(None), None, "a name with no tool must not wear a glyph");
}

/// Every texture id painted in the last frame. A widget outside its
/// clip rect paints nothing, so a row is only read once scrolled into view.
fn painted_textures(h: &Harness<'static, ShellUi>) -> Vec<egui::TextureId> {
    fn walk(s: &egui::Shape, out: &mut Vec<egui::TextureId>) {
        match s {
            egui::Shape::Mesh(m) => out.push(m.texture_id),
            egui::Shape::Rect(r) if r.fill_texture_id() != egui::TextureId::default() => out.push(r.fill_texture_id()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for c in &h.output().shapes {
        walk(&c.shape, &mut out);
    }
    out
}

/// The texture id and accessible name of a kind's glyph in the icon set.
fn kind_icon(h: &Harness<'static, ShellUi>, k: camcraft_app::tools::ToolKind) -> (egui::TextureId, String) {
    let set = IconSet::from_ctx(&h.ctx).expect("the icon set is up after the first frame");
    let tex = set.texture(tool_icon(k)).expect("the kind's glyph decoded");
    (tex.id(), tex.name())
}

/// Whether `row`'s horizontal container carries an `Image` labelled for `want`
/// and the frame painted that texture id.
fn glyph_in_row(
    h: &Harness<'static, ShellUi>,
    row: &egui_kittest::Node<'_>,
    (want_tex, want_alt): (egui::TextureId, String),
) -> bool {
    use egui_kittest::kittest::NodeT;
    let has_image = row.parent().is_some_and(|p| {
        p.children().any(|n| {
            let a = n.accesskit_node();
            a.role() == egui::accesskit::Role::Image && a.label().as_deref() == Some(want_alt.as_str())
        })
    });
    has_image && painted_textures(h).contains(&want_tex)
}

/// The painted twin of the pure test: each catalog row renders its kind's
/// glyph inside the row's container (scrolled into view first - the
/// catalog sits in a clipped scroll area). Deleting the `glyph(...)` call
/// from the row fails this.
#[test]
fn every_catalog_row_paints_its_kind_glyph() {
    let mut h = shell_harness();
    h.run();
    crate::actions::target(&mut h.state_mut().state, UiTarget::Tool(String::new()));
    h.run();
    h.run();
    for (name, ct) in tools::catalog() {
        let label = format!("{name}  \u{d8}");
        if !crate::machine_modes::tool_kind_offered(&h.state().state.world, ct.kind) {
            assert!(
                h.query_by_label_contains(&label).is_none(),
                "unoffered catalog entry {name} ({:?}) stays hidden",
                ct.kind
            );
            continue;
        }
        h.get_by_label_contains(&label).scroll_to_me();
        h.run();
        h.run();
        let row = h.get_by_label_contains(&label);
        assert!(glyph_in_row(&h, &row, kind_icon(&h, ct.kind)), "catalog row {name} paints no {:?} glyph", ct.kind);
    }
}

/// The picker's dropdown: with the whole catalog on the rack, the opened
/// list renders each document tool's kind glyph inside its item container
/// (items below the popup's fold are scrolled into view first), and every item
/// keeps its plain accessible name.
#[test]
fn every_picker_item_paints_its_kind_glyph() {
    let mut h = shell_harness();
    h.run();
    let names: Vec<&str> = tools::catalog().iter().map(|(n, _)| *n).collect();
    rack(&mut h.state_mut().state, &names);
    crate::actions::target(&mut h.state_mut().state, UiTarget::Operation("Contour1".into()));
    h.run();
    h.run();
    let current = h.state().state.op_ui.tool.clone();
    h.get_by(|n| n.role() == egui::accesskit::Role::ComboBox && n.value().as_deref() == Some(current.as_str())).click();
    h.run();
    h.run();
    for (name, ct) in tools::catalog() {
        let items: Vec<_> = h.query_all_by_label(name).collect();
        let Some(item) = items.last() else { panic!("picker item {name} is not listed") };
        item.scroll_to_me();
        h.run();
        h.run();
        let row = h.query_all_by_label(name).last().expect("item stays listed");
        assert!(glyph_in_row(&h, &row, kind_icon(&h, ct.kind)), "picker item {name} paints no {:?} glyph", ct.kind);
    }
}

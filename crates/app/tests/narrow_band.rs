//! #1655: a pocket-family op that clears the stock around the part (a
//! whole-model source, #12, or the stock source, #1554) names each band
//! of that area narrower than the tool. Before the fix, a 3 mm stock
//! margin beside a 10 mm tool got no pass while the pocket in the part
//! was cut, and the op came back with no note: the outside of the part
//! stayed uncut and nothing said so.
//!
//! Provenance: our own fixtures (a block with a blind square pocket in a
//! box stock); no vendor material consulted.
//!
//! Gate map: covers app::operation::keep_law (narrow_noted),
//! app::operation::generate (the #1655 note), motion::machining2d
//! (unreached_note).

mod area_support;
mod pocket_support;

use area_support::*;
use pocket_support::{block_with_pocket, rounded_rect};

/// A `2a x 2a x [0, 10]` block with a 30 x 30 pocket to Z 2, in the
/// 70 x 70 x 10 stock: the stock margin is `35 - a` on each side.
fn world(a: f64) -> World {
    let ring = rounded_rect(15.0, 15.0, 0.0, 1);
    world_with(Some(ImportModel::from_mesh("part", block_with_pocket(a, a, &ring, 2.0, 10.0))), 0.0, 10.0)
}

fn adaptive() -> OpParams {
    let mut p = operation::retype_params(&pocket(10.0, 2.0), OpType::Adaptive2D as usize);
    if let OpParams::Adaptive2D { top_z, bottom_z, stepdown, stepover, .. } = &mut p {
        *top_z = Height::fixed(10.0);
        *bottom_z = Height::fixed(2.0);
        *stepdown = 4.0;
        *stepover = 4.0;
    }
    assert_eq!(p.kind(), OpType::Adaptive2D);
    p
}

fn band_notes(t: &Toolpath) -> Vec<&String> {
    t.notes.iter().filter(|n| n.contains("narrower than the") && n.contains("no pass cuts")).collect()
}

#[test]
fn a_model_source_adaptive_names_the_outside_band_narrower_than_the_tool() {
    // 3 mm margin, 10 mm tool: the band erodes to nothing, the pocket
    // does not. The op still cuts the pocket - and now says the band.
    let w = world(32.0);
    for (label, params) in [("2D Adaptive", adaptive()), ("2D Pocket", pocket(10.0, 2.0))] {
        for src in [SrcRef::Entity("part".into()), SrcRef::Stock] {
            let (t, _) = run(&w, &op(src.clone(), params.clone(), AreaControls::default()))
                .unwrap_or_else(|e| panic!("{label} {src:?}: {e}"));
            let pts = feeds(&t);
            assert!(!pts.is_empty(), "{label} {src:?}: the pocket is cut");
            assert!(pts.iter().all(|p| p.x.abs() < 15.0 + EPS && p.y.abs() < 15.0 + EPS), "{label} {src:?}: no pass in the band");
            let notes = band_notes(&t);
            assert_eq!(notes.len(), 1, "{label} {src:?}: {:?}", t.notes);
            let n = notes[0];
            assert!(n.starts_with(label), "{n}");
            assert!(n.contains("1 band") && n.contains("10.000 mm tool"), "{n}");
            assert!(n.contains("X -35.000 mm to 35.000 mm, Y -35.000 mm to 35.000 mm"), "{n}");
        }
    }
}

#[test]
fn a_band_the_tool_fits_is_cut_and_says_nothing() {
    // 11 mm margin, 10 mm tool: the band erodes from both sides by the
    // 5 mm radius and keeps a 1 mm wide center area.
    let w = world(24.0);
    let (t, _) = run(&w, &op(SrcRef::Entity("part".into()), adaptive(), AreaControls::default())).expect("generate");
    assert!(band_notes(&t).is_empty(), "{:?}", t.notes);
    assert!(feeds(&t).iter().any(|p| p.x.abs() > 24.0), "the band is cut");
}

fn plate_band() -> (World, Operation) {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, ImportModel::from_mesh("part", cube(
        Point3::new(-12.7, -12.7, 0.0), Point3::new(12.7, 12.7, 1.27),
    )));
    let loc = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&w, SetStock::new(&w, StockShape::Box { dx: 29.4, dy: 29.4, dz: 5.27 }, loc).expect("stock"));
    let tool_name = "EM-0300-2FL";
    let tool = catalog().into_iter().find(|(n, _)| *n == tool_name).map(|(_, t)| t).expect("catalog");
    h.execute(&w, AddTool::new(&w, tool_name, tool).expect("tool"));
    let mut params = operation::retype_params(&pocket(3.27, 0.0), OpType::Adaptive2D as usize);
    if let OpParams::Adaptive2D { stepover, stepdown, helix_radius, .. } = &mut params {
        *stepover = 1.2;
        *stepdown = 1.0;
        *helix_radius = 0.6;
    }
    let mut operation = op(SrcRef::Stock, params, AreaControls::default());
    operation.tool = tool_name.into();
    (w, operation)
}

/// #3791: the whole area is the 2 mm stock band around a solid plate.
/// The 3 mm cutter still refuses, but the refusal must name that band.
#[test]
fn a_stock_source_adaptive_refusal_names_the_band_width_and_a_fix() {
    let (w, operation) = plate_band();
    let message = run(&w, &operation).expect_err("no cutter position fits the stock band");
    assert!(message.contains("stock") && message.contains("band") && message.contains("around the part"), "{message}");
    assert!(message.contains("2.000 mm") && message.contains("3.000 mm"), "{message}");
    assert!(message.contains("narrower than the tool"), "{message}");
    assert!(message.contains("wider stock") || message.contains("smaller tool"), "{message}");
    assert!(!message.to_lowercase().contains("pocket"), "{message}");
}

#[test]
fn a_stock_band_refusal_names_stock_to_leave() {
    let (w, mut operation) = plate_band();
    if let OpParams::Adaptive2D { stock_to_leave, .. } = &mut operation.params {
        *stock_to_leave = 0.25;
    }
    let message = run(&w, &operation).expect_err("the cutter plus leave cannot fit");
    assert!(message.contains("stock") && message.contains("band"), "{message}");
    assert!(message.contains("2.000 mm") && message.contains("3.000 mm"), "{message}");
    assert!(message.contains("0.250 mm stock to leave"), "{message}");
    assert!(!message.to_lowercase().contains("pocket"), "{message}");
}

#[test]
fn a_stock_band_refusal_measures_the_controlled_boundary() {
    let (w, mut operation) = plate_band();
    operation.area.offset = -0.25;
    operation.area.corner = CornerStyle::Sharp;
    let message = run(&w, &operation).expect_err("the reduced band cannot fit the cutter");
    assert!(message.contains("stock") && message.contains("band"), "{message}");
    assert!(message.contains("1.500 mm") && message.contains("3.000 mm"), "{message}");
    assert!(!message.to_lowercase().contains("pocket"), "{message}");
}

#[test]
fn a_part_source_narrow_pocket_keeps_its_own_refusal() {
    let (w, mut operation) = plate_band();
    let mut h = History::default();
    let ring = rounded_rect(1.0, 10.0, 0.0, 1);
    h.execute(&w, ImportModel::from_mesh("narrow", block_with_pocket(12.7, 12.7, &ring, 0.1, 1.27)));
    operation.src = SrcRef::Selection(vec![GeoSel { entity: "narrow".into(), feature: top_face(&w, "narrow", 0.1) }]);
    if let OpParams::Adaptive2D { top_z, bottom_z, .. } = &mut operation.params {
        *top_z = Height::fixed(1.27);
        *bottom_z = Height::fixed(0.1);
    }
    let message = run(&w, &operation).expect_err("the closed narrow area cannot fit the cutter");
    assert!(message.contains("pocket is narrower than the tool"), "{message}");
    assert!(!message.contains("stock band"), "{message}");
}

/// PR #3805 review: the stock and the part have the same outline, so the
/// stock-source area is only the closed 2 x 20 mm pocket in the part.
fn flush_pocket(half_x: f64) -> (World, Operation) {
    let (_, mut operation) = plate_band();
    let w = new_world();
    let mut h = History::default();
    let ring = rounded_rect(half_x, 10.0, 0.0, 1);
    h.execute(&w, ImportModel::from_mesh("part", block_with_pocket(12.7, 12.7, &ring, 0.1, 1.27)));
    h.execute(&w, SetStock::new(&w, StockShape::Box { dx: 25.4, dy: 25.4, dz: 1.27 }, Location::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == operation.tool).map(|(_, t)| t).expect("catalog");
    h.execute(&w, AddTool::new(&w, &operation.tool, tool).expect("tool"));
    if let OpParams::Adaptive2D { top_z, bottom_z, .. } = &mut operation.params {
        *top_z = Height::fixed(1.27);
        *bottom_z = Height::fixed(0.1);
    }
    (w, operation)
}

#[test]
fn a_stock_source_closed_pocket_keeps_its_own_refusal() {
    let (w, operation) = flush_pocket(1.0);
    assert!(matches!(operation.src, SrcRef::Stock));
    let message = run(&w, &operation).expect_err("the closed narrow pocket cannot fit the cutter");
    assert!(message.contains("pocket is narrower than the tool"), "{message}");
    assert!(!message.contains("stock band") && !message.contains("2D Contour"), "{message}");
}

/// A boss in the pocket puts kept material inside the area, but part
/// walls bound the area: it is a pocket around an island, not a stock band.
#[test]
fn a_stock_source_pocket_around_an_island_keeps_its_own_refusal() {
    let (w, operation) = flush_pocket(3.0);
    let mut h = History::default();
    h.execute(&w, ImportModel::from_mesh("boss", cube(Point3::new(-1.5, -8.5, 0.0), Point3::new(1.5, 8.5, 1.27))));
    let message = run(&w, &operation).expect_err("the 1.5 mm ring around the boss cannot fit the cutter");
    assert!(message.contains("pocket is narrower than the tool"), "{message}");
    assert!(!message.contains("stock band") && !message.contains("2D Contour"), "{message}");
}

#[test]
fn a_stock_band_refusal_uses_document_inches() {
    let (w, operation) = plate_band();
    let mut h = History::default();
    h.execute(&w, camcraft_app::commands::SetUnits::new(&w, camcraft_app::units::Unit::Inches).expect("inch document"));
    let message = run(&w, &operation).expect_err("units do not change whether the cutter fits");
    assert!(message.contains("0.07874 in") && message.contains("0.11811 in"), "{message}");
    assert!(!message.contains(" mm"), "{message}");
}

#[test]
fn a_curved_stock_band_refusal_does_not_invent_a_rectangular_width() {
    let (_, operation) = plate_band();
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, ImportModel::from_mesh("part", camcraft_app::geometry::cylinder(25.4, 1.27, 64)));
    let loc = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&w, SetStock::new(&w, StockShape::Cylinder { diameter: 29.4, height: 5.27 }, loc).expect("stock"));
    let tool_name = operation.tool.as_str();
    let tool = catalog().into_iter().find(|(n, _)| *n == tool_name).map(|(_, t)| t).expect("catalog");
    h.execute(&w, AddTool::new(&w, tool_name, tool).expect("tool"));
    let message = run(&w, &operation).expect_err("the curved band cannot fit the cutter");
    assert!(message.contains("stock band around the part") && message.contains("3.000 mm"), "{message}");
    assert!(message.contains("smaller tool"), "{message}");
    assert!(!message.contains("minimum side width") && !message.contains("pocket"), "{message}");
}

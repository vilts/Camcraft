//! #1551: the full-height corner material a carried body keeps after an
//! outside contour is true rest, not a sim or carry gap. A 50 x 50 x 10
//! part in 60 x 60 stock; Setup1 contours its sides down to Z 4 with an
//! end mill of radius `r`, on the part's offset with round corners, so
//! the tool sweeps every point within `2 r` of a part corner. The stock
//! corner is 5 * sqrt(2) = 7.07 mm from the part corner. A 6 mm tool
//! (reach 6) leaves a sliver at each stock corner; an 8 mm tool (reach 8)
//! leaves none. The carried body Setup2 is entered with must agree with
//! that geometry column by column: no full-height column inside the swept
//! disc (false rest), and no column outside it cut down (lost material).
//! Provenance: plain swept-disc geometry of a round-cornered offset;
//! our own fixture, no vendor material consulted.
//!
//! Gate map (#1551): covers app::stock_sim_carry (incoming_body),
//! sim::stock_sim::carry (Body::floor_at, carry), app::commands
//! (AlignPartToSetup, GenerateOperation).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, SetStock,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::setup_align::AlignPreset;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{Body, incoming_body};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

/// The part's half width and the stock's.
const PART: f64 = 25.0;
const STOCK: f64 = 30.0;

fn contour() -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(20.0),
        retract_z: Height::fixed(15.0),
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(4.0),
        stepdown: 2.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 4.0,
        clearance_z: Height::fixed(30.0),
        cut_z: Height::fixed(10.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// The body Setup2 (flipped over X) is entered with after Setup1's
/// contour with `tool`, and the tool's diameter.
fn carried_after(tool: &str) -> (Body, f64) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-PART, -PART, 0.0), Point3::new(PART, PART, 10.0))));
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Default::default() };
    let side = 2.0 * STOCK;
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: side, dy: side, dz: 10.0 }, at).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == tool).map(|(_, t)| t).expect("tool");
    let dia = t.diameter;
    h.execute(&world, AddTool::new(&world, tool, t).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    h.execute(&world, CreateOperation::new(&world, "Sides", tool, SrcRef::Entity("part".into()), contour()).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Sides").expect("generate"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::FlipX).expect("flip"));
    h.execute(&world, CreateOperation::new(&world, "Brim", tool, SrcRef::Stock, face()).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Brim").expect("generate"));
    (incoming_body(&world, "Setup2").expect("carried"), dia)
}

/// One column of the brim (outside the part's 50 x 50 by more than a
/// cell): its distance from the nearest part corner (0 beside a side)
/// measured from the column's centre, and whether it stands full height
/// (the flipped body's floor below the brim's Z 6).
struct Col {
    r: f64,
    full: bool,
}

fn brim_columns(body: &Body) -> Vec<Col> {
    let d = &body.dex;
    let mut out = Vec::new();
    for iy in 0..d.ny {
        for ix in 0..d.nx {
            let c = d.center(ix, iy);
            if c.x.abs() <= PART + d.cell && c.y.abs() <= PART + d.cell {
                continue;
            }
            let k = ix + iy * d.nx;
            let (f, t) = (body.floor_at(k), d.top_at(k));
            let full = f.is_finite() && t.is_finite() && f < 5.0;
            let r = ((c.x.abs() - PART).max(0.0).powi(2) + (c.y.abs() - PART).max(0.0).powi(2)).sqrt();
            out.push(Col { r, full });
        }
    }
    out
}

#[test]
fn a_6mm_contour_leaves_the_true_corner_sliver_and_nothing_else() {
    let (body, dia) = carried_after("EM-0600-4FL");
    assert!((dia - 6.0).abs() < 1e-9, "EM-0600-4FL is a 6 mm end mill: {dia}");
    let cell = body.dex.cell;
    let reach = dia; // 2 r: the swept disc about each part corner.
    let cols = brim_columns(&body);
    // No false rest: nothing full height inside the swept disc.
    let false_rest: Vec<f64> = cols.iter().filter(|c| c.full && c.r < reach - cell).map(|c| c.r).collect();
    assert!(false_rest.is_empty(), "full-height columns the tool swept: {false_rest:?}");
    // No lost material: every column clearly outside the disc stands.
    let lost: Vec<f64> = cols.iter().filter(|c| !c.full && c.r > reach + cell).map(|c| c.r).collect();
    assert!(lost.is_empty(), "true rest cut away: {lost:?}");
    // The sliver is there, at all four corners: area of the 5 x 5 corner
    // square outside the 6 mm quarter disc, 1.227 mm^2 each.
    let area = cols.iter().filter(|c| c.full).count() as f64 * cell * cell;
    let exact = 4.0 * 1.227;
    assert!((area - exact).abs() < 4.0 * 2.0 * 1.7 * cell, "sliver area {area} vs {exact}");
}

#[test]
fn an_8mm_contour_cuts_the_stock_corners_through() {
    // Reach 8 > 7.07: the stock corner is inside the swept disc.
    let (body, dia) = carried_after("EM-0800-4FL");
    assert!((dia - 8.0).abs() < 1e-9, "EM-0800-4FL is an 8 mm end mill: {dia}");
    let full: Vec<f64> = brim_columns(&body).into_iter().filter(|c| c.full).map(|c| c.r).collect();
    assert!(full.is_empty(), "full-height brim columns after an 8 mm contour: {full:?}");
}

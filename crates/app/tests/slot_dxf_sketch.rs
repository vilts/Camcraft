//! #1868: Slot and Trochoidal Slot on DXF sketch chains - a closed
//! line-arc slot outline seeds the ends and the width, an open straight
//! chain seeds the ends of the centerline, and every other pick is
//! refused by name.
//!
//! Gate map (M4.4): covers app::operation (slot_sketch, slot_pick, job,
//! generate, material_region), app::sketch_model, app::commands
//! (CreateOperation, SetOperationGeometry, EditOperationParams,
//! GenerateOperation, MoveSketch).
//! Provenance: our own fixtures over our own commands; DXF geometry uses
//! public Autodesk ASCII DXF group codes.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{
    AddTool, Command, CommandError, CreateOperation, DxfOptions, EditOperationParams, GenerateOperation, History, ImportDxf,
    MoveSketch, SetOperationGeometry, SetStock,
};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::sketch_chains::ChainId;
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, HeightReference, OpParams, OpStatus, OpType, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::sketch;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

use common::{gen_to_cltext, write_golden};

const TOOL: &str = "EM-0400-2FL"; // 4 mm endmill

/// One layer for each case:
/// - "slot": a line-arc slot outline, R 5, centres (45, 25) and (75, 25).
/// - "narrow": a line-arc slot outline, R 1.5 (3 mm wide, under the tool).
/// - "center": an open straight line (20, 10) -> (100, 10).
/// - "bent": an open quarter arc, R 20 about (60, 40).
/// - "plate": a closed rectangle 0..120 x 0..80.
/// - "round": a circle, R 6 at (95, 55).
const DXF: &str = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
     0\nLINE\n8\nslot\n10\n45\n20\n20\n11\n75\n21\n20\n\
     0\nARC\n8\nslot\n10\n75\n20\n25\n40\n5\n50\n270\n51\n90\n\
     0\nLINE\n8\nslot\n10\n75\n20\n30\n11\n45\n21\n30\n\
     0\nARC\n8\nslot\n10\n45\n20\n25\n40\n5\n50\n90\n51\n270\n\
     0\nLINE\n8\nnarrow\n10\n20\n20\n63.5\n11\n40\n21\n63.5\n\
     0\nARC\n8\nnarrow\n10\n40\n20\n65\n40\n1.5\n50\n270\n51\n90\n\
     0\nLINE\n8\nnarrow\n10\n40\n20\n66.5\n11\n20\n21\n66.5\n\
     0\nARC\n8\nnarrow\n10\n20\n20\n65\n40\n1.5\n50\n90\n51\n270\n\
     0\nLINE\n8\ncenter\n10\n20\n20\n10\n11\n100\n21\n10\n\
     0\nARC\n8\nbent\n10\n60\n20\n40\n40\n20\n50\n0\n51\n90\n\
     0\nLWPOLYLINE\n8\nplate\n90\n4\n70\n1\n10\n0\n20\n0\n10\n120\n20\n0\n10\n120\n20\n80\n10\n0\n20\n80\n\
     0\nCIRCLE\n8\nround\n10\n95\n20\n55\n40\n6\n\
     0\nENDSEC\n0\nEOF\n";

fn run(world: &World, history: &mut History, cmd: Result<impl Command + 'static, CommandError>, what: &str) {
    history.execute(world, cmd.unwrap_or_else(|e| panic!("{what}: {e}")));
}

fn refusal<T>(r: Result<T, CommandError>, what: &str) -> String {
    match r {
        Ok(_) => panic!("{what} should have been refused"),
        Err(e) => e.to_string(),
    }
}

fn sheet_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let shape = StockShape::Box { dx: 300.0, dy: 240.0, dz: 10.0 };
    run(&world, &mut history, SetStock::new(&world, shape, Location::default()), "sheet stock");
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    run(&world, &mut history, AddTool::new(&world, TOOL, tool), "tool");
    run(&world, &mut history, ImportDxf::from_bytes(&world, None, "plate.dxf", DXF.as_bytes(), &DxfOptions::default()), "import dxf");
    (world, history)
}

fn chain(world: &World, layer: &str) -> ChainId {
    let (_, sk) = sketch::find(world, "plate").expect("plate sketch");
    let chains = sk.data.chains().by_layer(layer);
    assert_eq!(chains.len(), 1, "layer {layer} holds one chain");
    chains[0].id()
}

fn pick(world: &World, layer: &str) -> SrcRef {
    SrcRef::Selection(vec![GeoSel { entity: "plate".into(), feature: FeatureSel::SketchChain(chain(world, layer)) }])
}

/// The kind's seed with stock heights; the seed's typed ends
/// (-20, 0) -> (20, 0) and width stay, so a pick must replace them.
fn seed(kind: OpType) -> OpParams {
    let mut params = operation::seed_params(kind.ordinal());
    match &mut params {
        OpParams::Slot { clearance_z, top_z, bottom_z, stepdown, .. }
        | OpParams::TrochoidalSlot { clearance_z, top_z, bottom_z, stepdown, .. } => {
            *clearance_z = Height::of(HeightReference::TopOfStock, 10.0);
            *top_z = Height::of(HeightReference::TopOfStock, 0.0);
            *bottom_z = Height::of(HeightReference::TopOfStock, -2.0);
            *stepdown = 2.0;
        }
        _ => panic!("not a slot kind"),
    }
    params
}

fn slot_numbers(world: &World, name: &str) -> (Point2, Point2, f64) {
    let op = model::find_by_name(world, name).and_then(|e| model::operation_of(world, &e)).expect("the operation");
    match op.params {
        OpParams::Slot { a, b, width, .. } | OpParams::TrochoidalSlot { a, b, width, .. } => (a, b, width),
        _ => panic!("not a slot kind"),
    }
}

fn near(p: Point2, x: f64, y: f64) -> bool {
    // The outline's round ends are sampled: the ends sit within the sample tolerance.
    (p.x - x).abs() < 1e-3 && (p.y - y).abs() < 1e-3
}

/// The ends are the given two points, in either order.
fn ends_are(a: Point2, b: Point2, p: (f64, f64), q: (f64, f64)) -> bool {
    (near(a, p.0, p.1) && near(b, q.0, q.1)) || (near(a, q.0, q.1) && near(b, p.0, p.1))
}

fn generated(world: &World, history: &mut History, name: &str) -> Toolpath {
    run(world, history, GenerateOperation::new(world, name), "generate");
    let e = model::find_by_name(world, name).expect("the operation");
    assert_eq!(operation::status_of(world, &e), OpStatus::Ok);
    model::toolpath_of(world, &e).expect("cached toolpath").path
}

/// The XY box of the cutter centre while it is in the material.
fn cut_box(path: &Toolpath, top: f64) -> (f64, f64, f64, f64) {
    let mut bx = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for m in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid && m.to.z < top - 1e-6 && m.from.z < top - 1e-6) {
        for p in [m.from, m.to] {
            bx = (bx.0.min(p.x), bx.1.max(p.x), bx.2.min(p.y), bx.3.max(p.y));
        }
    }
    bx
}

#[test]
fn a_slot_outline_seeds_the_slot_and_cuts_it_to_size() {
    let (world, mut history) = sheet_world();
    let src = pick(&world, "slot");
    run(&world, &mut history, CreateOperation::new(&world, "S", TOOL, src, seed(OpType::Slot)), "Slot on the outline");
    let (a, b, width) = slot_numbers(&world, "S");
    assert!(ends_are(a, b, (45.0, 25.0), (75.0, 25.0)), "the ends are the arc centres: {a:?} {b:?}");
    assert!((width - 10.0).abs() < 1e-6, "the width is the outline's: {width}");

    let path = generated(&world, &mut history, "S");
    // A 4 mm cutter in a 10 mm slot: the centre reaches 3 mm off the centerline.
    let (x0, x1, y0, y1) = cut_box(&path, 10.0);
    assert!((x0 - 42.0).abs() < 0.02 && (x1 - 78.0).abs() < 0.02, "length to size: {x0}..{x1}");
    assert!((y0 - 22.0).abs() < 0.02 && (y1 - 28.0).abs() < 0.02, "width to size: {y0}..{y1}");
    // #2906 / #3278: the 10 mm ends are wider than the 4 mm tool, so a wall lap rides
    // each round end on the 3 mm cutter-centre arc about its arc centre.
    for (cx, cy) in [(45.0, 25.0), (75.0, 25.0)] {
        let on_end = path.moves.iter().any(|m| {
            m.arc.is_some_and(|c| near(c.center, cx, cy))
                && [m.from, m.to].iter().all(|p| ((p.x - cx).hypot(p.y - cy) - 3.0).abs() < 1e-3)
        });
        assert!(on_end, "the round end about ({cx}, {cy}) is cut on its wall arc");
    }
    // No cutting move leaves the outline at the tool radius: each end point lies in the
    // cutter-centre stadium (3 mm about the axis (45, 25) -> (75, 25)), which is convex, so
    // each line stays in it; each arc turns on a 3 mm circle about an axis end.
    for m in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid && m.to.z < 10.0 - 1e-6 && m.from.z < 10.0 - 1e-6) {
        for p in [m.from, m.to] {
            let off = (p.x.clamp(45.0, 75.0) - p.x).hypot(p.y - 25.0);
            assert!(off < 3.0 + 1e-3, "{m:?} leaves the outline: {off} mm off the axis");
        }
        if let Some(c) = m.arc {
            let r = (m.from.x - c.center.x).hypot(m.from.y - c.center.y);
            assert!(near(c.center, 45.0, 25.0) || near(c.center, 75.0, 25.0), "{m:?}: an arc off the round ends");
            assert!(r < 3.0 + 1e-3, "{m:?}: an arc of radius {r} leaves the outline");
        }
    }
    write_golden("slot_dxf_outline.cl", &gen_to_cltext(&path));

    let sim = simulate_document(&world).expect("simulate on the stock, with no solid model");
    assert_eq!(sim.dev.gouge_cells, 0, "no gouge");

    // The sketch moves: the seeded ends no longer lie in the outline. Refused, never cut in the old place.
    run(&world, &mut history, MoveSketch::new(&world, "plate", 15.0, 0.0, 0.0), "move sketch");
    let why = refusal(GenerateOperation::new(&world, "S"), "a slot that left its outline");
    assert!(why.contains("the slot does not fit the selection"), "{why}");
    assert!(history.undo(&world));

    // The pick command seeds too, and one undo takes the pick and the seed back.
    run(&world, &mut history, CreateOperation::new(&world, "T", TOOL, SrcRef::Stock, seed(OpType::Slot)), "typed Slot");
    let items = vec![GeoSel { entity: "plate".into(), feature: FeatureSel::SketchLayer("slot".into()) }];
    run(&world, &mut history, SetOperationGeometry::new(&world, "T", items), "pick the layer");
    let (a, b, width) = slot_numbers(&world, "T");
    assert!(ends_are(a, b, (45.0, 25.0), (75.0, 25.0)) && (width - 10.0).abs() < 1e-6, "seeded: {a:?} {b:?} {width}");
    assert!(history.undo(&world));
    let (a, b, width) = slot_numbers(&world, "T");
    assert!(ends_are(a, b, (-20.0, 0.0), (20.0, 0.0)) && (width - 4.0).abs() < 1e-9, "undone: {a:?} {b:?} {width}");
}

#[test]
fn a_trochoidal_slot_reads_the_same_outline() {
    let (world, mut history) = sheet_world();
    let src = pick(&world, "slot");
    run(&world, &mut history, CreateOperation::new(&world, "S", TOOL, src, seed(OpType::TrochoidalSlot)), "Trochoidal Slot");
    let (a, b, width) = slot_numbers(&world, "S");
    assert!(ends_are(a, b, (45.0, 25.0), (75.0, 25.0)) && (width - 10.0).abs() < 1e-6, "seeded: {a:?} {b:?} {width}");

    let path = generated(&world, &mut history, "S");
    let (x0, x1, y0, y1) = cut_box(&path, 10.0);
    assert!(x0 > 42.0 - 0.02 && x1 < 78.0 + 0.02 && y0 > 22.0 - 0.02 && y1 < 28.0 + 0.02, "inside the outline: {x0}..{x1} {y0}..{y1}");
    assert!((y0 - 22.0).abs() < 0.05 && (y1 - 28.0).abs() < 0.05, "width to size: {y0}..{y1}");
    write_golden("trochoid_slot_dxf_outline.cl", &gen_to_cltext(&path));

    // A typed slot wider than the outline is refused by name.
    let mut wide = seed(OpType::TrochoidalSlot);
    if let OpParams::TrochoidalSlot { a, b, width, .. } = &mut wide {
        (*a, *b, *width) = (Point2::new(45.0, 25.0), Point2::new(75.0, 25.0), 12.0);
    }
    run(&world, &mut history, EditOperationParams::new(&world, "S", wide), "widen");
    let why = refusal(GenerateOperation::new(&world, "S"), "a slot wider than its outline");
    assert!(why.contains("the slot does not fit the selection") && why.contains("12.000 mm wide"), "{why}");
}

#[test]
fn an_open_straight_chain_is_the_centerline_at_the_typed_width() {
    let (world, mut history) = sheet_world();
    for (name, kind, width) in [("S", OpType::Slot, 4.0), ("T", OpType::TrochoidalSlot, 16.0)] {
        let src = pick(&world, "center");
        run(&world, &mut history, CreateOperation::new(&world, name, TOOL, src, seed(kind)), "on the centerline");
        let (a, b, w) = slot_numbers(&world, name);
        assert!(ends_are(a, b, (20.0, 10.0), (100.0, 10.0)), "{kind}: the ends are the chain's: {a:?} {b:?}");
        assert!((w - width).abs() < 1e-9, "{kind}: the typed width stands: {w}");
        let path = generated(&world, &mut history, name);
        let r = 0.5 * (width - 4.0);
        let (x0, x1, y0, y1) = cut_box(&path, 10.0);
        assert!(x0 > 20.0 - r - 0.02 && x1 < 100.0 + r + 0.02, "{kind}: along the centerline: {x0}..{x1}");
        assert!(y0 > 10.0 - r - 0.02 && y1 < 10.0 + r + 0.02, "{kind}: across the centerline: {y0}..{y1}");
    }

    // Typed ends that are not the ends of the picked centerline are refused by name.
    let mut moved = seed(OpType::Slot);
    if let OpParams::Slot { a, b, .. } = &mut moved {
        (*a, *b) = (Point2::new(20.0, 10.0), Point2::new(100.0, 30.0));
    }
    run(&world, &mut history, EditOperationParams::new(&world, "S", moved), "move an end");
    let why = refusal(GenerateOperation::new(&world, "S"), "ends off the centerline");
    assert!(why.contains("are not the ends of the picked centerline"), "{why}");
}

#[test]
fn picks_that_are_no_slot_are_refused_by_name() {
    let (world, mut history) = sheet_world();
    for kind in [OpType::Slot, OpType::TrochoidalSlot] {
        let name = format!("{kind}");
        let mut refused = |src: SrcRef, what: &str| -> String {
            let name = format!("{name} {what}");
            run(&world, &mut history, CreateOperation::new(&world, &name, TOOL, src, seed(kind)), what);
            refusal(GenerateOperation::new(&world, &name), what)
        };

        // A channel narrower than the tool: the 3 mm outline under the 4 mm cutter.
        // #3475: the plain Slot refuses it at create, in plain words; the
        // trochoidal slot still at generate.
        if kind == OpType::Slot {
            let created = CreateOperation::new(&world, &format!("{name} narrow outline"), TOOL, pick(&world, "narrow"), seed(kind));
            let why = refusal(created, "narrow outline");
            assert!(why.contains("is 4 mm wide and the slot is 3 mm wide: choose a tool of 3 mm or less"), "{kind}: {why}");
        } else {
            let why = refused(pick(&world, "narrow"), "narrow outline");
            let named = "is too narrow for a 4.000 mm tool";
            assert!(why.contains(named) && why.contains("3.000 mm") && why.contains("4.000 mm"), "{kind}: {why}");
        }

        // A closed outline that is not a slot.
        let why = refused(pick(&world, "plate"), "rectangle");
        assert!(why.contains("is not a slot outline"), "{kind}: {why}");
        let why = refused(pick(&world, "round"), "circle");
        assert!(why.contains("is not a slot outline"), "{kind}: {why}");

        // An open chain that is not straight.
        let why = refused(pick(&world, "bent"), "arc");
        assert!(why.contains("is not straight") && why.contains("centerline"), "{kind}: {why}");

        // More than one chain in one operation.
        let two = SrcRef::Selection(vec![
            GeoSel { entity: "plate".into(), feature: FeatureSel::SketchLayer("slot".into()) },
            GeoSel { entity: "plate".into(), feature: FeatureSel::SketchLayer("center".into()) },
        ]);
        let why = refused(two, "two chains");
        assert!(why.contains("this selection has 2 chains"), "{kind}: {why}");

        // The whole profile as the source names no slot: still refused by name.
        let why = refused(SrcRef::Entity("plate".into()), "whole profile");
        assert!(why.contains("pick one closed slot outline or one open centerline"), "{kind}: {why}");
    }
}

/// #3475: picks that are no slot seed no width, so the slot fit law does
/// not speak for the seed's fallback width: a 6 mm tool on them is made,
/// and generation names the picks, as before.
#[test]
fn picks_that_are_no_slot_keep_their_pick_refusal_under_a_wider_tool() {
    let (world, mut history) = sheet_world();
    const WIDE: &str = "EM-0600-4FL";
    let tool = catalog().into_iter().find(|(n, _)| *n == WIDE).map(|(_, t)| t).expect("catalog tool");
    run(&world, &mut history, AddTool::new(&world, WIDE, tool), "wide tool");
    let created = CreateOperation::new(&world, "S plate", WIDE, pick(&world, "plate"), seed(OpType::Slot));
    run(&world, &mut history, created, "a Slot on a rectangle");
    let why = refusal(GenerateOperation::new(&world, "S plate"), "rectangle");
    assert!(why.contains("is not a slot outline"), "{why}");
}

//! #1066: the planar ladder takes the top of its cut from the feature
//! (`top_z` for Pocket and Contour, the hole's mouth `entry_z` for Bore)
//! and rapids from clearance down to that top plus the plunge height.
//! When the stock stands higher than the feature's mouth - a modelled
//! pocket whose top sits under the block's top, nothing faced ahead -
//! that rapid ends inside uncut stock. These tests pin that the rapid
//! law (#96, `verify::rapids_in_stock`: a vertical rapid DOWN whose end
//! lies under the surface standing at its column) names the dive as a
//! `RapidInStock` on Bore, 2D Pocket and Contour alike, and the post gate
//! refuses the program by name; stock to the mouth is silent, and a
//! face ahead that takes the block down to the mouth acquits the dive
//! (`verify::acquit_by_prior`).
//!
//! The Bore case was the hole in it: a Bore whose source is the stock
//! reads a region with the holes' rims punched out, and the column law
//! fell back to that region for "is this stock?", so the dive down the
//! hole's own uncut column passed silent and posted. The verify now
//! reads the unpunched stock there (`operation::material_source::
//! judged_stock`).
//!
//! Provenance: our own fixtures (the blind square hole of
//! `pocket_overhead.rs` under a taller block), specced from the Prime
//! Directive and the shop rule "never rapid into uncut stock"; no vendor
//! material.
//!
//! Gate map (#1066): covers app::operation (generate's verify wiring,
//! the Bore / Pocket / Contour arms), motion::machining2d (levels,
//! descent), sim::verify (rapids, prior), app::posting (the gate).
//! Lane shape: all tests here run in seconds.

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::drilling::Hole;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube, feature};
use camcraft_app::machining2d::{Lead, PocketStrategy, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

/// The part's top and the blind hole's mouth: Z 20.
const MOUTH: f64 = 20.0;
/// The hole's floor.
const FLOOR: f64 = 10.0;
/// Half the square hole's side (20 x 20 in a 40 x 40 block).
const H: f64 = 10.0;
const TOOL: &str = "EM-0800-4FL";

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// A 40 x 40 x [0, 20] block with a 20 x 20 blind hole from its top to
/// Z 10, outward-wound (`pocket_overhead.rs`'s fixture).
fn block_with_hole() -> Mesh {
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, MOUTH));
    f.extend(outer.facets[4..12].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let i = [(-H, -H), (H, -H), (H, H), (-H, H)];
    for k in 0..4 {
        let n = (k + 1) % 4;
        let p = |(x, y): (f64, f64)| Point3::new(x, y, MOUTH);
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    let inner = cube(Point3::new(-H, -H, FLOOR), Point3::new(H, H, MOUTH));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(outer.facets[0..2].iter().cloned());
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

/// The part in a 60 x 60 block from Z -10 standing `lift` mm over the
/// hole's mouth, and the 8 mm end mill.
fn scene(lift: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", block_with_hole()));
    let loc = Location { pos: Point3::new(0.0, 0.0, -10.0), ..Default::default() };
    let shape = StockShape::Box { dx: 60.0, dy: 60.0, dz: MOUTH + 10.0 + lift };
    h.execute(&world, SetStock::new(&world, shape, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    (world, h)
}

/// The hole's floor face, picked (the pocket's source).
fn floor_pick(world: &World) -> SrcRef {
    let e = model::find_by_name(world, "part").expect("model");
    let f = model::features_of(world, &e).expect("features");
    let face = f
        .faces
        .iter()
        .filter(|face| face.normal[2] > 0.99)
        .find(|face| {
            face.loops.iter().filter(|l| l.kind == feature::LoopKind::Outer).any(|l| {
                l.verts
                    .iter()
                    .filter_map(|&v| f.verts.get(v as usize))
                    .all(|p| (p[2] - FLOOR).abs() < 1e-4 && p[0].abs() <= H + 1e-4 && p[1].abs() <= H + 1e-4)
            })
        })
        .expect("the floor face");
    SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: FeatureSel::Face(face.id) }])
}

fn pocket() -> OpParams {
    OpParams::Pocket {
        links: camcraft_app::links::Links::PLANAR,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(MOUTH),
        bottom_z: Height::fixed(FLOOR),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Waterline,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        tilt: None,
    }
}

/// A milled bore of a 16 mm hole inside the square one, mouth Z 20: the
/// ladder's top is the hole's `entry_z`, not the op's top height.
fn bore() -> OpParams {
    OpParams::Bore {
        links: camcraft_app::links::Links::PLANAR,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(MOUTH),
        bottom_z: None,
        stock_to_leave: 0.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 150.0,
        climb: true,
        tolerance: 0.01,
        holes: vec![Hole { xy: Point2::new(0.0, 0.0), radius: 8.0, entry_z: MOUTH, floor_z: FLOOR, through: false, stepped: false }],
    }
}

/// The block's outside wall, Z 20 down to 15.
fn contour() -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: Height::fixed(30.0),
        top_z: Height::fixed(MOUTH),
        bottom_z: Height::fixed(15.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// A one-pass face of the whole block down to the hole's mouth.
fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 5.0,
        clearance_z: Height::fixed(40.0),
        cut_z: Height::fixed(MOUTH),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Pocket,
    Bore,
    Contour,
}

/// One op of `kind` (behind a face to the mouth when `faced`) in the
/// block standing `lift` over the mouth: its first vertical rapid down
/// (the ladder's entry: index and end Z), its warnings, and the post's
/// verdict.
fn run(kind: Kind, lift: f64, faced: bool) -> ((usize, f64), Vec<Warning>, Result<(), String>) {
    let (world, mut h) = scene(lift);
    let (src, params) = match kind {
        Kind::Pocket => (floor_pick(&world), pocket()),
        Kind::Bore => (SrcRef::Stock, bore()),
        Kind::Contour => (SrcRef::Entity("part".into()), contour()),
    };
    if faced {
        h.execute(&world, CreateOperation::new(&world, "Op1Face", TOOL, SrcRef::Stock, face()).expect("create face"));
        h.execute(&world, GenerateOperation::new(&world, "Op1Face").expect("generate face"));
    }
    h.execute(&world, CreateOperation::new(&world, "Op2", TOOL, src, params).expect("create op"));
    h.execute(&world, GenerateOperation::new(&world, "Op2").expect("generate op"));
    let e = model::find_by_name(&world, "Op2").expect("op");
    let cache = model::toolpath_of(&world, &e).unwrap_or_else(|| panic!("{kind:?}: no toolpath"));
    let moves = &cache.path.moves;
    let entry = moves
        .iter()
        .position(|m| m.kind == MoveKind::Rapid && m.from.x == m.to.x && m.from.y == m.to.y && m.to.z < m.from.z)
        .and_then(|i| moves.get(i).map(|m| (i, m.to.z)))
        .unwrap_or_else(|| panic!("{kind:?}: no entry rapid"));
    let out = std::env::temp_dir().join(format!("camcraft-1066-{kind:?}-{lift}-{faced}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let posted = posting::post_setup(&world, &out).map(|_| ());
    let _ = std::fs::remove_file(&out);
    (entry, cache.warnings.clone(), posted)
}

#[test]
fn a_ladder_entry_rapid_under_a_taller_block_is_named_and_blocks_the_post() {
    // The block stands 5 mm over the mouth: the entry rapid from
    // clearance to the plunge height (Z 22) ends 3 mm inside uncut stock.
    for kind in [Kind::Pocket, Kind::Bore, Kind::Contour] {
        let ((at, z), warns, posted) = run(kind, 5.0, false);
        assert!(z < MOUTH + 5.0 - 1e-6, "{kind:?}: the fixture must rapid down under the block's top, not to Z {z}");
        assert!(warns.contains(&Warning::RapidInStock { at }), "{kind:?}: the entry dive went unnamed: {warns:?}");
        match posted {
            Ok(()) => panic!("{kind:?}: a program with a rapid into the block posted"),
            Err(e) => assert!(
                e.contains("post blocked") && e.contains(&format!("rapid move {at} ")),
                "{kind:?}: refused, but not by the rapid law: {e}"
            ),
        }
    }
}

#[test]
fn a_block_standing_inside_the_plunge_height_is_fed_through_silently() {
    // 1.5 mm of stock over the mouth sits under the plunge height (2 mm):
    // the rapid stops above the block and the descent feeds through it.
    for kind in [Kind::Pocket, Kind::Bore, Kind::Contour] {
        let ((_, z), warns, posted) = run(kind, 1.5, false);
        assert!(z >= MOUTH + 1.5, "{kind:?}: the entry rapid dove under the block's top to Z {z}");
        assert!(!warns.iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{kind:?}: {warns:?}");
        assert!(posted.is_ok(), "{kind:?}: {posted:?}");
    }
}

#[test]
fn stock_to_the_mouth_is_silent_and_posts() {
    for kind in [Kind::Pocket, Kind::Bore, Kind::Contour] {
        let ((_, z), warns, posted) = run(kind, 0.0, false);
        assert!(z >= MOUTH, "{kind:?}: Z {z}");
        assert!(!warns.iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{kind:?}: {warns:?}");
        assert!(posted.is_ok(), "{kind:?}: {posted:?}");
    }
}

#[test]
fn a_face_ahead_to_the_mouth_acquits_the_entry_rapid() {
    // The same taller block, faced down to the mouth first: the dive
    // falls through air the program ahead already cut.
    for kind in [Kind::Pocket, Kind::Bore, Kind::Contour] {
        let ((_, z), warns, posted) = run(kind, 5.0, true);
        assert!(z < MOUTH + 5.0 - 1e-6, "{kind:?}: the dive still goes under the raw block's top, not to Z {z}");
        assert!(!warns.iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{kind:?}: {warns:?}");
        assert!(posted.is_ok(), "{kind:?}: {posted:?}");
    }
}

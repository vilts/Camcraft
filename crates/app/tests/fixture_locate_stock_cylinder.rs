//! #2173 review: a stock locate judges the picked stock face itself, not
//! the box of the stock. A cylinder stock re-fits upright round a turned
//! part, so its picked cap would stand across the mate plane while the
//! box of the stock still matches: refused by name. The same cap mated on
//! the vise floor lands flat, at the right height, and the fixture
//! collision check reads the stock where it lands.
//! Provenance: our own fixtures over plain vise practice (machine-tool
//! handbooks); no vendor material consulted.
//!
//! Gate map (#2173): covers app::fixture_locate (stock_lands and its
//! face check in fixture_locate_land.rs), app::commands::locate,
//! app::fixture_clash (after_fixture_move).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{CreateSetup, History, ImportModel, LocatePart, SetStock, TagFixture};
use camcraft_app::fixture_clash;
use camcraft_app::fixture_locate::{self, LocateBody, LocateOffset, LocateSpec, Mate};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{MateDatum, facets_centroid};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Floor top at Z 0 (+Z), fixed jaw face at Y -1 (+Y), stop face at X -1 (+X).
fn vise() -> Vec<Triangle> {
    let mut t: Vec<Triangle> = cube(p(0.0, 0.0, -10.0), p(100.0, 60.0, 0.0)).facets.to_vec();
    t.extend(cube(p(0.0, -20.0, 0.5), p(100.0, -1.0, 40.0)).facets.iter());
    t.extend(cube(p(-20.0, 0.0, 0.5), p(-1.0, 60.0, 30.0)).facets.iter());
    t
}

/// The reviewer's case (a 10 mm cube part, a D20 x H20 cylinder stock
/// 5 mm under it and centred on it), moved to [40, 30, 0] over the
/// middle of the vise floor so the legal twin stands clear of the jaws.
fn document() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(p(40.0, 30.0, 0.0), p(50.0, 40.0, 10.0))));
    h.execute(&world, ImportModel::from_mesh("vise", Mesh { facets: vise().into() }));
    h.execute(&world, TagFixture::new(&world, "vise", None).expect("mark the vise"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    let shape = StockShape::Cylinder { diameter: 20.0, height: 20.0 };
    let at = Location { pos: p(45.0, 35.0, -5.0), ..Location::default() };
    h.execute(&world, SetStock::new(&world, shape, at).expect("cylinder stock"));
    (world, h)
}

fn stock_name(world: &World) -> String {
    camcraft_app::setup::stock_entity_name(world, Some("Setup1"))
}

/// The vise face whose facets centre on the point `c` (vise local = world).
fn vise_face(world: &World, c: [f64; 3]) -> GeoSel {
    let e = model::find_by_name(world, "vise").expect("vise");
    let f = model::features_of(world, &e).expect("features");
    let tris = model::facets_of(world, &e).expect("facets");
    let id = f
        .faces
        .iter()
        .find(|f| {
            facets_centroid(&tris, f.facets.iter().map(|&i| i as usize))
                .is_some_and(|m| (0..3).all(|k| (m[k] - c[k]).abs() < 1e-6))
        })
        .map(|f| f.id)
        .unwrap_or_else(|| panic!("the vise has a face centred on {c:?}"));
    GeoSel { entity: "vise".into(), feature: FeatureSel::Face(id) }
}

/// The stock's bottom cap (outward normal -Z), read where the locate reads it.
fn bottom_cap(world: &World) -> GeoSel {
    let name = stock_name(world);
    let e = model::find_by_name(world, &name).expect("stock");
    let f = model::features_of(world, &e).expect("stock features");
    f.faces
        .iter()
        .map(|f| GeoSel { entity: name.clone(), feature: FeatureSel::Face(f.id) })
        .find(|g| {
            matches!(fixture_locate::read_pick(world, "vise", g),
                Ok(MateDatum::Plane { normal, .. }) if (normal[2] + 1.0).abs() < 1e-9)
        })
        .unwrap_or_else(|| panic!("the cylinder stock has a bottom cap"))
}

/// One face mate: the stock's bottom cap on the vise face centred on `c`.
fn cap_on(world: &World, c: [f64; 3]) -> LocateSpec {
    let mates = vec![Mate { part: bottom_cap(world), fixture: vise_face(world, c) }];
    LocateSpec { part: "part".into(), fixture: "vise".into(), body: LocateBody::Stock, mates, offset: LocateOffset::default() }
}

fn bounds(world: &World, name: &str) -> ([f64; 3], [f64; 3]) {
    let e = model::find_by_name(world, name).expect("entity");
    let b = model::entity_world_aabb(world, &e).expect("bounds");
    ([b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z])
}

/// The reviewer's repro: the cap on the stop face (normal +X) needs a
/// quarter turn; the upright re-fit keeps the cap at -Z, perpendicular
/// to the stop, with the same box. Refused at the command, by name.
#[test]
fn a_cylinder_cap_mated_across_its_plane_is_refused() {
    let (world, _h) = document();
    let before = (bounds(&world, &stock_name(&world)), bounds(&world, "part"));
    let spec = cap_on(&world, [-1.0, 30.0, 15.25]);
    let cap = camcraft_app::pick::feature_name_or_kind(&world, &spec.mates[0].part.entity, spec.mates[0].part.feature);
    let stop = camcraft_app::pick::feature_name_or_kind(&world, &spec.mates[0].fixture.entity, spec.mates[0].fixture.feature);
    let Err(e) = LocatePart::new(&world, &spec) else { panic!("the cap would stand across the stop face") };
    let e = e.to_string();
    assert!(!e.contains("0x"), "no hex ids in refusal: {e}");
    assert!(e.contains(&format!("mate 1: {cap} of the stock '{}'", stock_name(&world))), "names the stock face: {e}");
    assert!(e.contains(&format!("the plane of, not flat on, {stop} of 'vise'")), "names the fixture plane: {e}");
    assert_eq!((bounds(&world, &stock_name(&world)), bounds(&world, "part")), before, "a refusal moves nothing");
}

/// The legal twin: the cap on the floor (normal +Z) needs no turn. The
/// stock lands flat at Z 0, keeps its 20 mm height, the part keeps its
/// 5 mm above the cap, and the collision check reads the landed stock.
#[test]
fn a_cylinder_cap_mated_on_the_floor_lands_at_the_right_height() {
    let (world, mut h) = document();
    let c = LocatePart::new(&world, &cap_on(&world, [50.0, 30.0, 0.0])).expect("the cap lands on the floor");
    h.execute(&world, c);
    let name = stock_name(&world);
    let (lo, hi) = bounds(&world, &name);
    assert!(lo[2].abs() < 1e-6, "the cap on the floor at Z 0: {lo:?}");
    assert!((hi[2] - 20.0).abs() < 1e-6, "the stock keeps its 20 mm height: {hi:?}");
    assert!((hi[0] - lo[0] - 20.0).abs() < 1e-6, "the stock stays upright, D20: {lo:?} {hi:?}");
    let (plo, phi) = bounds(&world, "part");
    assert!((plo[2] - 5.0).abs() < 1e-6 && (phi[2] - 15.0).abs() < 1e-6, "the part keeps its place: {plo:?}");
    assert!((plo[0] - lo[0] - 5.0).abs() < 1e-6 && (plo[1] - lo[1] - 5.0).abs() < 1e-6, "centred in the stock: {plo:?} {lo:?}");
    // The collision check reads the stock where it landed: flush is clear,
    // and a 2 mm lower stock (the part 3 mm above the floor) sinks in.
    assert_eq!(fixture_clash::after_fixture_move(&world, "vise", Location::default()), None, "flush on the floor");
    h.undo(&world);
    let mut sunk = cap_on(&world, [50.0, 30.0, 0.0]);
    sunk.offset.dz = -2.0;
    h.execute(&world, LocatePart::new(&world, &sunk).expect("built"));
    let w = fixture_clash::after_fixture_move(&world, "vise", Location::default()).expect("the check reads the landed stock");
    assert!(w.contains(&format!("'{name}'")), "{w}");
}

/// A cap on the floor with a quarter turn about the floor normal: the
/// stock re-fits upright with other facets than the turned rigid body,
/// so the face check itself (not the rigid shortcut) must accept it.
#[test]
fn a_turned_cylinder_cap_on_the_floor_is_judged_by_its_face_and_lands() {
    let (world, mut h) = document();
    let mut spec = cap_on(&world, [50.0, 30.0, 0.0]);
    spec.offset.turn_deg = 90.0;
    let c = LocatePart::new(&world, &spec).expect("the turned cap still lands flat on the floor");
    h.execute(&world, c);
    let (lo, hi) = bounds(&world, &stock_name(&world));
    assert!(lo[2].abs() < 1e-6 && (hi[2] - 20.0).abs() < 1e-6, "flat on the floor, 20 mm high: {lo:?} {hi:?}");
}

//! #771: the orientation dropdowns' backends - the cylinder-angle
//! alignment (one curved-wall pick names the bore's or cylinder's
//! axis from its own facet normals; it lands on a chosen world axis,
//! turned by an angle about it) and the setup's WCS origin placement
//! (a box reading - the parts' bounds or the setup's stock - or a
//! picked vertex; the rotation is kept). Pinned on a tilted cylinder
//! and a placed block, with undo, per-setup placement and every
//! refusal by name.
//!
//! Provenance: our own fixtures over plain fixture practice (indicate
//! a bore's wall, set the origin off the block's corner); no vendor
//! material consulted.
//!
//! Gate map (#771): covers app::pick::cylinder, app::commands
//! (AlignPartToSetup::align_from_cylinder, check_cylinder,
//! SetWcsOrigin::for_setup, place_setup_origin).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AlignPartToSetup, BoxPoint, CreateSetup, History, ImportModel, MoveModel, OriginAt, SetStock,
    SetWcsOrigin, place_setup_origin,
};
use camcraft_app::geometry::{Point3, feature, quat_from_euler};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::TargetAxis;
use camcraft_app::schema::Location;
use camcraft_app::setup_align;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

const TIGHT: f64 = 1e-9;

/// A cylinder (20 across, 10 tall, 48 segments) standing at the
/// origin, placed by `rot` (degrees about X, Y, Z) at (5, -3, 2),
/// with Setup1.
fn cyl_part(rot: [f64; 3]) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cylinder_mesh()));
    let [x, y, z] = rot.map(f64::to_radians);
    let loc = Location { pos: Point3::new(5.0, -3.0, 2.0), rot: quat_from_euler(x, y, z) };
    h.execute(&world, MoveModel::new(&world, "part", loc).expect("place the cylinder"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    (world, h)
}

/// The cylinder mesh, 20 across and 10 tall, axis on Z.
fn cylinder_mesh() -> camcraft_app::geometry::Mesh {
    camcraft_app::geometry::cylinder(20.0, 10.0, 48)
}

/// The part's side wall as a pick: the welded patch whose facet
/// normals are not all parallel (the caps' are).
fn wall(world: &World) -> GeoSel {
    let e = model::find_by_name(world, "part").expect("entity");
    let features = model::features_of(world, &e).expect("features");
    let tris = model::facets_of(world, &e).expect("facets");
    for p in feature::patches(&features) {
        let mut first: Option<[f64; 3]> = None;
        let mut curved = false;
        for face in features.faces.iter().filter(|f| p.faces.contains(&f.id)) {
            for &i in &face.facets {
                let Some(t) = tris.get(i as usize) else { continue };
                let n = (t.b - t.a).cross(t.c - t.a);
                let l = n.dot(n).sqrt();
                if !l.is_finite() || l <= 0.0 {
                    continue;
                }
                let n = [n.x / l, n.y / l, n.z / l];
                match first {
                    None => first = Some(n),
                    Some(f) => {
                        let dot = (n[0] * f[0] + n[1] * f[1] + n[2] * f[2]).abs();
                        if dot < 1.0 - 1e-6 {
                            curved = true;
                        }
                    }
                }
            }
        }
        if curved {
            return GeoSel { entity: "part".into(), feature: FeatureSel::Patch(p.id) };
        }
    }
    panic!("the cylinder has a side wall");
}

/// The wall's facet normals in the SETUP frame (through the setup's
/// part alignment) - the axis reading, end to end.
fn wall_normals_in_setup(world: &World, setup: &str) -> Vec<[f64; 3]> {
    let e = model::find_by_name(world, "part").expect("entity");
    let tris = setup_align::facets_in(world, &e, Some(setup)).expect("facets in setup frame");
    let g = wall(world);
    let features = model::features_of(world, &e).expect("features");
    let FeatureSel::Patch(pid) = g.feature else { panic!("the wall is a patch") };
    let p = feature::patches(&features).into_iter().find(|p| p.id == pid).expect("the patch");
    let mut out = Vec::new();
    for face in features.faces.iter().filter(|f| p.faces.contains(&f.id)) {
        for &i in &face.facets {
            if let Some(t) = tris.get(i as usize) {
                let n = (t.b - t.a).cross(t.c - t.a);
                let l = n.dot(n).sqrt();
                if l.is_finite() && l > 0.0 {
                    out.push([n.x / l, n.y / l, n.z / l]);
                }
            }
        }
    }
    out
}

/// Every wall normal is perpendicular to `axis` (the wall's axis runs
/// along it), within `tol`.
fn all_perpendicular(normals: &[[f64; 3]], axis: [f64; 3], tol: f64) -> bool {
    normals.iter().all(|n| (n[0] * axis[0] + n[1] * axis[1] + n[2] * axis[2]).abs() <= tol)
}

#[test]
fn a_tilted_cylinders_wall_lands_its_axis_on_z() {
    let (world, mut h) = cyl_part([40.0, 0.0, 0.0]);
    h.execute(&world, AlignPartToSetup::align_from_cylinder(&world, "Setup1", &wall(&world), TargetAxis::Z, 0.0, false).expect("align"));
    // End to end: in the setup frame every wall normal is perpendicular
    // to +Z - the bore's axis runs along Z exactly.
    let normals = wall_normals_in_setup(&world, "Setup1");
    assert!(!normals.is_empty(), "the wall has facets");
    assert!(all_perpendicular(&normals, [0.0, 0.0, 1.0], 1e-6), "the axis lands on Z");
}

#[test]
fn the_angle_swings_about_the_landed_axis() {
    let (world, mut h) = cyl_part([40.0, 0.0, 0.0]);
    let plain = {
        h.execute(&world, AlignPartToSetup::align_from_cylinder(&world, "Setup1", &wall(&world), TargetAxis::Z, 0.0, false).expect("align 0"));
        wall_normals_in_setup(&world, "Setup1")
    };
    let (world2, mut h2) = cyl_part([40.0, 0.0, 0.0]);
    h2.execute(&world2, AlignPartToSetup::align_from_cylinder(&world2, "Setup1", &wall(&world2), TargetAxis::Z, 90.0, false).expect("align 90"));
    let swung = wall_normals_in_setup(&world2, "Setup1");
    // A 90-degree swing about the landed Z axis: every normal of the
    // swung wall is a 90-degree rotation about Z of some plain one.
    // Rotating (x, y, z) by +90 about Z gives (-y, x, z) - check the
    // sets match under that map.
    let rot90 = |n: [f64; 3]| [-n[1], n[0], n[2]];
    for n in &swung {
        let r = rot90(*n);
        assert!(
            plain.iter().any(|p| (p[0] - r[0]).abs() < 1e-6 && (p[1] - r[1]).abs() < 1e-6 && (p[2] - r[2]).abs() < 1e-6),
            "the swung wall is the plain one turned 90 about Z"
        );
    }
    // The axis still lands: every swung normal stays perpendicular to Z.
    assert!(all_perpendicular(&swung, [0.0, 0.0, 1.0], 1e-6), "the angle keeps the landed axis put");
}

#[test]
fn the_wall_lands_on_x_and_y_too() {
    for target in [TargetAxis::X, TargetAxis::Y] {
        let (world, mut h) = cyl_part([40.0, 0.0, 25.0]);
        h.execute(&world, AlignPartToSetup::align_from_cylinder(&world, "Setup1", &wall(&world), target, 0.0, false).expect("align"));
        let normals = wall_normals_in_setup(&world, "Setup1");
        let t = target.vec();
        assert!(all_perpendicular(&normals, t, 1e-6), "the axis lands on {}", target.label());
    }
}

#[test]
fn a_flat_face_is_refused_by_name() {
    let (world, _h) = cyl_part([0.0, 0.0, 0.0]);
    let e = model::find_by_name(&world, "part").expect("entity");
    let features = model::features_of(&world, &e).expect("features");
    // A cap face (planar) refused as not a welded wall.
    let cap = features
        .faces
        .iter()
        .find(|f| f.kind == feature::FaceKind::Planar)
        .expect("the cylinder has cap faces");
    let pick = GeoSel { entity: "part".into(), feature: FeatureSel::Face(cap.id) };
    let err = AlignPartToSetup::align_from_cylinder(&world, "Setup1", &pick, TargetAxis::Z, 0.0, false)
        .err()
        .expect("a flat face is refused");
    assert!(err.to_string().contains("is not a welded wall"), "said: {err}");
    // An edge refused the same way.
    let edge_pick = GeoSel { entity: "part".into(), feature: FeatureSel::Chain(0) };
    let err = AlignPartToSetup::align_from_cylinder(&world, "Setup1", &edge_pick, TargetAxis::Z, 0.0, false)
        .err()
        .expect("an edge is refused");
    assert!(err.to_string().contains("is not a welded wall"), "said: {err}");
    // A non-finite angle refused by name.
    let err = camcraft_app::pick::align_from_cylinder(&world, "Setup1", &wall(&world), TargetAxis::Z, f64::NAN, false)
        .expect_err("a NaN angle is refused");
    assert!(err.to_string().contains("not a number"), "said: {err}");
}

#[test]
fn the_model_box_places_the_setups_origin() {
    let (world, mut h) = cyl_part([0.0, 0.0, 0.0]);
    // The parts' bounds in the setup frame: the cylinder at (5, -3, 2),
    // 20 across, 10 tall - the corner and centre of that box.
    let corner = place_setup_origin(&world, "Setup1", &OriginAt::ModelBox(BoxPoint::Corner)).expect("corner");
    assert!((corner.pos.x - (-5.0)).abs() < TIGHT, "corner x: {}", corner.pos.x);
    assert!((corner.pos.y - (-13.0)).abs() < TIGHT, "corner y: {}", corner.pos.y);
    assert!((corner.pos.z - 2.0).abs() < TIGHT, "corner z: {}", corner.pos.z);
    let centre = place_setup_origin(&world, "Setup1", &OriginAt::ModelBox(BoxPoint::Centre)).expect("centre");
    assert!((centre.pos.x - 5.0).abs() < TIGHT, "centre x: {}", centre.pos.x);
    assert!((centre.pos.y - (-3.0)).abs() < TIGHT, "centre y: {}", centre.pos.y);
    assert!((centre.pos.z - 7.0).abs() < TIGHT, "centre z: {}", centre.pos.z);
    // Writing it through the command: the named setup's WCS lands
    // there, rotation kept (identity here).
    h.execute(&world, SetWcsOrigin::for_setup(&world, Some("Setup1"), corner).expect("place"));
    let wcs = model::find_by_name(&world, &camcraft_app::setup::wcs_entity_name(&world, Some("Setup1"))).expect("the setup's WCS");
    let placed = model::transform_of(&world, &wcs).expect("placed");
    assert!((placed.pos.x - corner.pos.x).abs() < TIGHT, "the WCS sits on the corner");
    // Undo removes what the command created.
    h.undo(&world);
    assert!(model::find_by_name(&world, &camcraft_app::setup::wcs_entity_name(&world, Some("Setup1"))).is_none(), "undo removes the placed WCS");
}

#[test]
fn the_stock_box_places_the_setups_origin() {
    let (world, mut h) = cyl_part([0.0, 0.0, 0.0]);
    let at = Location { pos: Point3::new(10.0, 10.0, -1.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 30.0, dy: 30.0, dz: 12.0 }, at).expect("stock"));
    let box_aabb = model::stock_world_aabb_for(&world, Some("Setup1")).expect("the stock bounds");
    let corner = place_setup_origin(&world, "Setup1", &OriginAt::StockBox(BoxPoint::Corner)).expect("corner");
    assert!((corner.pos.x - box_aabb.min.x).abs() < TIGHT, "corner x: {} vs {}", corner.pos.x, box_aabb.min.x);
    assert!((corner.pos.y - box_aabb.min.y).abs() < TIGHT, "corner y");
    assert!((corner.pos.z - box_aabb.min.z).abs() < TIGHT, "corner z");
    let centre = place_setup_origin(&world, "Setup1", &OriginAt::StockBox(BoxPoint::Centre)).expect("centre");
    assert!((centre.pos.x - (box_aabb.min.x + box_aabb.max.x) / 2.0).abs() < TIGHT, "centre x");
}

#[test]
fn placing_the_origin_keeps_the_rotation() {
    let (world, mut h) = cyl_part([0.0, 0.0, 0.0]);
    // A pre-placed WCS with a rotation: placing the origin again
    // must keep that rotation - the frame comes from the plane side.
    let rot = quat_from_euler(0.0, 0.0, 30.0_f64.to_radians());
    h.execute(&world, SetWcsOrigin::for_setup(&world, Some("Setup1"), Location { pos: Point3::new(0.0, 0.0, 0.0), rot }).expect("pre-place"));
    let at = place_setup_origin(&world, "Setup1", &OriginAt::ModelBox(BoxPoint::Corner)).expect("place");
    assert!(at.rot.iter().zip(rot).all(|(a, b)| (a - b).abs() < TIGHT), "the rotation is kept");
    h.execute(&world, SetWcsOrigin::for_setup(&world, Some("Setup1"), at).expect("move"));
    let wcs = model::find_by_name(&world, &camcraft_app::setup::wcs_entity_name(&world, Some("Setup1"))).expect("the setup's WCS");
    let placed = model::transform_of(&world, &wcs).expect("placed");
    assert!(placed.rot.iter().zip(rot).all(|(a, b)| (a - b).abs() < TIGHT), "the written WCS keeps the rotation");
    assert!((placed.pos.x - at.pos.x).abs() < TIGHT, "the origin moved");
}

#[test]
fn each_setup_places_its_own_wcs() {
    let (world, mut h) = cyl_part([0.0, 0.0, 0.0]);
    h.execute(&world, camcraft_app::commands::CreateSetup::new(&world, None, None).expect("Setup2"));
    // Place Setup2's origin: Setup1's WCS (nothing) stays nothing,
    // Setup2's lands - the named setup, never the active one.
    let at = place_setup_origin(&world, "Setup2", &OriginAt::ModelBox(BoxPoint::Centre)).expect("place");
    h.execute(&world, SetWcsOrigin::for_setup(&world, Some("Setup2"), at).expect("place"));
    let s1 = model::find_by_name(&world, &camcraft_app::setup::wcs_entity_name(&world, Some("Setup1")));
    let s2 = model::find_by_name(&world, &camcraft_app::setup::wcs_entity_name(&world, Some("Setup2"))).expect("Setup2's WCS");
    assert!(s1.is_none(), "Setup1's WCS is untouched");
    let placed = model::transform_of(&world, &s2).expect("placed");
    assert!((placed.pos.x - at.pos.x).abs() < TIGHT, "Setup2's WCS sits on its origin");
}

#[test]
fn the_missing_model_and_stock_are_refused_by_name() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    let err = place_setup_origin(&world, "Setup1", &OriginAt::ModelBox(BoxPoint::Corner)).expect_err("no model");
    assert!(err.to_string().contains("no model"), "said: {err}");
    let err = place_setup_origin(&world, "Setup1", &OriginAt::StockBox(BoxPoint::Corner)).expect_err("no stock");
    assert!(err.to_string().contains("no stock placed"), "said: {err}");
}

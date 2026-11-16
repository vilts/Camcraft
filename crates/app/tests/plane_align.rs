//! #330: lay the part on a G17 / G18 / G19 plane from a pick - one flat
//! face, its outward normal onto the plane's axis (#554: a face only; an
//! edge cannot set a plane and is refused by name) - written as the
//! setup's part alignment (#326) through the
//! undoable `AlignPartToSetup`. Pinned on a square block and on one tilted
//! about all three axes, with the flip, the place-on-level choice, undo,
//! and every refusal by name. The work offset is never touched.
//! Provenance: our own fixtures over plain datum-face practice; no vendor
//! material consulted.
//!
//! Gate map (#330): covers app::pick::plane, app::pick::align (shared
//! readers), app::commands (AlignPartToSetup::lay_on_plane).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::ArcPlane;
use camcraft_app::commands::{AlignPartToSetup, CreateSetup, History, ImportModel, MoveModel, SetStock};
use camcraft_app::geometry::{Point3, cube, cylinder, feature, quat_from_euler, quat_rotate_point, transform_point};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{PlaneLay, lay_on_plane};
use camcraft_app::schema::Location;
use camcraft_app::setup_align;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

const TIGHT: f64 = 1e-9;

/// A 20 x 20 x 10 block at the origin, optionally tilted (degrees about
/// X, Y, Z), with Setup1.
fn block(tilt: [f64; 3]) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("block", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0))));
    if tilt != [0.0; 3] {
        let [x, y, z] = tilt.map(f64::to_radians);
        let loc = Location { pos: Point3::new(5.0, -3.0, 2.0), rot: quat_from_euler(x, y, z) };
        h.execute(&world, MoveModel::new(&world, "block", loc).expect("tilt the block"));
    }
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    (world, h)
}

fn features(world: &World, name: &str) -> std::sync::Arc<feature::Features> {
    let e = model::find_by_name(world, name).expect("entity");
    model::features_of(world, &e).expect("features")
}

/// A face by a point on its LOCAL plane (ids do not move with the part).
fn face(world: &World, p: [f64; 3]) -> GeoSel {
    let f = features(world, "block");
    let id = f
        .faces
        .iter()
        .find(|f| (f.normal[0] * p[0] + f.normal[1] * p[1] + f.normal[2] * p[2] - f.offset).abs() < 1e-9)
        .map(|f| f.id)
        .expect("the block has that face");
    GeoSel { entity: "block".into(), feature: FeatureSel::Face(id) }
}

/// An edge by its LOCAL ends, and its walk order (start, end).
fn edge(world: &World, a: [f64; 3], b: [f64; 3]) -> (GeoSel, [f64; 3], [f64; 3]) {
    let f = features(world, "block");
    let (id, p) = feature::chains(&f)
        .into_iter()
        .find_map(|c| {
            let p = feature::chain_points(&f, &c);
            (p.len() == 2 && ((p[0] == a && p[1] == b) || (p[0] == b && p[1] == a))).then_some((c.id, p))
        })
        .expect("the block has that edge");
    (GeoSel { entity: "block".into(), feature: FeatureSel::Chain(id) }, p[0], p[1])
}

fn lay(plane: ArcPlane, flip: bool, place: bool) -> PlaneLay {
    PlaneLay { plane, flip, place }
}

/// A local direction of the block as the alignment `a` lays it.
fn laid_dir(world: &World, a: Location, local: [f64; 3]) -> [f64; 3] {
    let e = model::find_by_name(world, "block").expect("block");
    let loc = setup_align::compose(a, model::transform_of(world, &e).unwrap_or_default());
    let v = quat_rotate_point(loc.rot, Point3::new(local[0], local[1], local[2]));
    [v.x, v.y, v.z]
}

fn laid_point(world: &World, a: Location, local: [f64; 3]) -> [f64; 3] {
    let e = model::find_by_name(world, "block").expect("block");
    let loc = setup_align::compose(a, model::transform_of(world, &e).unwrap_or_default());
    let v = transform_point(Point3::new(local[0], local[1], local[2]), loc.pos, loc.rot);
    [v.x, v.y, v.z]
}

fn along(v: [f64; 3], axis: [f64; 3]) -> f64 {
    v[0] * axis[0] + v[1] * axis[1] + v[2] * axis[2]
}

/// `v` (unit) lies within a tight angle (1e-9 rad, by the sine) of `axis`.
fn on_axis(v: [f64; 3], axis: [f64; 3]) -> bool {
    let c = [v[1] * axis[2] - v[2] * axis[1], v[2] * axis[0] - v[0] * axis[2], v[0] * axis[1] - v[1] * axis[0]];
    along(v, axis) > 0.0 && along(c, c).sqrt() <= TIGHT
}

const X: [f64; 3] = [1.0, 0.0, 0.0];
const Y: [f64; 3] = [0.0, 1.0, 0.0];
const Z: [f64; 3] = [0.0, 0.0, 1.0];
const NZ: [f64; 3] = [0.0, 0.0, -1.0];

#[test]
fn a_face_lands_its_outward_normal_on_each_plane_axis() {
    for tilt in [[0.0; 3], [20.0, 35.0, -50.0]] {
        let (w, _) = block(tilt);
        // The front wall (y = 0, outward -Y) and the bottom (outward -Z).
        for (pick, normal) in [([10.0, 0.0, 5.0], [0.0, -1.0, 0.0]), ([10.0, 10.0, 0.0], NZ)] {
            for (plane, axis) in [(ArcPlane::Xy, Z), (ArcPlane::Xz, Y), (ArcPlane::Yz, X)] {
                let a = lay_on_plane(&w, &[face(&w, pick)], lay(plane, false, false)).expect("laid");
                let n = laid_dir(&w, a, normal);
                assert!(on_axis(n, axis), "tilt {tilt:?}, {plane:?}: normal {n:?}");
                let f = lay_on_plane(&w, &[face(&w, pick)], lay(plane, true, false)).expect("flipped");
                let n = laid_dir(&w, f, normal);
                assert!(on_axis(n, axis.map(|v| -v)), "tilt {tilt:?}, {plane:?} flipped: normal {n:?}");
            }
        }
    }
}

#[test]
fn the_bottom_onto_g17_is_a_flip_over_x_about_the_centre() {
    let (w, _) = block([0.0; 3]);
    let a = lay_on_plane(&w, &[face(&w, [10.0, 10.0, 0.0])], lay(ArcPlane::Xy, false, false)).expect("laid");
    // The antiparallel case turns a half turn about X (G17's first axis).
    assert!(on_axis(laid_dir(&w, a, X), X), "X is the hinge");
    assert!(on_axis(laid_dir(&w, a, Y), [0.0, -1.0, 0.0]));
    // About the centre: the block's middle stays put.
    let c = laid_point(&w, a, [10.0, 10.0, 5.0]);
    assert!(c.iter().zip([10.0, 10.0, 5.0]).all(|(p, q)| (p - q).abs() < TIGHT), "{c:?}");
}

#[test]
fn place_on_level_sets_the_face_onto_the_plane() {
    for tilt in [[0.0; 3], [20.0, 35.0, -50.0]] {
        let (w, _) = block(tilt);
        for (plane, k) in [(ArcPlane::Xy, 2), (ArcPlane::Xz, 1), (ArcPlane::Yz, 0)] {
            for flip in [false, true] {
                let a = lay_on_plane(&w, &[face(&w, [10.0, 0.0, 5.0])], lay(plane, flip, true)).expect("laid");
                // Every corner of the front wall now sits at level 0.
                for p in [[0.0, 0.0, 0.0], [20.0, 0.0, 0.0], [20.0, 0.0, 10.0], [0.0, 0.0, 10.0]] {
                    let q = laid_point(&w, a, p);
                    assert!(q[k].abs() < 1e-7, "tilt {tilt:?} {plane:?} flip {flip}: {q:?}");
                }
            }
        }
    }
}

#[test]
fn the_command_writes_the_setup_alignment_and_undo_restores_it() {
    let (w, mut h) = block([0.0; 3]);
    let before = setup_align::stored(&w, "Setup1");
    assert_eq!(before, None);
    let picks = [face(&w, [10.0, 0.0, 5.0])];
    let cmd = AlignPartToSetup::lay_on_plane(&w, "Setup1", &picks, lay(ArcPlane::Xy, false, true));
    h.execute(&w, cmd.expect("lays the wall on G17"));
    let held = setup_align::stored(&w, "Setup1").expect("an alignment is held");
    assert!(on_axis(laid_dir(&w, held, [0.0, -1.0, 0.0]), Z));
    assert_eq!(camcraft_app::setup::work_offset(&w, "Setup1"), None, "the work offset is untouched");
    // #738: composed on the held pose, the same pick again is a zero step.
    let again = AlignPartToSetup::lay_on_plane(&w, "Setup1", &picks, lay(ArcPlane::Xy, false, true));
    assert!(again.is_err_and(|e| e.to_string().contains("already holds the part that way")));
    assert!(h.undo(&w));
    assert_eq!(setup_align::stored(&w, "Setup1"), before, "undo restores as modelled");
    assert!(h.redo(&w));
    assert_eq!(setup_align::stored(&w, "Setup1"), Some(held), "redo lays it again");
}

#[test]
fn a_second_setup_lays_a_different_face_down() {
    let (w, mut h) = block([0.0; 3]);
    h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup2"));
    let wall = AlignPartToSetup::lay_on_plane(&w, "Setup2", &[face(&w, [10.0, 0.0, 5.0])], lay(ArcPlane::Xy, true, true));
    h.execute(&w, wall.expect("Setup2 lays the wall down"));
    assert_eq!(setup_align::stored(&w, "Setup1"), None, "Setup1 keeps the part as modelled");
    let s2 = setup_align::stored(&w, "Setup2").expect("Setup2 is aligned");
    assert!(on_axis(laid_dir(&w, s2, [0.0, -1.0, 0.0]), NZ), "the wall faces the table");
}

#[test]
fn the_top_face_onto_g17_is_already_how_the_part_sits() {
    let (w, _) = block([0.0; 3]);
    let top = [face(&w, [10.0, 10.0, 10.0])];
    let cmd = AlignPartToSetup::lay_on_plane(&w, "Setup1", &top, lay(ArcPlane::Xy, false, false));
    assert!(cmd.is_err_and(|e| e.to_string().contains("already holds the part that way")));
}

fn says(w: &World, picks: &[GeoSel], want: &str) {
    let e = lay_on_plane(w, picks, lay(ArcPlane::Xy, false, false)).expect_err("must refuse");
    assert!(e.contains(want), "{picks:?}: '{e}' should say '{want}'");
    let c = AlignPartToSetup::lay_on_plane(w, "Setup1", picks, lay(ArcPlane::Xy, false, false));
    assert!(c.is_err(), "the command refuses too");
    assert_eq!(setup_align::stored(w, "Setup1"), None, "a refusal writes nothing");
}

#[test]
fn every_pick_that_sets_no_plane_is_refused_by_name() {
    let (w, mut h) = block([0.0; 3]);
    let top = face(&w, [10.0, 10.0, 10.0]);
    let (front, _, _) = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    let (back, _, _) = edge(&w, [0.0, 20.0, 10.0], [20.0, 20.0, 10.0]);
    says(&w, &[], "pick one flat face");
    says(&w, &[top.clone(), top.clone()], "pick one flat face");
    // #554: an edge never sets a plane - alone, paired, or beside a face.
    says(&w, &[top.clone(), front.clone()], "an edge cannot set a plane");
    says(&w, std::slice::from_ref(&front), "an edge cannot set a plane");
    says(&w, &[front.clone(), back], "an edge cannot set a plane");
    let f = features(&w, "block");
    let vid = feature::corner_verts(&f).first().and_then(|&vi| feature::vertex_id(&f, vi)).expect("a corner");
    says(&w, &[GeoSel { entity: "block".into(), feature: FeatureSel::Vertex(vid) }], "is a vertex");
    // A pick on the stock is not the model.
    let at = Location { pos: Point3::new(10.0, 10.0, -1.0), ..Default::default() };
    h.execute(&w, SetStock::new(&w, StockShape::Box { dx: 30.0, dy: 30.0, dz: 12.0 }, at).expect("stock"));
    let stock = camcraft_app::setup::stock_entity_name(&w, Some("Setup1"));
    let on_stock = GeoSel { entity: stock, feature: top.feature };
    says(&w, &[on_stock], "is not the part");
    says(&w, &[GeoSel { entity: "nothing".into(), feature: top.feature }], "no longer in the model");
}

#[test]
fn a_curved_face_and_a_rim_edge_are_refused_by_name() {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, ImportModel::from_mesh("block", cylinder(20.0, 10.0, 48)));
    h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
    let f = features(&w, "block");
    let wall = feature::patches(&f).first().map(|p| p.id).expect("the round wall welds into a patch");
    says(&w, &[GeoSel { entity: "block".into(), feature: FeatureSel::Patch(wall) }], "is curved");
    let rims: Vec<_> = feature::chains(&f).into_iter().filter(|c| c.closed).map(|c| c.id).take(2).collect();
    assert_eq!(rims.len(), 2, "the cylinder has two rims");
    let picks: Vec<GeoSel> = rims.iter().map(|&id| GeoSel { entity: "block".into(), feature: FeatureSel::Chain(id) }).collect();
    says(&w, &picks, "an edge cannot set a plane");
}

/// #330 (reopened): the maintainer's case through the command - a block
/// turned 30 degrees about X, its tilted top laid face down (flip) on each
/// plane with "face to level": the face's outward normal is the plane's
/// negative axis, the face lies at level 0 and the whole part stands on
/// the positive side. The part is TURNED, not only moved; undo restores.
#[test]
fn a_tilted_face_is_turned_flat_onto_each_plane_with_the_part_above_it() {
    let (w, mut h) = block([30.0, 0.0, 0.0]);
    let top = [face(&w, [10.0, 10.0, 10.0])];
    let corners: Vec<[f64; 3]> =
        (0..8).map(|i| [20.0 * f64::from(i & 1), 20.0 * f64::from((i >> 1) & 1), 10.0 * f64::from((i >> 2) & 1)]).collect();
    for (plane, k) in [(ArcPlane::Xy, 2), (ArcPlane::Xz, 1), (ArcPlane::Yz, 0)] {
        let cmd = AlignPartToSetup::lay_on_plane(&w, "Setup1", &top, lay(plane, true, true));
        h.execute(&w, cmd.expect("lays the tilted top"));
        let a = setup_align::stored(&w, "Setup1").expect("an alignment is held");
        let mut axis = [0.0; 3];
        axis[k] = -1.0;
        assert!(on_axis(laid_dir(&w, a, Z), axis), "{plane:?}: the top faces {axis:?}");
        for p in [[0.0, 0.0, 10.0], [20.0, 0.0, 10.0], [20.0, 20.0, 10.0], [0.0, 20.0, 10.0]] {
            assert!(laid_point(&w, a, p)[k].abs() < 1e-7, "{plane:?}: the top lies at level 0");
        }
        assert!(corners.iter().all(|&p| laid_point(&w, a, p)[k] > -1e-7), "{plane:?}: the part stands above");
        assert!(h.undo(&w));
        assert_eq!(setup_align::stored(&w, "Setup1"), None, "{plane:?}: undo restores as modelled");
    }
}

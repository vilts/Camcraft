//! #555: lay the part on a G17 / G18 / G19 plane through three picked
//! points - their plane's outside (away from the material) onto the
//! plane's axis, point 1 onto the level, flip available - through the
//! face pick's own apply path (#330), so three corners of a face and the
//! face itself give the same alignment. The optional 3-2-1 frame puts
//! point 1 at the origin and 1 -> 2 along +X. Written through the
//! undoable `AlignPartToSetup`; pinned on a 20 x 20 x 10 block tilted
//! about all three axes, with undo/redo, stale ops, the following stock
//! and every refusal by name.
//! Provenance: our own fixtures over the 3-2-1 datum practice of fixture
//! design; no vendor material consulted.
//!
//! Gate map (#555): covers app::pick::points, app::pick::plane (the shared
//! lay_datum), app::commands (AlignPartToSetup::lay_on_points,
//! check_points).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::ArcPlane;
use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, MoveModel,
    SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, cube, feature, quat_from_euler, quat_rotate_point, transform_point};
use camcraft_app::model;
use camcraft_app::operation::{self, Carried, FeatureSel, GeoSel, Height, HeightReference, KindInfo, OpStatus, OpType, SrcRef};
use camcraft_app::pick::{PlaneLay, PointsLay, lay_on_plane, lay_on_points, points_side};
use camcraft_app::schema::Location;
use camcraft_app::setup_align;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

const TIGHT: f64 = 1e-9;
const X: [f64; 3] = [1.0, 0.0, 0.0];
const Y: [f64; 3] = [0.0, 1.0, 0.0];
const Z: [f64; 3] = [0.0, 0.0, 1.0];
const TILT: [f64; 3] = [20.0, 35.0, -50.0];
/// The top face's corners (local z = 10).
const TOP: [[f64; 3]; 4] = [[0.0, 0.0, 10.0], [20.0, 0.0, 10.0], [20.0, 20.0, 10.0], [0.0, 20.0, 10.0]];

/// A 20 x 20 x 10 block at the origin, placed by `tilt` (degrees about
/// X, Y, Z) at (5, -3, 2), with Setup1.
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

fn features(world: &World) -> std::sync::Arc<feature::Features> {
    let e = model::find_by_name(world, "block").expect("entity");
    model::features_of(world, &e).expect("features")
}

/// A corner by its LOCAL position.
fn corner(world: &World, p: [f64; 3]) -> GeoSel {
    let f = features(world);
    let id = feature::corner_verts(&f)
        .into_iter()
        .find(|&vi| f.verts.get(vi as usize).is_some_and(|v| *v == p))
        .and_then(|vi| feature::vertex_id(&f, vi))
        .expect("the block has that corner");
    GeoSel { entity: "block".into(), feature: FeatureSel::Vertex(id) }
}

fn corners(world: &World, pts: &[[f64; 3]]) -> Vec<GeoSel> {
    pts.iter().map(|p| corner(world, *p)).collect()
}

/// A face by a point on its LOCAL plane.
fn face(world: &World, p: [f64; 3]) -> GeoSel {
    let f = features(world);
    let id = f
        .faces
        .iter()
        .find(|f| (f.normal[0] * p[0] + f.normal[1] * p[1] + f.normal[2] * p[2] - f.offset).abs() < 1e-9)
        .map(|f| f.id)
        .expect("the block has that face");
    GeoSel { entity: "block".into(), feature: FeatureSel::Face(id) }
}

fn pl(flip: bool, place: bool, frame: bool) -> PointsLay {
    PointsLay { lay: PlaneLay { plane: ArcPlane::Xy, flip, place }, frame }
}

fn held(world: &World, a: Location) -> Location {
    let e = model::find_by_name(world, "block").expect("block");
    setup_align::compose(a, model::transform_of(world, &e).unwrap_or_default())
}

fn laid_dir(world: &World, a: Location, local: [f64; 3]) -> [f64; 3] {
    let v = quat_rotate_point(held(world, a).rot, Point3::new(local[0], local[1], local[2]));
    let l = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    [v.x / l, v.y / l, v.z / l]
}

fn laid_point(world: &World, a: Location, local: [f64; 3]) -> [f64; 3] {
    let loc = held(world, a);
    let v = transform_point(Point3::new(local[0], local[1], local[2]), loc.pos, loc.rot);
    [v.x, v.y, v.z]
}

/// The block's eight local corners.
fn all_corners() -> Vec<[f64; 3]> {
    (0..8).map(|i| [f64::from(i & 1) * 20.0, f64::from((i >> 1) & 1) * 20.0, f64::from(i >> 2) * 10.0]).collect()
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn on_axis(v: [f64; 3], axis: [f64; 3]) -> bool {
    let c = [v[1] * axis[2] - v[2] * axis[1], v[2] * axis[0] - v[0] * axis[2], v[0] * axis[1] - v[1] * axis[0]];
    dot(v, axis) > 0.0 && dot(c, c).sqrt() <= TIGHT
}

fn neg(v: [f64; 3]) -> [f64; 3] {
    v.map(|c| -c)
}

fn close(a: Location, b: Location) -> bool {
    let p = (a.pos.x - b.pos.x).abs().max((a.pos.y - b.pos.y).abs()).max((a.pos.z - b.pos.z).abs());
    let s = if dot([a.rot[0], a.rot[1], a.rot[2]], [b.rot[0], b.rot[1], b.rot[2]]) + a.rot[3] * b.rot[3] < 0.0 { -1.0 } else { 1.0 };
    let r = a.rot.iter().zip(b.rot).map(|(x, y)| (x - s * y).abs()).fold(0.0, f64::max);
    p < 1e-9 && r < 1e-12
}

#[test]
fn three_points_on_a_tilted_face_lay_it_flat_on_g17_with_the_part_above() {
    let (w, _) = block(TILT);
    // Flip: the points' outside faces the table (-Z), the part sits above.
    let a = lay_on_points(&w, &corners(&w, &TOP[..3]), pl(true, true, false)).expect("laid");
    assert!(on_axis(laid_dir(&w, a, Z), neg(Z)), "the top's normal points down: {:?}", laid_dir(&w, a, Z));
    for p in TOP {
        let q = laid_point(&w, a, p);
        assert!(q[2].abs() < 1e-9, "every corner of the face sits at Z 0: {q:?}");
    }
    for p in all_corners() {
        assert!(laid_point(&w, a, p)[2] > -1e-9, "the part stays above the plane: {p:?}");
    }
    // The pick order does not decide the side: the material does.
    let rev = lay_on_points(&w, &corners(&w, &[TOP[2], TOP[1], TOP[0]]), pl(true, true, false)).expect("reversed");
    assert!(close(a, rev), "{a:?} vs {rev:?}");
}

#[test]
fn three_points_give_the_face_to_plane_result_on_that_face() {
    let (w, _) = block(TILT);
    for (pick, pts) in [
        ([10.0, 10.0, 10.0], [TOP[0], TOP[1], TOP[3]]),
        ([10.0, 0.0, 5.0], [[0.0, 0.0, 0.0], [20.0, 0.0, 0.0], [20.0, 0.0, 10.0]]),
    ] {
        for plane in [ArcPlane::Xy, ArcPlane::Xz, ArcPlane::Yz] {
            for flip in [false, true] {
                for place in [false, true] {
                    let lay = PlaneLay { plane, flip, place };
                    let f = lay_on_plane(&w, &[face(&w, pick)], lay).expect("face");
                    let p = lay_on_points(&w, &corners(&w, &pts), PointsLay { lay, frame: false }).expect("points");
                    assert!(close(f, p), "{pick:?} {plane:?} flip {flip} place {place}: {f:?} vs {p:?}");
                }
            }
        }
    }
}

#[test]
fn flip_lays_the_points_outside_up_with_the_part_below() {
    let (w, _) = block(TILT);
    let a = lay_on_points(&w, &corners(&w, &TOP[..3]), pl(false, true, false)).expect("laid");
    assert!(on_axis(laid_dir(&w, a, Z), Z), "unflipped, the outside faces +Z");
    for p in all_corners() {
        assert!(laid_point(&w, a, p)[2] < 1e-9, "the part hangs below the plane: {p:?}");
    }
}

#[test]
fn the_frame_puts_point_1_at_the_origin_and_1_to_2_along_x() {
    let (w, _) = block(TILT);
    let a = lay_on_points(&w, &corners(&w, &TOP[..3]), pl(true, true, true)).expect("framed");
    let o = laid_point(&w, a, TOP[0]);
    assert!(o.iter().all(|c| c.abs() < 1e-9), "point 1 is the origin: {o:?}");
    assert!(on_axis(laid_dir(&w, a, X), X), "point 1 -> 2 (local +X) runs along +X");
    assert!(on_axis(laid_dir(&w, a, Z), neg(Z)), "the plane still lies on G17, outside down");
    assert!(on_axis(laid_dir(&w, a, Y), neg(Y)), "right-handed: local +Y runs along -Y");
    for p in all_corners() {
        assert!(laid_point(&w, a, p)[2] > -1e-9, "the part stays above the plane");
    }
    // G19's first axis is +Y.
    let lay = PlaneLay { plane: ArcPlane::Yz, flip: false, place: true };
    let g19 = lay_on_points(&w, &corners(&w, &[TOP[1], TOP[2], TOP[0]]), PointsLay { lay, frame: true }).expect("g19");
    assert!(on_axis(laid_dir(&w, g19, Y), Y), "1 -> 2 (local +Y) runs along +Y on G19");
    assert!(on_axis(laid_dir(&w, g19, Z), X), "the outside faces +X");
    let o = laid_point(&w, g19, TOP[1]);
    assert!(o.iter().all(|c| c.abs() < 1e-9), "point 1 is the origin: {o:?}");
}

fn says(w: &World, picks: &[GeoSel], want: &str) {
    let e = lay_on_points(w, picks, pl(true, true, false)).expect_err("must refuse");
    assert!(e.contains(want), "{picks:?}: '{e}' should say '{want}'");
    let c = AlignPartToSetup::lay_on_points(w, "Setup1", picks, pl(true, true, false));
    assert!(c.is_err_and(|e| e.to_string().contains(want)), "the command refuses by the same name");
    assert!(AlignPartToSetup::check_points(w, "Setup1", picks, pl(true, true, false)).is_err());
    assert_eq!(setup_align::stored(w, "Setup1"), None, "a refusal writes nothing");
}

#[test]
fn coincident_too_few_and_wrong_kind_picks_are_refused_by_name() {
    let (w, mut h) = block([0.0; 3]);
    let p = corner(&w, TOP[0]);
    says(&w, &[p.clone(), p.clone(), corner(&w, TOP[1])], "points 1 and 2 coincide");
    says(&w, &[corner(&w, TOP[1]), p.clone(), p.clone()], "points 2 and 3 coincide");
    says(&w, &[], "pick three points - 0 picked");
    says(&w, &corners(&w, &TOP[..2]), "pick three points - 2 picked");
    says(&w, &corners(&w, &TOP), "pick three points - 4 picked");
    says(&w, &[face(&w, [10.0, 10.0, 10.0]), p.clone(), corner(&w, TOP[1])], "is a face");
    says(&w, &[GeoSel { entity: "nothing".into(), feature: p.feature }], "no longer in the model");
    let at = Location { pos: Point3::new(10.0, 10.0, -1.0), ..Default::default() };
    h.execute(&w, SetStock::new(&w, StockShape::Box { dx: 30.0, dy: 30.0, dz: 12.0 }, at).expect("stock"));
    let stock = camcraft_app::setup::stock_entity_name(&w, Some("Setup1"));
    says(&w, &[GeoSel { entity: stock, feature: p.feature }], "is not the part");
}

/// A stepped block: a 40 x 20 x 10 slab under a 20 x 20 x 10 one. Its
/// corners (0, 0, 20), (20, 0, 10) and (40, 0, 0) lie on one line.
fn step() -> Mesh {
    let lo = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(40.0, 20.0, 10.0));
    let hi = cube(Point3::new(0.0, 0.0, 10.0), Point3::new(20.0, 20.0, 20.0));
    Mesh::from_facets(lo.facets.iter().chain(hi.facets.iter()).cloned().collect())
}

#[test]
fn collinear_points_and_no_model_are_refused_by_name() {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
    let ghost = GeoSel { entity: "block".into(), feature: FeatureSel::Vertex(Default::default()) };
    says(&w, &[ghost.clone(), ghost.clone(), ghost], "there is no part to align");
    h.execute(&w, ImportModel::from_mesh("block", step()));
    let line = corners(&w, &[[0.0, 0.0, 20.0], [20.0, 0.0, 10.0], [40.0, 0.0, 0.0]]);
    says(&w, &line, "collinear");
    says(&w, &line, "1°");
    // Off the line, the same corners span a plane.
    let fine = corners(&w, &[[0.0, 0.0, 20.0], [20.0, 0.0, 10.0], [40.0, 20.0, 0.0]]);
    assert!(lay_on_points(&w, &fine, pl(true, true, false)).is_ok());
}

#[test]
fn the_command_lays_it_and_undo_redo_walk_it() {
    let (w, mut h) = block(TILT);
    let picks = corners(&w, &TOP[..3]);
    h.execute(&w, AlignPartToSetup::lay_on_points(&w, "Setup1", &picks, pl(true, true, false)).expect("lay"));
    let laid = setup_align::stored(&w, "Setup1").expect("an alignment is held");
    assert!(on_axis(laid_dir(&w, laid, Z), neg(Z)));
    assert_eq!(camcraft_app::setup::work_offset(&w, "Setup1"), None, "the work offset is untouched");
    // #738: composed on the held pose, the same picks again are a zero step.
    let again = AlignPartToSetup::lay_on_points(&w, "Setup1", &picks, pl(true, true, false));
    assert!(again.is_err_and(|e| e.to_string().contains("already holds the part that way")));
    assert!(points_side(&w, &picks).is_ok_and(|s| s.contains("away from the material")));
    assert!(h.undo(&w));
    assert_eq!(setup_align::stored(&w, "Setup1"), None, "undo restores as modelled");
    assert!(h.redo(&w));
    assert_eq!(setup_align::stored(&w, "Setup1"), Some(laid), "redo lays it again");
}

#[test]
fn the_stock_follows_like_face_to_plane_and_ops_go_stale_then_unstale() {
    let (w, mut h) = block([0.0, 0.0, 30.0]);
    let at = Location { pos: Point3::new(5.0, -3.0, 0.0), ..Default::default() };
    h.execute(&w, SetStock::new(&w, StockShape::Box { dx: 60.0, dy: 60.0, dz: 14.0 }, at).expect("stock"));
    let tool = camcraft_app::tools::catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .map(|(_, t)| t)
        .expect("tool");
    h.execute(&w, AddTool::new(&w, "EM-0400-2FL", tool).expect("add tool"));
    let carried = Carried {
        feed: 1000.0,
        plunge: 300.0,
        climb: true,
        clearance: Height::fixed(40.0),
        stepover: 3.0,
        stepdown: 5.0,
        top: Height::of(HeightReference::TopOfModel, 0.0),
        bottom: Height::of(HeightReference::BottomOfModel, 0.0),
        retract: Height::of(HeightReference::TopOfStock, 10.0),
        tilt: None,
    };
    let seed = (KindInfo::ALL[OpType::Face.ordinal()].seed)(&carried);
    h.execute(&w, CreateOperation::new(&w, "Face1", "EM-0400-2FL", SrcRef::Stock, seed).expect("face op"));
    h.execute(&w, GenerateOperation::new(&w, "Face1").expect("generate"));
    let status = |w: &World| operation::status_of(w, &model::find_by_name(w, "Face1").expect("op"));
    let stock_now = |w: &World| {
        let name = camcraft_app::setup::stock_entity_name(w, Some("Setup1"));
        let e = model::find_by_name(w, &name).expect("stock");
        (model::stock_def_of(w, &e).expect("shape"), model::transform_of(w, &e).expect("loc"))
    };
    assert_eq!(status(&w), OpStatus::Ok);
    let before = stock_now(&w);
    // The front wall (local y = 0) by its face, then by three corners.
    let lay = PlaneLay { plane: ArcPlane::Xy, flip: true, place: true };
    h.execute(&w, AlignPartToSetup::lay_on_plane(&w, "Setup1", &[face(&w, [10.0, 0.0, 5.0])], lay).expect("face"));
    let by_face = stock_now(&w);
    assert!(h.undo(&w));
    assert_eq!(stock_now(&w), before);
    let wall = corners(&w, &[[0.0, 0.0, 0.0], [20.0, 0.0, 0.0], [20.0, 0.0, 10.0]]);
    h.execute(&w, AlignPartToSetup::lay_on_points(&w, "Setup1", &wall, PointsLay { lay, frame: false }).expect("points"));
    assert_eq!(status(&w), OpStatus::Stale, "laying the part is a moved input");
    let (shape, loc) = stock_now(&w);
    assert_ne!((shape.clone(), loc), before, "the stock follows the part");
    let (StockShape::Box { dx, dy, dz }, StockShape::Box { dx: fx, dy: fy, dz: fz }) = (&shape, &by_face.0) else {
        panic!("the box stays a box");
    };
    assert!((dx - fx).abs() < 1e-6 && (dy - fy).abs() < 1e-6 && (dz - fz).abs() < 1e-6, "the same refit as face-to-plane");
    let gap = (loc.pos.x - by_face.1.pos.x).abs().max((loc.pos.y - by_face.1.pos.y).abs()).max((loc.pos.z - by_face.1.pos.z).abs());
    assert!(gap < 1e-6, "the same stock placement as face-to-plane: {loc:?} vs {:?}", by_face.1);
    assert!(h.undo(&w));
    assert_eq!(status(&w), OpStatus::Ok, "undo un-stales it");
    assert_eq!(stock_now(&w), before, "undo puts the stock back");
}

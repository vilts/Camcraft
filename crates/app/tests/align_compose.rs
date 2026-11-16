//! #738: the Setups page's alignment moves compose. Laying the part on a
//! face, on three points, setting its axes in the plane and the page's
//! "Turn by" / preset each move the part FROM THE POSE THE SETUP HOLDS
//! NOW - never back to the import pose first - and each is one undo step
//! that restores the pose before it exactly. Pinned on a 20 x 20 x 10
//! block, where the absolute (reset-first) reading gives a visibly
//! different pose: the first move's heading or face-up would be lost.
//! Provenance: our own fixtures over plain fixture practice (a re-clamp
//! starts from how the part sits); no vendor material consulted.
//!
//! Gate map (#738): covers app::pick::plane (lay_on_plane_in),
//! app::pick::points (lay_on_points_in), app::setup_turn (turned_by,
//! held_centre), app::commands (AlignPartToSetup::lay_on_plane,
//! lay_on_points, turn_in_plane, turn_by).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::ArcPlane;
use camcraft_app::commands::{AlignPartToSetup, CreateSetup, History, ImportModel};
use camcraft_app::geometry::{Point3, cube, feature, quat_rotate_point, transform_point};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{AxisPick, InPlaneAxes, InPlaneAxis, PlaneLay, PointsLay};
use camcraft_app::schema::Location;
use camcraft_app::setup_align;
use camcraft_app::setup_turn::TurnPivot;
use camcraft_app::world::new_world;

const TIGHT: f64 = 1e-9;
const Y: [f64; 3] = [0.0, 1.0, 0.0];
const Z: [f64; 3] = [0.0, 0.0, 1.0];

/// A 20 x 20 x 10 block at the origin (as imported), with Setup1.
fn block() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("block", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0))));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    (world, h)
}

fn features(world: &World) -> std::sync::Arc<feature::Features> {
    let e = model::find_by_name(world, "block").expect("block");
    model::features_of(world, &e).expect("features")
}

/// A face by a point on its plane.
fn face(world: &World, p: [f64; 3]) -> GeoSel {
    let id = features(world)
        .faces
        .iter()
        .find(|f| (f.normal[0] * p[0] + f.normal[1] * p[1] + f.normal[2] * p[2] - f.offset).abs() < 1e-9)
        .map(|f| f.id)
        .expect("the block has that face");
    GeoSel { entity: "block".into(), feature: FeatureSel::Face(id) }
}

/// An edge by its two ends.
fn edge(world: &World, a: [f64; 3], b: [f64; 3]) -> GeoSel {
    let f = features(world);
    let id = feature::chains(&f)
        .into_iter()
        .find(|c| {
            let p = feature::chain_points(&f, c);
            p.len() == 2 && ((p[0] == a && p[1] == b) || (p[0] == b && p[1] == a))
        })
        .map(|c| c.id)
        .expect("the block has that edge");
    GeoSel { entity: "block".into(), feature: FeatureSel::Chain(id) }
}

fn corner(world: &World, p: [f64; 3]) -> GeoSel {
    let f = features(world);
    let id = feature::corner_verts(&f)
        .into_iter()
        .find(|&vi| f.verts.get(vi as usize).is_some_and(|v| *v == p))
        .and_then(|vi| feature::vertex_id(&f, vi))
        .expect("the block has that corner");
    GeoSel { entity: "block".into(), feature: FeatureSel::Vertex(id) }
}

/// A block direction as the alignment `a` holds it (the block sits at
/// the identity, so the alignment is its whole pose).
fn dir(a: Location, v: [f64; 3]) -> [f64; 3] {
    let r = quat_rotate_point(a.rot, Point3::new(v[0], v[1], v[2]));
    [r.x, r.y, r.z]
}

fn at(a: Location, p: [f64; 3]) -> [f64; 3] {
    let r = transform_point(Point3::new(p[0], p[1], p[2]), a.pos, a.rot);
    [r.x, r.y, r.z]
}

/// `v` (unit) runs along `axis`, either way.
fn along(v: [f64; 3], axis: [f64; 3]) -> bool {
    let d = v[0] * axis[0] + v[1] * axis[1] + v[2] * axis[2];
    (d.abs() - 1.0).abs() <= TIGHT
}

fn same<const N: usize>(v: [f64; N], w: [f64; N]) -> bool {
    v.iter().zip(w).all(|(a, b)| (a - b).abs() <= TIGHT)
}

/// The lowest Z of the block's corners as `a` holds them.
fn low(a: Location) -> f64 {
    (0..8)
        .map(|i| at(a, [f64::from(i & 1) * 20.0, f64::from((i >> 1) & 1) * 20.0, f64::from(i >> 2) * 10.0])[2])
        .fold(f64::INFINITY, f64::min)
}

fn held(w: &World) -> Option<Location> {
    setup_align::stored(w, "Setup1")
}

/// The axes, then a face: the face lay flips the top down FROM the turned
/// pose, so the bottom edge that the axes set along +Y still runs along Y.
/// Reset-first, the flip about X would bring it back along X. Undo walks
/// back one move at a time: the turn alone, then the import pose.
#[test]
fn a_face_lay_composes_on_the_axes_turn_and_undo_steps_back_one_move_at_a_time() {
    let (w, mut h) = block();
    let x_edge = edge(&w, [0.0, 0.0, 0.0], [20.0, 0.0, 0.0]);
    let axes = InPlaneAxes { y: Some(AxisPick { picks: vec![x_edge], flip: false }), x: None, primary: InPlaneAxis::Y };
    h.execute(&w, AlignPartToSetup::turn_in_plane(&w, "Setup1", &axes).expect("turn"));
    let first = held(&w).expect("turned");
    assert!(along(dir(first, [1.0, 0.0, 0.0]), Y), "the edge runs along Y");

    let lay = PlaneLay { plane: ArcPlane::Xy, flip: true, place: true };
    h.execute(&w, AlignPartToSetup::lay_on_plane(&w, "Setup1", &[face(&w, [10.0, 10.0, 10.0])], lay).expect("lay"));
    let second = held(&w).expect("laid");
    assert!(same(dir(second, Z), [0.0, 0.0, -1.0]), "the top faces the table: {:?}", dir(second, Z));
    assert!(along(dir(second, [1.0, 0.0, 0.0]), Y), "composed: the edge still runs along Y: {:?}", dir(second, [1.0, 0.0, 0.0]));
    assert!(at(second, [0.0, 0.0, 10.0])[2].abs() < 1e-7, "the top lies at Z 0");
    // The absolute reading of the same pick would have lost the turn.
    let reset = camcraft_app::pick::lay_on_plane(&w, &[face(&w, [10.0, 10.0, 10.0])], lay).expect("absolute");
    assert!(!along(dir(reset, [1.0, 0.0, 0.0]), Y), "reset-first is a different pose");

    assert!(h.undo(&w));
    assert_eq!(held(&w), Some(first), "one undo: back to the turn alone, exactly");
    assert!(h.undo(&w));
    assert_eq!(held(&w), None, "two undos: the import pose");
    assert!(h.redo(&w) && h.redo(&w));
    assert_eq!(held(&w), Some(second), "redo walks forward again");
}

/// A face, then three points: the front wall laid face up, then the right
/// wall's three corners laid on G19 - in the held pose the wall already
/// faces +X, so the second move only slides it to X 0 and the front wall
/// stays up. Reset-first would stand the part back on its bottom.
#[test]
fn a_three_point_lay_composes_on_a_face_lay() {
    let (w, mut h) = block();
    let up = PlaneLay { plane: ArcPlane::Xy, flip: false, place: true };
    h.execute(&w, AlignPartToSetup::lay_on_plane(&w, "Setup1", &[face(&w, [10.0, 0.0, 5.0])], up).expect("wall up"));
    let first = held(&w).expect("laid");
    assert!(same(dir(first, [0.0, -1.0, 0.0]), Z), "the front wall faces up");

    let right: Vec<GeoSel> = [[20.0, 0.0, 0.0], [20.0, 20.0, 0.0], [20.0, 0.0, 10.0]].iter().map(|&p| corner(&w, p)).collect();
    let pl = PointsLay { lay: PlaneLay { plane: ArcPlane::Yz, flip: false, place: true }, frame: false };
    h.execute(&w, AlignPartToSetup::lay_on_points(&w, "Setup1", &right, pl).expect("points"));
    let second = held(&w).expect("laid again");
    assert!(same(dir(second, [0.0, -1.0, 0.0]), Z), "composed: the front wall still faces up");
    for p in [[20.0, 0.0, 0.0], [20.0, 20.0, 0.0], [20.0, 0.0, 10.0]] {
        assert!(at(second, p)[0].abs() < 1e-7, "the right wall lies at X 0: {:?}", at(second, p));
    }
    assert!(same(second.rot, first.rot) || same(second.rot, first.rot.map(|c| -c)), "no turn: the wall faced +X already");
    // The same points again are no move at all.
    let again = AlignPartToSetup::lay_on_points(&w, "Setup1", &right, pl);
    assert!(again.is_err_and(|e| e.to_string().contains("already holds the part that way")));

    assert!(h.undo(&w));
    assert_eq!(held(&w), Some(first), "undo: back to the face lay, exactly");
    assert!(h.undo(&w));
    assert_eq!(held(&w), None, "undo: the import pose");
}

/// The page's "Turn by" (and its presets) compose too: a quarter turn
/// after a face lay keeps the face up and turns the part about Z by 90
/// from there; undo restores the lay exactly.
#[test]
fn a_turn_by_composes_on_a_face_lay() {
    let (w, mut h) = block();
    let up = PlaneLay { plane: ArcPlane::Xy, flip: false, place: true };
    h.execute(&w, AlignPartToSetup::lay_on_plane(&w, "Setup1", &[face(&w, [10.0, 0.0, 5.0])], up).expect("wall up"));
    let first = held(&w).expect("laid");
    for pivot in [TurnPivot::PartCentre, TurnPivot::WcsOrigin] {
        h.execute(&w, AlignPartToSetup::turn_by(&w, "Setup1", [0.0, 0.0, 90.0], pivot).expect("turn by"));
        let second = held(&w).expect("turned");
        assert!(same(dir(second, [0.0, -1.0, 0.0]), Z), "{pivot:?}: the front wall still faces up");
        assert!(along(dir(second, [1.0, 0.0, 0.0]), Y), "{pivot:?}: X turned a quarter onto Y");
        assert!((low(second) - low(first)).abs() < 1e-7, "{pivot:?}: a turn about Z keeps the part's level");
        assert!(h.undo(&w));
        assert_eq!(held(&w), Some(first), "{pivot:?}: undo restores the lay exactly");
    }
    let none = AlignPartToSetup::turn_by(&w, "Setup1", [0.0; 3], TurnPivot::PartCentre);
    assert!(none.is_err_and(|e| e.to_string().contains("already holds the part that way")), "a zero turn moves nothing");
    let nan = AlignPartToSetup::turn_by(&w, "Setup1", [f64::NAN, 0.0, 0.0], TurnPivot::PartCentre);
    assert!(nan.is_err_and(|e| e.to_string().contains("finite")));
}

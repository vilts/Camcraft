//! #557: turn the part in the work plane from picks - a straight edge or
//! two points set the setup's +Y (and/or +X), projected into the setup's
//! XY, flip available, Y primary when both are given (X made
//! perpendicular), the missing axis right-handed - written as the setup's
//! part alignment through the undoable `AlignPartToSetup`, composed after
//! a face-to-plane lay (#330). Pinned on a 20 x 20 x 10 block turned 30
//! degrees about Z (and one tilted out of the plane), with undo/redo,
//! stale ops and every refusal by name.
//! Provenance: our own fixtures over plain fixture practice (indicate an
//! edge along the table's Y); no vendor material consulted.
//!
//! Gate map (#557): covers app::pick::axis, app::commands
//! (AlignPartToSetup::turn_in_plane, check_turn).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::ArcPlane;
use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, MoveModel,
    SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube, cylinder, feature, quat_from_euler, quat_rotate_point, transform_point};
use camcraft_app::model;
use camcraft_app::operation::{self, Carried, FeatureSel, GeoSel, Height, HeightReference, KindInfo, OpStatus, OpType, SrcRef};
use camcraft_app::pick::{AxisPick, InPlaneAxes, InPlaneAxis, PlaneLay, in_plane_turn_deg, turn_in_plane};
use camcraft_app::schema::Location;
use camcraft_app::setup_align;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

const TIGHT: f64 = 1e-9;
const X: [f64; 3] = [1.0, 0.0, 0.0];
const Y: [f64; 3] = [0.0, 1.0, 0.0];
const Z: [f64; 3] = [0.0, 0.0, 1.0];

/// A 20 x 20 x 10 block at the origin placed by `rot` (degrees about X,
/// Y, Z) at (5, -3, 2), with Setup1.
fn block(rot: [f64; 3]) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("block", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0))));
    let [x, y, z] = rot.map(f64::to_radians);
    let loc = Location { pos: Point3::new(5.0, -3.0, 2.0), rot: quat_from_euler(x, y, z) };
    h.execute(&world, MoveModel::new(&world, "block", loc).expect("place the block"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    (world, h)
}

fn features(world: &World, name: &str) -> std::sync::Arc<feature::Features> {
    let e = model::find_by_name(world, name).expect("entity");
    model::features_of(world, &e).expect("features")
}

/// An edge by its LOCAL ends, and its local walk direction.
fn edge(world: &World, a: [f64; 3], b: [f64; 3]) -> (GeoSel, [f64; 3]) {
    let f = features(world, "block");
    let (id, p) = feature::chains(&f)
        .into_iter()
        .find_map(|c| {
            let p = feature::chain_points(&f, &c);
            (p.len() == 2 && ((p[0] == a && p[1] == b) || (p[0] == b && p[1] == a))).then_some((c.id, p))
        })
        .expect("the block has that edge");
    let d = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
    (GeoSel { entity: "block".into(), feature: FeatureSel::Chain(id) }, d)
}

/// A corner by its LOCAL position.
fn corner(world: &World, p: [f64; 3]) -> GeoSel {
    let f = features(world, "block");
    let id = feature::corner_verts(&f)
        .into_iter()
        .find(|&vi| f.verts.get(vi as usize).is_some_and(|v| *v == p))
        .and_then(|vi| feature::vertex_id(&f, vi))
        .expect("the block has that corner");
    GeoSel { entity: "block".into(), feature: FeatureSel::Vertex(id) }
}

fn axis(picks: Vec<GeoSel>, flip: bool) -> Option<AxisPick> {
    Some(AxisPick { picks, flip })
}

fn y_only(picks: Vec<GeoSel>, flip: bool) -> InPlaneAxes {
    InPlaneAxes { y: axis(picks, flip), x: None, primary: InPlaneAxis::Y }
}

fn x_only(picks: Vec<GeoSel>, flip: bool) -> InPlaneAxes {
    InPlaneAxes { y: None, x: axis(picks, flip), primary: InPlaneAxis::Y }
}

/// A local direction of the block as the alignment `a` holds it.
fn laid_dir(world: &World, a: Location, local: [f64; 3]) -> [f64; 3] {
    let e = model::find_by_name(world, "block").expect("block");
    let loc = setup_align::compose(a, model::transform_of(world, &e).unwrap_or_default());
    let v = quat_rotate_point(loc.rot, Point3::new(local[0], local[1], local[2]));
    let l = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    [v.x / l, v.y / l, v.z / l]
}

fn laid_point(world: &World, a: Location, local: [f64; 3]) -> [f64; 3] {
    let e = model::find_by_name(world, "block").expect("block");
    let loc = setup_align::compose(a, model::transform_of(world, &e).unwrap_or_default());
    let v = transform_point(Point3::new(local[0], local[1], local[2]), loc.pos, loc.rot);
    [v.x, v.y, v.z]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `v` (unit) lies within 1e-9 rad (by the sine) of `axis`.
fn on_axis(v: [f64; 3], axis: [f64; 3]) -> bool {
    let c = [v[1] * axis[2] - v[2] * axis[1], v[2] * axis[0] - v[0] * axis[2], v[0] * axis[1] - v[1] * axis[0]];
    dot(v, axis) > 0.0 && dot(c, c).sqrt() <= TIGHT
}

fn neg(v: [f64; 3]) -> [f64; 3] {
    v.map(|c| -c)
}

#[test]
fn an_edge_at_30_degrees_picked_as_y_runs_along_plus_y() {
    let (w, _) = block([0.0, 0.0, 30.0]);
    let (e, d) = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    let axes = y_only(vec![e], false);
    let a = turn_in_plane(&w, "Setup1", &axes).expect("turned");
    assert!(on_axis(laid_dir(&w, a, d), Y), "the edge runs along +Y: {:?}", laid_dir(&w, a, d));
    assert!(on_axis(laid_dir(&w, a, [0.0, 0.0, 1.0]), Z), "the top stays up");
    // The turn is about Z only, about the part's centre: the middle stays.
    let deg = in_plane_turn_deg(&w, "Setup1", &axes).expect("an angle");
    assert!((deg.abs() - 60.0).abs() < 1e-9 || (deg.abs() - 120.0).abs() < 1e-9, "{deg}");
    let before = laid_point(&w, Location::default(), [10.0, 10.0, 5.0]);
    let after = laid_point(&w, a, [10.0, 10.0, 5.0]);
    assert!(before.iter().zip(after).all(|(p, q)| (p - q).abs() < 1e-9), "{before:?} -> {after:?}");
}

#[test]
fn two_points_picked_as_x_run_along_plus_x() {
    let (w, _) = block([0.0, 0.0, 30.0]);
    let pts = vec![corner(&w, [0.0, 0.0, 0.0]), corner(&w, [20.0, 0.0, 0.0])];
    let axes = x_only(pts, false);
    assert!((in_plane_turn_deg(&w, "Setup1", &axes).expect("angle") + 30.0).abs() < 1e-9, "a -30 degree turn");
    let a = turn_in_plane(&w, "Setup1", &axes).expect("turned");
    assert!(on_axis(laid_dir(&w, a, X), X), "first point to second runs along +X");
    // One axis given: the other follows right-handed (Y = Z x X).
    assert!(on_axis(laid_dir(&w, a, Y), Y), "local +Y lands on +Y");
}

#[test]
fn flip_reverses_the_picked_sense() {
    let (w, _) = block([0.0, 0.0, 30.0]);
    let pts = vec![corner(&w, [0.0, 0.0, 0.0]), corner(&w, [20.0, 0.0, 0.0])];
    let a = turn_in_plane(&w, "Setup1", &y_only(pts.clone(), false)).expect("turned");
    assert!(on_axis(laid_dir(&w, a, X), Y));
    let f = turn_in_plane(&w, "Setup1", &y_only(pts, true)).expect("flipped");
    assert!(on_axis(laid_dir(&w, f, X), neg(Y)), "flipped, the points run along -Y");
    // Right-handed: X = Y x Z. With local +X on +Y, local -Y is on +X.
    assert!(on_axis(laid_dir(&w, a, neg(Y)), X), "X follows from Y");
}

#[test]
fn both_axes_y_primary_makes_x_perpendicular_and_x_primary_the_other_way() {
    let (w, _) = block([0.0, 0.0, 30.0]);
    let y = vec![corner(&w, [0.0, 0.0, 0.0]), corner(&w, [0.0, 20.0, 0.0])];
    // X picked along the top's diagonal: 45 degrees off local X.
    let x = vec![corner(&w, [0.0, 0.0, 10.0]), corner(&w, [20.0, 20.0, 10.0])];
    let both = InPlaneAxes { y: axis(y.clone(), false), x: axis(x.clone(), false), primary: InPlaneAxis::Y };
    let a = turn_in_plane(&w, "Setup1", &both).expect("Y primary");
    assert!(on_axis(laid_dir(&w, a, Y), Y), "Y wins exactly");
    assert!(on_axis(laid_dir(&w, a, X), X), "X is made perpendicular to it");
    let xp = InPlaneAxes { primary: InPlaneAxis::X, ..both };
    let b = turn_in_plane(&w, "Setup1", &xp).expect("X primary");
    let diag = [1.0, 1.0, 0.0].map(|v: f64| v / 2.0_f64.sqrt());
    assert!(on_axis(laid_dir(&w, b, diag), X), "X primary: the diagonal runs along +X");
    assert!(!on_axis(laid_dir(&w, b, Y), Y), "and Y is the one made perpendicular");
    // A left-handed pair and a parallel pair are refused by name.
    let left = InPlaneAxes { y: axis(y.clone(), false), x: axis(x, true), primary: InPlaneAxis::Y };
    let e = turn_in_plane(&w, "Setup1", &left).expect_err("left-handed");
    assert!(e.contains("left-handed"), "{e}");
    let (side, _) = edge(&w, [20.0, 0.0, 0.0], [20.0, 20.0, 0.0]);
    let par = InPlaneAxes { y: axis(y, false), x: axis(vec![side], false), primary: InPlaneAxis::Y };
    let e = turn_in_plane(&w, "Setup1", &par).expect_err("parallel");
    assert!(e.contains("parallel"), "{e}");
}

#[test]
fn a_tilted_pick_projects_cleanly_into_the_plane() {
    // The block leans 20 degrees about X and turns 30 about Z: its edges
    // leave the setup's XY, and the space diagonal rises steeply.
    let (w, _) = block([20.0, 0.0, 30.0]);
    let (e, d) = edge(&w, [0.0, 0.0, 0.0], [0.0, 20.0, 0.0]);
    let a = turn_in_plane(&w, "Setup1", &y_only(vec![e], false)).expect("turned");
    let v = laid_dir(&w, a, d);
    assert!(v[0].abs() < 1e-9 && v[1] > 0.0 && v[2].abs() > 0.1, "in-plane part on +Y, the rise kept: {v:?}");
    let diag = vec![corner(&w, [0.0, 0.0, 0.0]), corner(&w, [20.0, 20.0, 10.0])];
    let a = turn_in_plane(&w, "Setup1", &y_only(diag, false)).expect("turned");
    let v = laid_dir(&w, a, [20.0, 20.0, 10.0]);
    assert!(v[0].abs() < 1e-9 && v[1] > 0.0, "the diagonal's shadow runs along +Y: {v:?}");
}

fn says(w: &World, axes: &InPlaneAxes, want: &str) {
    let e = turn_in_plane(w, "Setup1", axes).expect_err("must refuse");
    assert!(e.contains(want), "{axes:?}: '{e}' should say '{want}'");
    let c = AlignPartToSetup::turn_in_plane(w, "Setup1", axes);
    assert!(c.is_err_and(|e| e.to_string().contains(want)), "the command refuses by the same name");
    assert!(AlignPartToSetup::check_turn(w, "Setup1", axes).is_err(), "the check refuses too");
    assert_eq!(setup_align::stored(w, "Setup1"), None, "a refusal writes nothing");
}

#[test]
fn near_normal_and_degenerate_picks_are_refused_by_name() {
    let (w, mut h) = block([0.0; 3]);
    let (upright, _) = edge(&w, [0.0, 0.0, 0.0], [0.0, 0.0, 10.0]);
    says(&w, &y_only(vec![upright], false), "along the work plane's normal");
    let tall = vec![corner(&w, [20.0, 0.0, 0.0]), corner(&w, [20.0, 0.0, 10.0])];
    says(&w, &x_only(tall, false), "along the work plane's normal");
    let p = corner(&w, [0.0, 0.0, 0.0]);
    says(&w, &y_only(vec![p.clone(), p.clone()], false), "coincide");
    says(&w, &y_only(vec![p.clone()], false), "is one point");
    says(&w, &y_only(vec![], false), "pick a straight edge or two points for +Y");
    says(&w, &InPlaneAxes::default(), "for +Y or +X");
    let top = features(&w, "block")
        .faces
        .iter()
        .find(|f| f.normal[2] > 0.99)
        .map(|f| f.id)
        .expect("the top");
    let face = GeoSel { entity: "block".into(), feature: FeatureSel::Face(top) };
    says(&w, &y_only(vec![face], false), "is a face");
    let (e1, _) = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    says(&w, &y_only(vec![e1, p.clone()], false), "one straight edge, or two points");
    says(&w, &y_only(vec![GeoSel { entity: "nothing".into(), feature: p.feature.clone() }], false), "no longer in the model");
    let at = Location { pos: Point3::new(10.0, 10.0, -1.0), ..Default::default() };
    h.execute(&w, SetStock::new(&w, StockShape::Box { dx: 30.0, dy: 30.0, dz: 12.0 }, at).expect("stock"));
    let stock = camcraft_app::setup::stock_entity_name(&w, Some("Setup1"));
    says(&w, &y_only(vec![GeoSel { entity: stock, feature: p.feature }], false), "is not the part");
}

/// A D-shaped bar: a half disc (radius 10, y >= 0) extruded 0..10 in Z.
/// Its top rim has an open arc chain and a straight one.
fn d_bar() -> Mesh {
    let n = 24;
    let arc: Vec<(f64, f64)> = (0..=n)
        .map(|i| {
            let t = std::f64::consts::PI * f64::from(i) / f64::from(n);
            (10.0 * t.cos(), 10.0 * t.sin())
        })
        .collect();
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut f = Vec::new();
    for k in 0..n as usize {
        let (a, b) = (arc[k], arc[k + 1]);
        // Fan from the centre: CCW from above is the top's outside (+Z).
        f.push(Triangle::new(p((0.0, 0.0), 10.0), p(a, 10.0), p(b, 10.0)));
        f.push(Triangle::new(p((0.0, 0.0), 0.0), p(b, 0.0), p(a, 0.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 0.0), p(b, 10.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 10.0), p(a, 10.0)));
    }
    // The flat side (y = 0), split at the centre like the caps' fans.
    for (a, b) in [((-10.0, 0.0), (0.0, 0.0)), ((0.0, 0.0), (10.0, 0.0))] {
        f.push(Triangle::new(p(a, 0.0), p(b, 0.0), p(b, 10.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 10.0), p(a, 10.0)));
    }
    Mesh::from_facets(f)
}

#[test]
fn a_curved_edge_or_a_closed_rim_is_refused_by_name() {
    for (mesh, want) in [(d_bar(), "is not straight"), (cylinder(20.0, 10.0, 48), "is a closed loop")] {
        let w = new_world();
        let mut h = History::default();
        h.execute(&w, ImportModel::from_mesh("block", mesh));
        h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
        let f = features(&w, "block");
        let curved = feature::chains(&f)
            .into_iter()
            .find(|c| feature::chain_points(&f, c).len() > 3)
            .map(|c| c.id)
            .expect("a curved chain");
        let g = GeoSel { entity: "block".into(), feature: FeatureSel::Chain(curved) };
        says(&w, &y_only(vec![g], false), want);
    }
}

#[test]
fn the_command_writes_the_turn_and_undo_redo_walk_it() {
    let (w, mut h) = block([0.0, 0.0, 30.0]);
    let (e, d) = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    let axes = y_only(vec![e], false);
    h.execute(&w, AlignPartToSetup::turn_in_plane(&w, "Setup1", &axes).expect("turn"));
    let held = setup_align::stored(&w, "Setup1").expect("an alignment is held");
    assert!(on_axis(laid_dir(&w, held, d), Y));
    assert_eq!(camcraft_app::setup::work_offset(&w, "Setup1"), None, "the work offset is untouched");
    // Read in the setup's frame: the same pick again changes nothing.
    let again = AlignPartToSetup::turn_in_plane(&w, "Setup1", &axes);
    assert!(again.is_err_and(|e| e.to_string().contains("already holds the part that way")));
    assert!(h.undo(&w));
    assert_eq!(setup_align::stored(&w, "Setup1"), None, "undo restores as modelled");
    assert!(h.redo(&w));
    assert_eq!(setup_align::stored(&w, "Setup1"), Some(held), "redo turns it again");
}

#[test]
fn it_composes_after_a_face_to_plane_lay() {
    let (w, mut h) = block([0.0; 3]);
    // Lay the front wall (y = 0, outward -Y) on G17: it faces up.
    let wall = features(&w, "block")
        .faces
        .iter()
        .find(|f| f.normal[1].abs() > 0.99 && f.offset.abs() < 1e-9)
        .map(|f| f.id)
        .expect("the front wall");
    let picks = [GeoSel { entity: "block".into(), feature: FeatureSel::Face(wall) }];
    let lay = PlaneLay { plane: ArcPlane::Xy, flip: false, place: true };
    h.execute(&w, AlignPartToSetup::lay_on_plane(&w, "Setup1", &picks, lay).expect("laid"));
    let laid = setup_align::stored(&w, "Setup1").expect("laid");
    // Then the wall's bottom edge (local X) as +Y.
    let (e, d) = edge(&w, [0.0, 0.0, 0.0], [20.0, 0.0, 0.0]);
    h.execute(&w, AlignPartToSetup::turn_in_plane(&w, "Setup1", &y_only(vec![e], false)).expect("turn"));
    let both = setup_align::stored(&w, "Setup1").expect("held");
    assert!(on_axis(laid_dir(&w, both, [0.0, -1.0, 0.0]), Z), "the wall still faces up");
    assert!(on_axis(laid_dir(&w, both, d), Y), "and its edge runs along +Y");
    let lo = (0..8)
        .map(|i| laid_point(&w, both, [f64::from(i & 1) * 20.0, f64::from((i >> 1) & 1) * 20.0, f64::from(i >> 2) * 10.0])[2])
        .fold(f64::INFINITY, f64::min);
    let lo_laid = (0..8)
        .map(|i| laid_point(&w, laid, [f64::from(i & 1) * 20.0, f64::from((i >> 1) & 1) * 20.0, f64::from(i >> 2) * 10.0])[2])
        .fold(f64::INFINITY, f64::min);
    assert!((lo - lo_laid).abs() < 1e-9, "a turn about Z keeps the part's level");
    assert!(h.undo(&w));
    assert_eq!(setup_align::stored(&w, "Setup1"), Some(laid), "undo returns to the lay alone");
}

#[test]
fn ops_of_the_setup_go_stale_and_undo_unstales_them() {
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
    let face = (KindInfo::ALL[OpType::Face.ordinal()].seed)(&carried);
    h.execute(&w, CreateOperation::new(&w, "Face1", "EM-0400-2FL", SrcRef::Stock, face).expect("face op"));
    h.execute(&w, GenerateOperation::new(&w, "Face1").expect("generate"));
    let status = |w: &World| operation::status_of(w, &model::find_by_name(w, "Face1").expect("op"));
    assert_eq!(status(&w), OpStatus::Ok);
    let (e, _) = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    h.execute(&w, AlignPartToSetup::turn_in_plane(&w, "Setup1", &y_only(vec![e], false)).expect("turn"));
    assert_eq!(status(&w), OpStatus::Stale, "turning the part is a moved input");
    assert!(h.undo(&w));
    assert_eq!(status(&w), OpStatus::Ok, "undo un-stales it");
}

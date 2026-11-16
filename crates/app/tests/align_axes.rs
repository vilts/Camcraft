//! #769: align the part from its two picked axes - a straight edge or
//! two points for +Y and one for +X define the full 3D frame (the
//! primary kept exactly, the other made square to it, +Z where the
//! right-hand rule puts it), written as the setup's part alignment
//! through the undoable `AlignPartToSetup`. One picked axis keeps the
//! in-plane turn (#557), pinned equal here. Pinned on a 20 x 20 x 10
//! block: the user's exact case (a +Y edge running down -Z and a +X
//! edge running -X), the part-on-its-side landing, subsumption of the
//! in-plane turn, a skewed secondary landing within its skew, a
//! left-handed pair honoured literally, and every refusal by name.
//! Provenance: our own fixtures over plain fixture practice (indicate
//! two square edges); no vendor material consulted.
//!
//! Gate map (#769): covers app::pick::axis (align_from_axes),
//! app::commands (AlignPartToSetup::align_from_axes, check_align).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{CreateSetup, History, ImportModel};
use camcraft_app::geometry::{Point3, cube, feature, quat_rotate_point, transform_point};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{AxisPick, InPlaneAxes, InPlaneAxis, align_from_axes, turn_in_plane};
use camcraft_app::schema::Location;
use camcraft_app::setup_align;
use camcraft_app::world::new_world;

const TIGHT: f64 = 1e-9;
const X: [f64; 3] = [1.0, 0.0, 0.0];
const Y: [f64; 3] = [0.0, 1.0, 0.0];
const Z: [f64; 3] = [0.0, 0.0, 1.0];

/// A 20 x 20 x 10 block at the origin, with Setup1.
fn block() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(
        &world,
        ImportModel::from_mesh(
            "block",
            cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0)),
        ),
    );
    h.execute(
        &world,
        CreateSetup::new(&world, None, None).expect("Setup1"),
    );
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
            (p.len() == 2 && ((p[0] == a && p[1] == b) || (p[0] == b && p[1] == a)))
                .then_some((c.id, p))
        })
        .expect("the block has that edge");
    let d = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
    (
        GeoSel {
            entity: "block".into(),
            feature: FeatureSel::Chain(id),
        },
        d,
    )
}

/// A corner by its LOCAL position.
fn corner(world: &World, p: [f64; 3]) -> GeoSel {
    let f = features(world, "block");
    let id = feature::corner_verts(&f)
        .into_iter()
        .find(|&vi| f.verts.get(vi as usize).is_some_and(|v| *v == p))
        .and_then(|vi| feature::vertex_id(&f, vi))
        .expect("the block has that corner");
    GeoSel {
        entity: "block".into(),
        feature: FeatureSel::Vertex(id),
    }
}

fn axis(picks: Vec<GeoSel>, flip: bool) -> Option<AxisPick> {
    Some(AxisPick { picks, flip })
}

fn both(y: Vec<GeoSel>, y_flip: bool, x: Vec<GeoSel>, x_flip: bool) -> InPlaneAxes {
    InPlaneAxes {
        y: axis(y, y_flip),
        x: axis(x, x_flip),
        primary: InPlaneAxis::Y,
    }
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

fn unit(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `v` (unit) lies within 1e-9 rad (by the sine) of `axis`.
fn on_axis(v: [f64; 3], axis: [f64; 3]) -> bool {
    let c = [
        v[1] * axis[2] - v[2] * axis[1],
        v[2] * axis[0] - v[0] * axis[2],
        v[0] * axis[1] - v[1] * axis[0],
    ];
    dot(v, axis) > 0.0 && dot(c, c).sqrt() <= TIGHT
}

fn neg(v: [f64; 3]) -> [f64; 3] {
    v.map(|c| -c)
}

/// The pick whose walked direction is `d`, flipped the other way if the
/// walk runs opposite - so the picked DIRECTION is `d` exactly.
fn pick_as(edge_sel: GeoSel, walked: [f64; 3], d: [f64; 3]) -> (Vec<GeoSel>, bool) {
    let w = unit(walked);
    let want = unit(d);
    let flip = dot(w, want) < 0.0;
    (vec![edge_sel], flip)
}

#[test]
fn the_users_case_a_down_z_edge_as_y_and_a_minus_x_edge_as_x_aligns_in_full_3d() {
    let (w, _) = block();
    // +Y: the vertical edge, picked running DOWN (0, 0, -1); +X: a top
    // edge picked running -X - exactly the reported pick round.
    let (ey, dy) = edge(&w, [0.0, 0.0, 0.0], [0.0, 0.0, 10.0]);
    let (ex, dx) = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    let (yp, yf) = pick_as(ey, dy, [0.0, 0.0, -1.0]);
    let (xp, xf) = pick_as(ex, dx, [-1.0, 0.0, 0.0]);
    let axes = both(yp, yf, xp, xf);
    let a = align_from_axes(&w, "Setup1", &axes).expect("the picks define the frame");
    assert!(
        on_axis(laid_dir(&w, a, [0.0, 0.0, -1.0]), Y),
        "the +Y pick runs along +Y: {:?}",
        laid_dir(&w, a, [0.0, 0.0, -1.0])
    );
    assert!(
        on_axis(laid_dir(&w, a, [-1.0, 0.0, 0.0]), X),
        "the +X pick runs along +X"
    );
    // The directions decide where +Z goes: the picks' cross (0, -1, 0)
    // lands UP - the face that pointed -Y is the new top.
    assert!(
        on_axis(laid_dir(&w, a, [0.0, -1.0, 0.0]), Z),
        "the picks' cross lands +Z: {:?}",
        laid_dir(&w, a, [0.0, -1.0, 0.0])
    );
    // The old top now faces -Y (the part's height runs along Y).
    assert!(
        on_axis(laid_dir(&w, a, [0.0, 0.0, 1.0]), neg(Y)),
        "the old top faces -Y"
    );
    // The part stays on its spot: the centre does not move.
    let before = laid_point(&w, Location::default(), [10.0, 10.0, 5.0]);
    let after = laid_point(&w, a, [10.0, 10.0, 5.0]);
    assert!(
        before.iter().zip(after).all(|(p, q)| (p - q).abs() < TIGHT),
        "{before:?} -> {after:?}"
    );
    // The part is no longer held as modelled - it really turned 3D.
    assert!(!setup_align::is_identity_rot(a.rot), "a real alignment");
}

#[test]
fn a_vertical_edge_as_y_lays_the_part_on_its_side() {
    let (w, _) = block();
    // +Y: the vertical edge picked UP; +X: the front top edge picked
    // +X. The old +Y face becomes the top; the old top faces -Z.
    let (ey, dy) = edge(&w, [0.0, 0.0, 0.0], [0.0, 0.0, 10.0]);
    let (ex, dx) = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    let (yp, yf) = pick_as(ey, dy, [0.0, 0.0, 1.0]);
    let (xp, xf) = pick_as(ex, dx, [1.0, 0.0, 0.0]);
    let a = align_from_axes(&w, "Setup1", &both(yp, yf, xp, xf)).expect("aligned");
    assert!(
        on_axis(laid_dir(&w, a, [0.0, 0.0, 1.0]), Y),
        "the vertical edge runs +Y"
    );
    assert!(
        on_axis(laid_dir(&w, a, [1.0, 0.0, 0.0]), X),
        "the top edge runs +X"
    );
    assert!(
        on_axis(laid_dir(&w, a, [0.0, -1.0, 0.0]), Z),
        "the old -Y face is the new top"
    );
    // The part stays on its spot: the centre does not move.
    let before = laid_point(&w, Location::default(), [10.0, 10.0, 5.0]);
    let after = laid_point(&w, a, [10.0, 10.0, 5.0]);
    assert!(
        before.iter().zip(after).all(|(p, q)| (p - q).abs() < TIGHT),
        "{before:?} -> {after:?}"
    );
}

#[test]
fn in_plane_picks_align_exactly_as_the_in_plane_turn_does() {
    let (w, _) = block();
    // Two perpendicular right-handed in-plane picks: the full 3D path
    // must give the in-plane turn's own answer (subsumption).
    let (ey, dy) = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    let (ex, dx) = edge(&w, [0.0, 0.0, 0.0], [0.0, 20.0, 0.0]);
    let (yp, yf) = pick_as(ey, dy, [1.0, 0.0, 0.0]);
    let (xp, xf) = pick_as(ex, dx, [0.0, -1.0, 0.0]);
    let axes = both(yp, yf, xp, xf);
    let a3 = align_from_axes(&w, "Setup1", &axes).expect("the 3D path");
    let a2 = turn_in_plane(&w, "Setup1", &axes).expect("the in-plane path");
    for (p, q) in [a3.pos.x, a3.pos.y, a3.pos.z]
        .iter()
        .zip([a2.pos.x, a2.pos.y, a2.pos.z])
    {
        assert!(
            (p - q).abs() < TIGHT,
            "positions match: {:?} vs {:?}",
            a3.pos,
            a2.pos
        );
    }
    // A quaternion and its negation are one rotation.
    let s = if a3.rot[3] * a2.rot[3] < 0.0 {
        -1.0
    } else {
        1.0
    };
    for i in 0..4 {
        assert!(
            (a3.rot[i] - s * a2.rot[i]).abs() < TIGHT,
            "rotations match: {:?} vs {:?}",
            a3.rot,
            a2.rot
        );
    }
    assert!(
        on_axis(laid_dir(&w, a3, [1.0, 0.0, 0.0]), Y),
        "the +Y pick still lands"
    );
    assert!(
        on_axis(laid_dir(&w, a3, [0.0, 0.0, 1.0]), Z),
        "the top stays up"
    );
}

#[test]
fn one_axis_alone_keeps_the_in_plane_turn() {
    let (w, _) = block();
    let (ey, dy) = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    let (yp, yf) = pick_as(ey, dy, [1.0, 0.0, 0.0]);
    let axes = InPlaneAxes {
        y: axis(yp, yf),
        x: None,
        primary: InPlaneAxis::Y,
    };
    let a3 = align_from_axes(&w, "Setup1", &axes).expect("falls back");
    let a2 = turn_in_plane(&w, "Setup1", &axes).expect("the in-plane path");
    assert!(
        on_axis(laid_dir(&w, a3, [1.0, 0.0, 0.0]), Y),
        "the pick lands on +Y"
    );
    assert!(
        on_axis(laid_dir(&w, a3, [0.0, 0.0, 1.0]), Z),
        "the top stays up"
    );
    for (p, q) in [a3.pos.x, a3.pos.y, a3.pos.z]
        .iter()
        .zip([a2.pos.x, a2.pos.y, a2.pos.z])
    {
        assert!((p - q).abs() < TIGHT);
    }
}

#[test]
fn a_skewed_secondary_lands_within_its_own_skew() {
    let (w, _) = block();
    // +Y: the top edge running +X. +X: two corners diagonal in the
    // top plane - 45 degrees off square to the +Y pick.
    let (ey, dy) = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    let (yp, yf) = pick_as(ey, dy, [1.0, 0.0, 0.0]);
    let xp = vec![corner(&w, [0.0, 0.0, 10.0]), corner(&w, [20.0, 20.0, 10.0])];
    let raw = unit([1.0, 1.0, 0.0]);
    let along = dot([1.0, 0.0, 0.0], raw);
    let a = align_from_axes(&w, "Setup1", &both(yp, yf, xp, false)).expect("aligned");
    assert!(
        on_axis(laid_dir(&w, a, [1.0, 0.0, 0.0]), Y),
        "the primary lands exactly"
    );
    // The secondary's landing misses its axis by exactly the skew it
    // came with (the diagonal's Y component), never more.
    let landed = laid_dir(&w, a, raw);
    let miss = (landed[1] * landed[1] + landed[2] * landed[2]).sqrt();
    assert!(
        (miss - along).abs() < TIGHT,
        "misses by its own skew: {miss} vs {along}"
    );
}

#[test]
fn parallel_picks_refuse_by_name() {
    let (w, _) = block();
    let (ey, dy) = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    let (yp, yf) = pick_as(ey, dy, [1.0, 0.0, 0.0]);
    let xp = vec![corner(&w, [0.0, 0.0, 10.0]), corner(&w, [20.0, 0.0, 10.0])];
    let err =
        align_from_axes(&w, "Setup1", &both(yp, yf, xp, false)).expect_err("parallel is no frame");
    assert!(err.contains("along each other"), "{err}");
}

#[test]
fn a_left_handed_pair_is_honoured_literally_not_refused() {
    let (w, _) = block();
    // In-plane, clockwise: the old flow refused this ("left-handed
    // frame"); the picks' law honours it - the part lands on its head,
    // both picks exactly on their axes.
    let (ey, dy) = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    let (yp, yf) = pick_as(ey, dy, [1.0, 0.0, 0.0]);
    let xp = vec![corner(&w, [0.0, 0.0, 0.0]), corner(&w, [0.0, 20.0, 0.0])];
    let a = align_from_axes(&w, "Setup1", &both(yp, yf, xp, false)).expect("honoured, not refused");
    assert!(
        on_axis(laid_dir(&w, a, [1.0, 0.0, 0.0]), Y),
        "the +Y pick lands"
    );
    assert!(
        on_axis(laid_dir(&w, a, [0.0, 1.0, 0.0]), X),
        "the +X pick lands"
    );
    assert!(
        on_axis(laid_dir(&w, a, [0.0, 0.0, 1.0]), neg(Z)),
        "the old top faces down: as picked"
    );
}

#[test]
fn picks_read_through_a_held_alignment_compose_after_it() {
    let (w, mut h) = block();
    // Hold the part tipped -90 about X through its centre (a face lay's
    // result): the old top now faces +Y, the old front faces down.
    let rot = setup_align::axis_quat(0, -90.0).expect("quarter");
    let c = setup_align::part_centre(&w).expect("centre");
    let held = setup_align::about_centre(rot, c).expect("held");
    h.execute(
        &w,
        camcraft_app::commands::AlignPartToSetup::new(&w, "Setup1", Some(held)).expect("hold"),
    );
    let stored = setup_align::stored(&w, "Setup1").expect("the hold is stored");
    // The picks, as the tipped setup sees them: the old vertical edge
    // now runs +Y - pick it the OTHER way (running -Y) so a real
    // further turn is demanded; the old top-front edge still runs +X.
    let (ey, dy) = edge(&w, [0.0, 0.0, 0.0], [0.0, 0.0, 10.0]);
    let (ex, dx) = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    let (yp, yf) = pick_as(ey, dy, [0.0, 0.0, -1.0]);
    let (xp, xf) = pick_as(ex, dx, [1.0, 0.0, 0.0]);
    let a = align_from_axes(&w, "Setup1", &both(yp, yf, xp, xf)).expect("aligned through the held");
    // The composed alignment lands the picked directions exactly.
    assert!(
        on_axis(laid_dir(&w, a, [0.0, 0.0, -1.0]), Y),
        "the +Y pick lands: {:?}",
        laid_dir(&w, a, [0.0, 0.0, -1.0])
    );
    assert!(
        on_axis(laid_dir(&w, a, [1.0, 0.0, 0.0]), X),
        "the +X pick lands"
    );
    // And it is a real further turn, not the held one re-applied.
    let same = {
        let s = if a.rot[3] * stored.rot[3] < 0.0 {
            -1.0
        } else {
            1.0
        };
        (0..4).all(|i| (a.rot[i] - s * stored.rot[i]).abs() < TIGHT)
    };
    assert!(!same, "the alignment moved on from the held one");
}

#[test]
fn a_face_pick_still_refuses_by_name() {
    let (w, _) = block();
    let (ey, dy) = edge(&w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]);
    let (yp, yf) = pick_as(ey, dy, [1.0, 0.0, 0.0]);
    let f = features(&w, "block");
    let face_id = f.faces.first().expect("a face").id;
    let xp = vec![GeoSel {
        entity: "block".into(),
        feature: FeatureSel::Face(face_id),
    }];
    let err = align_from_axes(&w, "Setup1", &both(yp, yf, xp, false))
        .expect_err("a face sets no direction");
    assert!(err.contains("a face sets no direction"), "{err}");
}

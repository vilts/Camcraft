//! #1933: 2D Contour Side Auto on a PICKED ring reads the material at the
//! ring's own level. Before the fix Auto read the full XY silhouette, so
//! the floor of a blind bore filled the rim and Auto resolved Outside -
//! refused by the #1927 law, with Inside as the workaround. Now a blind
//! bore rim and a through hole resolve Inside, a boss and an outer wall
//! resolve Outside; each with the tool centre one radius off the wall
//! and no gouge.
//!
//! Gate map (#1933): covers camcraft_app::operation (contour_auto_pick,
//! selection, generate), camcraft_motion::machining2d (contour_auto).
//!
//! Provenance: our own fixtures (common::solid_part); the 2.5D profiling
//! rule of published machining practice. No vendor material consulted.

mod common;

use common::solid_part::{P2, cap, circle, plate, rect, walls};
use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel};
use camcraft_app::geometry::feature;
use camcraft_app::geometry::{Mesh, Triangle};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, HeightReference, OpParams, OpStatus, SrcRef};
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0600-4FL";
/// The Ø6 tool's radius, mm.
const R: f64 = 3.0;
const EPS: f64 = 1e-3;

/// A 60 x 60 x [0, 10] plate with a Ø20 hole (32 chords) down to `floor`
/// (0: a through hole).
fn bored_plate(floor: f64) -> Vec<Triangle> {
    let mut f = Vec::new();
    plate(&rect(-30.0, -30.0, 30.0, 30.0), Some((&circle((0.0, 0.0), 10.0, 32), floor)), 0.0, 10.0, &mut f);
    f
}

/// A 60 x 60 x [0, 5] base with a 20 x 20 boss on it up to Z 10.
fn bossed_base() -> Vec<Triangle> {
    let (o, b) = (rect(-30.0, -30.0, 30.0, 30.0), rect(-10.0, -10.0, 10.0, 10.0));
    let mut f = Vec::new();
    walls(&o, 0.0, 5.0, true, &mut f);
    cap(&o, None, 0.0, false, &mut f);
    cap(&o, Some(&b), 5.0, true, &mut f);
    walls(&b, 5.0, 10.0, true, &mut f);
    cap(&b, None, 10.0, true, &mut f);
    f
}

/// A plain 40 x 40 x [0, 10] block.
fn block() -> Vec<Triangle> {
    let o = rect(-20.0, -20.0, 20.0, 20.0);
    let mut f = Vec::new();
    walls(&o, 0.0, 10.0, true, &mut f);
    cap(&o, None, 0.0, false, &mut f);
    cap(&o, None, 10.0, true, &mut f);
    f
}

fn world_with(facets: Vec<Triangle>) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(facets)));
    let t = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, t).expect("tool"));
    (world, h)
}

fn pick(feature: FeatureSel) -> Vec<GeoSel> {
    vec![GeoSel { entity: "part".into(), feature }]
}

/// The hole's closed rim chain at Z `z`.
fn hole_rim(world: &World, z: f64) -> Vec<GeoSel> {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let id = feature::chains(&f)
        .into_iter()
        .find(|c| {
            c.closed && feature::chain_points(&f, c).iter().all(|p| p[0].hypot(p[1]) < 10.5 && (p[2] - z).abs() < 1e-9)
        })
        .map(|c| c.id)
        .expect("the hole rim chains closed");
    pick(FeatureSel::Chain(id))
}

/// The planar face at Z `z` facing up.
fn top_face(world: &World, z: f64) -> Vec<GeoSel> {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let id = f
        .faces
        .iter()
        .find(|fa| {
            fa.normal[2] > 0.99
                && fa.loops.iter().flat_map(|l| l.verts.iter()).all(|&v| f.verts.get(v as usize).is_some_and(|p| (p[2] - z).abs() < 1e-9))
        })
        .map(|fa| fa.id)
        .expect("an up face at z");
    pick(FeatureSel::Face(id))
}

/// Side Auto, model top to model bottom.
fn auto() -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: Height::fixed(20.0),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: Height::of(HeightReference::BottomOfModel, 0.0),
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
        blend_turns: false, blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        chain_overrides: Vec::new(),
        picks: Default::default(),
    }
}

fn generate(world: &World, h: &mut History, items: Vec<GeoSel>) -> Result<(Toolpath, Vec<Warning>), String> {
    h.execute(world, CreateOperation::new(world, "C", TOOL, SrcRef::Selection(items), auto()).expect("create"));
    let g = GenerateOperation::new(world, "C").map_err(|e| e.to_string())?;
    h.execute(world, g);
    let e = model::find_by_name(world, "C").expect("op");
    assert_eq!(camcraft_app::operation::status_of(world, &e), OpStatus::Ok);
    let tp = model::toolpath_of(world, &e).expect("cached path");
    Ok((tp.path.clone(), tp.warnings.clone()))
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

/// Distance from (x, y) to the closed polygon `wall`, and whether the
/// point is inside it (even-odd).
fn to_wall(wall: &[P2], x: f64, y: f64) -> (f64, bool) {
    let (mut d, mut inside) = (f64::INFINITY, false);
    for (i, &(ax, ay)) in wall.iter().enumerate() {
        let (bx, by) = wall[(i + 1) % wall.len()];
        let (ex, ey) = (bx - ax, by - ay);
        let t = (((x - ax) * ex + (y - ay) * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
        d = d.min((x - ax - t * ex).hypot(y - ay - t * ey));
        if (ay > y) != (by > y) && x < ax + (y - ay) * ex / ey {
            inside = !inside;
        }
    }
    (d, inside)
}

/// Every feed stands one tool radius off `wall`, on the side `inside`;
/// returns the deepest feed Z.
fn rides_the_wall(tag: &str, t: &Toolpath, wall: &[P2], inside: bool) -> f64 {
    let f: Vec<_> = t.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| m.to).collect();
    assert!(!f.is_empty(), "{tag}: Auto cuts the wall");
    let (mut lo, mut worst) = (f64::INFINITY, 0.0_f64);
    for p in &f {
        let (d, side) = to_wall(wall, p.x, p.y);
        assert_eq!(side, inside, "{tag}: the centre stands on the air side: {p:?}");
        worst = worst.max((d - R).abs());
        lo = lo.min(p.z);
    }
    assert!(worst < EPS, "{tag}: centre off the wall by {worst} beyond one radius");
    lo
}

#[test]
fn auto_on_a_blind_bore_rim_cuts_the_wall_from_inside_to_the_floor() {
    let wall = circle((0.0, 0.0), 10.0, 32);
    // The mouth rim at Z 10 decides from below it; the floor rim at Z 4
    // decides from above it (metal on both sides under the floor).
    for z in [10.0, 4.0] {
        let (world, mut h) = world_with(bored_plate(4.0));
        let items = hole_rim(&world, z);
        let (t, w) = generate(&world, &mut h, items).unwrap_or_else(|e| panic!("rim Z {z}: Auto must resolve Inside: {e}"));
        assert_eq!(gouges(&w), 0, "rim Z {z}: {w:?}");
        let lo = rides_the_wall(&format!("blind bore, rim Z {z}"), &t, &wall, true);
        assert!((lo - 4.0).abs() < EPS, "rim Z {z}: the wall is cut clean to the bore floor, never below it ({lo})");
    }
}

#[test]
fn auto_on_a_through_hole_rim_cuts_inside() {
    let (world, mut h) = world_with(bored_plate(0.0));
    let items = hole_rim(&world, 10.0);
    let (t, w) = generate(&world, &mut h, items).expect("Auto resolves Inside");
    assert_eq!(gouges(&w), 0, "{w:?}");
    let lo = rides_the_wall("through hole", &t, &circle((0.0, 0.0), 10.0, 32), true);
    assert!(lo.abs() < EPS, "the wall is cut to the bottom ({lo})");
}

#[test]
fn auto_on_a_boss_and_on_an_outer_wall_cuts_outside() {
    let (world, mut h) = world_with(bossed_base());
    let items = top_face(&world, 10.0);
    let (t, w) = generate(&world, &mut h, items).expect("Auto resolves Outside on a boss");
    assert_eq!(gouges(&w), 0, "{w:?}");
    let lo = rides_the_wall("boss", &t, &rect(-10.0, -10.0, 10.0, 10.0), false);
    assert!((lo - 5.0).abs() < EPS, "the boss is cut down to its floor ({lo})");
    let (world, mut h) = world_with(block());
    let items = top_face(&world, 10.0);
    let (t, w) = generate(&world, &mut h, items).expect("Auto resolves Outside on an outer wall");
    assert_eq!(gouges(&w), 0, "{w:?}");
    let lo = rides_the_wall("outer wall", &t, &rect(-20.0, -20.0, 20.0, 20.0), false);
    assert!(lo.abs() < EPS, "the block is cut to its bottom ({lo})");
}

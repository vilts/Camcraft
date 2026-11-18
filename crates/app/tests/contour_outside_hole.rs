//! #1927: a 2D Contour of a PICKED ring whose chosen side stands in the
//! part at every level is refused by name at generation. Before the fix
//! Side Outside on a plate's through-hole wall kept only the top plane
//! (nothing stands above the part top, so the depth ladder found that
//! one level legal), cut air there with the cutter centre 3 mm inside
//! the plate's footprint, and posted with no word - at any Bottom.
//! The legal sides stay legal: Inside and Auto on the hole, Outside on
//! an outer wall and on a boss standing on a floor. The verifier half
//! is pinned too: the full-depth Outside ring the generator now refuses
//! is a Gouge under the model law.
//!
//! Gate map (#1927): covers camcraft_app::operation (generate, Selection
//! arm), camcraft_motion::machining2d (contour_on_picked, contour_depth
//! picked_law), camcraft_sim::verify (model law).
//!
//! Provenance: our own fixtures (common::solid_part); the 2.5D
//! profiling rule of published machining practice. No vendor material.

mod common;

use common::solid_part::{cap, circle, mill, plate, rect, region, walls};
use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel};
use camcraft_app::geometry::feature;
use camcraft_app::geometry::{Mesh, Triangle};
use camcraft_app::machining2d::{ContourParams, Lead, Side, contour};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, HeightReference, OpParams, OpStatus, SrcRef};
use camcraft_app::tools::catalog;
use camcraft_app::verify::{Mode, VerifyContext, Warning, verify};
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0600-4FL";

/// A 60 x 60 x [0, 10] plate with a Ø20 through hole (32 chords).
fn holed_plate() -> Vec<Triangle> {
    let mut f = Vec::new();
    plate(&rect(-30.0, -30.0, 30.0, 30.0), Some((&circle((0.0, 0.0), 10.0, 32), 0.0)), 0.0, 10.0, &mut f);
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

/// A pick builder: the fixture world in, the selection out.
type Picker = fn(&World) -> Vec<GeoSel>;

fn pick(feature: FeatureSel) -> Vec<GeoSel> {
    vec![GeoSel { entity: "part".into(), feature }]
}

/// The hole's closed rim chain (the bottom rim welds closed here).
fn hole_chain(world: &World) -> Vec<GeoSel> {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let id = feature::chains(&f)
        .into_iter()
        .find(|c| c.closed && feature::chain_points(&f, c).iter().all(|p| p[0].hypot(p[1]) < 10.5))
        .map(|c| c.id)
        .expect("the hole rim chains closed");
    pick(FeatureSel::Chain(id))
}

/// The hole's welded wall patch.
fn hole_patch(world: &World) -> Vec<GeoSel> {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let ps = feature::patches(&f);
    assert_eq!(ps.len(), 1, "only the round hole wall welds");
    pick(FeatureSel::Patch(ps[0].id))
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

fn params(side: Side, top: Height, bottom: Height) -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: Height::fixed(20.0),
        top_z: top,
        bottom_z: bottom,
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side,
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

/// Model top to model bottom: the full depth of the part.
fn full(side: Side) -> OpParams {
    params(side, Height::of(HeightReference::TopOfModel, 0.0), Height::of(HeightReference::BottomOfModel, 0.0))
}

/// Create, then generate; the generate refusal or the cached path.
fn generate(world: &World, h: &mut History, items: Vec<GeoSel>, p: OpParams) -> Result<(Toolpath, Vec<Warning>), String> {
    h.execute(world, CreateOperation::new(world, "C", TOOL, SrcRef::Selection(items), p).expect("create"));
    let g = GenerateOperation::new(world, "C").map_err(|e| e.to_string())?;
    h.execute(world, g);
    let e = model::find_by_name(world, "C").expect("op");
    assert_eq!(camcraft_app::operation::status_of(world, &e), OpStatus::Ok);
    let tp = model::toolpath_of(world, &e).expect("cached path");
    Ok((tp.path.clone(), tp.warnings.clone()))
}

/// The distinct Z of every planar feed, descending.
fn feed_levels(t: &Toolpath) -> Vec<f64> {
    let mut v: Vec<f64> = t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z).map(|m| m.to.z).collect();
    v.sort_by(|a, b| b.total_cmp(a));
    v.dedup();
    v
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

#[test]
fn outside_on_a_picked_hole_wall_is_refused_by_name_at_every_depth() {
    let sel = (Height::of(HeightReference::TopOfSelection, 0.0), Height::of(HeightReference::BottomOfSelection, 0.0));
    let cases: [(&str, Picker, OpParams); 4] = [
        ("chain, full depth", hole_chain, full(Side::Outside)),
        ("patch, full depth", hole_patch, full(Side::Outside)),
        ("patch, selection heights", hole_patch, params(Side::Outside, sel.0, sel.1)),
        // A top above the part: the top level is air, still no wall.
        ("patch, top over the part", hole_patch, params(Side::Outside, Height::fixed(11.0), Height::fixed(0.0))),
    ];
    for (tag, items, p) in cases {
        let (world, mut h) = world_with(holed_plate());
        let items = items(&world);
        let err = generate(&world, &mut h, items, p).err().unwrap_or_else(|| panic!("{tag}: Outside of a hole must be refused"));
        assert!(err.contains("the cutter Outside this ring stands in the part material"), "{tag}: {err}");
        assert!(err.contains("set Side to Inside"), "{tag}: {err}");
        assert!(err.contains("part top Z 10.000"), "{tag}: {err}");
        let e = model::find_by_name(&world, "C").expect("op");
        assert!(model::toolpath_of(&world, &e).is_none(), "{tag}: nothing cached, nothing to post");
    }
}

#[test]
fn inside_and_auto_on_the_hole_wall_cut_every_level_clean() {
    for side in [Side::Inside, Side::Auto] {
        for items in [hole_chain as Picker, hole_patch] {
            let (world, mut h) = world_with(holed_plate());
            let items = items(&world);
            let (t, w) = generate(&world, &mut h, items, full(side)).expect("a hole cut from inside is legal");
            assert_eq!(feed_levels(&t), [7.5, 5.0, 2.5, 0.0], "{side:?}");
            assert_eq!(gouges(&w), 0, "{side:?}: {w:?}");
            for m in t.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
                let r = m.to.x.hypot(m.to.y);
                assert!(r < 7.01, "{side:?}: the centre stays a radius inside the wall: {m:?}");
            }
        }
    }
}

#[test]
fn outside_on_an_outer_wall_and_on_a_boss_stays_legal() {
    // The outer wall of a block: Outside walks every level.
    let (world, mut h) = world_with(block());
    let items = top_face(&world, 10.0);
    let (t, w) = generate(&world, &mut h, items, full(Side::Outside)).expect("Outside of an outer wall");
    assert_eq!(feed_levels(&t), [7.5, 5.0, 2.5, 0.0]);
    assert_eq!(gouges(&w), 0, "{w:?}");
    // A boss on a floor: Outside walks down to the floor at Z 5 and stops.
    // (#2004: Top Z 10 is the part top, so no level at 10.)
    let (world, mut h) = world_with(bossed_base());
    let items = top_face(&world, 10.0);
    let (t, w) = generate(&world, &mut h, items, full(Side::Outside)).expect("Outside of a boss");
    assert_eq!(feed_levels(&t), [7.5, 5.0]);
    assert_eq!(gouges(&w), 0, "{w:?}");
}

#[test]
fn inside_on_a_picked_block_top_is_refused_by_name() {
    // The mirror case: the ring of a block's own top face, cut Inside,
    // stands in the block at every level below its top.
    let (world, mut h) = world_with(block());
    let items = top_face(&world, 10.0);
    let err = generate(&world, &mut h, items, full(Side::Inside)).expect_err("Inside of a block is refused");
    assert!(err.contains("the cutter Inside this ring stands in the part material"), "{err}");
    assert!(err.contains("set Side to Outside"), "{err}");
}

#[test]
fn the_full_depth_outside_ring_is_a_gouge_to_the_verifier() {
    // The verifier half of #1927: the ring the generator now refuses,
    // built flat over the whole ladder, is convicted by the model law
    // at every level under the part top - the post gate would block it.
    let part = holed_plate();
    let hole = region(&[circle((0.0, 0.0), 10.0, 32)]);
    let p = ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 30.0,
        retract_z: 20.0,
        top_z: 10.0,
        bottom_z: 0.0,
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    };
    let t = contour(&hole, &p).expect("the flat contour builds the ring");
    let tool = mill(6.0);
    let w = verify(
        &t,
        &VerifyContext {
            region: &hole,
            model: Some(&part),
            tool: &tool,
            mode: Mode::Boundary,
            stock_top_z: 10.0,
            clearance_z: 30.0,
            gouge_tol: 0.01,
            z_tol: 1e-6,
            ..Default::default()
        },
    );
    assert!(gouges(&w) > 0, "the ring in the plate is a gouge: {w:?}");
    for wn in &w {
        if let Warning::Gouge { at } = wn {
            assert!(t.moves[*at].to.z < 10.0, "only moves under the top are convicted: {:?}", t.moves[*at]);
        }
    }
}

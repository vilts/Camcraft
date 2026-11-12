//! M13.11 facing rows honouring holes, through the commands: a plate
//! with a through-hole faced off its model lifts over the hole by
//! default, cuts across it under `CutAir` (the hole is air) and under
//! `Through` (the legacy band); a plate with a boss faced off its top
//! face lifts over the boss under `CutAir` too (an island is solid).
//! The project file's reading of the law (absent is `lift`) is pinned
//! beside the DTO, in `project::params_impl`.
//!
//! Gate map (M13.11): covers app::operation::generate (Face arm),
//! app::operation::material_source::standing_above,
//! app::operation::picks (the open-mesh silhouette, #241).

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machining2d::FaceHoles;
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

fn face_params(holes: FaceHoles) -> OpParams {
    OpParams::Face {
        holes,
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(4.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn world_with_tool() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0600-4FL")
        .map(|(_, t)| t)
        .expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    (world, history)
}

/// The four corners of a square of half-side `h` at height `z`, CCW.
fn ring(h: f64, z: f64) -> [Point3; 4] {
    [
        Point3::new(-h, -h, z),
        Point3::new(h, -h, z),
        Point3::new(h, h, z),
        Point3::new(-h, h, z),
    ]
}

/// The two triangles of the quad `a b c d`.
fn quad(a: Point3, b: Point3, c: Point3, d: Point3) -> [Triangle; 2] {
    [Triangle::new(a, b, c), Triangle::new(a, c, d)]
}

/// The square annulus between `outer` and `inner` (both CCW), facing up.
fn annulus(outer: [Point3; 4], inner: [Point3; 4], up: bool, out: &mut Vec<Triangle>) {
    for i in 0..4 {
        let (o0, o1, i0, i1) = (outer[i], outer[(i + 1) % 4], inner[i], inner[(i + 1) % 4]);
        let q = if up { quad(o0, o1, i1, i0) } else { quad(o0, i0, i1, o1) };
        out.extend(q);
    }
}

/// The walls between two rings at `z0` and `z1`, facing out of the ring.
fn walls(r: f64, z0: f64, z1: f64, outward: bool, out: &mut Vec<Triangle>) {
    let (lo, hi) = (ring(r, z0), ring(r, z1));
    for i in 0..4 {
        let (a, b, c, d) = (lo[i], lo[(i + 1) % 4], hi[(i + 1) % 4], hi[i]);
        let q = if outward { quad(a, b, c, d) } else { quad(a, d, c, b) };
        out.extend(q);
    }
}

/// A 40x40x5 plate with a 10x10 square hole through it.
fn holed_plate() -> Mesh {
    let mut f = Vec::new();
    annulus(ring(20.0, 5.0), ring(5.0, 5.0), true, &mut f);
    annulus(ring(20.0, 0.0), ring(5.0, 0.0), false, &mut f);
    walls(20.0, 0.0, 5.0, true, &mut f);
    walls(5.0, 0.0, 5.0, false, &mut f);
    Mesh::from_facets(f)
}

/// A 40x40x5 plate with a 10x10 boss standing 3 mm proud of its top.
fn bossed_plate() -> Mesh {
    let mut f = Vec::new();
    annulus(ring(20.0, 5.0), ring(5.0, 5.0), true, &mut f);
    let b = ring(20.0, 0.0);
    f.extend(quad(b[0], b[3], b[2], b[1]));
    walls(20.0, 0.0, 5.0, true, &mut f);
    walls(5.0, 5.0, 8.0, true, &mut f);
    let t = ring(5.0, 8.0);
    f.extend(quad(t[0], t[1], t[2], t[3]));
    Mesh::from_facets(f)
}

fn generate(world: &World, history: &mut History, name: &str, src: SrcRef, holes: FaceHoles) -> camcraft_app::schema::Toolpath {
    history.execute(world, CreateOperation::new(world, name, "EM-0600-4FL", src, face_params(holes)).expect("create"));
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op");
    model::toolpath_of(world, &e).expect("cache")
}

fn plunges(t: &camcraft_app::schema::Toolpath) -> usize {
    t.path.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count()
}

/// The planar up-facing face at height `z` of the named part.
fn top_face_at(world: &World, name: &str, z: f64) -> u64 {
    let e = model::find_by_name(world, name).expect("part exists");
    let f = model::features_of(world, &e).expect("features");
    f.faces
        .iter()
        .filter(|fa| fa.normal[2] > 0.9 && (fa.offset - z).abs() < 1e-3)
        .map(|fa| fa.id)
        .next()
        .expect("the planar top face at that height")
}

#[test]
fn a_through_hole_is_lifted_over_by_default_and_cut_across_when_asked() {
    let (world, mut history) = world_with_tool();
    history.execute(&world, ImportModel::from_mesh("plate", holed_plate()));
    let src = SrcRef::Entity("plate".into());

    let lift = generate(&world, &mut history, "Lift", src.clone(), FaceHoles::Lift);
    assert!(plunges(&lift) > 1, "the rows across the hole re-enter after the lift");
    assert!(lift.path.moves.iter().filter(|m| m.kind == MoveKind::Rapid).any(|m| m.to.z == 25.0 && m.from.z == 25.0 && (m.to.x - m.from.x).abs() > 9.0), "a rapid across the hole at clearance");
    assert!(lift.path.notes.iter().any(|n| n.contains("lifted over")), "{:?}", lift.path.notes);

    // The hole holds no material: cut straight across it.
    let air = generate(&world, &mut history, "Air", src.clone(), FaceHoles::CutAir);
    assert_eq!(plunges(&air), 1, "{:?}", air.path.notes);
    assert!(air.path.notes.iter().all(|n| !n.contains("lifted over")));

    // The legacy band serpentine: the same rows, the hole never read.
    let through = generate(&world, &mut history, "Through", src, FaceHoles::Through);
    assert_eq!(through.path.moves, air.path.moves);
}

#[test]
fn a_boss_is_an_island_and_is_lifted_over_even_under_cut_air() {
    let (world, mut history) = world_with_tool();
    history.execute(&world, ImportModel::from_mesh("part", bossed_plate()));
    let top = top_face_at(&world, "part", 5.0);
    let src = SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: FeatureSel::Face(top) }]);

    let op = camcraft_app::schema::Operation {
        tool: "EM-0600-4FL".into(),
        src: src.clone(),
        params: face_params(FaceHoles::CutAir),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    let region = camcraft_app::operation::region_of(&world, &op).expect("the top face's region");
    assert_eq!(region.loops().len(), 2, "the plate's top around the boss: an outer and the boss's hole");
    let air = generate(&world, &mut history, "Air", src.clone(), FaceHoles::CutAir);
    assert!(plunges(&air) > 1, "the boss stands above the face: lifted over; notes {:?}", air.path.notes);
    assert!(air.path.moves.iter().filter(|m| m.kind == MoveKind::Feed).all(|m| {
        // Every cut endpoint stands a radius (3) off the 10x10 boss, less
        // the region's 5 um chord tolerance at its rounded corners.
        let off = |x: f64, y: f64| (x.abs() - 5.0).max(0.0).hypot((y.abs() - 5.0).max(0.0)) >= 3.0 - 0.02;
        off(m.from.x, m.from.y) && off(m.to.x, m.to.y)
    }), "no cut stands within a radius of the boss: {:?}", air.path.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| (m.from.x, m.from.y, m.to.x, m.to.y)).collect::<Vec<_>>());
    let lift = generate(&world, &mut history, "Lift", src, FaceHoles::Lift);
    assert_eq!(lift.path.moves, air.path.moves);
}


/// #241: a 40x40x5 plate whose mesh the slicer refuses as open (the
/// lower half of its +X wall is missing - the dangling chord ends an
/// imported part can carry), with a two-triangle top over a finely
/// tessellated bottom, so most bottom facets' shadows nest inside a top
/// facet's shadow.
fn open_plate_with_nested_bottom() -> Mesh {
    let mut f = Vec::new();
    let t = ring(20.0, 5.0);
    f.extend(quad(t[0], t[1], t[2], t[3]));
    // The bottom, a 4x4 grid of quads facing down.
    for i in 0..4 {
        for j in 0..4 {
            let (x0, y0) = (-20.0 + 10.0 * f64::from(i), -20.0 + 10.0 * f64::from(j));
            let (a, b, c, d) = (
                Point3::new(x0, y0, 0.0),
                Point3::new(x0 + 10.0, y0, 0.0),
                Point3::new(x0 + 10.0, y0 + 10.0, 0.0),
                Point3::new(x0, y0 + 10.0, 0.0),
            );
            f.extend(quad(a, d, c, b));
        }
    }
    walls(20.0, 2.5, 5.0, true, &mut f);
    let (lo, mid) = (ring(20.0, 0.0), ring(20.0, 2.5));
    for i in [0usize, 2, 3] {
        f.extend(quad(lo[i], lo[(i + 1) % 4], mid[(i + 1) % 4], mid[i]));
    }
    Mesh::from_facets(f)
}

/// The closed edge chains rimming the planar face `face` of `name`, as
/// picks - what clicking each top edge of the part in the viewport picks.
fn rim_chains(world: &World, name: &str, face: u64) -> Vec<GeoSel> {
    let e = model::find_by_name(world, name).expect("part exists");
    let f = model::features_of(world, &e).expect("features");
    let on_face = |t: u32| f.facet_face.get(t as usize).and_then(|&i| f.faces.get(i as usize)).is_some_and(|fa| fa.id == face);
    camcraft_app::geometry::feature::chains(&f)
        .iter()
        .filter(|c| {
            c.edges.iter().all(|&i| {
                f.edges.get(i as usize).is_some_and(|ed| ed.facets[..ed.n_facets.min(3) as usize].iter().any(|&t| on_face(t)))
            })
        })
        .map(|c| GeoSel { entity: name.into(), feature: FeatureSel::Chain(c.id) })
        .collect()
}

#[test]
fn an_open_mesh_rim_pick_faces_the_whole_top_with_no_phantom_holes() {
    let (world, mut history) = world_with_tool();
    history.execute(&world, ImportModel::from_mesh("open", open_plate_with_nested_bottom()));
    let top = top_face_at(&world, "open", 5.0);
    let rim = rim_chains(&world, "open", top);
    assert!(rim.len() >= 4, "the top's rim picks: {rim:?}");
    assert!(rim.iter().all(|g| matches!(g.feature, FeatureSel::Chain(_))), "a chain-only pick, as the issue's edge clicks are");

    // The rim pick's region is the plate's whole top: the silhouette it
    // is clipped by (the open mesh's facet shadow) holds no holes.
    let op = camcraft_app::schema::Operation {
        tool: "EM-0600-4FL".into(),
        src: SrcRef::Selection(rim.clone()),
        params: face_params(FaceHoles::Lift),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    let region = camcraft_app::operation::region_of(&world, &op).expect("the rim's region");
    let areas: Vec<f64> = region.loops().iter().map(|l| l.area()).collect();
    assert_eq!(areas.len(), 1, "one loop, no phantom holes from nested bottom facets: {areas:?}");
    assert!((areas[0] - 1600.0).abs() < 1.0, "the whole 40x40 top: {areas:?}");

    // And the path is the face pick's path: one plunge, nothing lifted.
    let by_rim = generate(&world, &mut history, "Rim", SrcRef::Selection(rim), FaceHoles::Lift);
    let face_pick = SrcRef::Selection(vec![GeoSel { entity: "open".into(), feature: FeatureSel::Face(top) }]);
    let by_face = generate(&world, &mut history, "Top", face_pick, FaceHoles::Lift);
    assert_eq!(plunges(&by_rim), 1, "{:?}", by_rim.path.notes);
    assert!(by_rim.path.notes.iter().all(|n| !n.contains("lifted over")), "{:?}", by_rim.path.notes);
    assert_eq!(by_rim.path.moves, by_face.path.moves);
}

//! #1888 through the operation layer: a Deburr's picked edge chains are
//! the only edges it breaks, an excluded chain stays sharp, the minimum
//! edge angle and the round-hole filter reach the kernel, the command
//! door refuses a bad filter, and the project file carries every field.
//!
//! Gate map (#1888): covers app::operation (generate, deburr_picks,
//! sane), app::project (params), app::commands (CreateOperation).

mod common;

use common::solid_part::{circle, plate, rect};
use flecs_ecs::prelude::World;

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{AddTool, CommandError, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::feature;
use camcraft_app::geometry::{Mesh, Point3, cube};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::project;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

/// The 90-degree chamfer mill the deburr suite rides.
const CONE: &str = "CM-0600-90";

/// A 40 x 40 brick, z 0..10: twelve edges, each one welded crease.
fn job() -> (World, History) {
    job_of(cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0)))
}

/// The same footprint as a plate with a diameter 8 through hole at
/// the origin (the solid-law harness's shell).
fn holed_job() -> (World, History) {
    let mut f = Vec::new();
    plate(&rect(-20.0, -20.0, 20.0, 20.0), Some((&circle((0.0, 0.0), 4.0, 48), 0.0)), 0.0, 10.0, &mut f);
    job_of(Mesh::from_facets(f))
}

/// The part as its own stock (a finished part) and the cone.
fn job_of(mesh: Mesh) -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("plate", mesh));
    h.execute(
        &world,
        SetStock::new(&world, StockShape::FromModel { source: "plate".into() }, Default::default()).expect("stock"),
    );
    let tool = catalog().into_iter().find(|(n, _)| *n == CONE).map(|(_, t)| t).expect("catalog cone");
    h.execute(&world, AddTool::new(&world, CONE, tool).expect("tool"));
    (world, h)
}

/// A 1-wide break whose tips ride z 8.5..9.5: the edges at the plate's
/// top (z 10) and nothing else. Every filter at rest.
fn params() -> OpParams {
    OpParams::Deburr {
        width: 1.0 * std::f64::consts::SQRT_2, // #1890: the chord of a 1.0 mm setback
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(9.5),
        bottom_z: Height::fixed(8.5),
        feed: 500.0,
        plunge_feed: 100.0,
        exclude_edges: Vec::new(),
        min_edge_angle_deg: 0.0,
        exclude_holes: false,
        hole_min_diameter: 0.0,
        hole_max_diameter: 0.0,
        unreachable: camcraft_app::finishing3d::UnreachableEdges::SkipWithNote,
        sizing: camcraft_app::finishing3d::DeburrBreak::default(),
        picks: Default::default(),
    }
}

/// The plate's top outer edge at y = `y`, as a chain pick.
fn top_edge(world: &World, y: f64) -> GeoSel {
    let e = model::find_by_name(world, "plate").expect("plate");
    let f = model::features_of(world, &e).expect("features");
    let on = |p: &[f64; 3]| (p[1] - y).abs() < 1e-6 && (p[2] - 10.0).abs() < 1e-6;
    let hits: Vec<_> = feature::chains(&f)
        .into_iter()
        .filter(|c| {
            let pts = feature::chain_points(&f, c);
            pts.len() >= 2 && pts.iter().all(on)
        })
        .collect();
    assert_eq!(hits.len(), 1, "one chain runs the top edge at y {y}");
    GeoSel { entity: "plate".into(), feature: FeatureSel::Chain(hits[0].id) }
}

fn generate(world: &World, h: &mut History, src: SrcRef, p: OpParams) -> Result<Toolpath, String> {
    h.execute(world, CreateOperation::new(world, "DB", CONE, src, p).map_err(|e| e.to_string())?);
    match GenerateOperation::new(world, "DB") {
        Ok(cmd) => h.execute(world, cmd),
        Err(CommandError::Machining(msg)) => return Err(msg),
        Err(e) => return Err(e.to_string()),
    }
    let e = model::find_by_name(world, "DB").expect("op");
    Ok(model::toolpath_of(world, &e).expect("cached path").path.clone())
}

/// The edge points of every declared break.
fn broken(t: &Toolpath) -> Vec<Point3> {
    t.breaks.iter().flat_map(|b| b.edge.iter().copied()).collect()
}

fn on_hole(q: &Point3) -> bool {
    (q.x.hypot(q.y) - 4.0).abs() < 1e-3
}

fn on_edge(q: &Point3, y: f64) -> bool {
    (q.y - y).abs() < 1e-6
}

#[test]
fn a_picked_edge_is_the_only_edge_broken() {
    let (world, mut h) = job();
    let all = generate(&world, &mut h, SrcRef::Entity("plate".into()), params()).expect("automatic ride");
    assert!(broken(&all).iter().any(|q| on_edge(q, 20.0)), "the automatic ride breaks the far edge too");

    let (world, mut h) = job();
    let pick = top_edge(&world, -20.0);
    let t = generate(&world, &mut h, SrcRef::Selection(vec![pick]), params()).expect("the picked edge rides");
    assert_eq!(t.breaks.len(), 1, "one picked chain, one break: {:?}", t.breaks);
    assert!(broken(&t).iter().all(|q| on_edge(q, -20.0) && (q.z - 10.0).abs() < 1e-6), "{:?}", t.breaks);
}

/// Some face of the plate, as a pick.
fn a_face(world: &World) -> GeoSel {
    let e = model::find_by_name(world, "plate").expect("plate");
    let f = model::features_of(world, &e).expect("features");
    GeoSel { entity: "plate".into(), feature: FeatureSel::Face(f.faces.first().expect("a face").id) }
}

#[test]
fn a_face_pick_names_the_model_and_keeps_the_automatic_ride() {
    // #2284's reading stands: a face pick selects the model the ride
    // reads. Only an edge chain narrows it.
    let (world, mut h) = job();
    let whole = generate(&world, &mut h, SrcRef::Entity("plate".into()), params()).expect("automatic ride");
    let (world, mut h) = job();
    let face = a_face(&world);
    let t = generate(&world, &mut h, SrcRef::Selection(vec![face]), params()).expect("a face pick rides the model");
    assert_eq!(t.breaks, whole.breaks, "the same edges as the whole model");

    // A face beside a chain: the chain narrows, the face does not widen.
    let (world, mut h) = job();
    let picks = vec![a_face(&world), top_edge(&world, -20.0)];
    let t = generate(&world, &mut h, SrcRef::Selection(picks), params()).expect("the picked edge rides");
    assert_eq!(t.breaks.len(), 1, "{:?}", t.breaks);
    assert!(broken(&t).iter().all(|q| on_edge(q, -20.0)));
}

#[test]
fn an_excluded_pick_that_is_gone_is_refused_by_name() {
    let (world, mut h) = job();
    let mut p = params();
    if let OpParams::Deburr { exclude_edges, .. } = &mut p {
        exclude_edges.push(GeoSel { entity: "plate".into(), feature: FeatureSel::Chain(1) });
    }
    let e = generate(&world, &mut h, SrcRef::Entity("plate".into()), p).expect_err("no such chain");
    assert!(e.contains("no longer in the model"), "{e}");
}

#[test]
fn an_excluded_edge_stays_sharp() {
    let (world, mut h) = job();
    let skip = top_edge(&world, -20.0);
    let p = match params() {
        OpParams::Deburr { exclude_edges: _, width, clearance_z, top_z, bottom_z, feed, plunge_feed, min_edge_angle_deg, exclude_holes, hole_min_diameter, hole_max_diameter, unreachable, sizing, .. } => OpParams::Deburr {
            exclude_edges: vec![skip],
            width,
            clearance_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
            min_edge_angle_deg,
            exclude_holes,
            hole_min_diameter,
            hole_max_diameter,
            unreachable,
            sizing,
            picks: Default::default(),
        },
        other => other,
    };
    let t = generate(&world, &mut h, SrcRef::Entity("plate".into()), p).expect("the rest rides");
    let on_skipped = t.breaks.iter().any(|b| b.edge.windows(2).any(|w| on_edge(&w[0], -20.0) && on_edge(&w[1], -20.0)));
    assert!(!on_skipped, "the excluded edge was broken: {:?}", t.breaks);
    assert!(t.breaks.iter().any(|b| b.edge.windows(2).any(|w| on_edge(&w[0], 20.0) && on_edge(&w[1], 20.0))));
    assert!(t.notes.iter().any(|n| n.contains("1 edges excluded")), "{:?}", t.notes);
}

#[test]
fn the_minimum_edge_angle_reaches_the_kernel() {
    // Every top edge of the brick bends 90 degrees: a 100-degree
    // minimum leaves nothing, and the refusal names the filter.
    let (world, mut h) = job();
    let mut p = params();
    if let OpParams::Deburr { min_edge_angle_deg, .. } = &mut p {
        *min_edge_angle_deg = 100.0;
    }
    let e = generate(&world, &mut h, SrcRef::Entity("plate".into()), p).expect_err("nothing bends 100 degrees");
    assert!(e.contains("below the minimum edge angle"), "{e}");

    // At the seed's 40 degrees the same edges ride.
    let (world, mut h) = job();
    let mut p = params();
    if let OpParams::Deburr { min_edge_angle_deg, .. } = &mut p {
        *min_edge_angle_deg = 40.0;
    }
    let t = generate(&world, &mut h, SrcRef::Entity("plate".into()), p).expect("90-degree edges ride");
    assert!(broken(&t).iter().any(|q| on_edge(q, 20.0)));
}

#[test]
fn a_hole_inside_the_diameter_range_keeps_its_rim() {
    let ranged = |lo: f64, hi: f64| {
        let mut p = params();
        if let OpParams::Deburr { exclude_holes, hole_min_diameter, hole_max_diameter, .. } = &mut p {
            (*exclude_holes, *hole_min_diameter, *hole_max_diameter) = (true, lo, hi);
        }
        p
    };
    let (world, mut h) = holed_job();
    let all = generate(&world, &mut h, SrcRef::Entity("plate".into()), params()).expect("automatic ride");
    assert!(broken(&all).iter().any(on_hole), "no filter: the hole's rim is broken");

    let (world, mut h) = holed_job();
    let t = generate(&world, &mut h, SrcRef::Entity("plate".into()), ranged(7.0, 9.0)).expect("the outer edges ride");
    assert!(!broken(&t).iter().any(on_hole), "a diameter 8 hole is inside 7..9: {:?}", t.breaks);
    assert!(broken(&t).iter().any(|q| on_edge(q, 20.0)), "the plate's own edges still ride");
    assert!(t.notes.iter().any(|n| n.contains("round holes")), "{:?}", t.notes);

    let (world, mut h) = holed_job();
    let t = generate(&world, &mut h, SrcRef::Entity("plate".into()), ranged(1.0, 2.0)).expect("rides");
    assert!(broken(&t).iter().any(on_hole), "a diameter 8 hole is outside 1..2");
}

#[test]
fn the_command_door_refuses_a_bad_filter() {
    let (world, _h) = job();
    let src = || SrcRef::Entity("plate".into());
    let mut p = params();
    if let OpParams::Deburr { min_edge_angle_deg, .. } = &mut p {
        *min_edge_angle_deg = 180.0;
    }
    assert!(CreateOperation::new(&world, "A", CONE, src(), p).is_err(), "a 180-degree minimum is no bend");
    let mut p = params();
    if let OpParams::Deburr { hole_min_diameter, hole_max_diameter, .. } = &mut p {
        (*hole_min_diameter, *hole_max_diameter) = (5.0, 2.0);
    }
    assert!(CreateOperation::new(&world, "B", CONE, src(), p).is_err(), "the hole range is ordered");
    let mut p = params();
    if let OpParams::Deburr { exclude_edges, .. } = &mut p {
        exclude_edges.push(GeoSel { entity: "plate".into(), feature: FeatureSel::Face(1) });
    }
    assert!(CreateOperation::new(&world, "C", CONE, src(), p).is_err(), "an exclusion is an edge chain");
}

#[test]
fn the_filters_round_trip_the_project_file() {
    let (world, mut h) = job();
    let skip = top_edge(&world, -20.0);
    let mut p = params();
    if let OpParams::Deburr { exclude_edges, min_edge_angle_deg, exclude_holes, hole_min_diameter, hole_max_diameter, unreachable, .. } = &mut p {
        exclude_edges.push(skip);
        (*min_edge_angle_deg, *exclude_holes, *hole_min_diameter, *hole_max_diameter) = (35.0, true, 2.0, 9.0);
        *unreachable = camcraft_app::finishing3d::UnreachableEdges::Refuse;
    }
    h.execute(&world, CreateOperation::new(&world, "DB", CONE, SrcRef::Entity("plate".into()), p.clone()).expect("create"));
    let text = project::save_text(&world).expect("save");
    for key in ["exclude_edges", "min_edge_angle_deg", "exclude_holes", "hole_min_diameter", "hole_max_diameter"] {
        assert!(text.contains(key), "the file names {key}");
    }
    let back = camcraft_app::world::new_world();
    project::open(&back, &text).expect("open");
    let e = model::find_by_name(&back, "DB").expect("op");
    assert_eq!(model::operation_of(&back, &e).expect("operation").params, p);

    // A file from before the keys: the filters at rest, and a save
    // that still does not name them.
    let (world, mut h) = job();
    h.execute(&world, CreateOperation::new(&world, "DB", CONE, SrcRef::Entity("plate".into()), params()).expect("create"));
    let old = project::save_text(&world).expect("save");
    for key in ["exclude_edges", "min_edge_angle_deg", "exclude_holes", "hole_min_diameter", "hole_max_diameter"] {
        assert!(!old.contains(key), "a filter at rest is absent from the file: {key}");
    }
    let back = camcraft_app::world::new_world();
    project::open(&back, &old).expect("open");
    let e = model::find_by_name(&back, "DB").expect("op");
    assert_eq!(model::operation_of(&back, &e).expect("operation").params, params());
}

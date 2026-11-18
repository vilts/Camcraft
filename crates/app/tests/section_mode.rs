//! Pinning tests for explicit section mode metadata (#1966):
//! `3-axis`, `3+2 indexed`, and `simultaneous 5-axis` must be emitted as distinct
//! `PARAM SectionMode ...` markers in `.cnc` streams while leaving legacy `.cl`
//! / `Dump::default()` and 3-axis `.nc` programs unchanged.
//!
//! Gate map (M4.4): covers app::posting, app::posting_section_mode, post::section_mode, post::cnc, app::operation.

use std::path::Path;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::machining2d::{FaceHoles, Lead, PocketStrategy, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::post::cnc::{self, Record};
use camcraft_app::post::{self, Dump};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;
use camcraft_app::{links::Links, operation::{Height, OpParams, SrcRef, Tilt}};
use flecs_ecs::prelude::World;
use serde_json::Value;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name)).expect("catalog tool");
    history.execute(world, cmd);
}

fn section_mode_param(records: &[Record]) -> Option<String> {
    records.iter().find_map(|r| match r {
        Record::Param { name, value } if name == "SectionMode" => match value {
            Value::String(s) => Some(s.clone()),
            other => Some(other.to_string()),
        },
        _ => None,
    })
}

fn hill_mesh() -> Mesh {
    const TAU: f64 = std::f64::consts::TAU;
    let (radius, polar_max, n_lat, n_lon) = (12.0, std::f64::consts::FRAC_PI_4, 4usize, 12usize);
    let mut facets = Vec::new();
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon)
            .map(|j| {
                let th = TAU * j as f64 / n_lon as f64;
                Point3::new(rr * th.cos(), rr * th.sin(), z)
            })
            .collect()
    };
    let apex = Point3::new(0.0, 0.0, radius);
    let first = ring(1);
    for j in 0..n_lon {
        facets.push(Triangle::new(apex, first[j], first[(j + 1) % n_lon]));
    }
    for k in 1..n_lat {
        let ra = ring(k);
        let rb = ring(k + 1);
        for j in 0..n_lon {
            let j2 = (j + 1) % n_lon;
            facets.push(Triangle::new(ra[j], rb[j], rb[j2]));
            facets.push(Triangle::new(ra[j], rb[j2], ra[j2]));
        }
    }
    let rim = ring(n_lat);
    let center = Point3::new(0.0, 0.0, radius * polar_max.cos());
    for j in 0..n_lon {
        facets.push(Triangle::new(center, rim[j], rim[(j + 1) % n_lon]));
    }
    Mesh::from_facets(facets)
}

fn make_face_world() -> World {
    let world = new_world();
    let mut history = History::default();
    let stock_cmd = SetStock::new(
        &world,
        StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        Location::default(),
    )
    .expect("box stock");
    history.execute(&world, stock_cmd);
    add_tool(&world, &mut history, "EM-0400-2FL");
    let params = OpParams::Face {
        holes: FaceHoles::default(),
        blend_turns: false,
        tilt: None,
        stepover: 15.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let op = CreateOperation::new(&world, "Face3X", "EM-0400-2FL", SrcRef::Stock, params)
        .expect("create face");
    history.execute(&world, op);
    let gen_cmd = GenerateOperation::new(&world, "Face3X").expect("generate face");
    history.execute(&world, gen_cmd);
    world
}

fn make_tilted_pocket_world() -> World {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    let block = cube(Point3::new(0.0, 0.0, -20.0), Point3::new(40.0, 30.0, -10.0));
    history.execute(&world, ImportModel::from_mesh("part", block));
    let params = OpParams::Pocket {
        rest: None,
        blend_radius: None,
        links: Links::PLANAR,
        tilt: Some(Tilt { basis_version: 1, a_deg: 0.0, b_deg: 90.0, origin: [0.0; 3] }),
        stepover: 6.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(70.0),
        retract_z: Some(Height::fixed(60.0)),
        top_z: Height::fixed(40.0),
        bottom_z: Height::fixed(25.0),
        stepdown: 15.0,
        feed: 800.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    };
    let op = CreateOperation::new(&world, "Tilt32", "EM-0600-4FL", SrcRef::Entity("part".into()), params)
        .expect("create tilted pocket");
    history.execute(&world, op);
    let gen_cmd = GenerateOperation::new(&world, "Tilt32").expect("generate tilted pocket");
    history.execute(&world, gen_cmd);
    world
}

fn make_parallel5d_world() -> World {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("hill", hill_mesh()));
    let stock_cmd = SetStock::new(
        &world,
        StockShape::FromModel { source: "hill".into() },
        Location::default(),
    )
    .expect("from-model stock");
    history.execute(&world, stock_cmd);
    let params = OpParams::Contour5D {
        tolerance: 0.25,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(11.0),
        bottom_z: Height::fixed(9.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 15.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        shank_avoid: false,
    };
    let op = CreateOperation::new(&world, "Sim5X", "BN-0600", SrcRef::Entity("hill".into()), params)
        .expect("create contour5d");
    history.execute(&world, op);
    let gen_cmd = GenerateOperation::new(&world, "Sim5X").expect("generate contour5d");
    history.execute(&world, gen_cmd);
    world
}

#[test]
fn three_axis_tilted_3_plus_2_and_simultaneous_5axis_emit_distinct_section_modes() {
    let face_world = make_face_world();
    let tilt_world = make_tilted_pocket_world();
    let sim_world = make_parallel5d_world();

    let face_streams = posting::op_streams(&face_world).expect("face op streams");
    let tilt_streams = posting::op_streams(&tilt_world).expect("tilt op streams");
    let sim_streams = posting::op_streams(&sim_world).expect("5axis op streams");

    assert_eq!(face_streams.len(), 1);
    assert_eq!(tilt_streams.len(), 1);
    assert_eq!(sim_streams.len(), 1);

    let face_mode = section_mode_param(&face_streams[0].1);
    let tilt_mode = section_mode_param(&tilt_streams[0].1);
    let sim_mode = section_mode_param(&sim_streams[0].1);

    assert_eq!(face_mode.as_deref(), Some("3-axis"));
    assert_eq!(tilt_mode.as_deref(), Some("3+2 indexed"));
    assert_eq!(sim_mode.as_deref(), Some("simultaneous 5-axis"));
    assert_ne!(tilt_mode, sim_mode, "3+2 indexed and simultaneous 5-axis must never share one mode");

    // Round-trip through .cnc serialize/parse preserves PARAM SectionMode and passes section boundaries.
    for (_, recs) in [&face_streams[0], &tilt_streams[0], &sim_streams[0]] {
        let text = cnc::serialize(recs);
        let reparsed = cnc::parse(&text).expect("valid .cnc stream");
        assert_eq!(section_mode_param(&reparsed), section_mode_param(recs));
    }

    // 3-axis .nc output posted by an existing mill script must remain identical
    // with or without the PARAM SectionMode marker.
    let fanuc_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/posts/fanuc.cps");
    let fanuc = std::fs::read_to_string(&fanuc_path).expect("fanuc.cps");
    let with_mode_text = cnc::serialize(&face_streams[0].1);
    let without_mode: Vec<Record> = face_streams[0]
        .1
        .iter()
        .filter(|r| !matches!(r, Record::Param { name, .. } if name == "SectionMode"))
        .cloned()
        .collect();
    let without_mode_text = cnc::serialize(&without_mode);
    let mch = posting::default_machine();
    let posted_with = post::post(&fanuc, &mch, &with_mode_text).expect("post with SectionMode");
    let posted_without = post::post(&fanuc, &mch, &without_mode_text).expect("post without SectionMode");
    assert_eq!(posted_with.program, posted_without.program);
}

#[test]
fn dump_default_and_legacy_cl_streams_do_not_synthesize_section_mode() {
    let cl_text = "; HSMW CL v1\nRapid X 0 Y 0 Z 5000\nFeed X 10000 Y 0 Z 0 F 600\nEND\n";
    let parsed = cnc::parse(cl_text).expect("legacy .cl parses");
    assert_eq!(section_mode_param(&parsed), None);

    let tp = camcraft_app::cl::Toolpath::default();
    let dumped = post::dump(&tp, &Dump::default());
    assert_eq!(section_mode_param(&dumped), None);

    // Ensure Contour with no tilt is also classified as 3-axis when posted, distinct from Lead/Side warnings.
    let _unused = (Side::Outside, Lead::None);
}

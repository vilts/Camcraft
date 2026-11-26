//! App-side registration, EMCO VMC-200 gate checks, and air-cut kit tests
//! for Siemens Sinumerik 810M (`issue #2888`).
//!
//! Gate map (#2888): covers camcraft_post::post (sinumerik-810m.cps), app::posting
//! (BUILTINS, MACHINES, post_serves, post_offer, post_setup), app::project (save),
//! app::commands (LoadMachine, SelectPost, CreateOperation, GenerateOperation).

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, LoadMachine, SelectPost, SetStock,
};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Point3;
use camcraft_app::machine::Kind;
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting::{self, OfferWhy};
use camcraft_app::schema::PostRef;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use std::path::{Path, PathBuf};

const POST_NAME: &str = "Siemens 810M";
const MACHINE_NAME: &str = "EMCO VMC-200";
const KIT: &str = "docs/tester-kits/sinumerik-810m-vmc200";

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

fn tuned_tool(base: &str, diameter: f64, rpm: f64, feed: f64) -> Tool {
    let mut t = catalog()
        .into_iter()
        .find(|(n, _)| *n == base)
        .map(|(_, t)| t)
        .unwrap_or_else(|| panic!("missing catalog tool {base}"));
    t.diameter = diameter;
    t.shank_d = diameter;
    t.rpm = rpm;
    t.feed = feed;
    t
}

fn build_air_cut_world(spindle_rpm: f64) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::builtin(&world, MACHINE_NAME).expect("VMC-200 mounts"));
    h.execute(&world, SelectPost::new(&world, PostRef::Named(POST_NAME.into())).expect("810M selects"));
    // 60 x 40 x 20 mm test block; air-cut README shifts G54 Z +50 mm above stock top (Z20).
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 40.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("stock"));
    h.execute(&world, AddTool::new(&world, "6MM EM", tuned_tool("EM-0600-4FL", 6.0, spindle_rpm, 600.0)).expect("T1"));
    h.execute(&world, AddTool::new(&world, "5MM DRILL", tuned_tool("DR-0400", 5.0, 1800.0, 180.0)).expect("T2"));

    let contour = OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: Height::fixed(24.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(19.0),
        stepdown: 1.0,
        feed: 600.0,
        plunge_feed: 180.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        chain_overrides: Vec::new(),
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "Perimeter Skim", "6MM EM", SrcRef::Stock, contour).expect("contour"));
    h.execute(&world, GenerateOperation::new(&world, "Perimeter Skim").expect("gen contour"));

    let holes = vec![
        Hole { xy: Point2::new(20.0, 20.0), radius: 2.5, entry_z: 20.0, floor_z: 14.0, through: false, stepped: false },
        Hole { xy: Point2::new(40.0, 20.0), radius: 2.5, entry_z: 20.0, floor_z: 14.0, through: false, stepped: false },
    ];
    let drill = OpParams::Drill {
        break_through: None,
        break_through_from: Default::default(),
        clearance_z: Height::fixed(30.0),
        retract_z: Some(Height::fixed(24.0)),
        top_z: Height::fixed(20.0),
        bottom_z: None,
        cycle: CycleChoice::Peck,
        peck: 3.0,
        dwell: 0.0,
        feed: 180.0,
        plunge_feed: 180.0,
        holes,
        cross: Vec::new(),
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "Peck Pair", "5MM DRILL", SrcRef::Stock, drill).expect("drill"));
    h.execute(&world, GenerateOperation::new(&world, "Peck Pair").expect("gen drill"));

    for op in ["Perimeter Skim", "Peck Pair"] {
        let e = model::find_by_name(&world, op).expect(op);
        let cache = model::toolpath_of(&world, &e).expect("toolpath");
        assert!(cache.warnings.is_empty(), "{op}: {:?}", cache.warnings);
    }
    world
}

#[test]
fn builtin_registration_and_machine_offer() {
    assert!(posting::BUILTINS.iter().any(|(n, _)| *n == POST_NAME), "Siemens 810M in BUILTINS");
    assert!(posting::MACHINES.iter().any(|(n, _)| *n == MACHINE_NAME), "EMCO VMC-200 in MACHINES");
    assert!(posting::post_serves(POST_NAME, Kind::Mill));
    assert!(posting::post_serves(POST_NAME, Kind::MillTurn));
    assert!(!posting::post_serves(POST_NAME, Kind::Lathe));

    let world = new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::builtin(&world, MACHINE_NAME).expect("mounts"));
    let offer = posting::post_offer(&world).expect("VMC-200 offers Siemens 810M");
    assert_eq!((&offer.post, offer.why), (&PostRef::Named(POST_NAME.into()), OfferWhy::Machine));
}

#[test]
fn vmc200_spindle_limit_gates_excess_rpm() {
    let fast = build_air_cut_world(5500.0);
    let out = std::env::temp_dir().join(format!("camcraft_810m_fast_{}.mpf", std::process::id()));
    let err = posting::post_setup(&fast, &out).expect_err("5500 rpm exceeds VMC-200 4500 rpm");
    let _ = std::fs::remove_file(&out);
    assert!(err.contains("4500") && err.contains("5500"), "{err}");
}

#[test]
fn air_cut_kit_posts_cleanly_and_matches_committed_kit() {
    let world = build_air_cut_world(2500.0);
    let tmp = std::env::temp_dir().join(format!("camcraft_810m_kit_{}.mpf", std::process::id()));
    let posted = posting::post_setup(&world, &tmp).expect("air-cut kit posts clean");
    let _ = std::fs::remove_file(&tmp);
    assert_eq!(posted.ops, 2);
    let mpf = posted.program;
    assert!(mpf.starts_with("%MPF1\n"), "{mpf}");
    assert!(mpf.contains("G71 G90 G94 G54 G40 G80"), "{mpf}");
    assert!(mpf.contains("L83 P1"), "{mpf}");
    assert!(mpf.contains("G0 G53 Z0 D0\n"), "{mpf}");
    assert!(mpf.ends_with("M30\n"), "{mpf}");
    assert!(mpf.len() < 8192, "fits easily in base 31KB 810M memory ({} bytes)", mpf.len());
    for l in mpf.lines() {
        assert!(l.len() <= 120, "810M 120-char limit: {l}");
    }

    let mpf_path = repo(&format!("{KIT}/air_cut_vmc200.mpf"));
    let proj_path = repo(&format!("{KIT}/air_cut_vmc200.camproj"));
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::create_dir_all(repo(KIT)).expect("kit dir");
        std::fs::write(&mpf_path, &mpf).expect("write air_cut_vmc200.mpf");
        camcraft_app::project::save(&world, &proj_path).expect("save air_cut_vmc200.camproj");
    } else {
        let want = std::fs::read_to_string(&mpf_path).expect("committed air_cut_vmc200.mpf");
        assert_eq!(mpf, want, "kit .mpf stale; regenerate with WRITE_GOLDEN=1");
        let proj = std::fs::read_to_string(&proj_path).expect("committed air_cut_vmc200.camproj");
        assert!(proj.contains("EMCO VMC-200") && proj.contains("Siemens 810M"), "kit project identifies machine and post");
        let readme = std::fs::read_to_string(repo(&format!("{KIT}/README.md"))).expect("kit README.md");
        assert!(readme.contains("%MPF1") && readme.contains("+50") && readme.contains("G53 Z0"), "README covers safe air-cut steps");
    }
    let _ = Point3::ZERO;
}

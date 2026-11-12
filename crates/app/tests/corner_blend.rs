//! M11.1 B/C through the app: blended contours and pockets verify as
//! silent as their sharp twins (the verifier's own eyes on the gouge
//! law), a blended pocket op generates clean and posts its fillets as
//! G2/G3 words, `blend_turns` moves the cache stamp, and the key is
//! written to the project file only when ON - so every legacy save
//! stays byte-identical.
//!
//! Gate map (M11.1): covers app::commands, motion::machining2d, app::operation, app::posting, app::project, sim::verify.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::machining2d::{
    contour, pocket, ContourParams, Lead, PocketParams, PocketStrategy, Side,
};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::{Mode, ToolGeom, VerifyContext, verify};
use camcraft_app::world::new_world;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()
}

fn plate_with_island() -> Region {
    Region::new(vec![rect(0.0, 0.0, 30.0, 30.0), rect(10.0, 10.0, 20.0, 20.0)]).unwrap()
}

fn tool() -> ToolGeom {
    ToolGeom {
        radius: 4.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 9.0,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    }
}

fn ctx<'a>(region: &'a Region, stock: Option<&'a Region>, tool: &'a ToolGeom, mode: Mode) -> VerifyContext<'a> {
    VerifyContext {
        region,
        stock,
        model: None,
        tool,
        cutter: None,
        mode,
        stock_top_z: 0.0,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    }
}

fn cp(blend: bool) -> ContourParams {
    ContourParams {
        tool_diameter: 8.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 8.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: blend,
        links: camcraft_app::links::Links::PLANAR,
    }
}

fn pp(strategy: PocketStrategy, blend: bool) -> PocketParams {
    PocketParams {
        links: camcraft_app::links::Links::PLANAR,
        tool_diameter: 8.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 400.0,
        plunge_feed: 80.0,
        climb: true,
        strategy,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: blend,
    }
}

#[test]
fn blended_contour_verifies_as_silent_as_the_sharp_one() {
    // The island's grown ring of the plate-with-island contour (Auto:
    // the Outside law) holds the hole's sharp corners; they round.
    let r = plate_with_island();
    let t = contour(&r, &cp(true)).unwrap();
    assert!(t.arc_count() > contour(&r, &cp(false)).unwrap().arc_count(), "{:?}", t.notes);
    assert_eq!(verify(&t, &ctx(&r, None, &tool(), Mode::Boundary)), Vec::new());
}

#[test]
fn blended_pockets_verify_silent() {
    let r = plate_with_island();
    let stock = Region::new(vec![rect(0.0, 0.0, 30.0, 30.0)]).unwrap();
    for strategy in [PocketStrategy::Zigzag, PocketStrategy::Waterline] {
        let t = pocket(&r, &pp(strategy, true)).unwrap();
        assert!(t.notes.iter().any(|n| n.contains("rounded")), "{strategy:?}: {:?}", t.notes);
        assert_eq!(
            verify(&t, &ctx(&r, Some(&stock), &tool(), Mode::Area)),
            Vec::new(),
            "{strategy:?}"
        );
    }
}

fn pocket_op(blend: bool) -> OpParams {
    OpParams::Pocket {
        links: camcraft_app::links::Links::PLANAR,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(15.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: blend,
        tilt: None,
    }
}

fn contour_op(blend: bool) -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: Height::fixed(22.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(15.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Linear { length: 8.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: blend,
        tilt: None,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// 60x60x20 box stock, an 8 mm end mill, one generated op on the stock.
fn job(name: &str, params: OpParams) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    history.execute(&world, stock.expect("box stock"));
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0800-4FL")
        .map(|(_, t)| t)
        .expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0800-4FL", tool).expect("add tool"));
    let op = CreateOperation::new(&world, name, "EM-0800-4FL", SrcRef::Stock, params);
    history.execute(&world, op.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
    (world, history)
}

fn tmp(tag: &str, ext: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-corner-blend-{tag}-{}.{ext}", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
fn a_blended_pocket_generates_clean_and_posts_arc_words() {
    for blend in [false, true] {
        let (world, _h) = job("Pocket1", pocket_op(blend));
        let e = model::find_by_name(&world, "Pocket1").expect("op");
        assert_eq!(operation::status_of(&world, &e), OpStatus::Ok);
        let cache = model::toolpath_of(&world, &e).expect("cached path");
        assert!(cache.warnings.is_empty(), "blend {blend}: {:?}", cache.warnings);
        assert_eq!(cache.path.has_arcs(), blend);
        let nc = tmp(if blend { "on" } else { "off" }, "nc");
        posting::post_setup(&world, &nc).expect("posts");
        let text = std::fs::read_to_string(&nc).expect("written");
        let _ = std::fs::remove_file(&nc);
        let words = |w: &str| text.lines().filter(|l| l.split_whitespace().any(|t| t == w)).count();
        let arcs = words("G2") + words("G3") + words("G02") + words("G03");
        if blend {
            assert_eq!(arcs, cache.path.arc_count(), "one arc word per fillet");
        } else {
            assert_eq!(arcs, 0);
        }
    }
}

#[test]
fn the_stamp_tells_blended_from_sharp() {
    let (world, _h) = job("Pocket1", pocket_op(false));
    for (name, of) in [("Pocket1", pocket_op as fn(bool) -> OpParams), ("Contour1", contour_op)] {
        let stamp = |blend: bool| operation::current_stamp(&world, "EM-0800-4FL", &SrcRef::Stock, &of(blend), name, None);
        assert!(stamp(false).is_some());
        assert_ne!(stamp(false), stamp(true), "{name}: a sharp cache never answers for a blended op");
    }
}

#[test]
fn the_key_is_written_only_when_on_and_round_trips() {
    for (name, of) in [("Pocket1", pocket_op as fn(bool) -> OpParams), ("Contour1", contour_op)] {
        for blend in [false, true] {
            let (world, _h) = job(name, of(blend));
            let path = tmp(&format!("{name}-{blend}"), "camproj");
            project::save(&world, &path).expect("save");
            let first = std::fs::read_to_string(&path).expect("read");
            assert_eq!(first.contains("blend_turns"), blend, "legacy saves carry no new key");
            let world2 = new_world();
            project::load(&world2, &path).expect("load");
            project::save(&world2, &path).expect("resave");
            let second = std::fs::read_to_string(&path).expect("reread");
            let _ = std::fs::remove_file(&path);
            assert_eq!(first, second, "round-trip is byte-identical");
            let e = model::find_by_name(&world2, name).expect("loaded op");
            assert_eq!(operation::status_of(&world2, &e), OpStatus::Ok, "stamp survives");
            assert_eq!(model::operation_of(&world2, &e).map(|o| o.params), Some(of(blend)));
        }
    }
}

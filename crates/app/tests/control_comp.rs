//! Control-side cutter compensation through the document (M13.11): a
//! Contour op set to In control posts the PART contour under G41/G42
//! with the tool's D register through the Fanuc post - the golden
//! `golden/nc/contour_control_comp.nc` locks the text - while the same
//! op in computer posts the centre path every earlier golden holds.
//! Haas, Mazak and Siemens 840D write their own comp words (#39) to
//! `golden/nc/<dialect>/contour_control_comp.nc`; the lathe post,
//! Heidenhain and Tormach refuse it by name, and so does a user script
//! that lacks the hook, so a Control op can never leave as an
//! undersize part.
//!
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test control_comp`
//!
//! Gate map (M4.4): covers app::commands, motion::links, motion::machining2d,
//! app::model, app::operation, post::post, app::posting, app::project.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::links::{Approach, Compensation, Links};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

fn post_text(stem: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../assets/posts/{stem}.cps"));
    std::fs::read_to_string(path).expect("post script")
}

fn contour_params(comp: Compensation) -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Linear { length: 6.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: Links { approach: Approach { compensation: comp, ..Approach::DEFAULT }, ..Links::PLANAR },
    }
}

/// The posting fixture's box (10 x 10 x 5 at the origin) under one
/// contour with a 6 mm end mill, generated and silent.
fn world_with(comp: Compensation) -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    // The stock stands to the part's top, Z 5 (#96): the contour's top
    // is 5, so its rapid down to the plunge height (Z 7) is air - the
    // old 10 mm block put that rapid 3 mm into uncut stock.
    let cmd = SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 5.0 }, Default::default());
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let cmd = CreateOperation::new(&world, "Contour1", "EM-0600-4FL", SrcRef::Entity("part".into()), contour_params(comp));
    history.execute(&world, cmd.expect("create contour"));
    history.execute(&world, GenerateOperation::new(&world, "Contour1").expect("generate"));
    let e = model::find_by_name(&world, "Contour1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("cache");
    assert!(cache.warnings.is_empty(), "the fixture generates silent: {:?}", cache.warnings);
    (world, history)
}

fn stream(world: &World) -> String {
    cnc::serialize(&posting::stream_records(world).expect("records"))
}

#[test]
fn in_computer_records_no_span_and_posts_a_centre_path_without_comp_words() {
    let (world, _) = world_with(Compensation::Computer);
    let e = model::find_by_name(&world, "Contour1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("cache");
    assert!(cache.path.comps.is_empty());
    let text = stream(&world);
    assert!(!text.contains("COMP"), "{text}");
    let out = engine::run(&post_text("fanuc"), &posting::machine_value(&world), &text).unwrap();
    assert!(out.warnings.is_empty());
    assert!(!out.program.contains("G41") && !out.program.contains("G42"), "{}", out.program);
}

#[test]
fn in_control_posts_the_part_contour_under_g42_through_fanuc_to_the_golden() {
    let (world, _) = world_with(Compensation::Control);
    let e = model::find_by_name(&world, "Contour1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("cache");
    let mut levels: Vec<f64> = Vec::new();
    for m in &cache.path.moves {
        if m.kind == camcraft_app::cl::MoveKind::Feed && m.from.z == m.to.z && !levels.contains(&m.to.z) {
            levels.push(m.to.z);
        }
    }
    assert!(levels.len() >= 2, "{levels:?}");
    assert_eq!(cache.path.comps.len(), levels.len(), "one span per level: {:?}", cache.path.notes);
    // The centre path cuts the same levels as the computer path - the
    // ring is seamed mid-run under control, so the moves differ there.
    let (plain, _) = world_with(Compensation::Computer);
    let pe = model::find_by_name(&plain, "Contour1").expect("op");
    assert_eq!(cache.path.moves.len(), model::toolpath_of(&plain, &pe).expect("cache").path.moves.len());
    let text = stream(&world);
    assert!(text.contains("COMP RIGHT 3000\n") && text.contains("COMP OFF\n"), "{text}");
    // The programmed contour is the part's 10 x 10 box - its corners
    // ride the stream verbatim, the centre's 13-off corners do not.
    assert!(text.contains("Feed X 10000 Y 10000 Z"), "{text}");
    let out = engine::run(&post_text("fanuc"), &posting::machine_value(&world), &text).unwrap();
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(out.program.matches("G42 ").count(), levels.len(), "{}", out.program);
    assert!(out.program.contains(" D01 "), "the tool's register: {}", out.program);
    assert_eq!(out.program.matches("G01 G40 ").count(), levels.len(), "{}", out.program);
    assert!(out.program.contains("X10 Y10 "), "the part corner: {}", out.program);
    common::write_golden("nc/contour_control_comp.nc", &out.program);
    // The in-app gate releases it: no warning, the file written.
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft_control_comp_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path).expect("the gate releases a Fanuc program under control comp");
    assert_eq!(posted.program, out.program);
    let _ = std::fs::remove_file(&path);
}

/// The dialects that write the comp words (#39): (post stem, the
/// engage block's leading words, the cancel block's, the D word - None
/// where the tool call's own D selects the edge).
const DIALECTS: &[(&str, &str, &str, Option<&str>)] = &[
    ("haas", "G1 G42 ", "G1 G40 ", Some(" D1 ")),
    ("mazak", "G01 G42 ", "G01 G40 ", Some(" D1 ")),
    ("sinumerik", "G1 G42 ", "G1 G40 ", None),
];

#[test]
fn haas_mazak_and_840d_post_the_part_contour_under_their_own_comp_words_to_the_goldens() {
    let (world, _) = world_with(Compensation::Control);
    let e = model::find_by_name(&world, "Contour1").expect("op");
    let spans = model::toolpath_of(&world, &e).expect("cache").path.comps.len();
    assert!(spans >= 2);
    let text = stream(&world);
    let m = posting::machine_value(&world);
    for (stem, engage, cancel, d) in DIALECTS {
        let out = engine::run(&post_text(stem), &m, &text).unwrap();
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let p = &out.program;
        assert_eq!(p.matches(engage).count(), spans, "{stem}: {p}");
        assert_eq!(p.matches(cancel).count(), spans, "{stem}: {p}");
        assert_eq!(p.matches("G41").count(), 0, "{stem}: {p}");
        match d {
            Some(d) => assert_eq!(p.matches(d).count(), spans, "{stem}: the D word rides each engage: {p}"),
            None => assert!(
                p.lines().filter(|l| l.contains(engage)).all(|l| !l.contains(" D")),
                "{stem}: the tool call's D selects the edge: {p}"
            ),
        }
        assert!(p.contains("X10 Y10 "), "{stem}: the part corner: {p}");
        assert!(!p.contains("WEAR COMP"), "{stem}: no wear note under control: {p}");
        common::write_golden(&format!("nc/{stem}/contour_control_comp.nc"), p);
        let mut path = std::env::temp_dir();
        path.push(format!("camcraft_control_comp_ok_{stem}_{}.nc", std::process::id()));
        let posted = posting::post_script(&world, &path, &post_text(stem)).expect("the gate releases it");
        assert_eq!(&posted.program, p, "{stem}");
        let _ = std::fs::remove_file(&path);
    }
}

#[test]
fn the_lathe_and_the_other_mill_posts_refuse_a_control_op_by_name_and_the_gate_blocks() {
    let (world, _) = world_with(Compensation::Control);
    let text = stream(&world);
    let m = posting::machine_value(&world);
    let out = engine::run(&post_text("fanuc-lathe"), &m, &text).unwrap();
    assert!(out.warnings.iter().any(|w| w.contains("nose-radius compensation")), "{:?}", out.warnings);
    for stem in ["heidenhain", "tormach"] {
        let out = engine::run(&post_text(stem), &m, &text).unwrap();
        assert!(out.warnings.iter().any(|w| w.contains("G41/G42) is not supported")), "{stem}: {:?}", out.warnings);
        let mut path = std::env::temp_dir();
        path.push(format!("camcraft_control_comp_{stem}_{}.nc", std::process::id()));
        let err = posting::post_script(&world, &path, &post_text(stem)).expect_err("the gate blocks");
        assert!(err.contains("post blocked") && err.contains("compensation"), "{err}");
        assert!(!path.exists(), "a blocked post writes nothing");
    }
    // A user script without the hook: refused by the engine itself.
    let bare = "function onLinear() { writeln(\"L\"); }\nfunction onRapid() { writeln(\"R\"); }\n";
    let out = engine::run(bare, &m, &text).unwrap();
    assert!(out.warnings.iter().any(|w| w.contains("does not define onRadiusCompensation")), "{:?}", out.warnings);
}

#[test]
fn an_arc_lead_under_control_is_refused_at_generation_by_name() {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let cmd = SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 10.0 }, Default::default());
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let mut params = contour_params(Compensation::Control);
    if let OpParams::Contour { lead, .. } = &mut params {
        *lead = Lead::Arc { radius: 3.0 };
    }
    let cmd = CreateOperation::new(&world, "Contour1", "EM-0600-4FL", SrcRef::Entity("part".into()), params);
    history.execute(&world, cmd.expect("create contour"));
    // Generation refuses at command creation - the Machining error
    // names the law, and no path is born.
    let why = match GenerateOperation::new(&world, "Contour1") {
        Err(e) => format!("{e:?}"),
        Ok(_) => panic!("an arc lead under control compensation must refuse"),
    };
    assert!(why.contains("control compensation needs a straight lead-in"), "{why}");
    let e = model::find_by_name(&world, "Contour1").expect("op");
    assert!(model::toolpath_of(&world, &e).is_none(), "no path under a refused generation");
}

//! Wear cutter compensation through the document (#38): a Contour op
//! set to Wear posts the CUTTER-CENTRE path under G41/G42 with the
//! tool's D register (which then holds only the wear delta) through the
//! Fanuc post - the golden `golden/nc/contour_wear_comp.nc` locks the
//! text; the mode rides EditOperationParams (undo / redo), the project
//! file (an additive `wear_comp` key) and the cached span's `wear`
//! flag. Haas, Mazak and Siemens 840D write their own comp words (#39)
//! to `golden/nc/<dialect>/contour_wear_comp.nc`; Heidenhain (its
//! radius is always the table's R plus DR), Tormach and the lathe post refuse it
//! by name.
//!
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test wear_comp`
//!
//! Gate map (M4.4): covers app::commands, motion::links, motion::machining2d,
//! app::model, app::operation, post::post, app::posting, app::project.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::links::{Approach, Compensation, Links};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::project;
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

/// The control_comp fixture: a 10 x 10 x 5 box under one contour with
/// a 6 mm end mill, generated and silent.
fn world_with(comp: Compensation) -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
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

fn comp_of(world: &World) -> Compensation {
    let e = model::find_by_name(world, "Contour1").expect("op");
    match model::operation_of(world, &e).expect("operation").params {
        OpParams::Contour { links, .. } => links.approach.compensation,
        other => panic!("contour expected, got {other:?}"),
    }
}

#[test]
fn wear_posts_the_centre_path_under_g42_through_fanuc_to_the_golden() {
    let (world, _) = world_with(Compensation::Wear);
    let e = model::find_by_name(&world, "Contour1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("cache");
    assert!(!cache.path.comps.is_empty() && cache.path.comps.iter().all(|c| c.wear));
    for c in &cache.path.comps {
        assert_eq!(c.programmed.as_slice(), &cache.path.moves[c.first..=c.last], "the centre walk, verbatim");
    }
    let text = stream(&world);
    assert!(text.contains("COMP RIGHT 3000 WEAR\n") && text.contains("COMP OFF\n"), "{text}");
    // The centre path rides the stream: the 10 x 10 box's corners
    // stand 3 mm off, so the part corner (10, 10) is never programmed.
    assert!(!text.contains("Feed X 10000 Y 10000 Z"), "{text}");
    let out = engine::run(&post_text("fanuc"), &posting::machine_value(&world), &text).unwrap();
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let spans = cache.path.comps.len();
    assert_eq!(out.program.matches("G42 ").count(), spans, "{}", out.program);
    assert_eq!(out.program.matches("G01 G40 ").count(), spans, "{}", out.program);
    assert!(out.program.contains(" D01 "), "the tool's register: {}", out.program);
    assert_eq!(out.program.matches("HOLDS THE WEAR DELTA ONLY").count(), 1, "one note per section: {}", out.program);
    common::write_golden("nc/contour_wear_comp.nc", &out.program);
    // The gate releases it.
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft_wear_comp_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path).expect("the gate releases a Fanuc program under wear comp");
    assert_eq!(posted.program, out.program);
    let _ = std::fs::remove_file(&path);
}

/// (post stem, the engage block's leading words, the wear note's
/// register) for the dialects that write the comp words (#39).
const DIALECTS: &[(&str, &str, &str)] =
    &[("haas", "G1 G42 ", "(WEAR COMP - D1 "), ("mazak", "G01 G42 ", "(WEAR COMP - D1 "), ("sinumerik", "G1 G42 ", "; WEAR COMP - D1 ")];

#[test]
fn haas_mazak_and_840d_post_wear_over_the_centre_path_to_the_goldens() {
    let (world, _) = world_with(Compensation::Wear);
    let e = model::find_by_name(&world, "Contour1").expect("op");
    let spans = model::toolpath_of(&world, &e).expect("cache").path.comps.len();
    let text = stream(&world);
    let m = posting::machine_value(&world);
    for (stem, engage, note) in DIALECTS {
        let out = engine::run(&post_text(stem), &m, &text).unwrap();
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        let p = &out.program;
        assert_eq!(p.matches(engage).count(), spans, "{stem}: {p}");
        assert_eq!(p.matches(note).count(), 1, "{stem}: one note per section: {p}");
        assert!(!p.contains("X10 Y10 "), "{stem}: the centre path, never the part corner: {p}");
        common::write_golden(&format!("nc/{stem}/contour_wear_comp.nc"), p);
        let mut path = std::env::temp_dir();
        path.push(format!("camcraft_wear_comp_ok_{stem}_{}.nc", std::process::id()));
        let posted = posting::post_script(&world, &path, &post_text(stem)).expect("the gate releases it");
        assert_eq!(&posted.program, p, "{stem}");
        let _ = std::fs::remove_file(&path);
    }
}

#[test]
fn the_other_posts_refuse_wear_by_name_and_the_gate_blocks() {
    let (world, _) = world_with(Compensation::Wear);
    let text = stream(&world);
    let m = posting::machine_value(&world);
    let out = engine::run(&post_text("fanuc-lathe"), &m, &text).unwrap();
    assert!(out.warnings.iter().any(|w| w.contains("nose-radius compensation")), "{:?}", out.warnings);
    for stem in ["heidenhain", "tormach"] {
        let mut path = std::env::temp_dir();
        path.push(format!("camcraft_wear_comp_{stem}_{}.nc", std::process::id()));
        let err = posting::post_script(&world, &path, &post_text(stem)).expect_err("the gate blocks");
        assert!(err.contains("post blocked") && err.contains("compensation"), "{stem}: {err}");
        assert!(!path.exists(), "a blocked post writes nothing");
    }
}

#[test]
fn wear_is_an_undoable_param_edit() {
    let (world, mut history) = world_with(Compensation::Computer);
    assert_eq!(comp_of(&world), Compensation::Computer);
    let cmd = EditOperationParams::new(&world, "Contour1", contour_params(Compensation::Wear));
    history.execute(&world, cmd.expect("edit"));
    assert_eq!(comp_of(&world), Compensation::Wear);
    assert!(history.undo(&world));
    assert_eq!(comp_of(&world), Compensation::Computer);
    assert!(history.redo(&world));
    assert_eq!(comp_of(&world), Compensation::Wear);
}

#[test]
fn wear_round_trips_through_the_project_file_with_its_spans() {
    let (world, _) = world_with(Compensation::Wear);
    let path = std::env::temp_dir().join(format!("camcraft_proj_wearcomp_{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"wear_comp\": true") && !first.contains("control_comp"), "{first}");
    assert!(first.contains("\"wear\": true"), "the cached span keeps its flag: {first}");
    let back = camcraft_app::world::new_world();
    project::load(&back, &path).expect("reload");
    project::save(&back, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "wear round-trips byte-identically");
    assert_eq!(comp_of(&back), Compensation::Wear);
    assert_eq!(stream(&back), stream(&world), "the reloaded cache posts the same stream");
    // A Control file never grows the new keys.
    let (ctl, _) = world_with(Compensation::Control);
    let cpath = std::env::temp_dir().join(format!("camcraft_proj_ctlcomp_{}.camproj", std::process::id()));
    project::save(&ctl, &cpath).expect("save control");
    let ctext = std::fs::read_to_string(&cpath).expect("read control");
    let _ = std::fs::remove_file(&cpath);
    assert!(ctext.contains("\"control_comp\": true") && !ctext.contains("wear_comp") && !ctext.contains("\"wear\":"), "{ctext}");
}

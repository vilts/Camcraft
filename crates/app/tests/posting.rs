//! The in-app post (M1.7b): a live world, not a golden stream. The
//! laws here are the ones the file-level goldens cannot see - the
//! validation gate refusing (never writing, never silently posting a
//! stale or warned program), and the merged stream keeping every
//! operation's cycles inside its own section (the AT anchors are
//! stream-global, the dump builds them per-operation).
//!
//! Fixtures reuse the proven operation-level ones: the contour test's
//! model-box (operations.rs) and the drill test's matched-hole peck
//! (M1.6), so any warning here is a posting bug, not a generator one.
//!
//! Gate map (M4.4): covers app::commands, motion::drilling, geometry::geo2d, app::geometry, app::machine, post::machine, motion::machining2d, app::model, app::operation, app::posting, app::schema, sim::sim, geometry::geometry::stock, app::stock_sim, sim::stock_sim, tools::tools, app::world.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, LoadMachine,
    SelectPost, SetSetupWorkOffset, SetStock, SetWcsOrigin,
};
use camcraft_app::drilling::CycleChoice;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{IDENTITY_Q, Point3, cube};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

fn catalog_tool(name: &str) -> camcraft_app::tools::Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name));
    history.execute(world, cmd.expect("catalog tool"));
}

fn box_stock(world: &World, history: &mut History, dx: f64, dy: f64, dz: f64) {
    let cmd = SetStock::new(world, StockShape::Box { dx, dy, dz }, Default::default());
    history.execute(world, cmd.expect("box stock"));
}

fn contour_params() -> OpParams {
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
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

fn drill_params(hole: camcraft_app::drilling::Hole, top_z: f64, clearance_z: f64) -> OpParams {
    OpParams::Drill {
        clearance_z: Height::fixed(clearance_z),
        retract_z: None,
        top_z: Height::fixed(top_z),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 130.0,
        plunge_feed: 100.0,
        holes: vec![hole],
        cross: Vec::new(),
    }
}

fn hole(
    x: f64,
    y: f64,
    radius: f64,
    entry_z: f64,
    floor_z: f64,
    through: bool,
) -> camcraft_app::drilling::Hole {
    camcraft_app::drilling::Hole {
        xy: Point2::new(x, y),
        radius,
        entry_z,
        floor_z,
        through,
        stepped: false,
    }
}

/// Contour1 (a model box, many moves, no cycles) + Drill1 (one matched
/// through hole, one peck cycle) - the two-op world the anchor law
/// needs. Both generate silent: any stream warning here is the post's.
fn contour_and_drill() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    // The stock stands to the part's top, Z 5 (#96): the contour's top
    // is 5, so its rapid down to the plunge height (Z 7) must be air -
    // the old 10 mm block put that rapid 3 mm into uncut stock.
    box_stock(&world, &mut history, 100.0, 100.0, 5.0);
    add_tool(&world, &mut history, "EM-0600-4FL");
    add_tool(&world, &mut history, "DR-0400");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    let cmd = CreateOperation::new(
        &world,
        "Contour1",
        "EM-0600-4FL",
        SrcRef::Entity("part".into()),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create contour"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Contour1").expect("generate contour"),
    );
    let cmd = CreateOperation::new(
        &world,
        "Drill1",
        "DR-0400",
        SrcRef::Stock,
        drill_params(hole(20.0, 20.0, 2.0, 10.0, 0.0, true), 10.0, 14.0),
    );
    history.execute(&world, cmd.expect("create drill"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Drill1").expect("generate drill"),
    );
    for op in ["Contour1", "Drill1"] {
        let e = model::find_by_name(&world, op).expect("op");
        let cache = model::toolpath_of(&world, &e).expect("cache");
        assert!(
            cache.warnings.is_empty(),
            "{op} fixture must generate silent: {:?}",
            cache.warnings
        );
    }
    (world, history)
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "camcraft-posting-{}-{}.nc",
        tag,
        std::process::id()
    ));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
fn two_ops_post_to_one_program_cycles_stay_in_their_section() {
    let (world, _h) = contour_and_drill();
    let path = tmp_nc("two");
    let posted = posting::post_setup(&world, &path).expect("a fresh, silent world posts");
    assert_eq!(posted.ops, 2, "both sections are in the program");
    let text = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);

    // The two section preambles split the program; the cycle block
    // must live in the drill's half. Without the merge-time anchor
    // offset, normalize would re-seat the cycle before the stream's
    // move #0 - inside the contour - and its inCycle flag would
    // swallow the contour's chain: the silent-wrong-answer this
    // pairing exists to catch.
    let parts: Vec<&str> = text.split("G90 G54 G40 G49 G80").collect();
    assert_eq!(parts.len(), 3, "one preamble per section: {text}");
    assert!(parts[1].contains("T02 M6"), "EM-0600-4FL is library rank 2");
    assert!(
        parts[1].contains("G01") || parts[1].contains("G02"),
        "the contour chain posted"
    );
    assert!(
        !(parts[1].contains("G73") || parts[1].contains("G81")),
        "no cycle belongs in the contour's section: {}",
        parts[1]
    );
    assert!(parts[2].contains("T01 M6"), "DR-0400 is library rank 1");
    assert!(
        parts[2].contains("G99"),
        "the cycle returns to R (#994)"
    );
    assert!(
        parts[2].contains("G73 X20 Y20"),
        "the peck block, at its hole: {}",
        parts[2]
    );
    assert!(
        text.starts_with('%') && text.trim_end().ends_with('%'),
        "one % wrapped program"
    );
    assert_eq!(text.matches("M30").count(), 1, "one footer");
}

#[test]
fn an_unborn_toolpath_refuses_to_post() {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("create"));
    // Never generated: the gate refuses by name, and writes nothing.
    let path = tmp_nc("never");
    let err = posting::post_setup(&world, &path).expect_err("an empty path must not post");
    assert!(
        err.contains("Face1") && err.contains("generate it first"),
        "{err}"
    );
    assert!(!path.exists(), "a refusal leaves no file");
}

fn face_params() -> OpParams {
    OpParams::Face { holes: Default::default(), blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

#[test]
fn a_stale_operation_blocks_the_post_and_leaves_the_file_untouched() {
    let (world, mut history) = contour_and_drill();
    let path = tmp_nc("stale");
    posting::post_setup(&world, &path).expect("fresh posts");
    let before = std::fs::read_to_string(&path).expect("written");

    // The drill op reads the stock, so moving the part goes stale
    // through the world-Aabb digest term.
    let e = model::find_by_name(&world, "part").expect("part");
    model::set_transform(
        &world,
        &e,
        Location {
            pos: Point3::new(3.0, 0.0, 0.0),
            rot: IDENTITY_Q,
        },
    );
    let err = posting::post_setup(&world, &path).expect_err("stale blocks");
    assert!(
        err.contains("Contour1") && err.contains("regenerate"),
        "{err}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("still there"),
        before,
        "a refusal rewrites nothing"
    );

    // Regenerating through the commands, in program order, clears it
    // and the post runs again (issue #89: Drill1 reads what Contour1
    // ahead of it removed, so Contour1's new path moves Drill1 too).
    for op in ["Contour1", "Drill1"] {
        history.execute(&world, GenerateOperation::new(&world, op).expect("regenerate"));
    }
    posting::post_setup(&world, &path).expect("fresh again posts");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_warned_path_blocks_even_though_it_generated() {
    // A ShankHit fixture: generation succeeds WITH the warning (the
    // user must be able to read the numbers). The post gate is where
    // that warning becomes a refusal - the path exists, is fresh, and is
    // still not safe to machine. (Once the M1.6 deep hole; issue #84
    // refuses a hole past the flute at generation, so the warned path is
    // now a contour whose Ø8 shank, 2 mm over a Ø6 flute, rides 1 mm
    // into the part's wall.)
    let world = new_world();
    let mut history = History::default();
    // Stock to the part's top (#96), so the shank is the only warning.
    box_stock(&world, &mut history, 100.0, 100.0, 5.0);
    let mut stub = catalog_tool("EM-0600-4FL");
    stub.flute_len = 2.0;
    stub.shank_d = 8.0;
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", stub).expect("stub tool"));
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    let cmd = CreateOperation::new(
        &world,
        "Deep1",
        "EM-0600-4FL",
        SrcRef::Entity("part".into()),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Deep1").expect("generate"),
    );
    let e = model::find_by_name(&world, "Deep1").expect("op");
    assert!(
        !model::toolpath_of(&world, &e)
            .expect("cache")
            .warnings
            .is_empty(),
        "the fixture must carry its warning"
    );

    let path = tmp_nc("warned");
    let err = posting::post_setup(&world, &path).expect_err("a warning blocks the gate");
    assert!(
        err.contains("post blocked") && err.contains("shank"),
        "{err}"
    );
    assert!(!path.exists(), "blocked output is never written");
}

#[test]
fn a_gouging_program_is_refused_by_the_verification_gate() {
    // Stage D's own law: the program-level sim is no longer just an
    // informed verdict - it is the gate. The fixture is Deviate's
    // stage-B sentence made into a world: a face 0.5 mm below the top
    // of a model that fills the stock. Generation is SILENT (the face
    // verifies against the stock, not the model - that net is this
    // gate's job), so the refusal below can only come from the sim.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(
        &world,
        ImportModel::from_mesh(
            "part",
            cube(
                Point3::new(-30.0, -30.0, 0.0),
                Point3::new(30.0, 30.0, 20.0),
            ),
        ),
    );
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("generate"),
    );
    let e = model::find_by_name(&world, "Face1").expect("op");
    assert!(
        model::toolpath_of(&world, &e)
            .expect("cache")
            .warnings
            .is_empty(),
        "the fixture must generate silent - the sim is the only net here"
    );

    let path = tmp_nc("gouge");
    let err = posting::post_setup(&world, &path).expect_err("a gouged program must not post");
    assert!(
        err.contains("post blocked") && err.contains("gouge:") && err.contains("below the model"),
        "{err}"
    );
    assert!(!path.exists(), "the gate writes nothing on refusal");

    // The gate's evidence is the Deviate cell's evidence: the same
    // walk, so the count in the sentence is the sim's own.
    let sim = camcraft_app::stock_sim::simulate_document(&world).expect("sim runs");
    let lines = posting::verification_gate(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains(&format!("{} cell(s)", sim.dev.gouge_cells)),
        "{}",
        lines[0]
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn machine_limits_block_the_post_and_within_limits_does_not() {
    // The mounted machine joins the gate with the same walk stage C
    // informs with. No model in this world: the sim judges nothing, so
    // any refusal here can only be the machine's sentence.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("generate"),
    );

    let tight = camcraft_app::machine::Machine {
        model: "TestMill 1".into(),
        vendor: "CamCraft Testbench".into(),
        spindle_min: Some(50.0),
        spindle_max: Some(10000.0), // EM-0400-2FL runs 12000
        ..Default::default()
    };
    history.execute(
        &world,
        LoadMachine::new(&world, "tight.mch".into(), Some(tight.clone())).expect("sane"),
    );
    let path = tmp_nc("limits");
    let err = posting::post_setup(&world, &path).expect_err("the ceiling blocks");
    assert!(
        err.contains("machine TestMill 1: 1 limit problem(s)")
            && err.contains("above the machine's 10000"),
        "{err}"
    );
    assert!(!path.exists(), "a limit refusal writes nothing");

    // The same program under a machine that can give it: the gate
    // opens without touching the program - it never re-plans.
    let roomy = camcraft_app::machine::Machine {
        spindle_max: Some(12000.0),
        ..tight
    };
    history.execute(
        &world,
        LoadMachine::new(&world, "roomy.mch".into(), Some(roomy)).expect("sane"),
    );
    assert!(posting::verification_gate(&world).is_empty());
    posting::post_setup(&world, &path).expect("within limits posts");
    assert!(path.exists(), "and writes");
    let _ = std::fs::remove_file(&path);

    // No machine mounted, nothing claimed (the unstated-limit law's
    // gate half): clearing the machine cannot make this program illegal.
    history.execute(
        &world,
        LoadMachine::new(&world, String::new(), None).expect("clear"),
    );
    posting::post_setup(&world, &path).expect("an unmounted checker blocks nothing");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_program_the_sim_cannot_judge_still_posts() {
    // The refusal-to-judge is not a judgment: a model-sourced contour
    // with fixed heights and NO stock gives the program sim nothing to
    // chew, so the gate holds no verdict and must not borrow one. The
    // per-op WARN gate is the net under this world, and it is silent.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    let cmd = CreateOperation::new(
        &world,
        "Contour1",
        "EM-0600-4FL",
        SrcRef::Entity("part".into()),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Contour1").expect("generate"),
    );
    let e = model::find_by_name(&world, "Contour1").expect("op");
    assert!(
        model::toolpath_of(&world, &e)
            .expect("cache")
            .warnings
            .is_empty(),
        "generation must be silent - the sim's silence here is the tested thing"
    );
    assert!(
        camcraft_app::stock_sim::simulate_document(&world).is_err(),
        "fixture: no stock, no judge"
    );
    assert!(
        posting::verification_gate(&world).is_empty(),
        "no verdict, no veto"
    );
    posting::post_setup(&world, &tmp_nc("nostock")).expect("unjudged is not unsafe");
}

#[test]
fn an_empty_document_refuses_before_anything_else() {
    let world = new_world();
    let err = posting::post_setup(&world, &tmp_nc("empty")).expect_err("nothing to post");
    assert_eq!(err, "no operations to post");
}

#[test]
fn a_moved_wcs_origin_moves_the_posted_numbers() {
    // The context law: the posted program measures from the slot's
    // origin as the machine would - the OriginX/Y/Z PARAMs must carry
    // it, so a relocated WCS shows up as geometry that moved in XY,
    // not as a silent G55 vs G54 mismatch.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("generate"),
    );

    // Both the default (no slot placed) and the shifted program.
    let path = tmp_nc("wcs");
    posting::post_setup(&world, &path).expect("faces post");
    let plain = std::fs::read_to_string(&path).expect("written");

    history.execute(
        &world,
        SetWcsOrigin::new(
            &world,
            Location {
                pos: Point3::new(10.0, 5.0, 1.0),
                rot: IDENTITY_Q,
            },
        )
        .expect("set G54"),
    );
    // The face's heights are Fixed: the origin is pure post context
    // (the OriginX/Y/Z PARAMs), not a digest input - the toolpath is
    // right where it was, the machine underneath it moved. The posted
    // numbers must follow the origin, the stamp must not.
    posting::post_setup(&world, &path).expect("still fresh: WCS moved the machine, not the inputs");
    let moved = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert!(moved.contains("G90 G54"), "still the G54 word");
    assert_ne!(moved, plain, "the origin rode the PARAMs into the program");
}

#[test]
fn a_non_g54_slot_is_what_the_script_writes() {
    // The WorkOffset PARAM is grammar-driven: an op of a setup that
    // is labelled G55 (#196, #342) - its heights measured from the setup's
    // one WCS (#329) - posts its motions under G55.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    // A heights-referenced WCS must be placed before it can be measured
    // from (the same law the panels enforce by refusing to resolve).
    history.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    history.execute(
        &world,
        SetWcsOrigin::new(&world, Location::default()).expect("place the WCS"),
    );
    history.execute(&world, SetSetupWorkOffset::new(&world, "Setup1", Some(1)).expect("pick G55"));
    let params = OpParams::Face { holes: Default::default(), blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::WorkOffset, 25.0),
        cut_z: Height::of(HeightReference::WorkOffset, 19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("generate"),
    );
    let path = tmp_nc("g55");
    posting::post_setup(&world, &path).expect("posts");
    let text = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert!(
        text.contains("G90 G55"),
        "the script took the slot from the PARAM: {text}"
    );
}

#[test]
fn posted_carries_line_positions_matching_the_move_chain() {
    // M1.7d viewer contract, the app half: every position a program
    // line resolves to is the endpoint of some real move (the map
    // invents nothing), at least every cycle-free contour move gets a
    // positioned line (coverage), and the first positioned line is the
    // stream's first move (order). Floats compare on the lattice law's
    // tolerance, not bitwise: the round trip is mm -> um -> mm.
    let (world, _h) = contour_and_drill();
    let path = tmp_nc("linepos");
    let posted = posting::post_setup(&world, &path).expect("a fresh, silent world posts");
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        posted.line_pos.len(),
        posted.lines,
        "one slot per program line"
    );
    assert_eq!(
        posted.lines,
        posted.program.lines().count(),
        "the retained text is the program"
    );

    let mut endpoints: Vec<[f64; 3]> = Vec::new();
    let mut contour_moves = 0usize;
    for op in ["Contour1", "Drill1"] {
        let e = model::find_by_name(&world, op).expect("op");
        let tp = model::toolpath_of(&world, &e).expect("cached path");
        if op == "Contour1" {
            contour_moves = tp.path.moves.len();
        }
        endpoints.extend(tp.path.moves.iter().map(|m| [m.to.x, m.to.y, m.to.z]));
    }
    let same = |a: &[f64; 3], b: &[f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
    let resolved: Vec<[f64; 3]> = posted.line_pos.iter().copied().flatten().collect();
    assert!(
        resolved.len() >= contour_moves,
        "the contour's {} moves must each reach the viewer, got {} positioned lines",
        contour_moves,
        resolved.len()
    );
    assert!(
        same(&resolved[0], &endpoints[0]),
        "first positioned line {:?} is not the first move {:?}",
        resolved[0],
        endpoints[0]
    );
    for p in &resolved {
        assert!(
            endpoints.iter().any(|e| same(e, p)),
            "position {p:?} is posted by no move - the viewer would highlight a lie"
        );
    }
}

// ------------------------------------------------------------------- M3.3 A

/// A scratch `.cps` with the given text, named for this process.
fn tmp_cps(tag: &str, text: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "camcraft-posting-{}-{}.cps",
        tag,
        std::process::id()
    ));
    let _ = std::fs::remove_file(&p);
    std::fs::write(&p, text).expect("write temp cps");
    p
}

#[test]
fn the_built_in_is_the_default_post_and_needs_no_choice() {
    // The unchosen document posts exactly what it posted before the
    // picker existed - the built-in is a default, not a dialog.
    let (world, _h) = contour_and_drill();
    assert_eq!(posting::post_of(&world), PostRef::Builtin);
    assert!(
        posting::post_label(&world).contains("Fanuc"),
        "{}",
        posting::post_label(&world)
    );
    let (label, cps) = posting::script_source(&world).expect("the built-in always resolves");
    assert_eq!(
        cps,
        posting::BUILTINS[0].1,
        "the built-in is the shipped script text"
    );
    assert!(label.contains("built-in"));
}

#[test]
fn a_mounted_machine_names_the_program_and_speaks_only_its_own_coolant() {
    // M3.3 A's whole claim in one program: the document's `.mch` - not
    // the built-in default - is what the SCRIPT sees. The header names
    // the mounted model, and the coolant word disappears with the
    // coolant the file never states (honest absence, the
    // unstated-limit law in coolant clothes). The limit gate already
    // judged this machine in M2.3; this proves the same fact reaches
    // the output, not just the veto.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("generate"),
    );

    let path = tmp_nc("mount-name");
    posting::post_setup(&world, &path).expect("the unmounted default posts");
    let plain = std::fs::read_to_string(&path).expect("written");
    assert!(
        plain.contains("(VMC 3-AXIS)"),
        "the shipped machine names the header: {plain}"
    );
    assert!(plain.contains("M8"), "the shipped VMC states flood coolant");

    let mounted = camcraft_app::machine::Machine {
        model: "TestMill 2".into(),
        vendor: "CamCraft Testbench".into(),
        spindle_min: Some(100.0),
        spindle_max: Some(24000.0), // the 12000 rpm tool is inside it
        axes: vec![
            camcraft_app::machine::AxisLimits {
                name: "X".into(),
                feed: Some(5000.0),
                rapid: Some(9000.0),
                travel: None,
                ..Default::default()
            },
            camcraft_app::machine::AxisLimits {
                name: "Y".into(),
                feed: Some(5000.0),
                rapid: Some(9000.0),
                travel: None,
                ..Default::default()
            },
            camcraft_app::machine::AxisLimits {
                name: "Z".into(),
                feed: Some(3000.0),
                rapid: Some(4000.0),
                travel: None,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    history.execute(
        &world,
        LoadMachine::new(&world, "test2.mch".into(), Some(mounted)).expect("sane"),
    );
    posting::post_setup(&world, &path).expect("the mounted machine posts the same program");
    let text = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert!(
        text.contains("(TestMill 2)"),
        "the mounted identity reaches the script: {text}"
    );
    assert!(
        !text.contains("M8"),
        "a machine that states no coolant gets no coolant words: {text}"
    );

    // The mirror is the whole bridge; the sim clock walks the same one.
    let v = posting::machine_value(&world);
    assert_eq!(v["name"], "TestMill 2");
    let axes = v["kinematics"]["static"]["axes"]
        .as_array()
        .expect("three axes");
    assert_eq!(axes.len(), 3);
    assert_eq!(camcraft_app::sim::Traverse::from_machine(&v).z, 4000.0);
}

#[test]
fn a_mounted_coolant_list_writes_its_own_codes() {
    // M3.3 C finishes A's sentence: the very word A's honest absence
    // suppressed now flows - and it flows from the mount, not from the
    // shipped default the unmounted document speaks.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("generate"),
    );

    let mounted = camcraft_app::machine::Machine {
        model: "CoolMill".into(),
        coolants: vec![
            camcraft_app::machine::Coolant {
                name: "Brine".into(),
                m_code: "M51".into(),
            },
            camcraft_app::machine::Coolant {
                name: "Air".into(),
                m_code: String::new(),
            },
        ],
        ..Default::default()
    };
    let path = tmp_nc("mount-coolant");
    history.execute(
        &world,
        LoadMachine::new(&world, "cool.mch".into(), Some(mounted)).expect("sane"),
    );
    posting::post_setup(&world, &path).expect("the mounted coolant posts");
    let text = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert!(text.contains("(CoolMill)"));
    assert!(text.contains("M51"), "the mount's own code speaks: {text}");
    assert!(
        !text.contains("M8"),
        "the shipped default's M8 is gone with the default: {text}"
    );
}

#[test]
fn a_user_script_is_validated_on_choice_undone_cleanly_and_missed_loudly() {
    // The picker's three laws in one walk: a chosen file REPLACES the
    // built-in in the program's text; the choice is a command (undo
    // puts the built-in back); and a script that cannot run - broken
    // at the pick, vanished at the post - is a refusal that names it,
    // never a silent return to the shipped default.
    let (world, mut history) = contour_and_drill();
    let cps = tmp_cps("ok", "function onOpen() { writeln(\"(USERPOST-7)\"); }");
    let cps_name = cps.display().to_string();
    history.execute(
        &world,
        SelectPost::new(&world, PostRef::File(cps_name.clone())).expect("the script runs"),
    );
    assert!(matches!(posting::post_of(&world), PostRef::File(_)));

    let path = tmp_nc("user");
    posting::post_setup(&world, &path).expect("the user script posts");
    let text = std::fs::read_to_string(&path).expect("written");
    assert!(
        text.contains("USERPOST-7"),
        "the user's own words in the program: {text}"
    );
    assert!(
        !text.contains("O0001"),
        "the built-in's header is gone, not mixed in: {text}"
    );

    assert!(history.undo(&world), "the choice is in the history");
    assert_eq!(
        posting::post_of(&world),
        PostRef::Builtin,
        "undo restores the built-in"
    );
    assert!(history.redo(&world));
    assert!(matches!(posting::post_of(&world), PostRef::File(_)));

    // Re-selecting the same post is a nothing, refused visibly (the
    // LoadMachine "already loaded" law).
    assert!(SelectPost::new(&world, PostRef::File(cps_name.clone())).is_err());

    let bad = tmp_cps("bad", "function onOpen() { throw new Error(\"nope\"); }");
    let err = SelectPost::new(&world, PostRef::File(bad.display().to_string()))
        .expect_err("a script that throws must not enter the history");
    assert!(err.to_string().contains("does not run"), "{err}");
    let _ = std::fs::remove_file(&bad);

    let missing = std::env::temp_dir().join(format!("camcraft-gone-{}.cps", std::process::id()));
    let _ = std::fs::remove_file(&missing);
    let err = SelectPost::new(&world, PostRef::File(missing.display().to_string()))
        .expect_err("an unreadable path is refused at the pick");
    assert!(err.to_string().contains("post script"), "{err}");

    // Chosen, then the file vanishes: post time refuses by name.
    std::fs::remove_file(&cps).expect("chosen post disappears");
    let gone_path = tmp_nc("user-gone");
    let err = posting::post_setup(&world, &gone_path).expect_err("a vanished post is a refusal");
    assert!(
        err.contains("post script") && err.contains(&cps_name),
        "{err}"
    );
    assert!(!gone_path.exists(), "the refusal writes nothing, ever");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_trace_rides_the_program_it_wrote() {
    // Stage D's viewer contract, the app half: the trace pane draws
    // from the SAME run that wrote the file - blocks run forward and
    // stay inside the program, the header and the footer belong to no
    // record, every record that spoke is a block, and the tool change
    // names its own. No re-post: the blocks and the text arrive on
    // one struct or the viewer would be tracing a different program.
    let (world, _h) = contour_and_drill();
    let path = tmp_nc("trace");
    let posted = posting::post_setup(&world, &path).expect("posts");
    let _ = std::fs::remove_file(&path);
    assert_eq!(posted.lines, posted.program.lines().count());
    assert!(!posted.blocks.is_empty(), "a real program leaves a trace");
    let mut cursor = 0usize;
    for b in &posted.blocks {
        assert!(
            b.first >= cursor && b.first <= b.last.max(b.first + 1),
            "blocks run forward: {b:?}"
        );
        assert!(
            b.last <= posted.lines,
            "a block reaches past the program: {b:?}"
        );
        cursor = b.last;
    }
    assert_eq!(
        posted.blocks[0].record, None,
        "the header belongs to no record"
    );
    assert_eq!(posted.blocks[0].label, "header");
    assert_eq!(
        posted.blocks.last().unwrap().record,
        None,
        "so does the footer"
    );
    // The tool record itself writes no line (this post emits T/M6 in
    // the section preamble, from the stored ToolNumber property - the
    // trace shows where lines were WRITTEN, not what was heard), but
    // the sections and the peck cycle the two ops became all ride it.
    assert!(
        posted
            .blocks
            .iter()
            .all(|b| b.record.is_none() || !b.label.starts_with("tool")),
        "a silent record has no empty block: {:?}",
        posted.blocks.iter().map(|b| &b.label).collect::<Vec<_>>()
    );
    assert!(
        posted.blocks.iter().any(|b| b.label.starts_with("cycle")),
        "the canned cycle rides the trace: {:?}",
        posted.blocks.iter().map(|b| &b.label).collect::<Vec<_>>()
    );
    // Every record that wrote a line is named exactly once - the
    // viewer's list is the run, not the stream echoed back.
    let mut seen: Vec<Option<usize>> = Vec::new();
    for b in &posted.blocks {
        if let Some(r) = b.record {
            assert!(
                !seen.contains(&Some(r)),
                "record {r} appears twice in one trace"
            );
            seen.push(Some(r));
        }
    }
}

/// A 40x40x5 block (centered on the origin) with a 16x16 square through
/// hole: one wall quad per side, caps wound outward. Sectioned, the
/// hole stays a sharp-cornered square loop - its tool-radius inset
/// starts the contour's ring on a corner.
fn square_holed_block() -> camcraft_app::geometry::Mesh {
    use camcraft_app::geometry::Triangle;
    let tau = std::f64::consts::TAU;
    let at = |i: usize, half: f64, z: f64| {
        let a = tau * (i % 4) as f64 / 4.0 + tau / 8.0;
        let (x, y) = (a.cos(), a.sin());
        let s = x.abs().max(y.abs());
        Point3::new(x * half / s, y * half / s, z)
    };
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    for i in 0..4 {
        let j = i + 1;
        quad(at(j, 8.0, 5.0), at(i, 8.0, 5.0), at(i, 20.0, 5.0), at(j, 20.0, 5.0), &mut f);
        quad(at(i, 8.0, 0.0), at(j, 8.0, 0.0), at(j, 20.0, 0.0), at(i, 20.0, 0.0), &mut f);
        quad(at(i, 8.0, 5.0), at(j, 8.0, 5.0), at(j, 8.0, 0.0), at(i, 8.0, 0.0), &mut f);
        quad(at(j, 20.0, 5.0), at(i, 20.0, 5.0), at(i, 20.0, 0.0), at(j, 20.0, 0.0), &mut f);
    }
    camcraft_app::geometry::Mesh::from_facets(f)
}

#[test]
fn an_arc_lead_at_a_hole_corner_neither_gouges_nor_blocks_the_post() {
    // The corner-start lead bug end to end: the square hole's inset ring
    // starts on a corner, where the quarter arc used to begin a full
    // tool radius into the wall. The verifier caught it (a Gouge
    // warning) and the gate refused the post - safe, but the generator
    // itself must never emit it: the lead moves to a straight run (or
    // shrinks, or is refused by name) and the program posts clean.
    let world = new_world();
    let mut history = History::default();
    // Stock flush with the block top: the loop-to-loop hop rides the
    // 8 mm retract plane, which must clear the stock.
    box_stock(&world, &mut history, 100.0, 100.0, 5.0);
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("holed", square_holed_block()));
    let mut params = contour_params();
    if let OpParams::Contour { lead, .. } = &mut params {
        *lead = Lead::Arc { radius: 3.0 };
    }
    let cmd = CreateOperation::new(
        &world,
        "Holed1",
        "EM-0600-4FL",
        SrcRef::Entity("holed".into()),
        params,
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Holed1").expect("generate"),
    );
    let e = model::find_by_name(&world, "Holed1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("cache");
    let path = tmp_nc("holed");
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    assert!(
        cache.warnings.is_empty(),
        "the lead must not gouge the hole wall: {:?} / post: {:?}",
        cache.warnings,
        posted.as_ref().err()
    );
    posted.expect("a clean contour posts");
}

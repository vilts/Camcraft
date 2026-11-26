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
    contour_params_of(comp, camcraft_app::machining3d::Entry3D::Plunge)
}

/// #3891: the fixture's shape - the Perpendicular lead (the Linear shape
/// is refused under control and wear: its dive point rides the ring's own
/// offset line) and the entry arm under test.
fn contour_params_of(comp: Compensation, entry: camcraft_app::machining3d::Entry3D) -> OpParams {
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
        lead: Lead::Perpendicular { length: 6.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: Links { approach: Approach { compensation: comp, ..Approach::DEFAULT }, ..Links::PLANAR },
        chain_overrides: Vec::new(),
        picks: Default::default(),
    }
}

/// The posting fixture's box (10 x 10 x 5 at the origin) under one
/// contour with a 6 mm end mill, generated and silent.
fn world_with(comp: Compensation) -> (World, History) {
    world_with_entry(comp, camcraft_app::machining3d::Entry3D::Plunge)
}

/// [`world_with`] under one entry arm (#3891's Arc pin).
fn world_with_entry(comp: Compensation, entry: camcraft_app::machining3d::Entry3D) -> (World, History) {
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
    let cmd = CreateOperation::new(&world, "Contour1", "EM-0600-4FL", SrcRef::Entity("part".into()), contour_params_of(comp, entry));
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
fn in_control_posts_the_part_contour_under_g41_through_fanuc_to_the_golden() {
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
    let mut plain_levels: Vec<f64> = Vec::new();
    for m in &model::toolpath_of(&plain, &pe).expect("cache").path.moves {
        if m.kind == camcraft_app::cl::MoveKind::Feed && m.from.z == m.to.z && !plain_levels.contains(&m.to.z) {
            plain_levels.push(m.to.z);
        }
    }
    assert_eq!(levels, plain_levels, "the control op cuts the computer op's levels");
    let text = stream(&world);
    assert!(text.contains("COMP LEFT 3000\n") && text.contains("COMP OFF\n"), "{text}");
    // The programmed contour is the part's 10 x 10 box - its corners
    // ride the stream verbatim, the centre's 13-off corners do not.
    assert!(text.contains("Feed X 10000 Y 10000 Z"), "{text}");
    let out = engine::run(&post_text("fanuc"), &posting::machine_value(&world), &text).unwrap();
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(out.program.matches("G41 ").count(), levels.len(), "{}", out.program);
    assert!(out.program.contains(" D01 "), "the tool's register: {}", out.program);
    assert_eq!(out.program.matches("G01 G40 ").count(), levels.len(), "{}", out.program);
    assert!(out.program.contains("X10. Y10. "), "the part corner: {}", out.program);
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
    ("haas", "G1 G41 ", "G1 G40 ", Some(" D1 ")),
    ("mazak", "G01 G41 ", "G01 G40 ", Some(" D1 ")),
    ("sinumerik", "G1 G41 ", "G1 G40 ", None),
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
        // #1555: climb outside (M3) is G41 - never G42.
        assert_eq!(p.matches("G42").count(), 0, "{stem}: {p}");
        match d {
            Some(d) => assert_eq!(p.matches(d).count(), spans, "{stem}: the D word rides each engage: {p}"),
            None => assert!(
                p.lines().filter(|l| l.contains(engage)).all(|l| !l.contains(" D")),
                "{stem}: the tool call's D selects the edge: {p}"
            ),
        }
        // #1836: the Fanuc family writes a whole length with its point.
        let corner = if *stem == "sinumerik" { "X10 Y10 " } else { "X10. Y10. " };
        assert!(p.contains(corner), "{stem}: the part corner: {p}");
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
    for stem in ["heidenhain", "tormach", "grblhal", "tinyg", "linuxcnc", "grbl", "mach3"] {
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

/// #3888: the box contour of `world_with` on `side` with `entry`, three
/// levels deep, generated; `Err` carries the generation's refusal.
fn ramped_world(comp: Compensation, side: Side, entry: camcraft_app::machining3d::Entry3D) -> (World, Result<(), String>) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let cmd = SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 5.0 }, Default::default());
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(40.0, 40.0, 5.0))));
    let mut params = contour_params(comp);
    if let OpParams::Contour { side: s, entry: e, links, .. } = &mut params {
        *s = side;
        *e = entry;
        links.approach.ramp_angle = 5.0;
    }
    let cmd = CreateOperation::new(&world, "Contour1", "EM-0600-4FL", SrcRef::Entity("part".into()), params);
    history.execute(&world, cmd.expect("create contour"));
    match GenerateOperation::new(&world, "Contour1") {
        Ok(cmd) => {
            history.execute(&world, cmd);
            (world, Ok(()))
        }
        Err(e) => (world, Err(format!("{e:?}"))),
    }
}

#[test]
fn a_ramp_or_helix_before_g41_under_control_or_wear_is_refused_and_posts_nothing() {
    use camcraft_app::machining3d::Entry3D;
    // #3888: the ramp ran on the ring under G40, before the lead-in that
    // engages G41/G42 - a register over the nominal radius, or a positive
    // wear value, cut the wall. The generation now refuses it by name, and
    // the post gate writes no compensated program. The door reads only the
    // parameters, before any geometry: the motion pin
    // `a_ramp_or_helix_under_control_or_wear_is_refused_by_name_on_both_sides`
    // holds the bore side (Side::Inside), which a whole-model source refuses.
    for comp in [Compensation::Control, Compensation::Wear] {
        for side in [Side::Outside, Side::Auto] {
            for (entry, name) in [(Entry3D::Ramp, "Ramp"), (Entry3D::Helix, "Helix")] {
                let (world, made) = ramped_world(comp, side, entry);
                let why = made.expect_err("a ramp before G41/G42 must refuse");
                let want = format!("{} compensation does not ride a {name} entry", comp.label());
                assert!(why.contains(&want), "{comp:?} {side:?} {name}: {why}");
                let e = model::find_by_name(&world, "Contour1").expect("op");
                assert!(model::toolpath_of(&world, &e).is_none(), "no path under a refused generation");
                let mut path = std::env::temp_dir();
                path.push(format!("camcraft_3888_{}_{comp:?}_{side:?}_{name}.nc", std::process::id()));
                if let Ok(posted) = posting::post_setup(&world, &path) {
                    assert!(
                        !posted.program.contains("G41") && !posted.program.contains("G42"),
                        "no compensated program from a refused op: {}",
                        posted.program
                    );
                }
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

#[test]
fn a_wall_side_ramp_in_computer_posts_its_ramp_without_comp_words() {
    use camcraft_app::machining3d::Entry3D;
    // #3888 leaves computer compensation as it was: the ramp stands, no span.
    let (world, made) = ramped_world(Compensation::Computer, Side::Outside, Entry3D::Ramp);
    made.expect("computer ramp generates");
    let e = model::find_by_name(&world, "Contour1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("cache");
    assert!(cache.path.comps.is_empty());
    let ramps = cache.path.moves.iter().filter(|m| {
        m.kind == camcraft_app::cl::MoveKind::Feed && m.to.z < m.from.z && (m.to.x != m.from.x || m.to.y != m.from.y)
    });
    assert!(ramps.count() >= 2, "the zig-zag ramp stands: {:?}", cache.path.notes);
    let out = engine::run(&post_text("fanuc"), &posting::machine_value(&world), &stream(&world)).unwrap();
    assert!(!out.program.contains("G41") && !out.program.contains("G42"), "{}", out.program);
}

/// #3891: a Contour under control or wear with a Linear lead-in or
/// lead-out, generated; `Err` carries the generation's refusal.
fn linear_world(comp: Compensation, lead: Lead, lead_out: Option<Lead>) -> (World, Result<(), String>) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let cmd = SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 5.0 }, Default::default());
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let mut params = contour_params(comp);
    if let OpParams::Contour { lead: l, links, .. } = &mut params {
        *l = lead;
        links.approach.lead_out = lead_out;
    }
    let cmd = CreateOperation::new(&world, "Contour1", "EM-0600-4FL", SrcRef::Entity("part".into()), params);
    history.execute(&world, cmd.expect("create contour"));
    match GenerateOperation::new(&world, "Contour1") {
        Ok(cmd) => {
            history.execute(&world, cmd);
            (world, Ok(()))
        }
        Err(e) => (world, Err(format!("{e:?}"))),
    }
}

#[test]
fn a_linear_lead_in_or_out_under_control_or_wear_is_refused_and_posts_nothing() {
    // #3891: a Linear lead's start (and the mirrored lead-out's end)
    // ride the ring's own offset line, so the Plunge's dive there - and
    // the Arc entry's chord, tangent to the lead - post under G40 one
    // nominal radius off the finished wall: a register over the nominal
    // radius, or a positive wear value, cut the wall before G41/G42
    // engages. The generation refuses the shape by name, and the post
    // gate writes no compensated program. The bore side is held by the
    // motion pin `a_linear_lead_under_control_or_wear_is_refused_by_name_on_both_sides`.
    for comp in [Compensation::Control, Compensation::Wear] {
        for (lead, lead_out, end) in [
            (Lead::Linear { length: 6.0 }, Some(Lead::Perpendicular { length: 6.0 }), "lead-in"),
            (Lead::Perpendicular { length: 6.0 }, Some(Lead::Linear { length: 6.0 }), "lead-out"),
        ] {
            let (world, made) = linear_world(comp, lead, lead_out);
            let why = made.expect_err("a Linear lead under compensation must refuse");
            let want = format!("{} compensation does not ride a Linear {end}", comp.label());
            assert!(why.contains(&want), "{comp:?} {end}: {why}");
            assert!(why.contains("pick the Perpendicular"), "{comp:?} {end}: {why}");
            let e = model::find_by_name(&world, "Contour1").expect("op");
            assert!(model::toolpath_of(&world, &e).is_none(), "no path under a refused generation");
            let mut path = std::env::temp_dir();
            path.push(format!("camcraft_3891_linear_{}_{}_{comp:?}_{end}.nc", std::process::id(), lead_out.is_some()));
            if let Ok(posted) = posting::post_setup(&world, &path) {
                assert!(
                    !posted.program.contains("G41") && !posted.program.contains("G42"),
                    "no compensated program from a refused op: {}",
                    posted.program
                );
            }
            let _ = std::fs::remove_file(&path);
        }
        // Computer takes both Linear ends: the option narrows only the
        // compensated modes.
        let (world, made) = linear_world(
            Compensation::Computer,
            Lead::Linear { length: 6.0 },
            Some(Lead::Linear { length: 6.0 }),
        );
        made.expect("computer takes the Linear lead");
        let e = model::find_by_name(&world, "Contour1").expect("op");
        let cache = model::toolpath_of(&world, &e).expect("cache");
        assert!(cache.path.comps.is_empty(), "computer records no span");
    }
}

/// #3891: the XY clearance of one position from the finished wall - the
/// part's 10 x 10 box (the fixture cuts it with no stock to leave).
fn wall_clear(x: f64, y: f64) -> f64 {
    let ox = (0.0 - x).max(x - 10.0);
    let oy = (0.0 - y).max(y - 10.0);
    ox.max(0.0).hypot(oy.max(0.0))
}

/// #3891: every position the machine stands at while G40 is active,
/// read from a Fanuc program's absolute X/Y words: the stands between
/// the cancels and the engages (each G41 line contributes the stand
/// before it - the dive point - and each G40 line's end is itself a
/// nominal stand). The G91 tail is not read.
fn under_g40_stands(program: &str) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let (mut x, mut y) = (0.0f64, 0.0f64);
    let mut have = [false; 2];
    let mut comp = false;
    'lines: for raw in program.lines() {
        let line = match raw.find('(') {
            Some(i) => &raw[..i],
            None => raw,
        };
        for w in line.split_whitespace() {
            match w {
                "G91" => break 'lines,
                "G40" => comp = false,
                "G41" | "G42" => {
                    if have[0] && have[1] {
                        out.push((x, y));
                    }
                    comp = true;
                }
                _ => {}
            }
            if let Some(v) = w.strip_prefix('X')
                && let Ok(v) = v.parse::<f64>()
            {
                x = v;
                have[0] = true;
            }
            if let Some(v) = w.strip_prefix('Y')
                && let Ok(v) = v.parse::<f64>()
            {
                y = v;
                have[1] = true;
            }
        }
        if !comp && have[0] && have[1] {
            out.push((x, y));
        }
    }
    out
}

#[test]
fn the_entry_descents_before_g41_hold_the_wall_clear_under_an_oversized_register_and_positive_wear() {
    use camcraft_app::machining3d::Entry3D;
    // #3891: the pre-engage moves post at the nominal centre under G40.
    // An oversized register - 5.0 mm on the Ø6 tool, 2 mm over its
    // nominal radius 3 - and a positive wear value - 1.0 mm, the
    // effective radius 4.0 - must not touch the finished wall there.
    // The Perpendicular lead holds the dive point (and the Arc entry's
    // chord, which lands on it) its 6 mm length beyond the nominal
    // radius: 9 mm off the part, the closest stand before G41.
    for comp in [Compensation::Control, Compensation::Wear] {
        let reach = match comp {
            Compensation::Control => 5.0,
            _ => 3.0 + 1.0,
        };
        for entry in [Entry3D::Plunge, Entry3D::Arc] {
            let (world, _) = world_with_entry(comp, entry);
            let out = engine::run(&post_text("fanuc"), &posting::machine_value(&world), &stream(&world)).unwrap();
            assert!(out.warnings.is_empty(), "{comp:?} {entry:?}: {:?}", out.warnings);
            assert!(out.program.contains("G41"), "{comp:?} {entry:?}: a compensated program: {}", out.program);
            let stands = under_g40_stands(&out.program);
            assert!(stands.len() >= 2, "{comp:?} {entry:?}: the program states its stands: {}", out.program);
            let gaps: Vec<f64> = stands.iter().map(|&(x, y)| wall_clear(x, y)).collect();
            let worst = gaps.iter().copied().fold(f64::INFINITY, f64::min);
            assert!(
                (worst - 9.0).abs() <= 0.01,
                "{comp:?} {entry:?}: the closest pre-G41 stand is {worst:.3} mm off the wall, \
                 not the radius 3 plus the lead 6: {}",
                out.program
            );
            assert!(
                worst > reach,
                "{comp:?} {entry:?}: the register {reach} mm reaches within {worst:.3} mm of the wall: {}",
                out.program
            );
        }
    }
}

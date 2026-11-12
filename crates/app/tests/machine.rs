//! Machine limits (M2.3 stage C): what a program asks that the
//! machine cannot give. Two halves - the `.mch` reader (every field
//! optional by the unstated-limit law, garbage refuses, never panics)
//! and `check_document` on a live world. The check reuses the shared
//! gate: it walks `posting::program_ops`, the same walk the sim and
//! the post make, so all three judge exactly the same program.
//!
//! Fixture law (clean-room): the machine JSON below mirrors the
//! observed shape of a `.mch` file - the layout user instruction
//! authorizes us to reuse - while every VALUE is our own invention.
//! Program tests build `Machine` structs directly; only the reader
//! tests feed it text.
//!
//! Gate map (M4.4): covers app::commands, app::geometry, app::machine, post::machine, app::model, app::operation, post::post, app::posting, app::schema, geometry::geometry::stock, tools::tools, app::world.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock, SetWcsOrigin,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3, cube};
use camcraft_app::machine::{AxisLimits, Machine, check_document, parse_mch};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
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

/// The reference machine: a 3-axis VMC whose window admits the
/// EM-0400-2FL (12000 rpm at the inclusive top, 4 mm across, 87 mm
/// out) and a face program running XY 1200 / plunge 300 inside
/// 5000/5000/3000 ceilings, all inside a ±100 mm box of travel.
fn reference() -> Machine {
    Machine {
        model: "TestMill 1".into(),
        vendor: "CamCraft Testbench".into(),
        spindle_min: Some(50.0),
        spindle_max: Some(12000.0),
        axes: vec![
            AxisLimits { name: "X".into(), feed: Some(5000.0), rapid: Some(15000.0), travel: Some((-100.0, 100.0)), ..Default::default() },
            AxisLimits { name: "Y".into(), feed: Some(5000.0), rapid: Some(15000.0), travel: Some((-100.0, 100.0)), ..Default::default() },
            AxisLimits { name: "Z".into(), feed: Some(3000.0), rapid: Some(10000.0), travel: Some((-100.0, 100.0)), ..Default::default() },
        ],
        max_tool_length: Some(150.0),
        max_tool_diameter: Some(20.0),
        // The file's one head states a spindle, so the M5.3 C
        // inventory reads it: one hand, stated, not guessed.
        spindles: Some(1),
        // No spindle body stated: this reference machine keeps the
        // pre-M3.2B verdicts byte-identical - the spindle band only
        // exists when the `.mch` says it does. Stage C's new fields
        // (kind, power, changer, coolants) join that law: unstated
        // here, silent in every verdict the pre-stage-C tests hold.
        spindle_d: None,
        spindle_len: None,
        ..Default::default()
    }
}

/// One Face op on a 60x60x20 box at the origin, faced to z 18 - the
/// same program shape the stage B sim tests judge, so the machine
/// verdict reads the moves the viewer already draws.
fn faced_doc() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: IDENTITY_Q };
    let cmd = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, loc);
    history.execute(&world, cmd.expect("box stock"));
    let cmd = AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL"));
    history.execute(&world, cmd.expect("catalog tool"));
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 18.0))),
    );
    let cmd = CreateOperation::new(
        &world,
        "Face1",
        "EM-0400-2FL",
        SrcRef::Stock,
        OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
            stepover: 3.0,
            clearance_z: Height::fixed(25.0),
            cut_z: Height::fixed(18.0),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
        },
    );
    history.execute(&world, cmd.expect("create face"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate face"));
    (world, history)
}

// ---- workspaces by kind (M3.2 stage E) ----

#[test]
fn the_axis_inventory_picks_the_workspace() {
    use camcraft_app::machine::{Kind, kind};
    let axis = |n: &str| AxisLimits {
        name: n.to_string(),
        ..Default::default()
    };
    let m = |names: &[&str]| Machine {
        axes: names.iter().copied().map(axis).collect(),
        ..Default::default()
    };
    assert_eq!(kind(None), Kind::Mill, "no machine is today's document");
    assert_eq!(kind(Some(&m(&["X", "Y", "Z"]))), Kind::Mill);
    assert_eq!(kind(Some(&m(&["Z", "X"]))), Kind::Lathe, "order-free");
    assert_eq!(kind(Some(&m(&["x", "z"]))), Kind::Lathe, "names, not cases");
    assert_eq!(kind(Some(&m(&["X", "Y", "Z", "C"]))), Kind::MillTurn);
    assert_eq!(
        kind(Some(&m(&["X", "Z", "C"]))),
        Kind::Lathe,
        "a C axis alone is a turning center, not a mill-turn - no Y"
    );
    assert_eq!(
        kind(Some(&m(&[]))),
        Kind::Mill,
        "an inventory that cannot decide falls back to the assumption the code already is"
    );
    assert_eq!(Kind::Lathe.label(), "turning");
}

// ---- the reader ----

#[test]
fn parse_reads_the_limits() {
    // Our own file, our own values; the layout is the .mch shape the
    // kinematics chain (head -> X -> Y -> Z) takes in the wild.
    let text = r#"{
      "general": {"model": "TestMill 1", "vendor": "CamCraft Testbench"},
      "kinematics": {"default": {
        "units": {"angle": "degrees", "length": "mm"},
        "parts": [
          {"type": "static", "name": "base", "parts": [
            {"type": "head", "name": "s",
             "spindle": {"min_speed": 50, "max_speed": 12000},
             "tool_station": {"max_tool_length": 150, "max_tool_diameter": 20},
             "parts": [
               {"type": "linear", "name": "X", "control": "driven",
                "travel": {"min": -100, "max": 100},
                "parts": [
                  {"type": "linear", "name": "Y", "control": "driven",
                   "travel": {"min": -100, "max": 100},
                   "parts": [
                     {"type": "linear", "name": "Z", "control": "driven",
                      "travel": {"min": -100, "max": 100}}
                   ]}
                ]}
             ]}
          ]}
        ]}},
      "controller": {"synced_configuration": {"parts": {
        "X": {"coordinate": 0, "max_normal_speed": 5000, "max_rapid_speed": 15000},
        "Y": {"coordinate": 1, "max_normal_speed": 5000, "max_rapid_speed": 15000},
        "Z": {"coordinate": 2, "max_normal_speed": 3000, "max_rapid_speed": 10000}
      }}}
    }"#;
    let m = parse_mch(text).expect("well-formed file");
    assert_eq!(m, reference(), "the reader and the fixture struct are the same machine");
    assert!(m.summary().starts_with("CamCraft Testbench TestMill 1"));
    assert!(m.summary().contains("3 axes (3 with a known stroke)"));
}

#[test]
fn zero_means_the_file_does_not_say() {
    // The Minimill-shaped case: a real vendor file that carries zeros
    // for speeds nobody configured. Zeros are NOT ceilings - a green
    // checkmark earned by an invented limit is the silent wrong
    // answer this law exists to refuse.
    let text = r#"{
      "general": {},
      "kinematics": {"default": {"units": {"length": "mm"}, "parts": [
        {"type": "head", "name": "h",
         "spindle": {"min_speed": 0, "max_speed": 0},
         "tool_station": {"max_tool_length": 0, "max_tool_diameter": 0},
         "parts": [{"type": "linear", "name": "X"}]}
      ]}},
      "controller": {"synced_configuration": {"parts": {
        "X": {"coordinate": 0, "max_normal_speed": 0, "max_rapid_speed": 0}
      }}}
    }"#;
    let m = parse_mch(text).expect("well-formed");
    assert_eq!(m.spindle_min, None);
    assert_eq!(m.spindle_max, None);
    assert_eq!(m.max_tool_length, None);
    assert_eq!(m.axes.len(), 1);
    assert_eq!(m.axes[0].feed, None);
    assert_eq!(m.axes[0].travel, None);
    assert_eq!(m.spindles, Some(1), "an unconfigured spindle is still a spindle the file stated");
    assert!(m.summary().contains("?-? rpm"), "unknown shows as unknown: {}", m.summary());
}

#[test]
fn heads_that_state_a_spindle_are_counted() {
    // The M5.3 C inventory, at its source: the count walks the WHOLE
    // parts tree (a sub-spindle nested under the main head counts),
    // a head without a spindle statement is furniture, and a file
    // that states no spindle at all stays None - never a 1 the
    // reader guessed for a machine that said nothing.
    let counted = |parts: &str| {
        let text = format!(
            r#"{{ "general": {{}},
              "kinematics": {{ "default": {{ "units": {{ "length": "mm" }}, "parts": [ {parts} ] }} }} }}"#
        );
        parse_mch(&text).expect("well-formed").spindles
    };
    assert_eq!(
        counted(r#"{"type": "head", "name": "main", "spindle": {"max_speed": 4000}, "parts": [
                      {"type": "head", "name": "sub", "spindle": {"max_speed": 6000}} ]},
                   {"type": "head", "name": "no spindle stated"}"#),
        Some(2),
        "nested hands count; a head with no spindle is not a hand"
    );
    assert_eq!(counted(r#"{"type": "head", "name": "solo"}"#), None, "unstated stays absent");
    assert_eq!(counted(r#"{"type": "static", "name": "base"}"#), None, "no heads at all");
}

#[test]
fn garbage_refuses_and_emptiness_judges_nothing() {
    // Degenerate input must error cleanly - the reader walks a
    // serde_json::Value, so no shape can panic it.
    for bad in ["", "not json at all", "{", "{\"kinematics\": 5"] {
        assert!(parse_mch(bad).is_err(), "{bad:?} must refuse, not guess");
    }
    // Well-formed JSON that names no machine is legal and mute - the
    // reader describes what the file says, and these files say
    // nothing. (A bare array or null is JSON that merely isn't a
    // machine file: refuse.)
    assert!(parse_mch("[]").is_err());
    assert!(parse_mch("null").is_err());
    for mute in ["{}", "{\"general\": {}}", "{\"kinematics\": 5}"] {
        parse_mch(mute).expect("well-formed, mute");
    }
    let m = parse_mch("{}").expect("the empty file is legal, just mute");
    assert_eq!(m.axes.len(), 0);
    assert_eq!(m.spindle_max, None);
    assert_eq!(m.vendor, "unknown");
}

#[test]
fn inches_scale_into_millimeters() {
    // One factor at the door, and the whole checker stays mm-only.
    // rpm is not a length and rides along unscaled.
    let text = r#"{
      "general": {"model": "Inchmill", "vendor": "Testbench"},
      "kinematics": {"default": {"units": {"length": "inch"}, "parts": [
        {"type": "head", "name": "h", "spindle": {"min_speed": 100, "max_speed": 8000},
         "tool_station": {"max_tool_length": 4},
         "parts": [{"type": "linear", "name": "X", "travel": {"min": -10, "max": 10}}]}
      ]}},
      "controller": {"synced_configuration": {"parts": {
        "X": {"coordinate": 0, "max_normal_speed": 200}
      }}}
    }"#;
    let m = parse_mch(text).expect("well-formed");
    let near = |a: Option<f64>, b: f64| a.is_some_and(|x| (x - b).abs() < 1e-9);
    assert!(near(m.axes[0].feed, 5080.0), "200 ipm = 5080 mm/min: {:?}", m.axes[0].feed);
    assert!(m.axes[0].travel.is_some_and(|(lo, hi)| (lo + 254.0).abs() < 1e-9 && (hi - 254.0).abs() < 1e-9));
    assert!(near(m.max_tool_length, 101.6));
    assert!(near(m.spindle_max, 8000.0), "rpm is not a length");
}

// ---- the checker on a live program ----

#[test]
fn the_reference_machine_accepts_the_reference_program() {
    // The honest-green case: every stated limit stated, every ask
    // inside it. This is the verdict stage D will make structural.
    let (world, _h) = faced_doc();
    let c = check_document(&world, &reference()).expect("a fresh program checks");
    assert!(c.is_ok(), "reference program on reference machine: {:?}", c.problems);
}

#[test]
fn a_spindle_outside_the_window_names_the_op() {
    // The tool's rpm IS the program's spindle word today, so the
    // window is a tool-table truth - located by op.
    let (world, _h) = faced_doc();
    let mut slow = reference();
    slow.spindle_max = Some(1000.0);
    let c = check_document(&world, &slow).expect("checks");
    assert_eq!(c.problems.len(), 1, "{:?}", c.problems);
    assert!(c.problems[0].starts_with("Face1:") && c.problems[0].contains("above the machine's 1000"));
    let mut fast = reference();
    fast.spindle_min = Some(20000.0);
    let c = check_document(&world, &fast).expect("checks");
    assert!(c.problems.iter().any(|p| p.contains("below the machine's 20000")), "{:?}", c.problems);
}

#[test]
fn an_unstated_machine_passes_anything_it_cannot_judge() {
    // Same program, mute file: nothing to violate. A green verdict
    // here must read "the machine said nothing", which is what stage
    // D's gate text will say - stage C's law is that it is NOT a
    // claim of safety.
    let (world, _h) = faced_doc();
    let c = check_document(&world, &Machine::default()).expect("checks");
    assert!(c.is_ok());
}

#[test]
fn feed_is_charged_to_the_slowest_axis_that_moves() {
    // X tops out at 500; the face's XY passes hit 1200 on every move
    // that drives X, and the pure-Z plunges (300 under Z's 3000) are
    // not in it. The report names the axis that ran out.
    let (world, _h) = faced_doc();
    let mut m = reference();
    m.axes[0].feed = Some(500.0);
    let c = check_document(&world, &m).expect("checks");
    assert!(!c.problems.is_empty());
    assert!(
        c.problems.iter().all(|p| !p.contains("move") || p.contains("X tops out at 500")),
        "only X-driven moves may be charged: {:?}",
        &c.problems[..c.problems.len().min(3)]
    );
    assert!(c.problems.iter().any(|p| p.contains("feeds 1200") && p.contains("X tops out at 500")));
    assert!(!c.problems.iter().any(|p| p.contains("feeds 300")), "plunges stay under Z's ceiling");
}

#[test]
fn travel_reports_where_the_tool_reaches() {
    // A 20 mm-wide column cannot take a 60 mm face: the path swings
    // past x +/- 30 with the 4 mm tool, and the verdict says the
    // number and the stroke.
    let (world, _h) = faced_doc();
    let mut m = reference();
    m.axes[0].travel = Some((-20.0, 20.0));
    let c = check_document(&world, &m).expect("checks");
    assert!(!c.problems.is_empty(), "a 40 mm stroke cannot hide a 64 mm sweep");
    assert!(
        c.problems.iter().any(|p| p.starts_with("Face1:") && p.contains("in X") && p.contains("the stroke is -20.000..20.000")),
        "{:?}", c.problems[0]
    );
}

#[test]
fn rapids_pay_the_same_travel_bill_as_feeds() {
    // The stroke is geometry, not a feedrate argument: a clearance
    // move across the table crosses the column even though it has no
    // feed word to fail. Same machine, same program as the travel
    // test - the approach moves alone breach it, and the walk never
    // branches on move kind.
    let (world, _h) = faced_doc();
    let mut m = reference();
    m.axes[0].travel = Some((-20.0, 20.0));
    let c = check_document(&world, &m).expect("checks");
    let cache = model::toolpath_of(&world, &model::find_by_name(&world, "Face1").expect("op")).expect("cache");
    let any_rapid_out = cache.path.moves.iter().any(|mv| {
        mv.feed.is_none() && (mv.from.x.abs() > 20.0 || mv.to.x.abs() > 20.0)
    });
    assert!(any_rapid_out, "the fixture must actually rapid outside the column");
    assert!(c.problems.iter().any(|p| p.contains("move")), "{:?}", c.problems[0]);
}

#[test]
fn the_tool_table_refuses_a_cutter_the_magazine_cannot_hold() {
    // Gauge length is the whole protrusion - flute, shank, holder -
    // 87 mm on the 0400; the diameter reads off the tool as stored.
    let (world, _h) = faced_doc();
    let mut m = reference();
    m.max_tool_length = Some(50.0);
    m.max_tool_diameter = Some(2.0);
    let c = check_document(&world, &m).expect("checks");
    assert_eq!(c.problems.len(), 2, "{:?}", c.problems);
    assert!(c.problems[0].contains("is 87.0 mm long, the machine takes 50.0"));
    assert!(c.problems[1].contains("is 4.0 mm across, the machine takes 2.0"));
}

#[test]
fn a_g54_slide_moves_the_stroke_exactly_like_the_g_code() {
    // Travel is a program-coordinate law and the moves are world
    // millimeters; the bridge is the post's own wcs_context, so the
    // same G54 slide that shifts the emitted X word shifts what
    // passes. A 40 mm column holds a 30 mm sweep at the origin and
    // loses it entirely 100 mm over.
    let mut m = reference();
    m.axes[0].travel = Some((-40.0, 40.0));
    let (world, mut history) = faced_doc();
    assert!(check_document(&world, &m).expect("origin check").is_ok(), "±30 inside ±40");
    // The slot is born when it is first placed (the SetWcsOrigin law) -
    // and the slide moves through history, like every other mutation.
    history.execute(
        &world,
        SetWcsOrigin::new(
            &world,
            Location { pos: Point3::new(100.0, 0.0, 0.0), rot: IDENTITY_Q },
        )
        .expect("slide G54"),
    );
    // Regenerate over whatever the digest makes stale (a fresh
    // regenerate is a no-op if nothing staled it), then re-judge.
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate slid"));
    let c = check_document(&world, &m).expect("slid check");
    assert!(!c.is_ok(), "the program swings -70..-100, outside -40..40");
    assert!(
        c.problems.iter().all(|p| !p.contains("reaches") || p.contains("reaches -")),
        "every breach is on the negative side: {:?}",
        &c.problems[..c.problems.len().min(2)]
    );
}

#[test]
fn the_machine_walk_is_the_post_walk() {
    // The shared-gate law again: check_document validates through
    // posting::program_ops, so an empty or stale document refuses
    // the machine check with the post's own sentence, byte-for-byte.
    let world = new_world();
    let err = check_document(&world, &reference()).expect_err("nothing to check");
    let same = posting::program_ops(&world).expect_err("same law, one copy");
    assert_eq!(err, same);

    let (world, mut history) = faced_doc();
    // Stale it the way stage B's tests do - the stock's placement is
    // in the op's digest.
    let stock = model::find_by_name(&world, camcraft_app::schema::STOCK_NAME).expect("stock");
    model::set_transform(&world, &stock, camcraft_app::schema::Location { pos: Point3::new(0.0, 0.0, 1.0), rot: IDENTITY_Q });
    let err = check_document(&world, &reference()).expect_err("stale blocks the check");
    assert!(err.contains("Face1") && err.contains("regenerate"), "{err}");
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    check_document(&world, &reference()).expect("fresh again checks");
}

#[test]
fn a_flood_of_violations_reports_forty_and_counts_the_rest() {
    // The cap is a report bound, not a truth bound: more stays honest.
    let (world, _h) = faced_doc();
    let mut m = reference();
    m.axes[0].travel = Some((-0.0005, 0.0005));
    let c = check_document(&world, &m).expect("checks");
    assert_eq!(c.problems.len(), 40, "the list caps at its bound");
    assert!(c.more > 0, "a 60 mm face crosses a 1 um column more than 40 times");
}

#[test]
fn the_post_mirror_states_only_what_the_machine_stated() {
    // M3.3 A's bridge, widened by stages C and D: a checker-parsed
    // `Machine` reaches the script through `post::machine::mirror` -
    // the static grammar, stated numbers only. An unstated spindle
    // limit is ABSENT (the posts' `> 0` guard keeps the silence
    // honest), rotary axes mirror as GEOMETRY under `rotaries` and
    // never join `axes` (no rate follows them across - a deg/min
    // number in an mm/min field would be the unit lie stage C named),
    // and coolant says nothing the machine did not: no `coolants`
    // field on the machine, no coolant key in the mirror.
    let m = Machine {
        model: "MirrorMill".into(),
        vendor: "CamCraft Testbench".into(),
        spindle_max: Some(8000.0),
        axes: vec![
            AxisLimits {
                name: "X".into(),
                feed: Some(4000.0),
                rapid: Some(7000.0),
                travel: None,
                ..Default::default()
            },
            AxisLimits { name: "C".into(), rotary: true, rapid: Some(9000.0), ..Default::default() },
        ],
        ..Default::default()
    };
    let v = camcraft_app::post::machine::mirror(&m);
    assert_eq!(v["name"], "MirrorMill");
    let axes = v["kinematics"]["static"]["axes"].as_array().expect("axes array");
    assert_eq!(axes.len(), 1, "the rotary C keeps out of the traverse grammar");
    assert_eq!(v["kinematics"]["static"]["rotaries"].as_array().map(Vec::len), Some(1), "and rides the geometry section instead");
    assert_eq!(axes[0]["letter"], "X");
    assert_eq!(axes[0]["speeds"]["rapid"], 7000.0);
    assert_eq!(axes[0]["speeds"]["normal"], 4000.0);
    assert_eq!(axes[0]["direction"], serde_json::json!([1.0, 0.0, 0.0]));
    assert_eq!(v["kinematics"]["static"]["head"]["spindle"]["max"], 8000.0);
    assert_eq!(v["rapidRate"], 7000.0, "the ceiling is the fastest stated axis");

    let props = camcraft_app::post::machine::script_props(&v);
    assert_eq!(props["name"], "MirrorMill");
    assert_eq!(props["spindleMax"], 8000.0);
    assert_eq!(props["coolants"].as_array().map(Vec::len), Some(0), "silent coolant, stated");

    // The mute machine mirrors to silence, not to zeros that lie.
    let mute = camcraft_app::post::machine::script_props(
        &camcraft_app::post::machine::mirror(&Machine::default()),
    );
    assert_eq!(mute["name"], "");
    assert_eq!(mute["spindleMax"], 0.0, "absent reads 0: the `> 0` guards keep it silent");
    assert_eq!(mute["rapidRate"], 0.0);
    assert_eq!(mute["coolants"].as_array().map(Vec::len), Some(0), "and an empty list");
}

// ---- declared kind + full numbers (M3.3 stage C) ----

#[test]
fn the_declared_kind_outranks_the_axis_inventory() {
    // stage C: a file that says what it is wins; the M3.2 inference
    // stays as the fallback for files that state nothing.
    use camcraft_app::machine::{Kind, kind};
    let axis = |n: &str| AxisLimits {
        name: n.to_string(),
        ..Default::default()
    };
    let m = |names: &[&str], k: Option<&str>| Machine {
        axes: names.iter().copied().map(axis).collect(),
        kind: k.map(str::to_owned),
        ..Default::default()
    };
    assert_eq!(kind(Some(&m(&["X", "Y", "Z"], Some("mill")))), Kind::Mill);
    assert_eq!(kind(Some(&m(&["X", "Z"], Some("lathe")))), Kind::Lathe);
    assert_eq!(
        kind(Some(&m(&["X", "Y", "Z"], Some("turning")))),
        Kind::Lathe,
        "declared wins even over a milling inventory - the file knows parts the kinematics tree hides"
    );
    assert_eq!(
        kind(Some(&m(&["X", "Y", "Z", "C"], Some("Mill-Turn")))),
        Kind::MillTurn,
        "the word reader is case-and-punctuation tolerant"
    );
    assert_eq!(kind(Some(&m(&["X", "Y", "Z"], None))), Kind::Mill, "unstated: the old law");
    assert_eq!(Kind::MillTurn.word(), "mill-turn", "one canonical spelling");
    assert_eq!(Kind::from_word("mill turn"), Some(Kind::MillTurn));
    assert_eq!(Kind::from_word("drill press"), None);
    // A flagged rotary decides without letter games.
    let mut milled = m(&["X", "Y", "Z"], None);
    milled.axes.push(AxisLimits {
        name: "Q".to_string(),
        rotary: true,
        ..Default::default()
    });
    assert_eq!(kind(Some(&milled)), Kind::MillTurn, "Y plus a rotary is a mill-turn");
}

#[test]
fn parse_reads_the_full_stage_c_grammar() {
    let text = r#"{
      "general": {"model": "StageC 1", "vendor": "CamCraft Testbench", "kind": "mill-turn"},
      "kinematics": {"default": {
        "units": {"length": "mm", "angle": "degrees"},
        "parts": [
          {"name": "head", "type": "head",
           "spindle": {"min_speed": 30.0, "max_speed": 12000.0, "power": 11.0},
           "tool_station": {"max_tool_length": 250.0, "coolants": [
             {"name": "Flood", "mCode": "M8"}, "Mist", {"name": "Air", "mCode": 5}
           ]}},
          {"name": "X", "type": "linear", "travel": {"min": -200.0, "max": 200.0}},
          {"name": "Y", "type": "linear", "resolution": 0.001},
          {"name": "C", "type": "rotary", "travel": {"min": -360.0, "max": 360.0}, "resolution": 0.0001}
        ]}},
      "tooling": {"number_of_tools": 24}}"#;
    let m = parse_mch(text).expect("the widened grammar parses");
    assert_eq!(m.kind.as_deref(), Some("mill-turn"), "the word, as stated");
    assert_eq!(m.spindle_power, Some(11.0));
    assert_eq!(m.tool_count, Some(24));
    assert_eq!(m.coolants.len(), 3);
    assert_eq!(
        (m.coolants[0].name.as_str(), m.coolants[0].m_code.as_str()),
        ("Flood", "M8")
    );
    assert_eq!(m.coolants[1].name, "Mist", "a bare string names the option");
    assert_eq!(m.coolants[1].m_code, "", "with no code to state");
    assert_eq!(m.coolants[2].m_code, "M5", "a bare number normalizes to the word");
    let c = m.axes.iter().find(|a| a.name == "C").expect("the rotary joined the table");
    assert!(c.rotary, "flagged by the part type, not the letter");
    assert_eq!(c.travel, Some((-360.0, 360.0)), "the turning range, degrees");
    assert_eq!(c.resolution, Some(0.0001), "degrees, unscaled by any inch factor");
    let y = m.axes.iter().find(|a| a.name == "Y").expect("Y");
    assert_eq!(y.resolution, Some(0.001), "linear resolution is a length");
    assert!(m.is_sane());
    assert_eq!(
        camcraft_app::machine::kind(Some(&m)),
        camcraft_app::machine::Kind::MillTurn
    );
}

#[test]
fn a_coolant_list_at_the_tooling_root_also_reads() {
    // Some files keep the options at the tooling root rather than on
    // the station; either home is the same list.
    let text = r#"{
      "kinematics": {"default": {"parts": [{"name": "head", "type": "head"}]}},
      "tooling": {"coolants": ["Flood"]}}"#;
    let m = parse_mch(text).expect("the alternate home parses");
    assert_eq!(m.coolants.len(), 1);
    assert_eq!(
        (m.coolants[0].name.as_str(), m.coolants[0].m_code.as_str()),
        ("Flood", "")
    );
}

#[test]
fn a_kind_that_is_not_a_kind_refuses_at_the_door() {
    // The one gate is parse_mch: an unreadable word never reaches a
    // mounted machine, so nothing downstream re-litigates it.
    let bad = r#"{"general": {"kind": "router"}}"#;
    let err = parse_mch(bad).expect_err("a word the grammar does not know");
    assert!(err.contains("router"), "the refusal names the word: {err}");
    let notaword = r#"{"general": {"kind": {"surprise": 1}}}"#;
    assert!(parse_mch(notaword).is_err(), "a table is not a declaration");
    let empty = r#"{"general": {"kind": ""}}"#;
    assert_eq!(parse_mch(empty).expect("empty states nothing").kind, None);
    // Hand-edited project files face the loader's is_sane gate.
    let smuggled = Machine {
        kind: Some("router".to_string()),
        ..Default::default()
    };
    assert!(!smuggled.is_sane());
    let blank_option = Machine {
        coolants: vec![camcraft_app::machine::Coolant {
            name: String::new(),
            m_code: "M8".to_string(),
        }],
        ..Default::default()
    };
    assert!(!blank_option.is_sane(), "a coolant nobody can name");
}

#[test]
fn the_changer_counts_the_distinct_tools() {
    // Two ops drawing on two tools, one slot: physically impossible,
    // and the checker says the numbers.
    let (world, mut history) = faced_doc();
    history.execute(
        &world,
        AddTool::new(&world, "EM-0600-4FL", catalog_tool("EM-0600-4FL")).expect("second tool"),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Face2",
            "EM-0600-4FL",
            SrcRef::Stock,
            OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
                stepover: 3.0,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(18.0),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        )
        .expect("second op"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face2").expect("generate second"),
    );
    let mut tight = reference();
    tight.tool_count = Some(1);
    let c = check_document(&world, &tight).expect("checks");
    assert!(
        c.problems.iter().any(|p| p.contains("2 tools, the changer holds 1")),
        "{:?}",
        c.problems
    );
    let mut roomy = reference();
    roomy.tool_count = Some(2);
    let c = check_document(&world, &roomy).expect("checks");
    assert!(
        !c.problems.iter().any(|p| p.contains("changer")),
        "two slots for two tools says nothing about the changer: {:?}",
        c.problems
    );
}

#[test]
fn the_changer_says_nothing_when_the_machine_stated_nothing() {
    // The unstated limit keeps its unstated face: no `number_of_tools`
    // is no claim, never a one-slot verdict.
    let (world, _h) = faced_doc();
    let mut mute = reference();
    mute.tool_count = None;
    let c = check_document(&world, &mute).expect("checks");
    assert!(c.problems.iter().all(|p| !p.contains("changer")), "{:?}", c.problems);
}

#[test]
fn the_coolant_list_reaches_the_script() {
    // stage C completes the bridge the unmounted document already
    // walked: the mounted list, codes and name-only entries alike.
    use camcraft_app::machine::Coolant;
    let m = Machine {
        coolants: vec![
            Coolant {
                name: "Flood".to_string(),
                m_code: "M8".to_string(),
            },
            Coolant {
                name: "Air blast".to_string(),
                m_code: String::new(),
            },
        ],
        ..reference()
    };
    let props = camcraft_app::post::machine::script_props(&camcraft_app::post::machine::mirror(&m));
    let list = props["coolants"].as_array().expect("a coolants list");
    assert_eq!(list.len(), 2);
    assert_eq!(
        (list[0]["name"].as_str(), list[0]["mCode"].as_str()),
        (Some("Flood"), Some("M8"))
    );
    assert_eq!(list[1]["mCode"].as_str(), Some(""), "named only - the post stays silent on it");
}

#[test]
fn every_shipped_machine_mounts_and_speaks_its_declaration() {
    // The built-in library the Setup pane offers: every entry is
    // parseable, sane, and its declared kind matches what its own
    // axis inventory infers - the files are honest both ways.
    use camcraft_app::machine::{Kind, kind};
    use camcraft_app::posting::MACHINES;
    let expectations = [
        ("VMC 3-axis", Kind::Mill),
        ("Tilt-head 5-axis", Kind::Mill),
        ("Mill-turn", Kind::MillTurn),
        ("Lathe", Kind::Lathe),
        ("Tormach 770MX", Kind::Mill),
        ("Tormach 770MX (ML)", Kind::Mill),
        // #785: an A about X leans the tool - still a mill, never a mill-turn.
        ("Tormach 770MX + microARC 4", Kind::Mill),
        ("Tormach 770M", Kind::Mill),
        ("Tormach 1100MX", Kind::Mill),
        ("Tormach 1500MX", Kind::Mill),
        ("Tormach PCNC 440", Kind::Mill),
        ("Tormach 1100M", Kind::Mill),
        ("Tormach PCNC 770", Kind::Mill),
        ("Tormach PCNC 770 (Series 3)", Kind::Mill),
        ("Tormach PCNC 1100 (Series I)", Kind::Mill),
        ("Tormach PCNC 1100 (Series II)", Kind::Mill),
        ("Tormach PCNC 1100 (Series 3)", Kind::Mill),
        ("Tormach 770M+", Kind::Mill),
        ("Tormach 1100M+", Kind::Mill),
        ("Tormach 24R", Kind::Mill),
        ("Tormach xsTECH Pro", Kind::Mill),
        ("Tormach xsTECH", Kind::Mill),
        ("Tormach 1300PL", Kind::Plasma),
        ("Tormach 8L", Kind::Lathe),
        ("Tormach 15L Slant-PRO", Kind::Lathe),
        ("Tormach 15LX Slant-PRO", Kind::Lathe),
        ("Makera Carvera", Kind::Mill),
        ("Makera Carvera Air", Kind::Mill),
        ("Doosan DNM 5700", Kind::Mill),
    ];
    assert_eq!(MACHINES.len(), expectations.len(), "the pane offers what the tests pin");
    for ((name, text), (want_name, want_kind)) in MACHINES.iter().zip(expectations) {
        assert_eq!(*name, want_name);
        let m = parse_mch(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(m.is_sane(), "{name} must be a machine this app can check");
        assert_eq!(kind(Some(&m)), want_kind, "{name} declared");
        let mut inferred = m.clone();
        inferred.kind = None;
        // #669: plasma is declared, never inferred - its X/Y/Z reads mill.
        let axes_say = if want_kind == Kind::Plasma { Kind::Mill } else { want_kind };
        assert_eq!(kind(Some(&inferred)), axes_say, "{name}'s axes agree with its word");
    }
}

// ---- the checker on a live program: poses (M5.1 stage A) ----

/// Lean the first FEED move of the faced doc's cached path (a test
/// fixture, not a UI state poke - generation stands prismatic until
/// stage B gives it a reason to lean; the gate must judge a pose
/// anyway). Returns the move index, which the problems name.
fn lean_first_feed(world: &World, axis: Option<[f64; 3]>) -> usize {
    let e = model::find_by_name(world, "Face1").expect("the face op");
    let mut t = model::toolpath_of(world, &e).expect("a fresh cache");
    let i = t
        .path
        .moves
        .iter()
        .position(|m| m.kind == camcraft_app::cl::MoveKind::Feed)
        .expect("a face path feeds");
    t.path.moves[i].axis = axis;
    e.entity_view(world).set(t);
    i
}

#[test]
fn a_machine_without_a_tilting_rotary_refuses_a_lean_by_name() {
    // The pose gate (M5.1 A): absence is the ordinary case, and when
    // the stream does state an angle, the machine answers with its
    // rotaries - by name, never silently. Two sentences, both true:
    // the machine states no tilting rotary at all, and it states no
    // B axis to hold the 20 degrees the lean's canonical names.
    let (world, _h) = faced_doc();
    let (sa, ca) = (20f64.to_radians().sin(), 20f64.to_radians().cos());
    let i = lean_first_feed(&world, Some([sa, 0.0, ca]));
    let c = check_document(&world, &reference()).expect("checks");
    let mine: Vec<&String> = c
        .problems
        .iter()
        .filter(|p| p.contains(&format!("move {i}")))
        .collect();
    assert_eq!(mine.len(), 2, "{:?}", c.problems);
    assert!(
        mine[0].contains("tilts the tool")
            && mine[0].contains("TestMill 1 states no tilting rotary"),
        "{mine:?}"
    );
    assert!(
        mine[1].contains("commands B 20.000 deg") && mine[1].contains("TestMill 1 states no B axis"),
        "{mine:?}"
    );
}

#[test]
fn a_tilting_machine_judges_the_letters_and_their_travel() {
    // Same program, a machine that owns A and B: the lean posts, and
    // only an angle beyond the stated stroke comes back named.
    let (world, _h) = faced_doc();
    let mut m = reference();
    m.model = "TiltMill".into();
    for (name, d) in [("A", [1.0, 0.0, 0.0]), ("B", [0.0, 1.0, 0.0])] {
        m.axes.push(AxisLimits {
            name: name.into(),
            rotary: true,
            axis: Some(d),
            travel: Some((-120.0, 120.0)),
            ..Default::default()
        });
    }
    let (sa, ca) = (20f64.to_radians().sin(), 20f64.to_radians().cos());
    // Lean toward +x: the canonical is B 20, inside the stroke.
    lean_first_feed(&world, Some([sa, 0.0, ca]));
    let c = check_document(&world, &m).expect("checks");
    assert!(c.is_ok(), "{:?}", c.problems);
    // Lean toward -y: the canonical is A -20, also inside.
    lean_first_feed(&world, Some([0.0, -sa, ca]));
    assert!(check_document(&world, &m).expect("checks").is_ok());
    // Past horizontal toward +x reads B 130 - beyond ±120, refused
    // with the stroke named.
    let (sb, cb) = (130f64.to_radians().sin(), 130f64.to_radians().cos());
    let i = lean_first_feed(&world, Some([sb, 0.0, cb]));
    let c = check_document(&world, &m).expect("checks");
    assert_eq!(c.problems.len(), 1, "{:?}", c.problems);
    assert!(
        c.problems[0].contains(&format!("move {i} commands B 130.000 deg"))
            && c.problems[0].contains("the travel is -120.000..120.000"),
        "{:?}",
        c.problems
    );
    // The exact vertical states no angle: not one word.
    lean_first_feed(&world, Some([0.0, 0.0, 1.0]));
    assert!(check_document(&world, &m).expect("checks").is_ok());
    lean_first_feed(&world, None);
    assert!(check_document(&world, &m).expect("checks").is_ok());
}

// ---- rotary geometry + RTCP judgment (M3.3 stage D) ----

#[test]
fn a_rotary_states_its_direction() {
    // The tilt question a post asks is geometric: WHICH way does the
    // part turn? The file states it as a direction vector, kept as
    // written (a ratio, never scaled by the file's length units).
    let text = r#"{
      "general": {"model": "Trunnion D"},
      "kinematics": {"default": {"parts": [
        {"name": "X", "type": "linear"},
        {"name": "A", "type": "rotary", "axis": [1.0, 0.0, 0.0],
         "travel": {"min": -120.0, "max": 120.0}}
      ]}}}"#;
    let m = parse_mch(text).expect("the stage D grammar reads");
    let a = m.axes.iter().find(|x| x.name == "A").expect("rotary A");
    assert_eq!(a.axis, Some([1.0, 0.0, 0.0]));
    assert!(m.is_sane());
}

#[test]
fn a_garbled_direction_refuses_by_name() {
    // The whole RTCP judgment rides on the vector, so a garbled one
    // must never read as "no tilt stated" - that would be the silent
    // answer the Prime Directive forbids.
    for bad in [
        r#""axis": [1.0, "up", 0.0]"#,
        r#""axis": [1.0, 0.0]"#,
        r#""axis": 90.0"#,
    ] {
        let text = format!(
            r#"{{ "general": {{}}, "kinematics": {{ "default": {{ "parts": [
                {{ "name": "B", "type": "rotary", {bad} }} ] }} }} }}"#
        );
        let e = parse_mch(&text).expect_err("garbled direction refuses");
        assert!(e.contains("axis B") || e.contains("B"), "names the axis: {e}");
    }
    // The zero vector directions nothing.
    let zero = r#"{ "general": {}, "kinematics": { "default": { "parts": [
        { "name": "C", "type": "rotary", "axis": [0.0, 0.0, 0.0] } ] } } }"#;
    let e = parse_mch(zero).expect_err("a zero direction is no direction");
    assert!(e.contains("zero"), "names the lie: {e}");
}

#[test]
fn rotaries_mirror_as_geometry_and_rates_never_as_the_ceiling() {
    // Stage C's unit law through stage G: a rotary lives in its own
    // `rotaries` section - letter, direction, degrees of travel -
    // and since G, its deg/min rates ride there too, under the
    // rotary's own roof, because the paced head swing
    // (`sim::Traverse::rot`) has to know how fast. What the law
    // still forbids is the leak INTO the prismatic numbers: the
    // ceiling fold pre-stage-D absorbed a rotary rapid into
    // rapidRate, and a 9000 deg/min table out-rating a 7000 mm/min
    // slide would be a lie in the mirror itself. The rapidRate pin
    // below is that part of the law; G (this lane's blind spot,
    // recorded in RECOMMENDED.md) added the rates half.
    let m = Machine {
        model: "TiltMill".into(),
        axes: vec![
            AxisLimits {
                name: "X".into(),
                rapid: Some(7000.0),
                feed: Some(4000.0),
                ..Default::default()
            },
            AxisLimits {
                name: "A".into(),
                rotary: true,
                rapid: Some(9000.0),
                feed: Some(1500.0),
                travel: Some((-120.0, 120.0)),
                axis: Some([1.0, 0.0, 0.0]),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let v = camcraft_app::post::machine::mirror(&m);
    let axes = v["kinematics"]["static"]["axes"].as_array().expect("axes");
    assert_eq!(axes.len(), 1, "only the slide is an axis");
    assert_eq!(v["kinematics"]["static"]["axes"][0]["letter"], "X");
    assert_eq!(v["rapidRate"], 7000.0, "the deg/min never enters the ceiling");
    let rots = v["kinematics"]["static"]["rotaries"].as_array().expect("rotaries");
    assert_eq!(rots.len(), 1);
    assert_eq!(rots[0]["letter"], "A");
    assert_eq!(rots[0]["direction"], serde_json::json!([1.0, 0.0, 0.0]));
    assert_eq!(rots[0]["travel"], serde_json::json!([-120.0, 120.0]));
    assert_eq!(
        rots[0]["speeds"],
        serde_json::json!({"normal": 1500.0, "rapid": 9000.0}),
        "the head swing paces itself off the rotary's own rates"
    );

    let p = camcraft_app::post::machine::script_props(&v);
    assert_eq!(p["rtcp"], serde_json::json!(true), "an A about X tilts");
    assert_eq!(p["rotaries"].as_array().map(Vec::len), Some(1));
}

#[test]
fn a_spin_axis_turns_but_never_tilts() {
    // The millturn law, geometric: a C about Z spins the part in the
    // table plane. It is a rotary (it rides the rotaries array) but
    // machine.rtcp is FALSE - no post may promise tilt to a machine
    // whose only rotary cannot leave the vertical.
    let m = Machine {
        model: "SpinMill".into(),
        axes: vec![AxisLimits {
            name: "C".into(),
            rotary: true,
            axis: Some([0.0, 0.0, 1.0]),
            ..Default::default()
        }],
        ..Default::default()
    };
    let p = camcraft_app::post::machine::script_props(&camcraft_app::post::machine::mirror(&m));
    assert_eq!(p["rtcp"], serde_json::json!(false));
    assert_eq!(p["rotaries"].as_array().map(Vec::len), Some(1));

    // And the shipped mill-turn file agrees with its own mirror.
    let mch: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/machines/millturn.mch"),
        )
        .expect("the stock machine file ships"),
    )
    .expect("valid json");
    let raw = camcraft_app::post::machine::script_props(&mch);
    assert_eq!(raw["rtcp"], serde_json::json!(false), "the C is a spindle column");
}

#[test]
fn an_unstated_rotary_tilts_nothing_a_post_may_promise() {
    // Silence stays silence through BOTH doors: the mirror of a
    // rotary without a direction, and the raw-grammar axes that
    // carry A/B/C letters with no vector at all.
    let m = Machine {
        model: "Quiet".into(),
        axes: vec![AxisLimits {
            name: "B".into(),
            rotary: true,
            ..Default::default()
        }],
        ..Default::default()
    };
    let p = camcraft_app::post::machine::script_props(&camcraft_app::post::machine::mirror(&m));
    assert_eq!(p["rtcp"], serde_json::json!(false));
    assert_eq!(p["rotaries"][0]["name"], "B");
    assert_eq!(p["rotaries"][0]["direction"], serde_json::Value::Null);

    let raw = serde_json::json!({ "kinematics": { "static": { "axes": [
        { "letter": "X", "speeds": { "rapid": 5000.0 } },
        { "letter": "A", "axis": [0.0, 1.0, 0.0] } ] } } });
    let r = camcraft_app::post::machine::script_props(&raw);
    assert_eq!(r["rtcp"], serde_json::json!(true), "the raw door reads geometry too");
    assert_eq!(r["rapidRate"], 5000.0, "the rotary letter never joins the ceiling");
}

#[test]
fn rows_state_each_fact_and_never_a_bare_question_mark() {
    use camcraft_app::units::Unit;
    let m = Machine {
        vendor: "Acme".into(),
        model: "Mill".into(),
        spindle_max: Some(15000.0),
        axes: vec![
            AxisLimits { name: "X".into(), travel: Some((0.0, 300.0)), ..Default::default() },
            AxisLimits { name: "Y".into(), ..Default::default() },
        ],
        tool_count: Some(6),
        ..Default::default()
    };
    let rows = m.rows(Unit::Millimeters);
    let get = |k: &str| rows.iter().find(|(l, _)| *l == k).map(|(_, v)| v.clone()).unwrap_or_default();
    assert_eq!(get("Machine"), "Acme Mill");
    assert_eq!(get("Spindle"), "up to 15000 rpm (min not stated)");
    assert_eq!(get("Axes"), "X 0 \u{2026} 300 mm \u{b7} Y stroke not stated");
    assert_eq!(get("Tool changer"), "6 slots");
    assert_eq!(get("Coolant"), "not stated");
    assert_eq!(get("Kind"), "not stated");
    assert!(rows.iter().all(|(_, v)| !v.contains('?')), "{rows:?}");
    let inch = m.rows(Unit::Inches);
    assert!(inch.iter().any(|(l, v)| *l == "Axes" && v.contains("11.8") && v.contains("in")), "{inch:?}");
}

//! The G-EDM wire post (#2700): a 2-axis Wire Contour through `g-edm.cps`,
//! posted by the app's own gate (verification and the wire laws included)
//! to its golden, and its refusals by name.
//! Provenance: the words asserted here are the Grbl-family words that the
//! public Grbl_Esp32 G-code documentation lists (G20 / G21, G90, G17, G0,
//! G1, G2 / G3 with incremental I J, `(...)` comments, M30; no `%` fence,
//! no G41 / G42), and gedm.org's public statements on G-EDM wire mode
//! (spark power set at the controller, the gap servo sets the feed). No
//! G-RBLv3 source was read. The output was not run on a G-EDM controller
//! (docs/decisions/2700-g-edm-post.md).
//!
//! Regenerate the golden after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test g_edm_post`
//!
//! Gate map (#2700): covers app::posting (BUILTINS, WIRE_POSTS,
//! post_serves, post_script), the g-edm.cps script.

use flecs_ecs::prelude::World;
use serde_json::json;

use camcraft_app::cl::{CompSide, MoveKind};
use camcraft_app::commands::{CreateOperation, GenerateOperation, History, ImportModel, LoadMachine};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{parse_mch, Kind};
use camcraft_app::operation::{self, Height, OpParams, SrcRef, WireContourOp, WireSide, WireSkim};
use camcraft_app::post::{self, cnc};
use camcraft_app::posting::{self, WIRE_POSTS};
use camcraft_app::world::new_world;

/// A synthetic 2-axis G-EDM build: no `edm.codes`, no AWT.
const FIXTURE: &str = include_str!("edm/g-edm-fixture.mch");
const GEDM: &str = "G-EDM Wire EDM";
const GOLDEN: &str = "tests/golden/nc/g-edm/wire_slug_punch.nc";

fn cps() -> &'static str {
    posting::builtin_post_text(GEDM).expect("registered")
}

/// A sharp 40 x 20 slug from z = 0 to 12, centred on the origin.
fn slug() -> Mesh {
    let pts = [(20.0, 10.0), (-20.0, 10.0), (-20.0, -10.0), (20.0, -10.0)];
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut f = Vec::new();
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        f.push(Triangle::new(Point3::new(0.0, 0.0, 12.0), p(a, 12.0), p(b, 12.0)));
        f.push(Triangle::new(Point3::new(0.0, 0.0, 0.0), p(b, 0.0), p(a, 0.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 0.0), p(b, 12.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 12.0), p(a, 12.0)));
    }
    Mesh::from_facets(f)
}

/// The machine mounted, a punch cut around the slug on condition R1 from
/// a start point outside it, with `skims` after the rough cut, generated.
fn punch_doc(skims: Vec<WireSkim>) -> World {
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(FIXTURE).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "g-edm.mch".into(), Some(m)).expect("mount"));
    h.execute(
        &world,
        camcraft_app::commands::AddTool::new(&world, "EDM-W025", camcraft_app::wire_tools::wire_tool(0.25))
            .expect("wire"),
    );
    h.execute(&world, ImportModel::from_mesh("slug", slug()));
    let op = WireContourOp {
        lead_arc_radius: None,
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: "R1".into(),
        side: WireSide::Punch,
        picks: camcraft_app::path_picks::PathPicks { start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(-30.0, 0.0))], exit: Vec::new() },
        tabs: None,
        capture: true,
        both_pieces_restrained: false,
        stock_centered_in_xy_travel: false,
        taper_deg: 0.0,
        four_axis: None,
        burnout: None,
        clearance: operation::DEFAULT_CLEARANCE,
        skims,
        sharp_corners: false,
    };
    let c = CreateOperation::new(&world, "Punch1", "EDM-W025", SrcRef::Entity("slug".into()), OpParams::WireContour(op)).expect("op");
    h.execute(&world, c);
    h.execute(&world, GenerateOperation::new(&world, "Punch1").expect("generates"));
    world
}

fn records() -> Vec<cnc::Record> {
    posting::stream_records(&punch_doc(Vec::new())).expect("the wire section streams")
}

/// `recs` with `name` stated right after the section's EDM condition.
fn with_param(mut recs: Vec<cnc::Record>, name: &str, value: serde_json::Value) -> Vec<cnc::Record> {
    let at = recs
        .iter()
        .position(|r| matches!(r, cnc::Record::Param { name, .. } if name == "EdmCondition"))
        .expect("the stream states its EDM condition");
    recs.insert(at + 1, cnc::Record::Param { name: name.into(), value });
    recs
}

fn run_on(machine_text: &str, recs: &[cnc::Record], options: &serde_json::Value) -> post::engine::PostOutput {
    let m = parse_mch(machine_text).expect("parses");
    post::engine::run_with(cps(), &post::machine::mirror(&m), &cnc::serialize(recs), options).expect("the engine runs")
}

fn run(recs: &[cnc::Record]) -> post::engine::PostOutput {
    run_on(FIXTURE, recs, &serde_json::Value::Null)
}

/// The words of the program outside its comments.
fn code_words(program: &str) -> Vec<String> {
    program.lines().filter(|l| !l.starts_with('(')).flat_map(str::split_whitespace).map(str::to_owned).collect()
}

/// A refused stream: a warning that holds each of `said`, and no cut.
fn refused(out: &post::engine::PostOutput, said: &[&str]) {
    assert!(out.warnings.iter().any(|w| said.iter().all(|s| w.contains(s))), "{said:?} not in {:?}", out.warnings);
    let words = code_words(&out.program);
    assert!(!words.iter().any(|w| ["G1", "G2", "G3"].contains(&w.as_str())), "{}", out.program);
}

/// Done means: the 2-axis contour posts its golden through the app's own
/// gate, with no `edm.codes` in the machine file.
#[test]
fn g_edm_punch_contour_posts_its_golden_through_the_gate() {
    let world = punch_doc(Vec::new());
    let out = std::env::temp_dir().join(format!("camcraft-g-edm-{}.nc", std::process::id()));
    let posted = posting::post_script(&world, &out, cps()).expect("the wire program posts");
    let written = std::fs::read_to_string(&out);
    let _ = std::fs::remove_file(&out);
    let text = posted.program;
    assert_eq!(written.ok().as_deref(), Some(text.as_str()), "the program is written");
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(GOLDEN, &text).expect("write golden");
    }
    let want = std::fs::read_to_string(GOLDEN).expect("golden present");
    assert_eq!(text, want, "the G-EDM wire program drifted from its golden");

    let lines: Vec<&str> = text.lines().collect();
    let at = |w: &str| lines.iter().position(|l| *l == w).unwrap_or_else(|| panic!("no {w} in\n{text}"));
    let (modes, g0, first, end) = (at("G21 G90 G17"), at("G0 X-30. Y0."), at("G1 X-20.165 Y0."), at("M30"));
    assert!(modes < g0 && g0 < first && first < end, "{text}");
    assert_eq!(end, lines.len() - 1, "M30 ends the program:\n{text}");
    // Grbl reads no tape fence, no O word, no radius offset; the gap servo
    // sets the feed, so no F, no spindle, no Z, no U V.
    for l in lines.iter().filter(|l| !l.starts_with('(')) {
        let words: Vec<&str> = l.split_whitespace().collect();
        let banned = ["G40", "G41", "G42", "G54", "G92", "M0", "M00", "M1", "M2", "M3", "M5", "%"];
        assert!(!words.iter().any(|w| banned.contains(w)), "{l}");
        assert!(!words.iter().any(|w| w.starts_with(['O', 'N', 'F', 'S', 'Z', 'U', 'V', 'H', 'D', 'T'])), "{l}");
    }
    assert!(!text.contains('%'), "{text}");
    // Grbl's line buffer: every line is short.
    assert!(lines.iter().all(|l| l.len() <= 70), "{text}");
    // The header states what the program leaves to the operator.
    for said in [
        "(NOT VERIFIED ON A CONTROL - CHECK BEFORE CUTTING)",
        "(SPARK POWER IS SET AT THE G-EDM CONTROLLER)",
        "(CONDITION R1 - SET ITS POWER AT THE CONTROLLER)",
        "(THREAD THE WIRE AT X-30. Y0. BEFORE START)",
    ] {
        assert!(lines.contains(&said), "{said}:\n{text}");
    }
}

#[test]
fn g_edm_post_serves_wire_edms_only_and_is_never_the_default() {
    assert!(WIRE_POSTS.contains(&GEDM));
    assert!(posting::post_serves(GEDM, Kind::WireEdm));
    for k in [Kind::Mill, Kind::Lathe, Kind::MillTurn, Kind::Plasma, Kind::SinkerEdm] {
        assert!(!posting::post_serves(GEDM, k), "{k:?}");
    }
    assert_eq!(WIRE_POSTS.first(), Some(&"Generic ISO Wire EDM"), "the generic post stays the default");
}

/// The document's unit decides G21 / G20 (#902); an arc is G2 / G3 with
/// the centre as I J increments from the arc's start.
#[test]
fn g_edm_inch_and_arc_words() {
    let recs = records();
    let out = run_on(FIXTURE, &recs, &json!({ "Units": "inch" }));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    assert!(lines.contains(&"G20 G90 G17") && lines.contains(&"G1 X-0.7939 Y0."), "{}", out.program);

    let mut arc = recs;
    let mut made = false;
    for r in &mut arc {
        if let cnc::Record::Move(m) = r
            && m.kind == MoveKind::Feed
            && (m.x - 20.165).abs() < 1e-6
            && (m.y - 10.165).abs() < 1e-6
        {
            m.center = Some((20.165, 0.0));
            m.ccw = true;
            made = true;
        }
    }
    assert!(made, "the punch has its right side");
    let out = run(&arc);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.lines().any(|l| l == "G3 X20.165 Y10.165 I0. J10.165"), "{}", out.program);
}

/// Done means: taper, 4-axis, a leaning move and an angle word refuse by
/// name; no U V word is written.
#[test]
fn g_edm_refuses_taper_four_axis_and_lean_by_name() {
    let recs = records();
    assert!(run(&recs).warnings.is_empty(), "the vertical punch posts clean");
    refused(&run(&with_param(recs.clone(), "WireTaper", json!(3.0))), &["Punch1", "is tapered", "2-axis"]);
    refused(&run(&with_param(recs.clone(), "WireFourAxis", json!("arc-length"))), &["Punch1", "4-axis"]);
    refused(&run(&with_param(recs.clone(), "WireAngle", json!(2.0))), &["Punch1", "angle word"]);
    let mut lean = recs;
    let m = lean.iter().rposition(|r| matches!(r, cnc::Record::Move(m) if m.kind == MoveKind::Feed)).expect("a cut");
    lean.insert(m, cnc::Record::Param { name: "WireU".into(), value: json!(0.5) });
    lean.insert(m + 1, cnc::Record::Param { name: "WireV".into(), value: json!(0.0) });
    let out = run(&lean);
    assert!(out.warnings.iter().any(|w| w.contains("a leaning move")), "{:?}", out.warnings);
    assert!(!code_words(&out.program).iter().any(|w| w.starts_with(['U', 'V'])), "{}", out.program);
}

/// Done means: a G41 / G42 (or any COMP record) refuses by name.
#[test]
fn g_edm_refuses_radius_compensation_by_name() {
    let mut recs = records();
    let at = recs.iter().position(|r| matches!(r, cnc::Record::Move(_))).expect("a move");
    recs.insert(at, cnc::Record::Comp { side: Some(CompSide::Left), radius_mm: 0.165, wear: false });
    let out = run(&recs);
    refused(&out, &["Punch1", "G41 / G42"]);
    assert!(!code_words(&out.program).iter().any(|w| w == "G41" || w == "G42"), "{}", out.program);
}

/// The cut lines (G1 / G2 / G3) of a program, in order.
fn cut_lines(program: &str) -> Vec<&str> {
    program.lines().filter(|l| l.split_whitespace().next().is_some_and(|w| ["G1", "G2", "G3"].contains(&w))).collect()
}

/// A rapid after the first cut writes no word, so it must refuse: a
/// dropped rapid lets the next G1 cut a straight line that the verified
/// path does not have (#3312 review). No cut is written after it.
#[test]
fn g_edm_refuses_a_rapid_with_the_wire_threaded_by_name() {
    let recs = records();
    let clean = run(&recs);
    assert!(clean.warnings.is_empty(), "{:?}", clean.warnings);
    let feeds: Vec<usize> = recs
        .iter()
        .enumerate()
        .filter(|(_, r)| matches!(r, cnc::Record::Move(m) if m.kind == MoveKind::Feed))
        .map(|(i, _)| i)
        .collect();
    assert!(feeds.len() >= 3, "the punch has a lead-in and a contour");
    let at = feeds[1];
    let mut rapid = recs[at].clone();
    if let cnc::Record::Move(m) = &mut rapid {
        m.kind = MoveKind::Rapid;
    }
    let mut recs = recs;
    recs.insert(at, rapid);
    let out = run(&recs);
    assert!(
        out.warnings.iter().any(|w| w.contains("Punch1") && w.contains("rapid with the wire threaded")),
        "{:?}",
        out.warnings
    );
    // Only the cut before the rapid is written; no cut follows it.
    let before = &cut_lines(&clean.program)[..1];
    assert_eq!(cut_lines(&out.program), before, "{}", out.program);
}

/// Done means: skims refuse by name, through the app's own post - no file.
#[test]
fn g_edm_refuses_skims_by_name() {
    let world = punch_doc(vec![WireSkim { condition: "S2".into(), reverse: false }]);
    let out = std::env::temp_dir().join(format!("camcraft-g-edm-skim-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let err = posting::post_script(&world, &out, cps()).expect_err("a skim never posts");
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    assert!(err.contains("Punch1") && err.contains("skim"), "{err}");
    assert!(!wrote, "the refused post wrote {}", out.display());
}

/// Done means: a non-wire machine and a mill golden refuse by name.
#[test]
fn g_edm_refuses_a_mill_machine_and_a_milling_stream_by_name() {
    let mill = std::fs::read_to_string("../../assets/machines/3axis-vmc.mch").expect("the vmc");
    let out = run_on(&mill, &records(), &serde_json::Value::Null);
    refused(&out, &["is not declared kind wire_edm"]);
    for g in ["face_square_40x40.cl", "drill_peck.cl"] {
        let golden = std::fs::read_to_string(format!("tests/golden/{g}")).expect("a mill golden");
        let m = parse_mch(FIXTURE).expect("parses");
        let out = post::engine::run(cps(), &post::machine::mirror(&m), &golden).expect("the engine runs");
        assert!(!out.warnings.is_empty(), "{g}: a mill stream must refuse");
        assert!(!code_words(&out.program).iter().any(|w| ["G0", "G1", "G2", "G3"].contains(&w.as_str())), "{g}: {}", out.program);
    }
}

/// What G-EDM cannot be told in G-code is refused by name: automatic
/// threading, a work offset other than G54, a spindle, a trip home, a
/// stop the controller is not confirmed to read, a second wire section
/// (no confirmed pause to cut and thread again).
#[test]
fn g_edm_refuses_what_the_controller_cannot_be_told() {
    let recs = records();
    let awt = FIXTURE.replace("\"awt\": false", "\"awt\": true");
    assert_ne!(awt, FIXTURE);
    refused(&run_on(&awt, &recs, &serde_json::Value::Null), &["edm.awt"]);
    let mut g55 = recs.clone();
    for r in &mut g55 {
        if let cnc::Record::Param { name, value } = r
            && name == "WorkOffset"
        {
            *value = json!("G55");
        }
    }
    refused(&run(&g55), &["Punch1", "G55"]);
    refused(&run(&with_param(recs.clone(), "SpindleSpeed", json!(1000))), &["Punch1", "rpm"]);
    for (ask, said) in [("home", "trip home"), ("stop", "stop"), ("optional-stop", "stop")] {
        let out = run(&with_param(recs.clone(), "AfterOperation", json!(ask)));
        assert!(out.warnings.iter().any(|w| w.contains("Punch1") && w.contains(said)), "{ask}: {:?}", out.warnings);
    }
    // The same section twice: the second is refused by name.
    let mut twice = recs.clone();
    let body: Vec<cnc::Record> = recs
        .iter()
        .skip_while(|r| !matches!(r, cnc::Record::Param { name, .. } if name == "EdmCondition"))
        .cloned()
        .collect();
    assert!(body.iter().any(|r| matches!(r, cnc::Record::Section { begin: true })), "the copy holds the section");
    twice.extend(body);
    let out = run(&twice);
    assert!(out.warnings.iter().any(|w| w.contains("one wire section")), "{:?}", out.warnings);
}

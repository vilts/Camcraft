//! The Sodick wire EDM post (#617): a 2-axis Wire Contour through
//! `sodick-wire.cps`, posted by the app's own gate (verification and the
//! wire laws included) to its golden, and its refusals by name.
//! Provenance: the words asserted here are the ones Sodick's 1998 "Wire
//! EDM Machine Operation" manual (Mark 21 / EX21 control) prints - T91 /
//! T90, the C condition word, G40 on a move (RS-274 for G40 beside G00),
//! the decimal point, M02; no
//! other CAM product's post or output was read. The output was not run
//! on a Sodick control (docs/decisions/617-sodick-wire-post.md).
//!
//! Regenerate the golden after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test sodick_wire_post`
//!
//! Gate map (#617): covers app::posting (BUILTINS, WIRE_POSTS,
//! post_serves, post_script), the sodick-wire.cps script.

use flecs_ecs::prelude::World;
use serde_json::json;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{CreateOperation, GenerateOperation, History, ImportModel, LoadMachine};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{parse_mch, Kind};
use camcraft_app::operation::{self, Height, OpParams, SrcRef, WireContourOp, WireSide, WireSkim};
use camcraft_app::post::{self, cnc};
use camcraft_app::posting::{self, BUILTINS, WIRE_POSTS};
use camcraft_app::world::new_world;

/// The synthetic wire EDM of #533: its `edm.codes` are fictional and this
/// post reads none of them - only `kind`, `edm.awt` and the condition.
const FIXTURE: &str = include_str!("edm/wire-iso-fixture.mch");
const SODICK: &str = "Sodick Wire EDM (Mark 21)";
const GOLDEN: &str = "tests/golden/nc/sodick-wire/wire_slug_punch.nc";

fn cps() -> &'static str {
    BUILTINS.iter().find(|(n, _)| *n == SODICK).map(|(_, s)| *s).expect("registered")
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

/// The machine mounted, a punch cut around the slug under condition
/// `condition` from a start hole outside it, generated.
fn punch_doc(machine_text: &str, condition: &str) -> World {
    punch_doc_with(machine_text, condition, Vec::new())
}

/// [`punch_doc`] with skim passes after the rough cut (#538).
fn punch_doc_with(machine_text: &str, condition: &str, skims: Vec<WireSkim>) -> World {
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(machine_text).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "wire.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, ImportModel::from_mesh("slug", slug()));
    let op = WireContourOp {
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: condition.into(),
        side: WireSide::Punch,
        start: Some([-30.0, 0.0]),
        tabs: None,
        capture: true,
        taper_deg: 0.0,
        four_axis: None,
        burnout: None,
        clearance: operation::DEFAULT_CLEARANCE,
        skims,
    };
    let c = CreateOperation::new(&world, "Punch1", "", SrcRef::Entity("slug".into()), OpParams::WireContour(op)).expect("op");
    h.execute(&world, c);
    h.execute(&world, GenerateOperation::new(&world, "Punch1").expect("generates"));
    world
}

fn records(world: &World) -> Vec<cnc::Record> {
    posting::stream_records(world).expect("the wire section streams")
}

/// `recs` with `extra` stated right after the section's EDM condition.
fn with_param(mut recs: Vec<cnc::Record>, name: &str, value: serde_json::Value) -> Vec<cnc::Record> {
    let at = recs
        .iter()
        .position(|r| matches!(r, cnc::Record::Param { name, .. } if name == "EdmCondition"))
        .expect("the stream states its EDM condition");
    recs.insert(at + 1, cnc::Record::Param { name: name.into(), value });
    recs
}

fn run(machine_text: &str, recs: &[cnc::Record], options: &serde_json::Value) -> post::engine::PostOutput {
    let m = parse_mch(machine_text).expect("parses");
    post::engine::run_with(cps(), &post::machine::mirror(&m), &cnc::serialize(recs), options).expect("the engine runs")
}

fn warnings(machine_text: &str, recs: &[cnc::Record]) -> Vec<String> {
    run(machine_text, recs, &serde_json::Value::Null).warnings
}

/// Every X Y I J word of `line` carries a decimal point.
fn pointed(line: &str) -> bool {
    line.split_whitespace().filter(|w| w.starts_with(['X', 'Y', 'I', 'J'])).all(|w| w.contains('.'))
}

/// Done means: the 2-axis contour posts its golden through the app's own
/// gate, in the manual's words.
#[test]
fn the_punch_contour_posts_its_sodick_golden_through_the_gate() {
    let world = punch_doc(FIXTURE, "C110");
    let out = std::env::temp_dir().join(format!("camcraft-sodick-wire-{}.nc", std::process::id()));
    let posted = posting::post_script(&world, &out, cps()).expect("the wire program posts");
    let written = std::fs::read_to_string(&out);
    let _ = std::fs::remove_file(&out);
    let text = posted.program;
    assert_eq!(written.ok().as_deref(), Some(text.as_str()), "the program is written");
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(GOLDEN, &text).expect("write golden");
    }
    let want = std::fs::read_to_string(GOLDEN).expect("golden present");
    assert_eq!(text, want, "the Sodick wire program drifted from its golden");

    // The shape the golden locks, said out loud: the unit and G90, the
    // setup's offset, the wire offset cancelled on the G00 to the start
    // hole (before any motion, so an offset left by an earlier program
    // never shifts the positioning or the threading point), threaded
    // (T91), the condition word, X Y cuts with no F and no Z, the wire
    // cut (T90), M02.
    let lines: Vec<&str> = text.lines().collect();
    let at = |w: &str| lines.iter().position(|l| *l == w).unwrap_or_else(|| panic!("no {w} in\n{text}"));
    let (unit, abs, wcs, g0, th, cond, first, cut, end) = (
        at("G21"),
        at("G90"),
        at("G54"),
        at("G40 G00 X-30. Y0."),
        at("T91"),
        at("C110"),
        at("G01 X-20.165 Y0."),
        at("T90"),
        at("M02"),
    );
    assert!(unit < abs && abs < wcs && wcs < g0 && g0 < th && th < cond && cond < first && first < cut && cut < end, "{text}");
    assert_eq!(end, lines.len() - 1, "M02 ends the program:\n{text}");
    let cuts = &lines[first + 1..cut];
    assert!(!cuts.is_empty() && cuts.iter().all(|l| l.starts_with("G01 X") && !l.contains('Z') && !l.contains('F')), "{text}");
    // A number without its decimal point is read in the control's
    // smallest unit: every coordinate carries one.
    for l in lines.iter().filter(|l| !l.starts_with('(')) {
        assert!(pointed(l), "{l}");
        let words: Vec<&str> = l.split_whitespace().collect();
        // Nothing the manual does not confirm for this slice, no control
        // wire offset, no tape fence, no spindle or feed words.
        let banned = ["G41", "G42", "G92", "G29", "G17", "G50", "G51", "G52", "G74", "G75", "M30", "M03", "M3", "T84", "T85", "T94", "%"];
        assert!(!words.iter().any(|w| banned.contains(w)), "{l}");
        assert!(!words.iter().any(|w| w.starts_with(['H', 'A', 'U', 'V', 'S', 'F', 'N'])), "{l}");
    }
    assert_eq!(lines.iter().filter(|l| l.contains("G40")).count(), 1, "{text}");
    // The C word selects a row of the control's own table, which the
    // program does not carry: the header says so, and the section names
    // its condition before the first motion.
    let table = at("(EACH C CONDITION NAMED BELOW MUST EXIST IN THE CONTROL CONDITION TABLE BEFORE START)");
    let named = at("(CONDITION C110 MUST EXIST IN THE CONTROL CONDITION TABLE - CHECK ITS VALUES)");
    assert!(table < unit && named < g0, "{text}");
    // The fixture's fictional codes are never read by this post.
    assert!(!text.contains("FX-"), "{text}");
    // The program says it was not verified and what it leaves to the operator.
    for said in ["NOT VERIFIED ON A CONTROL", "WIRE NOT THREADED AT START", "NO G92 IS WRITTEN", "FLUSHING ARE NOT PROGRAMMED"] {
        assert!(lines.iter().any(|l| l.starts_with('(') && l.contains(said)), "{said}:\n{text}");
    }
}

#[test]
fn the_registry_offers_the_sodick_post_for_wire_edms_only_and_never_by_default() {
    assert!(WIRE_POSTS.contains(&SODICK));
    assert!(posting::post_serves(SODICK, Kind::WireEdm));
    for k in [Kind::Mill, Kind::Lathe, Kind::MillTurn, Kind::Plasma, Kind::SinkerEdm] {
        assert!(!posting::post_serves(SODICK, k), "{k:?}");
    }
    assert_ne!(posting::post_name(&posting::default_post_for(Kind::WireEdm)), Some(SODICK), "the generic post stays the default");
}

/// What the slice cannot express is refused by name - never written as
/// something plausible - and the app writes no file.
#[test]
fn taper_and_four_axis_are_refused_by_name() {
    let world = punch_doc(FIXTURE, "C110");
    let recs = records(&world);
    assert!(warnings(FIXTURE, &recs).is_empty(), "the vertical punch posts clean");

    let out = run(FIXTURE, &with_param(recs.clone(), "WireTaper", json!(3.0)), &serde_json::Value::Null);
    assert!(out.warnings.iter().any(|m| m.contains("Punch1") && m.contains("is tapered") && m.contains("2-axis cut only")), "{:?}", out.warnings);
    assert!(!out.program.contains("G01") && !out.program.contains("T91") && !out.program.contains("C110"), "{}", out.program);

    let out = run(FIXTURE, &with_param(recs.clone(), "WireFourAxis", json!("arc-length")), &serde_json::Value::Null);
    assert!(out.warnings.iter().any(|m| m.contains("Punch1") && m.contains("is a 4-axis cut")), "{:?}", out.warnings);
    assert!(!out.program.contains("G01") && !out.program.contains("T91"), "{}", out.program);

    // A leaning move (an upper guide offset) in a section that states no
    // taper: refused, and no U V word is written.
    let mut lean = recs.clone();
    let m = lean.iter().rposition(|r| matches!(r, cnc::Record::Move(m) if m.kind == MoveKind::Feed)).expect("a cut");
    lean.insert(m, cnc::Record::Param { name: "WireU".into(), value: json!(0.5) });
    lean.insert(m + 1, cnc::Record::Param { name: "WireV".into(), value: json!(0.0) });
    let out = run(FIXTURE, &lean, &serde_json::Value::Null);
    assert!(out.warnings.iter().any(|m| m.contains("a leaning move")), "{:?}", out.warnings);
    let words = out.program.lines().filter(|l| !l.starts_with('(')).flat_map(str::split_whitespace);
    assert!(!words.into_iter().any(|w| w.starts_with(['U', 'V'])), "{}", out.program);
}

/// The condition id is the C word itself: anything but C and three
/// digits is refused by name, through the app's own post - no file.
#[test]
fn a_condition_id_that_is_not_a_sodick_c_word_is_refused_by_name() {
    let machine = FIXTURE.replace("\"id\": \"C110\"", "\"id\": \"E-ROUGH\"");
    let world = punch_doc(&machine, "E-ROUGH");
    let out = std::env::temp_dir().join(format!("camcraft-sodick-wire-cond-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let err = posting::post_script(&world, &out, cps()).expect_err("an unreadable condition word never posts");
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    assert!(err.contains("Punch1") && err.contains("E-ROUGH") && err.contains("is not a Sodick condition word"), "{err}");
    assert!(!wrote, "the refused post wrote {}", out.display());
}

#[test]
fn a_mill_machine_and_a_milling_section_are_refused_by_name() {
    let world = punch_doc(FIXTURE, "C110");
    let recs = records(&world);
    let mill = std::fs::read_to_string("../../assets/machines/3axis-vmc.mch").expect("the vmc");
    let w = warnings(&mill, &recs);
    assert!(w.iter().any(|m| m.contains("is not declared kind wire_edm")), "{w:?}");
    let golden = std::fs::read_to_string("tests/golden/face_square_40x40.cl").expect("a mill golden");
    let m = parse_mch(FIXTURE).expect("parses");
    let out = post::engine::run(cps(), &post::machine::mirror(&m), &golden).expect("the engine runs");
    assert!(out.warnings.iter().any(|m| m.contains("not wire EDM work") || m.contains("outside a wire section")), "{:?}", out.warnings);
    assert!(!out.program.contains("G01") && !out.program.contains("G00"), "{}", out.program);
}

/// Without automatic threading the operator threads and cuts at a stop
/// the program states: no T91 / T90, two M00.
#[test]
fn without_awt_the_operator_threads_and_cuts_at_a_stated_stop() {
    let world = punch_doc(FIXTURE, "C110");
    let machine = FIXTURE.replace("\"awt\": true", "\"awt\": false");
    assert_ne!(machine, FIXTURE);
    let out = run(&machine, &records(&world), &serde_json::Value::Null);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    assert_eq!(lines.iter().filter(|l| **l == "M00").count(), 2, "{}", out.program);
    assert!(!lines.contains(&"T91") && !lines.contains(&"T90"), "{}", out.program);
    assert!(lines.contains(&"(THREAD THE WIRE AT THE START HOLE)") && lines.contains(&"(CUT THE WIRE)"), "{}", out.program);
}

/// The document's unit decides G21 / G20 (#902), and an inch coordinate
/// keeps its point at four places.
#[test]
fn an_inch_document_posts_g20_with_pointed_four_place_words() {
    let world = punch_doc(FIXTURE, "C110");
    let out = run(FIXTURE, &records(&world), &json!({ "Units": "inch" }));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    assert!(lines.contains(&"G20") && !lines.contains(&"G21"), "{}", out.program);
    // -30 mm is -1.1811 in; -20.165 mm is -0.7939 in.
    assert!(lines.contains(&"G40 G00 X-1.1811 Y0."), "{}", out.program);
    assert!(lines.contains(&"G01 X-0.7939 Y0."), "{}", out.program);
    assert!(lines.iter().filter(|l| !l.starts_with('(')).all(|l| pointed(l)), "{}", out.program);
}

/// An arc is G02 / G03 with the centre as I J increments from the arc's
/// start (the manual's I J), each with its decimal point; an arc as the
/// first cut is refused (every cut starts straight from the threading point).
#[test]
fn an_arc_posts_g03_with_incremental_i_j_and_never_as_the_first_cut() {
    let world = punch_doc(FIXTURE, "C110");
    let recs = records(&world);
    // The punch's right side, (20.165, -10.165) up to (20.165, 10.165),
    // made a half circle about (20.165, 0).
    let mut arc = recs.clone();
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
    let out = run(FIXTURE, &arc, &serde_json::Value::Null);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.lines().any(|l| l == "G03 X20.165 Y10.165 I0. J10.165"), "{}", out.program);

    // The first cutting move made an arc: refused by name, nothing cut.
    let mut first = recs.clone();
    let f = first
        .iter_mut()
        .find_map(|r| match r {
            cnc::Record::Move(m) if m.kind != MoveKind::Rapid => Some(m),
            _ => None,
        })
        .expect("a first cut");
    f.center = Some((f.x - 5.0, f.y));
    let out = run(FIXTURE, &first, &serde_json::Value::Null);
    assert!(out.warnings.iter().any(|m| m.contains("the first cut is an arc")), "{:?}", out.warnings);
    assert!(!out.program.contains("G01") && !out.program.contains("G03") && !out.program.contains("G02"), "{}", out.program);
}

/// #538: this post writes its C word once, when the wire is threaded, so
/// a skimmed contour would cut every skim on the rough pass's condition.
/// A condition change after the section's first cut is refused by name,
/// through the app's own post - no file, no C word for the skims.
#[test]
fn skims_are_refused_by_name_never_cut_on_the_rough_condition() {
    let machine = FIXTURE.replace("\"id\": \"S21\"", "\"id\": \"C121\"").replace("\"id\": \"S31\"", "\"id\": \"C131\"");
    assert_ne!(machine, FIXTURE);
    let skims = vec![WireSkim { condition: "C121".into(), reverse: false }, WireSkim { condition: "C131".into(), reverse: false }];
    let world = punch_doc_with(&machine, "C110", skims);
    let out = std::env::temp_dir().join(format!("camcraft-sodick-wire-skims-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let err = posting::post_script(&world, &out, cps()).expect_err("a skimmed contour never posts through the Sodick post");
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    assert!(err.contains("Punch1") && err.contains("a condition change between passes (skims) is not supported by this post"), "{err}");
    assert!(!wrote, "the refused post wrote {}", out.display());

    // The stream itself: the skims' conditions are announced mid-section,
    // and the post refuses there - neither skim's C word is written.
    let recs = records(&world);
    let run_out = run(&machine, &recs, &serde_json::Value::Null);
    assert!(run_out.warnings.iter().any(|m| m.contains("skims") && m.contains("wire-iso")), "{:?}", run_out.warnings);
    assert!(!run_out.program.contains("C121") && !run_out.program.contains("C131"), "{}", run_out.program);
}

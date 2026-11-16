//! #533 (part of #290): the generic ISO wire EDM post. A Wire Contour
//! posts end to end through the app's gate on a synthetic wire EDM whose
//! `edm.codes` are invented (clearly fictional `FX-...` words, #290
//! D10) - the golden locks the RS-274 motion and where each machine code
//! lands; every code the post needs, removed, refuses the post by its
//! name; a mill machine, and a cutting post on the wire section, are
//! refused by name; the registry offers the post for wire EDMs only.
//! The wire laws (#534) gate it: a clean cut posts its golden, a
//! gouging one is refused by name with no NC text written. An open
//! cut-off (#1109, kerf left / right of travel) posts its own golden -
//! threaded at its run-in point in the air, cut off at its run-out point,
//! no return move - and an open run whose kerf burns the part is refused.
//! #538: a rough pass and two skims post with three distinct offsets
//! and condition selects (their own golden); each skim refusal by name;
//! the rough-only golden above does not move.
//! Not exercised on a real EDM control (D10).
//!
//! Gate map (#533): covers app::posting (posting_wire, WIRE_POSTS,
//! post_serves, default_post_for, op_streams' wire branch),
//! app::posting_serves (the serves door: a sinker refused), post::engine
//! (the wireEdm door), post::machine (edm codes mirror, edm_codes), and
//! the wire-iso.cps script, and the wire clause's place in the post
//! gate (posting_wire's wire_clause before NC text, #534); #538:
//! operation::wire_op (skims), motion::wire_edm::skims, posting_wire's
//! skim announcements.
//! Provenance: our own tests over our own synthetic fixture.

#[path = "edm/sinker_fixture.rs"]
mod sinker_fixture;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{parse_mch, Kind};
use camcraft_app::operation::{self, Height, OpParams, SrcRef, WireContourOp, WireSide, WireSkim};
use camcraft_app::post::{self, cnc};
use camcraft_app::model;
use camcraft_app::posting::{self, BUILTINS, WIRE_POSTS};
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

const FIXTURE: &str = include_str!("edm/wire-iso-fixture.mch");
const WIRE: &str = "Generic ISO Wire EDM";
const GOLDEN: &str = "tests/golden/nc/wire-iso/wire_slug_punch.nc";
const SKIMS_GOLDEN: &str = "tests/golden/nc/wire-iso/wire_contour_punch_skims.nc";
const OPEN_GOLDEN: &str = "tests/golden/nc/wire-iso/wire_contour_open_cutoff.nc";

fn cps(name: &str) -> &'static str {
    BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s).expect("registered")
}

/// A sharp 40 x 20 slug from z = 0 to 12, centred on the origin.
fn slug() -> Mesh {
    prism(&[(20.0, 10.0), (-20.0, 10.0), (-20.0, -10.0), (20.0, -10.0)])
}

/// A 40 x 20 die opening with 2 mm corners (3-degree facets) from z = 0
/// to 12 - a die cannot cut a sharp inside corner.
fn opening() -> Mesh {
    let (r, hx, hy) = (2.0, 18.0, 8.0);
    let mut pts = Vec::new();
    for (cx, cy, a0) in [(hx, hy, 0.0_f64), (-hx, hy, 90.0), (-hx, -hy, 180.0), (hx, -hy, 270.0)] {
        for s in 0..=30 {
            let t = (a0 + 3.0 * f64::from(s)).to_radians();
            pts.push((cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    prism(&pts)
}

/// The CCW outline `pts` extruded from z = 0 to 12.
fn prism(pts: &[(f64, f64)]) -> Mesh {
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

/// The fixture machine mounted, a punch cut around the slug from a
/// start hole outside it, generated.
fn punch_doc(machine_text: &str) -> World {
    punch_doc_with(machine_text, Vec::new())
}

/// [`punch_doc`] with skim passes after the rough cut (#538).
fn punch_doc_with(machine_text: &str, skims: Vec<WireSkim>) -> World {
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(machine_text).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "wire-iso.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, ImportModel::from_mesh("slug", slug()));
    let op = WireContourOp {
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
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

/// A die cut through the 12 mm deep opening in a 60 x 60 x 20 plate - the
/// wire's kerf enters the plate above the opening. Generated: the generator does not judge the plate; the laws do.
fn gouging_die() -> World {
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(FIXTURE).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "wire-iso.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, ImportModel::from_mesh("opening", opening()));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock");
    h.execute(&world, stock);
    let op = WireContourOp {
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side: WireSide::Die,
        start: Some([0.0, 0.0]),
        tabs: None,
        capture: true,
        taper_deg: 0.0,
        four_axis: None,
        burnout: None,
        clearance: operation::DEFAULT_CLEARANCE,
        skims: Vec::new(),
    };
    let c = CreateOperation::new(&world, "Die1", "", SrcRef::Entity("opening".into()), OpParams::WireContour(op)).expect("op");
    h.execute(&world, c);
    h.execute(&world, GenerateOperation::new(&world, "Die1").expect("generates"));
    world
}

fn stream_text(world: &World) -> String {
    cnc::serialize(&posting::stream_records(world).expect("the wire section streams"))
}

/// The engine's counted warnings for `cps` over `stream` on `machine_text`.
fn warnings(cps: &str, machine_text: &str, stream: &str) -> Vec<String> {
    let m = parse_mch(machine_text).expect("parses");
    post::engine::run(cps, &post::machine::mirror(&m), stream).expect("the engine runs").warnings
}

/// Done means: the 2-axis contour posts a golden against the synthetic
/// fixture dialect - through the app's own gate, verification included.
#[test]
fn the_punch_contour_posts_its_golden_through_the_gate() {
    let world = punch_doc(FIXTURE);
    let out = std::env::temp_dir().join(format!("camcraft-wire-iso-{}.nc", std::process::id()));
    let posted = posting::post_script(&world, &out, cps(WIRE)).expect("the wire program posts");
    let _ = std::fs::remove_file(&out);
    let text = posted.program;
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(GOLDEN, &text).expect("write golden");
    }
    let want = std::fs::read_to_string(GOLDEN).expect("golden present");
    assert_eq!(text, want, "the wire program drifted from its golden");
    // The shape the golden locks, said out loud: the wire goes to the
    // start hole cut, threads, selects the condition, cancels comp, cuts
    // in X Y only with no F, then cuts the wire and drops its tip.
    let lines: Vec<&str> = text.lines().collect();
    let at = |w: &str| lines.iter().position(|l| *l == w).unwrap_or_else(|| panic!("no {w} in\n{text}"));
    let (g0, th, cond, comp, cut, tip) =
        (at("G0 X-30 Y0"), at("FX-THREAD"), at("FX-COND C110"), at("FX-COMP-OFF"), at("FX-CUT"), at("FX-TIP-OUT"));
    assert!(g0 < th && th < cond && cond < comp && comp < cut && cut < tip, "{text}");
    let cuts: Vec<&&str> = lines[comp + 1..cut].iter().collect();
    assert!(!cuts.is_empty() && cuts.iter().all(|l| l.starts_with("G1 X") && !l.contains('Z') && !l.contains('F')), "{text}");
    // No control compensation, no spindle words.
    for l in &lines {
        let words: Vec<&str> = l.split_whitespace().collect();
        assert!(
            !words.iter().any(|w| ["G41", "G42", "M3", "M03", "M4", "M04"].contains(w) || (w.starts_with('S') && w[1..].parse::<f64>().is_ok())),
            "{l}"
        );
    }
}

/// Done means (#534 over #533): the wire laws stand between a checked
/// cut and its NC text. The clean punch is judged by them and posts its
/// golden; a die whose kerf burns the plate above its opening streams
/// like any wire cut but is refused by name at the gate, and no NC file
/// is written.
#[test]
fn the_wire_laws_stand_between_the_cut_and_its_nc_text() {
    let world = punch_doc(FIXTURE);
    let clause = posting::wire_clause(&world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
    let line = clause.line.expect("the clean punch is judged, not skipped");
    assert!(line.contains("Punch1 frees") && line.ends_with("(the part) into the stated capture"), "{line}");
    let out = std::env::temp_dir().join(format!("camcraft-wire-laws-ok-{}.nc", std::process::id()));
    let posted = posting::post_script(&world, &out, cps(WIRE)).expect("the judged punch posts");
    let written = std::fs::read_to_string(&out);
    let _ = std::fs::remove_file(&out);
    assert_eq!(written.ok().as_deref(), Some(posted.program.as_str()), "the program is written");
    assert_eq!(posted.program, std::fs::read_to_string(GOLDEN).expect("golden present"), "the golden holds");
    assert!(posted.not_judged.is_some_and(|n| n.contains(posting::WIRE_VERDICT)), "said, not silent");

    // A die through a 12 mm deep opening in a 20 mm plate: above
    // the opening the vertical wire burns the plate itself (#534's own
    // gouge), now in front of a real wire post.
    let world = gouging_die();
    posting::op_streams(&world).expect("the gouging cut still streams - the laws, not the stream, refuse it");
    let out = std::env::temp_dir().join(format!("camcraft-wire-laws-gouge-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let err = posting::post_script(&world, &out, cps(WIRE)).expect_err("a gouging wire cut never posts");
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    assert!(err.starts_with("post blocked") && err.contains("Die1: wire gouge: the kerf of move"), "{err}");
    assert!(!wrote, "no NC text is written for a refused wire cut");
}

/// Done means: removing any required code refuses by name.
#[test]
fn each_required_code_removed_refuses_the_post_by_name() {
    let world = punch_doc(FIXTURE);
    let stream = stream_text(&world);
    assert!(warnings(cps(WIRE), FIXTURE, &stream).is_empty(), "the full fixture posts clean");
    for code in ["condition", "comp_off", "thread", "cut", "tip_disposal"] {
        let mut v: serde_json::Value = serde_json::from_str(FIXTURE).expect("json");
        v["edm"]["codes"].as_object_mut().expect("codes").remove(code);
        let w = warnings(cps(WIRE), &v.to_string(), &stream);
        let want = format!("machine SYNTHETIC WIRE EDM (FICTIONAL CODES) states no edm.codes.{code}");
        assert!(w.iter().any(|m| m.contains(&want)), "{code}: {w:?}");
    }
    // Without AWT the operator threads and cuts at a stop: the AWT codes
    // are not needed, and the program says so with M00.
    let mut v: serde_json::Value = serde_json::from_str(FIXTURE).expect("json");
    v["edm"]["awt"] = serde_json::json!(false);
    for code in ["thread", "cut", "tip_disposal"] {
        v["edm"]["codes"].as_object_mut().expect("codes").remove(code);
    }
    let m = parse_mch(&v.to_string()).expect("parses");
    let out = post::engine::run(cps(WIRE), &post::machine::mirror(&m), &stream).expect("runs");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(out.program.lines().filter(|l| *l == "M00").count(), 2, "{}", out.program);
    // A condition code with no place for the id selects nothing: refused.
    let bad = FIXTURE.replace("FX-COND {id}", "FX-COND");
    let w = warnings(cps(WIRE), &bad, &stream);
    assert!(w.iter().any(|m| m.contains("edm.codes.condition states no {id}")), "{w:?}");
}

/// The post refuses a non-EDM machine by name, and no cutting post
/// voices the wire section (the engine's wireEdm door).
#[test]
fn a_mill_machine_and_a_cutting_post_are_refused_by_name() {
    let world = punch_doc(FIXTURE);
    let stream = stream_text(&world);
    let mill = std::fs::read_to_string("../../assets/machines/3axis-vmc.mch").expect("the vmc");
    let w = warnings(cps(WIRE), &mill, &stream);
    assert!(w.iter().any(|m| m.contains("is not declared kind wire_edm")), "{w:?}");
    for (n, s) in BUILTINS.iter().filter(|(n, _)| !WIRE_POSTS.contains(n)) {
        let w = warnings(s, FIXTURE, &stream);
        assert!(w.iter().any(|m| m.contains("Punch1") && m.contains("does not declare wireEdm")), "{n}: {w:?}");
    }
    // And a milling section through the wire post is refused by name.
    let golden = std::fs::read_to_string("tests/golden/face_square_40x40.cl").expect("a mill golden");
    let w = warnings(cps(WIRE), FIXTURE, &golden);
    assert!(w.iter().any(|m| m.contains("not wire EDM work") || m.contains("outside a wire section")), "{w:?}");
}

#[test]
fn the_registry_offers_the_wire_post_for_wire_edms_only() {
    // #617 added the Sodick dialect; the generic post stays first (the default).
    assert_eq!(WIRE_POSTS.first(), Some(&WIRE));
    for (n, _) in BUILTINS {
        assert_eq!(posting::post_serves(n, Kind::WireEdm), WIRE_POSTS.contains(n), "{n}");
    }
    for k in [Kind::Mill, Kind::Lathe, Kind::MillTurn, Kind::Plasma, Kind::SinkerEdm] {
        assert!(!posting::post_serves(WIRE, k), "{k:?}");
    }
    assert_eq!(posting::post_name(&posting::default_post_for(Kind::WireEdm)), Some(WIRE));
}

fn skim(id: &str, reverse: bool) -> WireSkim {
    WireSkim { condition: id.into(), reverse }
}

fn post_text(world: &World) -> String {
    let out = std::env::temp_dir().join(format!("camcraft-wire-skims-{}.nc", std::process::id()));
    let posted = posting::post_script(world, &out, cps(WIRE)).expect("the wire program posts");
    let _ = std::fs::remove_file(&out);
    posted.program
}

/// #538 Done means: a rough plus two skims post with three distinct
/// offsets and condition selects - one threading, one cut, the second
/// skim walked the other way round.
#[test]
fn a_rough_and_two_skims_post_three_offsets_and_condition_selects() {
    let world = punch_doc_with(FIXTURE, vec![skim("S21", false), skim("S31", true)]);
    let text = post_text(&world);
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(SKIMS_GOLDEN, &text).expect("write golden");
    }
    let want = std::fs::read_to_string(SKIMS_GOLDEN).expect("golden present");
    assert_eq!(text, want, "the skimmed wire program drifted from its golden");
    let lines: Vec<&str> = text.lines().collect();
    let count = |w: &str| lines.iter().filter(|l| **l == w).count();
    assert_eq!((count("FX-THREAD"), count("FX-CUT"), count("FX-COMP-OFF")), (1, 1, 3), "{text}");
    let selects: Vec<usize> = ["FX-COND C110", "FX-COND S21", "FX-COND S31"]
        .iter()
        .map(|w| lines.iter().position(|l| l == w).unwrap_or_else(|| panic!("no {w} in\n{text}")))
        .collect();
    assert!(selects.windows(2).all(|w| w[0] < w[1]), "{text}");
    // Each pass's lead-in meets its own offset: wire radius 0.125 plus
    // the gap 0.04, 0.02, 0.01 - the punch's left side at X = -20 - k.
    for (i, x) in ["-20.165", "-20.145", "-20.135"].iter().enumerate() {
        let lead = lines[selects[i] + 2];
        assert_eq!(lead, format!("G1 X{x} Y0"), "pass {i}: {text}");
    }
    // The first skim keeps the rough's direction (down the left side),
    // the reversed second one goes up it.
    assert_eq!(lines[selects[1] + 3], "G1 X-20.145 Y-10.145", "{text}");
    assert_eq!(lines[selects[2] + 3], "G1 X-20.135 Y10.135", "{text}");
    assert_eq!(count("(SKIM 1)") + count("(SKIM 2)"), 2, "{text}");
}

/// #538: the rough-only program is byte-identical with skims absent,
/// and its project / MCP JSON carries no `skims` key.
#[test]
fn no_skims_is_the_rough_program_and_json_unchanged() {
    let world = punch_doc(FIXTURE);
    assert_eq!(post_text(&world), std::fs::read_to_string(GOLDEN).expect("golden present"));
    let op = model::find_by_name(&world, "Punch1").and_then(|e| model::operation_of(&world, &e)).expect("op");
    let json = project::params_to_json(&op.params);
    assert!(json["wire_contour"].get("skims").is_none(), "{json}");
}

/// #538: skims ride EditOperationParams (undoable) and the JSON door
/// the file and the MCP patch share.
#[test]
fn skims_edit_undo_and_round_trip() {
    let world = punch_doc(FIXTURE);
    let mut h = History::default();
    let params = |w: &World| model::find_by_name(w, "Punch1").and_then(|e| model::operation_of(w, &e)).map(|o| o.params);
    let Some(OpParams::WireContour(mut w)) = params(&world) else { panic!("a wire contour") };
    let p0 = OpParams::WireContour(w.clone());
    w.skims = vec![skim("S21", false), skim("S31", true)];
    let p1 = OpParams::WireContour(w);
    h.execute(&world, EditOperationParams::new(&world, "Punch1", p1.clone()).expect("edit"));
    assert_eq!(params(&world), Some(p1.clone()));
    assert!(h.undo(&world));
    assert_eq!(params(&world), Some(p0));
    assert!(h.redo(&world));
    let json = project::params_to_json(&p1);
    assert_eq!(json["wire_contour"]["skims"], serde_json::json!([{ "condition": "S21" }, { "condition": "S31", "reverse": true }]));
    assert_eq!(project::params_from_json(&json).expect("reads back"), p1);
}

/// #538: each skim refusal by name, at command creation.
#[test]
fn each_skim_refusal_is_named_at_the_door() {
    let world = punch_doc(FIXTURE);
    let refusal = |rough: &str, skims: Vec<WireSkim>| {
        let Some(OpParams::WireContour(mut w)) =
            model::find_by_name(&world, "Punch1").and_then(|e| model::operation_of(&world, &e)).map(|o| o.params)
        else {
            panic!("a wire contour")
        };
        w.condition = rough.into();
        w.skims = skims;
        EditOperationParams::new(&world, "Punch1", OpParams::WireContour(w)).map(|_| ()).expect_err("refused").to_string()
    };
    let e = refusal("C110", vec![skim("", false)]);
    assert!(e.contains("skim 1 of the Wire Contour names no EDM condition") && e.contains("C110, S21, S31, W20"), "{e}");
    let e = refusal("C110", vec![skim("S21", false), skim("S99", false)]);
    assert!(e.contains("skim 2 of the Wire Contour") && e.contains("states no EDM condition \"S99\""), "{e}");
    let e = refusal("C110", vec![skim("S31", false), skim("S21", false)]);
    assert!(e.contains("skim 1 (condition S31)") && e.contains("cut past the finished profile"), "{e}");
    let e = refusal("S31", vec![skim("S21", false)]);
    assert!(e.contains("the rough pass (S31)") && e.contains("cut past the finished profile"), "{e}");
    let e = refusal("C110", vec![skim("S31", false), skim("S21", false), skim("S31", false)]);
    assert!(e.contains("skim 2 (condition S21)") && e.contains("farther out than skim 1") && e.contains("cut only air"), "{e}");
    let e = refusal("C110", vec![skim("W20", false)]);
    assert!(e.contains("skim 1 (condition W20)") && e.contains("one threading cuts every pass with one wire"), "{e}");
}

/// #1109: a 60 x 50 x 12 part at the left end of a 100 x 50 x 12 bar,
/// cut off along the part's top edge at x = 60 by an open run, the kerf
/// on the offcut's side (`wrong` puts it on the part's). The side is read
/// from the picked chain's own travel, so the fixture holds whichever way
/// the feature graph orders the edge.
fn cutoff_doc(wrong: bool) -> World {
    use camcraft_app::geometry::feature;
    use camcraft_app::model;
    use camcraft_app::operation::{FeatureSel, GeoSel};
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(FIXTURE).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "wire-iso.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, ImportModel::from_mesh("part", prism(&[(0.0, 0.0), (60.0, 0.0), (60.0, 50.0), (0.0, 50.0)])));
    let loc = Location { pos: Point3::new(50.0, 25.0, 0.0), ..Location::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 50.0, dz: 12.0 }, loc).expect("stock"));
    let e = model::find_by_name(&world, "part").expect("model");
    let f = model::features_of(&world, &e).expect("features");
    let (chain, up) = feature::chains(&f)
        .into_iter()
        .filter(|c| !c.closed)
        .find_map(|c| {
            let p = feature::chain_points(&f, &c);
            let on = p.len() >= 2 && p.iter().all(|q| (q[0] - 60.0).abs() < 1e-9 && (q[2] - 12.0).abs() < 1e-9);
            on.then(|| (c.id, p[p.len() - 1][1] > p[0][1]))
        })
        .expect("the part's top edge at x = 60 is its own open chain");
    // Travelling +Y the offcut (x > 60) is on the right.
    let side = if up != wrong { WireSide::Right } else { WireSide::Left };
    let op = WireContourOp {
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side,
        start: None,
        tabs: None,
        capture: false,
        taper_deg: 0.0,
        burnout: None,
        clearance: operation::DEFAULT_CLEARANCE,
        skims: Vec::new(),
        four_axis: None,
    };
    let src = SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: FeatureSel::Chain(chain) }]);
    h.execute(&world, CreateOperation::new(&world, "Cutoff1", "", src, OpParams::WireContour(op)).expect("op"));
    h.execute(&world, GenerateOperation::new(&world, "Cutoff1").expect("the open run generates"));
    world
}

/// Done means (#1109 over #533/#534): an open cut-off posts through
/// the wire post to its golden - threaded at the run-in point outside
/// the bar, one cut across it, the wire cut at the run-out point, never
/// a move back to the threading point - judged by the kerf law with the
/// start-hole and slug laws said skipped; the same run with its kerf on
/// the part's side is refused at the gate with no NC text written, and
/// the closed punch's golden is untouched.
#[test]
fn an_open_cutoff_posts_its_golden_and_a_gouging_one_is_refused() {
    let world = cutoff_doc(false);
    let clause = posting::wire_clause(&world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
    let line = clause.line.expect("the open run is judged, not skipped");
    assert!(line.contains("Cutoff1 open run (") && line.contains("kerf judged") && line.contains("slug laws not applied"), "{line}");
    let out = std::env::temp_dir().join(format!("camcraft-wire-open-{}.nc", std::process::id()));
    let posted = posting::post_script(&world, &out, cps(WIRE)).expect("the open cut-off posts");
    let _ = std::fs::remove_file(&out);
    let text = posted.program;
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(OPEN_GOLDEN, &text).expect("write golden");
    }
    assert_eq!(text, std::fs::read_to_string(OPEN_GOLDEN).expect("golden present"), "the open program drifted from its golden");
    let lines: Vec<&str> = text.lines().collect();
    let at = |w: &str| lines.iter().position(|l| *l == w).unwrap_or_else(|| panic!("no {w} in\n{text}"));
    let (comp, cut) = (at("FX-COMP-OFF"), at("FX-CUT"));
    let g0: Vec<&&str> = lines.iter().filter(|l| l.starts_with("G0 ")).collect();
    assert_eq!(g0.len(), 1, "one positioning move, to the run-in point: {text}");
    let xy = |l: &str| -> (f64, f64) {
        let w = |c: char| l.split_whitespace().find_map(|t| t.strip_prefix(c)?.parse::<f64>().ok()).unwrap_or(f64::NAN);
        (w('X'), w('Y'))
    };
    let thread = xy(g0[0]);
    assert!((thread.0 - 60.165).abs() < 1e-9 && (thread.1 < -1.0 || thread.1 > 51.0), "threaded in the air: {text}");
    let cuts = &lines[comp + 1..cut];
    assert!(!cuts.is_empty() && cuts.iter().all(|l| l.starts_with("G1 X")), "{text}");
    let end = cuts.last().map(|l| xy(l)).expect("a cut");
    assert!((end.1 - thread.1).abs() > 50.0, "the run ends across the bar at its run-out, not back at its start: {text}");
    assert!(cuts.iter().all(|l| (xy(l).0 - 60.165).abs() < 1e-9), "every cut stands the kerf off the part: {text}");
    // Without AWT the stops say where the operator threads and cuts.
    let mut v: serde_json::Value = serde_json::from_str(FIXTURE).expect("json");
    v["edm"]["awt"] = serde_json::json!(false);
    let m = parse_mch(&v.to_string()).expect("parses");
    let manual = post::engine::run(cps(WIRE), &post::machine::mirror(&m), &stream_text(&world)).expect("runs").program;
    assert!(manual.contains("(THREAD THE WIRE AT THE RUN-IN POINT OUTSIDE THE STOCK)") && manual.contains("(CUT THE WIRE AT THE RUN-OUT POINT)"), "{manual}");

    // The kerf on the part's side: the generator cannot see the part,
    // the wire laws can - refused at the post, nothing written.
    let world = cutoff_doc(true);
    let out = std::env::temp_dir().join(format!("camcraft-wire-open-gouge-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let err = posting::post_script(&world, &out, cps(WIRE)).expect_err("an open run that burns the part never posts");
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    assert!(err.starts_with("post blocked") && err.contains("Cutoff1: wire gouge: the kerf of move"), "{err}");
    assert!(!wrote, "no NC text is written for a refused open run");
    // The closed punch's golden stands byte for byte beside it.
    let punch = posting::post_script(&punch_doc(FIXTURE), &out, cps(WIRE)).expect("the punch posts").program;
    let _ = std::fs::remove_file(&out);
    assert_eq!(punch, std::fs::read_to_string(GOLDEN).expect("golden present"));
}

/// Review of #1139: nothing at post refuses a wire setup before its
/// post (the retired EDM door's wire arm passed it), so a wire machine missing a
/// code the post needs is refused by that code's name through the app's
/// own `post_script` - and no file is written.
#[test]
fn a_wire_setup_missing_a_code_is_refused_by_name_through_post_script() {
    for code in ["condition", "comp_off", "thread", "cut", "tip_disposal"] {
        let mut v: serde_json::Value = serde_json::from_str(FIXTURE).expect("json");
        v["edm"]["codes"].as_object_mut().expect("codes").remove(code);
        let world = punch_doc(&v.to_string());
        let out = std::env::temp_dir().join(format!("camcraft-wire-iso-missing-{code}-{}.nc", std::process::id()));
        let _ = std::fs::remove_file(&out);
        let err = posting::post_script(&world, &out, cps(WIRE)).expect_err("a wire machine missing a code never posts");
        let wrote = out.exists();
        let _ = std::fs::remove_file(&out);
        assert!(err.contains(&format!("edm.codes.{code}")), "{code}: {err}");
        assert!(!err.contains("#533"), "{code}: the EDM door no longer refuses a wire setup: {err}");
        assert!(!wrote, "{code}: the refused wire post wrote {}", out.display());
    }
}

/// Review of #1139: a Sinker Burn is refused by name through
/// `post_script` by the wire post and by every cutting post alike, and
/// no file is written. Since #542 the refusal is the serves door's
/// (#1232); only the sinker post serves the sinker (its own refusals -
/// this fixture states no `edm.codes` - are `sinker_burn.rs`'s).
#[test]
fn a_sinker_setup_is_still_refused_at_post_by_name() {
    let (world, mut h) = sinker_fixture::sinker_doc(6.0);
    let burn = sinker_fixture::burn(camcraft_app::operation::SinkerDatum::WorkTop, 3.0, 14.0);
    let op = CreateOperation::new(&world, "Burn0", sinker_fixture::ELECTRODE, SrcRef::Stock, burn).expect("a burn on its sinker");
    h.execute(&world, op);
    h.execute(&world, GenerateOperation::new(&world, "Burn0").expect("generates"));
    let out = std::env::temp_dir().join(format!("camcraft-wire-iso-sinker-{}.nc", std::process::id()));
    for (n, script) in BUILTINS.iter().filter(|(n, _)| !posting::SINKER_POSTS.contains(n)) {
        let _ = std::fs::remove_file(&out);
        let err = posting::post_script(&world, &out, script).expect_err("a sinker burn never posts");
        let wrote = out.exists();
        let _ = std::fs::remove_file(&out);
        // A turning dialect meets the lathe door (#1190) first; every
        // other post the serves door.
        let door = if posting::LATHE_POSTS.contains(n) { "on the work spindle" } else { "does not serve" };
        assert!(err.contains(door) && err.contains("Burn0"), "{n}: {err}");
        assert!(posting::LATHE_POSTS.contains(n) || err.contains("a sinker EDM machine"), "{n}: {err}");
        assert!(!wrote, "{n}: the refused sinker post wrote {}", out.display());
    }
}

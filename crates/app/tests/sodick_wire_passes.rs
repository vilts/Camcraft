//! The Sodick wire post's pass structure (#2649): a rough cut and its skim
//! passes on LN1W / LN2W condition words, through the app's own gate.
//! Provenance: Sodick Inc., "Wire EDM Machine Operation Training, LN1W /
//! LN2W Control" (part number 6300015, January 2012) - C001 for the first
//! cut, C002 for the second, C903 for the third; each next pass's C word
//! before its first move, the wire left threaded; C000 / H000 named
//! "Approach". The output was not run on a Sodick control
//! (docs/decisions/2649-sodick-ln-pass-structure.md).
//!
//! Regenerate the golden after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test sodick_wire_passes`
//!
//! Gate map (#2649): covers app::posting (post_script, stream_records) and
//! the sodick-wire.cps script's skim and approach-condition words; #2686:
//! app::machine (the edm skim_flushing key) and the skim's flushing word.

use flecs_ecs::prelude::World;
use serde_json::json;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::operation::{self, Height, OpParams, SrcRef, WireContourOp, WireSide, WireSkim};
use camcraft_app::post::{self, cnc};
use camcraft_app::posting;

const FIXTURE: &str = include_str!("edm/wire-iso-fixture.mch");
const SODICK: &str = "Sodick Wire EDM (Mark 21)";
const GOLDEN: &str = "tests/golden/nc/sodick-wire/wire_slug_punch_passes.nc";
const FLUSHED_GOLDEN: &str = "tests/golden/nc/sodick-wire/wire_slug_punch_passes_flushed.nc";

fn cps() -> &'static str {
    posting::builtin_post_text(SODICK).expect("registered")
}

/// The fixture with its rough and skim conditions named the LN way.
fn ln_machine() -> String {
    let m = FIXTURE
        .replace("\"id\": \"C110\"", "\"id\": \"C001\"")
        .replace("\"id\": \"S21\"", "\"id\": \"C002\"")
        .replace("\"id\": \"S31\"", "\"id\": \"C903\"");
    assert!(m.contains("\"C001\"") && m.contains("\"C002\"") && m.contains("\"C903\""), "the fixture renames");
    m
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

/// A punch around the slug on `condition` from a start hole at (-30, 0),
/// with `skims` after the rough cut, generated.
fn punch(machine_text: &str, condition: &str, skims: Vec<WireSkim>) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let m = parse_mch(machine_text).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "wire.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, AddTool::new(&world, "EDM-W025", camcraft_app::wire_tools::wire_tool(0.25)).expect("wire"));
    h.execute(&world, ImportModel::from_mesh("slug", slug()));
    let op = WireContourOp {
        lead_arc_radius: None,
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: condition.into(),
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

fn three_pass() -> World {
    let skims = vec![WireSkim { condition: "C002".into(), reverse: false }, WireSkim { condition: "C903".into(), reverse: true }];
    punch(&ln_machine(), "C001", skims)
}

fn run(machine_text: &str, recs: &[cnc::Record], options: &serde_json::Value) -> post::engine::PostOutput {
    let m = parse_mch(machine_text).expect("parses");
    post::engine::run_with(cps(), &post::machine::mirror(&m), &cnc::serialize(recs), options).expect("the engine runs")
}

/// Post `world` through the app's gate to a temp file; the program or the
/// refusal, and whether a file was left behind.
fn post_app(world: &World, tag: &str) -> (Result<String, String>, bool) {
    let out = std::env::temp_dir().join(format!("camcraft-sodick-passes-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let r = posting::post_script(world, &out, cps()).map(|p| p.program);
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    (r, wrote)
}

/// Done means: a rough cut and two skims post as one runnable program -
/// one threading, each pass's C word before that pass's first move, one
/// wire cut, every pass link a G01 (never a rapid with the wire threaded).
#[test]
fn a_rough_cut_and_two_skims_post_each_pass_on_its_own_ln_condition() {
    let (r, wrote) = post_app(&three_pass(), "golden");
    let text = r.expect("the skimmed contour posts");
    assert!(wrote, "the program is written");
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(GOLDEN, &text).expect("write golden");
    }
    let want = std::fs::read_to_string(GOLDEN).expect("golden present");
    assert_eq!(text, want, "the Sodick pass program drifted from its golden");

    let lines: Vec<&str> = text.lines().collect();
    let at = |w: &str| lines.iter().position(|l| *l == w).unwrap_or_else(|| panic!("no {w} in\n{text}"));
    let count = |w: &str| lines.iter().filter(|l| **l == w).count();
    assert_eq!((count("T91"), count("T90"), count("M02")), (1, 1, 1), "{text}");
    assert_eq!((count("C001"), count("C002"), count("C903")), (1, 1, 1), "{text}");
    assert_eq!(count("C000"), 0, "no approach condition unless asked:\n{text}");
    let (th, c1, c2, c3, cut) = (at("T91"), at("C001"), at("C002"), at("C903"), at("T90"));
    assert!(th < c1 && c1 < c2 && c2 < c3 && c3 < cut, "{text}");
    // Each pass starts with a straight move from the threading point; the
    // one before each skim's C word is the return to it.
    // #3214: the skim's gap note sits between its SKIM note and its C word.
    for c in [c2, c3] {
        assert_eq!(lines[c - 3], "G01 X-30. Y0.", "a skim starts at the threading point:\n{text}");
        assert!(lines[c - 2].starts_with("(SKIM "), "{text}");
        assert!(lines[c - 1].contains(" KERF GAP "), "{text}");
        assert!(lines[c + 1].starts_with("G01 X-20."), "the skim's lead-in follows its C word:\n{text}");
    }
    // The skims cut nearer the part: the rough lead-in, then each skim's.
    let lead = |c: usize| lines[c + 1];
    assert_eq!((lead(c1), lead(c2), lead(c3)), ("G01 X-20.165 Y0.", "G01 X-20.145 Y0.", "G01 X-20.135 Y0."), "{text}");
    // The wire stays threaded between passes: no rapid after T91.
    assert!(!lines[th..cut].iter().any(|l| l.contains("G00")), "{text}");
    assert!(lines.contains(&"(SKIM 1 - CONDITION C002 MUST EXIST IN THE CONTROL CONDITION TABLE)"), "{text}");
    assert!(lines.contains(&"(SKIM 2 - CONDITION C903 MUST EXIST IN THE CONTROL CONDITION TABLE)"), "{text}");
}

/// With the approachCondition option the approach move (threading point
/// to the contour) is cut on C000, then the operation's C word takes over
/// for the contour. Skims start on their own C word, without C000.
#[test]
fn the_approach_condition_option_cuts_the_first_approach_on_c000() {
    let machine = ln_machine();
    let recs = posting::stream_records(&three_pass()).expect("the wire section streams");
    let out = run(&machine, &recs, &json!({ "approachCondition": true }));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    let at = |w: &str| lines.iter().position(|l| *l == w).unwrap_or_else(|| panic!("no {w} in\n{}", out.program));
    let (th, c0, c1, c2) = (at("T91"), at("C000"), at("C001"), at("C002"));
    assert_eq!(lines.iter().filter(|l| **l == "C000").count(), 1, "{}", out.program);
    assert!(th < c0 && c0 < c1 && c1 < c2, "{}", out.program);
    assert_eq!(lines[c0 + 1], "G01 X-20.165 Y0.", "the approach is cut on C000:\n{}", out.program);
    assert_eq!(c1, c0 + 2, "C001 follows the approach move:\n{}", out.program);
    assert!(lines.iter().any(|l| l.starts_with('(') && l.contains("C000 APPROACH CONDITION")), "{}", out.program);

    // Off (the default): the program has no C000 and is the one without the option.
    let off = run(&machine, &recs, &serde_json::Value::Null);
    assert!(off.warnings.is_empty() && !off.program.lines().any(|l| l == "C000"), "{}", off.program);
    let explicit = run(&machine, &recs, &json!({ "approachCondition": false }));
    assert_eq!(explicit.program, off.program);
}

/// The LN machine with `edm` keys stated after its `awt`.
fn ln_machine_stating(keys: &str) -> String {
    let m = ln_machine().replace("\"awt\": true,", &format!("\"awt\": true, {keys},"));
    assert!(m.contains(keys), "the fixture states {keys}");
    m
}

/// #2686 done means: a skim states its flushing (the machine file's
/// `edm.skim_flushing`), and the post writes that T word before each
/// skim's C word. Against the #2649 golden only those words, the rough
/// cut's T84 and the header's flushing lines differ.
#[test]
fn a_stated_skim_flushing_posts_its_t_word_before_each_skims_c_word() {
    let machine = ln_machine_stating("\"flushing\": \"high\", \"skim_flushing\": \"low\"");
    let skims = vec![WireSkim { condition: "C002".into(), reverse: false }, WireSkim { condition: "C903".into(), reverse: true }];
    let (r, wrote) = post_app(&punch(&machine, "C001", skims), "skimflush");
    let text = r.expect("the flushed skims post");
    assert!(wrote, "the program is written");
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(FLUSHED_GOLDEN, &text).expect("write golden");
    }
    let want = std::fs::read_to_string(FLUSHED_GOLDEN).expect("golden present");
    assert_eq!(text, want, "the Sodick flushed pass program drifted from its golden");

    let plain = std::fs::read_to_string(GOLDEN).expect("the #2649 golden");
    let mut expect: Vec<&str> = Vec::new();
    for l in plain.lines() {
        match l {
            "(TANK FILL AND FLUSHING ARE NOT PROGRAMMED - SET THEM AT THE CONTROL)" => expect.extend([
                "(TANK IS NOT PROGRAMMED - SET IT AT THE CONTROL)",
                "(T84 HIGH-PRESSURE FLUSHING AFTER THREADING)",
                "(T85 LOW-PRESSURE FLUSHING BEFORE EACH SKIM PASS)",
            ]),
            "T91" => expect.extend(["T91", "T84"]),
            "C002" | "C903" => expect.extend(["T85", l]),
            other => expect.push(other),
        }
    }
    assert_eq!(text.lines().collect::<Vec<_>>(), expect, "{text}");
}

/// #2686: the rough cut's flushing is programmed and the skim's is not
/// stated - the value is not known, the post is refused by name and the
/// app writes no file. A skim flushing without the rough cut's is no
/// machine file.
#[test]
fn a_skim_with_only_the_rough_flushing_stated_is_refused_by_name() {
    let machine = ln_machine_stating("\"flushing\": \"high\"");
    let world = punch(&machine, "C001", vec![WireSkim { condition: "C002".into(), reverse: false }]);
    let (r, wrote) = post_app(&world, "noskimflush");
    let err = r.expect_err("a skim on an unknown flushing never posts");
    assert!(err.contains("Punch1") && err.contains("edm.skim_flushing") && err.contains("edm.flushing"), "{err}");
    assert!(!wrote, "the refused post wrote a file");
    // Without a skim the same machine posts (#1499).
    let (r, wrote) = post_app(&punch(&machine, "C001", Vec::new()), "roughonly");
    assert!(r.is_ok() && wrote, "{r:?}");

    let err = parse_mch(&ln_machine_stating("\"skim_flushing\": \"low\"")).err().unwrap_or_default();
    assert!(err.contains("edm.skim_flushing needs edm.flushing"), "{err}");
}

/// A skim whose condition id is not a Sodick C word is refused by name;
/// the app writes no file, and no word of that skim is written.
#[test]
fn a_skim_condition_that_is_not_a_sodick_c_word_is_refused_by_name() {
    let machine = ln_machine().replace("\"id\": \"C903\"", "\"id\": \"S31\"");
    let skims = vec![WireSkim { condition: "C002".into(), reverse: false }, WireSkim { condition: "S31".into(), reverse: false }];
    let world = punch(&machine, "C001", skims);
    let (r, wrote) = post_app(&world, "badskim");
    let err = r.expect_err("a skim on an unreadable condition word never posts");
    assert!(err.contains("Punch1") && err.contains("S31") && err.contains("is not a Sodick condition word"), "{err}");
    assert!(!wrote, "the refused post wrote a file");

    let out = run(&machine, &posting::stream_records(&world).expect("streams"), &serde_json::Value::Null);
    assert!(out.warnings.iter().any(|m| m.contains("skim condition id \"S31\"")), "{:?}", out.warnings);
    assert!(!out.program.lines().any(|l| l == "S31"), "{}", out.program);
}

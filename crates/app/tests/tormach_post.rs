//! The Tormach PathPilot post (#131): the program shape PathPilot reads
//! (no % fence, no O program number, no N numbers, M30), the machine's
//! own unit (an inch `.mch` posts G20 with every length and feed / 25.4,
//! a metric one G21 and mm), dwell in seconds, and the machine's stated
//! units reaching the script as `machine.units`.
//! Provenance: our own tests over our own post and machine file.
//!
//! Gate map (M4.4): covers post::post (tormach.cps), post::machine (units),
//! app::posting (registry).

use camcraft_app::machine::parse_mch;
use camcraft_app::post::{engine, machine};
use std::path::{Path, PathBuf};

const TORMACH_MCH: &str = "assets/machines/tormach-770mx.mch";
const METRIC_MCH: &str = "assets/machines/3axis-vmc.mch";

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn json(rel: &str) -> serde_json::Value {
    serde_json::from_str(&read(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// A machine as the app hands it to a post: the Tormach file goes through
/// the parser and the mirror (`posting::machine_value`'s path), the metric
/// reference file is already in the mirror's layout.
fn machine_for(mch: &str) -> serde_json::Value {
    if mch == TORMACH_MCH {
        machine::mirror(&parse_mch(&read(mch)).unwrap_or_else(|e| panic!("{mch}: {e}")))
    } else {
        json(mch)
    }
}

fn post(mch: &str, golden: &str) -> String {
    let cps = read("assets/posts/tormach.cps");
    let stream = read(&format!("crates/app/tests/golden/{golden}.cl"));
    let out =
        engine::run(&cps, &machine_for(mch), &stream).unwrap_or_else(|e| panic!("{golden}: {e}"));
    assert!(
        out.warnings.is_empty(),
        "{golden} on {mch}: {:?}",
        out.warnings
    );
    out.program
}

/// The letter and number of a word like `X-1.25`, if it is one.
fn word(t: &str) -> Option<(char, f64)> {
    let mut c = t.chars();
    let l = c.next()?;
    let n: f64 = c.as_str().parse().ok()?;
    Some((l, n))
}

#[test]
fn the_machine_file_states_inches_and_the_script_sees_them() {
    let m = parse_mch(&read(TORMACH_MCH)).expect("the Tormach machine parses");
    assert_eq!(m.units.as_deref(), Some("inch"));
    assert_eq!(
        machine::mirror(&m)["units"],
        "inch",
        "the mirror carries the unit"
    );
    assert_eq!(machine::script_props(&machine::mirror(&m))["units"], "inch");
    assert_eq!(
        machine::script_props(&json(TORMACH_MCH))["units"],
        "inch",
        "the raw door too"
    );
    // A metric file, and a file that states nothing, are mm.
    let metric = parse_mch(&read(METRIC_MCH)).expect("metric machine");
    assert_eq!(metric.units, None);
    assert_eq!(
        machine::script_props(&machine::mirror(&metric))["units"],
        "mm"
    );
    assert_eq!(machine::script_props(&serde_json::json!({}))["units"], "mm");
}

#[test]
fn a_pathpilot_program_has_no_fence_and_no_o_or_n_words() {
    for golden in ["face_square_40x40", "contour_disc_hole", "drill_peck"] {
        let nc = post(TORMACH_MCH, golden);
        let lines: Vec<&str> = nc.lines().collect();
        assert!(
            lines.first().is_some_and(|l| l.starts_with('(')),
            "{golden}: opens with the name comment"
        );
        assert_eq!(lines.last(), Some(&"M30"), "{golden}: M30 ends it");
        for l in &lines {
            // Tormach's guidelines: no % or $ fence. LinuxCNC reads an O
            // word as a subroutine label, and the post numbers no blocks.
            assert!(!l.contains('%') && !l.contains('$'), "{golden}: {l}");
            assert!(!l.starts_with('O') && !l.starts_with('N'), "{golden}: {l}");
        }
        assert!(
            nc.contains("G20 G17 G90 G91.1 G94"),
            "{golden}: inch header\n{nc}"
        );
        assert!(
            lines.contains(&"G64 P0.001"),
            "{golden}: blending tolerance on its own line"
        );
        // Z straight up, then G30, right before every tool change (the
        // tool-change streams are pinned in the belt-range test below).
        for (i, l) in lines.iter().enumerate() {
            if l.ends_with(" M6") {
                assert_eq!(
                    lines.get(i.saturating_sub(2)..i),
                    Some(&["G53 G0 Z0", "G30"][..]),
                    "{golden}: Z up and G30 before {l}"
                );
            }
        }
        assert!(
            !nc.contains("G21"),
            "{golden}: an inch program never states mm"
        );
        // The end: Z straight up in machine coordinates, then the
        // operator's preset G30 location, then M30 - PathPilot's own
        // ending order.
        let n = lines.len();
        assert_eq!(
            lines.get(n.saturating_sub(3)..),
            Some(&["G53 G0 Z0", "G30", "M30"][..]),
            "{golden}: the program ends Z up, G30, M30"
        );
    }
}

/// The same stream posted for an inch and a metric machine: the same
/// blocks, every length / 25.4 at four decimals, every feed / 25.4.
#[test]
fn an_inch_machine_posts_every_length_and_feed_in_inches() {
    for golden in [
        "face_square_40x40",
        "contour_disc_hole",
        "drill_peck",
        "pocket_zigzag_plate_with_island",
    ] {
        let inch = post(TORMACH_MCH, golden);
        let mm = post(METRIC_MCH, golden);
        assert!(
            mm.contains("G21 G17 G90 G91.1 G94"),
            "{golden}: metric header"
        );
        let (a, b): (Vec<&str>, Vec<&str>) = (inch.lines().collect(), mm.lines().collect());
        assert_eq!(
            a.len(),
            b.len(),
            "{golden}: the same blocks\n{inch}\n----\n{mm}"
        );
        let mut compared = 0usize;
        for (la, lb) in a.iter().zip(&b) {
            if la.starts_with('(') {
                assert!(lb.starts_with('('), "{golden}: {la} / {lb}");
                continue; // the machine-name comment
            }
            let (ta, tb): (Vec<&str>, Vec<&str>) =
                (la.split(' ').collect(), lb.split(' ').collect());
            assert_eq!(ta.len(), tb.len(), "{golden}: {la} / {lb}");
            for (wa, wb) in ta.iter().zip(&tb) {
                match (word(wa), word(wb)) {
                    (Some((l, x)), Some((lm, y))) if l == lm && "XYZIJKRQ".contains(l) => {
                        assert!(
                            (x * 25.4 - y).abs() <= 0.002,
                            "{golden}: {wa} is not {wb} in inches"
                        );
                        if let Some((_, dec)) = wa.split_once('.') {
                            assert!(dec.len() <= 4, "{golden}: {wa} has more than four decimals");
                        }
                        compared += 1;
                    }
                    (Some(('F', x)), Some(('F', y))) => {
                        assert!(
                            (x * 25.4 - y).abs() <= 0.5 + 0.13,
                            "{golden}: {wa} is not {wb} per minute"
                        );
                        compared += 1;
                    }
                    (Some(('G', _)), _) if *wa == "G20" => assert_eq!(*wb, "G21"),
                    // The G64 tolerance: 0.001 in, and its round metric twin.
                    (Some(('P', _)), _) if *wa == "P0.001" => assert_eq!(*wb, "P0.025"),
                    (Some(('M', _)), _) => {} // coolant codes are the machine's own
                    _ => assert_eq!(wa, wb, "{golden}: {la} / {lb}"),
                }
            }
        }
        assert!(compared > 0, "{golden}: some length was compared");
    }
}

/// LinuxCNC's G82/G86/G89 dwell P is in seconds, not milliseconds.
#[test]
fn a_dwell_posts_in_seconds() {
    let cps = read("assets/posts/tormach.cps");
    let stream = read("crates/app/tests/golden/drill_peck.cl")
        .replace("; CYCLE Peck", "; CYCLE Spot")
        .replace("DWELL0", "DWELL500");
    let out = engine::run(&cps, &machine_for(TORMACH_MCH), &stream).expect("posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let g82 = out
        .program
        .lines()
        .find(|l| l.starts_with("G82"))
        .unwrap_or_else(|| panic!("{}", out.program));
    assert!(
        g82.split(' ').any(|w| w == "P0.5"),
        "half a second, not 500: {g82}"
    );
}

#[test]
fn the_registry_ships_the_tormach_post_for_mills() {
    use camcraft_app::posting;
    assert!(
        posting::BUILTINS
            .iter()
            .any(|(n, _)| *n == "Tormach PathPilot")
    );
    assert!(posting::post_serves(
        "Tormach PathPilot",
        camcraft_app::machine::Kind::Mill
    ));
    assert!(!posting::post_serves(
        "Tormach PathPilot",
        camcraft_app::machine::Kind::Lathe
    ));
}

/// The 770MX belt (its .mch ranges): a speed only one range serves names that range
/// in a comment before the spindle starts; a tool number outside
/// PathPilot's table is refused by name.
#[test]
fn the_belt_range_is_named_and_a_stray_tool_number_is_refused() {
    let cps = read("assets/posts/tormach.cps");
    let mch = machine_for(TORMACH_MCH);
    let run = |rpm: u32, tool: u32| {
        let stream = format!(
            "PARAM SpindleSpeed {rpm}\nTOOL {tool} T{tool}\nSECTION\nRapid X 0 Y 0 Z 5000\nENDSECTION\n"
        );
        engine::run(&cps, &mch, &stream).expect("runs")
    };
    let high = run(8000, 1);
    assert!(
        high.program
            .contains("(SPINDLE 8000 RPM NEEDS THE HIGH BELT RANGE)"),
        "{}",
        high.program
    );
    let low = run(300, 1);
    assert!(
        low.program
            .contains("(SPINDLE 300 RPM NEEDS THE LOW BELT RANGE)"),
        "{}",
        low.program
    );
    let both = run(2000, 1);
    assert!(!both.program.contains("BELT RANGE"), "{}", both.program);
    assert!(both.warnings.is_empty(), "{:?}", both.warnings);
    let stray = run(2000, 1001);
    assert!(
        stray.warnings.iter().any(|w| w.contains("1-1000")),
        "{:?}",
        stray.warnings
    );
}

/// Tormach documents G84 for PathPilot: the tap posts as G84 with the
/// stream's pitch-times-rpm feed (in inches per minute on an inch
/// machine) and no P - PathPilot sizes the bottom dwell itself.
#[test]
fn a_tap_posts_g84_with_its_feed_and_no_dwell_word() {
    let cps = read("assets/posts/tormach.cps");
    // A tap marker carrying its synthesized feed: 1270 mm/min = 50 in/min.
    let stream =
        read("crates/app/tests/golden/tap_plate.cl").replace("DWELL0 AT1", "DWELL500 F1270000 AT1");
    let out = engine::run(&cps, &machine_for(TORMACH_MCH), &stream).expect("posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let g84 = out
        .program
        .lines()
        .find(|l| l.starts_with("G84"))
        .unwrap_or_else(|| panic!("{}", out.program));
    assert!(
        !g84.split(' ').any(|w| w.starts_with('P')),
        "no dwell word, even with a dwell asked: {g84}"
    );
    assert!(
        g84.split(' ').any(|w| w == "F50"),
        "the tap feed in inches per minute: {g84}"
    );
}

/// Between tools, Z goes straight up in machine coordinates before the
/// G30 that may move X, Y and Z together - PathPilot's own Z-first order -
/// and the same holds for the first tool of the program.
#[test]
fn every_tool_change_lifts_z_before_g30() {
    let cps = read("assets/posts/tormach.cps");
    let stream = "PARAM SpindleSpeed 2000\nTOOL 1 T1\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 1000 Y 0 Z -1000 F 500000\nENDSECTION\n\
                  PARAM SpindleSpeed 2000\nTOOL 2 T2\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 1000 Y 0 Z -1000 F 500000\nENDSECTION\n";
    let out = engine::run(&cps, &machine_for(TORMACH_MCH), stream).expect("posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    let changes: Vec<usize> = (0..lines.len())
        .filter(|i| lines[*i].ends_with(" M6"))
        .collect();
    assert_eq!(changes.len(), 2, "two tool changes\n{}", out.program);
    for i in changes {
        assert_eq!(
            lines.get(i.saturating_sub(2)..i),
            Some(&["G53 G0 Z0", "G30"][..]),
            "Z up, then G30, before {}\n{}",
            lines[i],
            out.program
        );
    }
}

/// X and Y are unknown at the program's start and after every G30, so the
/// first move after a tool change is preceded by a G0 to its X Y alone,
/// at the Z-up height: PathPilot otherwise warns "Axis XY not specified
/// by this or previous motions" and the move starts wherever the table
/// happens to be.
#[test]
fn the_first_move_after_a_tool_change_positions_xy_with_z_up() {
    let cps = read("assets/posts/tormach.cps");
    let stream = "PARAM SpindleSpeed 2000\nTOOL 1 T1\nSECTION\nRapid X 25400 Y 12700 Z 5080\nFeed X 25400 Y 12700 Z -2540 F 254000\nENDSECTION\n\
                  PARAM SpindleSpeed 2000\nTOOL 2 T2\nSECTION\nFeed X 50800 Y 0 Z -2540 F 254000\nENDSECTION\n";
    let out = engine::run(&cps, &machine_for(TORMACH_MCH), stream).expect("posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    let is_motion = |l: &&str| {
        ["G0 ", "G1 ", "G2 ", "G3 ", "G8"]
            .iter()
            .any(|g| l.starts_with(g))
            && !l.starts_with("G80")
    };
    let mut checked = 0;
    for (i, l) in lines.iter().enumerate() {
        if !l.ends_with(" M6") {
            continue;
        }
        let first = lines[i..]
            .iter()
            .find(|l| is_motion(l))
            .unwrap_or_else(|| panic!("{}", out.program));
        assert!(
            first.starts_with("G0 X") && first.contains(" Y") && !first.contains('Z'),
            "XY alone first, not {first}\n{}",
            out.program
        );
        checked += 1;
    }
    assert_eq!(checked, 2, "{}", out.program);
    // The rapid's own target, then the rapid itself: Z comes straight down.
    assert!(
        out.program.contains("G0 X1 Y0.5\nG0 X1 Y0.5 Z0.2\n"),
        "{}",
        out.program
    );
    // A feed positions at the point the stream says it starts from (the
    // second tool's stream carries on from X1 Y0.5), then runs as planned.
    assert!(
        out.program.contains("G0 X1 Y0.5\nG1 X2 Y0 Z-0.1 F10\n"),
        "{}",
        out.program
    );
}

/// The machine's travel bands are in PROGRAM coordinates (relative to the
/// work offset, `assets/machines/AGENTS.md`), so the 770MX states its full
/// stroke each way: a band of -13.25..0 in Z blocked every clearance move
/// above G54 Z0 - the first real job through the GUI (a Face) refused to
/// post with "40 limit problem(s) ... the stroke is -336.550..0.000".
#[test]
fn a_face_job_on_the_built_in_770mx_passes_the_gate_and_posts_inch() {
    use camcraft_app::commands::{
        AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost,
        SetStock,
    };
    use camcraft_app::geometry::{Point3, cube};
    use camcraft_app::operation::{Height, OpParams, SrcRef};
    use camcraft_app::schema::{Location, PostRef};
    use camcraft_app::stock::StockShape;
    use camcraft_app::tools::catalog;

    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(
        &world,
        LoadMachine::builtin(&world, "Tormach 770MX").expect("built-in machine"),
    );
    h.execute(
        &world,
        SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())).expect("post"),
    );
    let stock = SetStock::new(
        &world,
        StockShape::Box {
            dx: 60.0,
            dy: 60.0,
            dz: 20.0,
        },
        Location::default(),
    );
    h.execute(&world, stock.expect("stock"));
    h.execute(
        &world,
        ImportModel::from_mesh(
            "part",
            cube(Point3::new(0.0, 0.0, 0.0), Point3::new(40.0, 40.0, 19.0)),
        ),
    );
    // The catalog's 12000 rpm is over the 770MX's 10000 - a separate
    // gate refusal; this test is about travel, so run within the spindle.
    let mut tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .map(|(_, t)| t)
        .expect("tool");
    tool.rpm = 8000.0;
    h.execute(
        &world,
        AddTool::new(&world, "EM-0400-2FL", tool).expect("add tool"),
    );
    // G54 is unplaced (the world origin, the stock's floor): clearance at
    // Z 25 mm and the cut at Z 19.5 both stand ABOVE G54 Z0.
    let face = OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    h.execute(
        &world,
        CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face).expect("face"),
    );
    h.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("generate"),
    );
    let path =
        std::env::temp_dir().join(format!("camcraft_tormach_face_{}.nc", std::process::id()));
    let posted = camcraft_app::posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    let posted =
        posted.unwrap_or_else(|e| panic!("the 770MX gate must pass a plain face job: {e}"));
    assert!(
        posted.program.contains("G20 G17 G90 G91.1 G94"),
        "{}",
        posted.program
    );
    // #131, found on the 770MX: the face's first move is a plunge FEED from
    // its clearance start, and after the tool change Z stands at the top of
    // travel - the program crawled the whole stroke at the plunge rate. The
    // post rapids to the start height (clearance, Z 25 mm = 0.9843 in) once
    // X Y are known, and the feed runs only the stretch the generator planned.
    let lines: Vec<&str> = posted.program.lines().collect();
    let first_feed = lines
        .iter()
        .position(|l| l.starts_with("G1 "))
        .expect("a feed");
    // Since #690 the face opens with its own approach rapid, so the line
    // before the feed restates X Y beside the clearance Z - still a rapid
    // down to clearance, never a feed from the top of travel.
    let down = lines[first_feed - 1];
    assert!(
        down.starts_with("G0 ") && down.ends_with("Z0.9843"),
        "rapid to clearance before the first feed\n{}",
        posted.program
    );
    assert!(
        lines[first_feed - 2].starts_with("G0 X") && !lines[first_feed - 2].contains('Z'),
        "X Y first, Z up\n{}",
        posted.program
    );
}

//! #257: a posted program says what it is - the setup, each operation
//! (name, kind, work offset) and each tool (number, name, kind, size) -
//! in every dialect's own comment syntax, with the user's text cleaned
//! of anything that would end the comment early or read as a word. Only
//! comment lines are added: the conformance goldens (legacy streams,
//! no PARAMs) post byte-identical, which `conformance.rs` locks.
//! Provenance: our own fixtures; comment forms from RS-274 / ISO 6983
//! `( )`, DIN 66025 `;`, and the TNC ISO manual's numbered blocks.
//!
//! Gate map (#257): covers post::post (the seven assets/posts/*.cps
//! comment helpers, post::dump's commentary PARAMs), app::posting
//! (op_streams' op kind / setup / tool kind context).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, SetStock,
};
use camcraft_app::geometry::Point3;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{Dump, cnc, dump, engine};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn mill() -> serde_json::Value {
    serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json")
}

/// A short contour pass: enough motion for every preamble to open.
fn path() -> Toolpath {
    let p = |x, y, z| Point3 { x, y, z };
    Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(0.0, 0.0, 10.0), p(5.0, 5.0, 10.0), None),
        Move::new(
            MoveKind::Plunge,
            p(5.0, 5.0, 10.0),
            p(5.0, 5.0, -1.0),
            Some(200.0),
        ),
        Move::new(
            MoveKind::Feed,
            p(5.0, 5.0, -1.0),
            p(25.0, 5.0, -1.0),
            Some(800.0),
        ),
        Move::new(
            MoveKind::Rapid,
            p(25.0, 5.0, -1.0),
            p(25.0, 5.0, 10.0),
            None,
        ),
    ])
}

fn post(stem: &str, op_name: &str, tool_name: &str, setup: &str) -> String {
    let d = Dump {
        op_name,
        op_kind: "2D Contour",
        setup_name: setup,
        tool_number: 3,
        tool_name,
        tool_kind: "bull-nose",
        tool_diameter: 6.0,
        tool_corner_r: 0.5,
        rpm: 8000.0,
        clearance_z: 10.0,
        retract_z: 5.0,
        ..Default::default()
    };
    // Two sections of one setup: the second restates its operation and
    // tool, never the setup.
    let mut records = dump(&path(), &d);
    records.extend(dump(&path(), &d));
    let stream = cnc::serialize(&records);
    let out = engine::run(&read(&format!("assets/posts/{stem}.cps")), &mill(), &stream)
        .unwrap_or_else(|e| panic!("{stem}: must post: {e}"));
    assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
    out.program
}

/// Each mill dialect: its setup, operation and tool comment lines, in
/// order, and the motion word the tool comment must sit just above.
const MILL: &[(&str, [&str; 3], &str)] = &[
    (
        "fanuc",
        [
            "(SETUP: Setup1)",
            "(OPERATION: Contour1 - 2D CONTOUR - G54)",
            "(T03 EM-0600 - BULL-NOSE D6 R0.5)",
        ],
        "T03 M6",
    ),
    (
        "haas",
        [
            "(SETUP: Setup1)",
            "(OPERATION: Contour1 - 2D CONTOUR - G54)",
            "(T3 EM-0600 - BULL-NOSE D6 R0.5)",
        ],
        "T3 M6",
    ),
    (
        "mazak",
        [
            "(SETUP: Setup1)",
            "(OPERATION: Contour1 - 2D CONTOUR - G54)",
            "(T3 EM-0600 - BULL-NOSE D6 R0.5)",
        ],
        "T3 M6",
    ),
    (
        "tormach",
        [
            "(SETUP: Setup1)",
            "(OPERATION: Contour1 - 2D CONTOUR - G54)",
            "(T3 EM-0600 - BULL-NOSE D6 R0.5)",
        ],
        "T3 M6",
    ),
    (
        "sinumerik",
        [
            "; SETUP: Setup1",
            "; OPERATION: Contour1 - 2D CONTOUR - G54",
            "; T3 EM-0600 - BULL-NOSE D6 R0.5",
        ],
        "T3 D3 M6",
    ),
    (
        "heidenhain",
        [
            "; SETUP: Setup1*",
            "; OPERATION: Contour1 - 2D CONTOUR*",
            "; T3 EM-0600 - BULL-NOSE D6 R0.5*",
        ],
        "T3 G17 S8000*",
    ),
];

/// A line's text with the dialect's block number stripped (`N20 `).
fn body(line: &str) -> &str {
    match line.strip_prefix('N').and_then(|r| r.split_once(' ')) {
        Some((n, rest)) if n.chars().all(|c| c.is_ascii_digit()) => rest,
        _ => line,
    }
}

#[test]
fn every_mill_dialect_comments_the_setup_operation_and_tool_in_its_own_syntax() {
    for (stem, notes, tool_call) in MILL {
        let nc = post(stem, "Contour1", "EM-0600", "Setup1");
        let lines: Vec<&str> = nc.lines().map(body).collect();
        let at = |s: &str| {
            lines
                .iter()
                .position(|l| *l == s)
                .unwrap_or_else(|| panic!("{stem}: no {s:?} in\n{nc}"))
        };
        let (s, o, t) = (at(notes[0]), at(notes[1]), at(notes[2]));
        assert!(
            s < o && o < t,
            "{stem}: setup, then operation, then tool:\n{nc}"
        );
        // The note leads its tool-change sequence (Tormach's Z-up and G30
        // run first, then the call).
        assert!(
            lines.iter().skip(t + 1).take(3).any(|l| l == tool_call),
            "{stem}: the tool comment leads its tool call:\n{nc}"
        );
        let count = |s: &str| lines.iter().filter(|l| **l == s).count();
        assert_eq!(
            [count(notes[0]), count(notes[1]), count(notes[2])],
            [1, 2, 2],
            "{stem}: the setup once, the rest per section:\n{nc}"
        );
    }
}

#[test]
fn user_text_never_breaks_a_comment() {
    let op = "Pocket (rough); 50% \u{e9}t\u{e9} and a very long name that goes on and on";
    let tool = "EM(6)*;%";
    for (stem, _, _) in MILL {
        let nc = post(stem, op, tool, "Set(up);1");
        for l in nc
            .lines()
            .filter(|l| l.contains("SETUP:") || l.contains("OPERATION:") || l.contains("EM 6"))
        {
            assert!(l.is_ascii(), "{stem}: {l:?}");
            let b = body(l);
            match *stem {
                "sinumerik" => {
                    assert!(
                        b.starts_with("; ") && b.matches(';').count() == 1,
                        "{stem}: {l:?}"
                    );
                    assert!(
                        !b.contains('(') && !b.contains(')') && !b.contains('%'),
                        "{stem}: {l:?}"
                    );
                }
                "heidenhain" => {
                    assert!(b.starts_with("; ") && b.ends_with('*'), "{stem}: {l:?}");
                    assert!(
                        b.matches(';').count() == 1 && b.matches('*').count() == 1,
                        "{stem}: {l:?}"
                    );
                    assert!(
                        !b.contains('(') && !b.contains(')') && !b.contains('%'),
                        "{stem}: {l:?}"
                    );
                }
                _ => {
                    assert!(b.starts_with('(') && b.ends_with(')'), "{stem}: {l:?}");
                    assert!(
                        b.matches('(').count() == 1 && b.matches(')').count() == 1,
                        "{stem}: nested parens {l:?}"
                    );
                    assert!(!b.contains(';') && !b.contains('%'), "{stem}: {l:?}");
                }
            }
        }
        // Capped: the operation's name is cut at 40 characters.
        assert!(
            nc.contains("OPERATION: Pocket rough 50 ?t? and a very long name"),
            "{stem}:\n{nc}"
        );
        assert!(
            !nc.contains("goes on and on"),
            "{stem}: the cap holds:\n{nc}"
        );
    }
}

#[test]
fn a_stream_without_the_commentary_params_writes_no_comment() {
    // A legacy .cl carries no PARAM and no TOOL: its program is the
    // same bytes it always was (the conformance goldens lock the rest).
    let stream = read("crates/app/tests/golden/contour_disc_hole.cl");
    for (stem, _, _) in MILL {
        let out = engine::run(&read(&format!("assets/posts/{stem}.cps")), &mill(), &stream)
            .expect("posts");
        for word in ["SETUP:", "OPERATION:", "BULL-NOSE"] {
            assert!(
                !out.program.contains(word),
                "{stem}: {word} from nowhere:\n{}",
                out.program
            );
        }
    }
}

#[test]
fn a_document_posts_its_setup_operation_kind_and_tool_kind() {
    const TOOL: &str = "EM-0400-2FL";
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(
        &world,
        StockShape::Box {
            dx: 60.0,
            dy: 60.0,
            dz: 20.0,
        },
        Default::default(),
    );
    h.execute(&world, stock.expect("box stock"));
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == TOOL)
        .map(|(_, t)| t)
        .expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(
        &world,
        CreateSetup::new(&world, None, None).expect("materialize Setup1"),
    );
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
        CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face).expect("create"),
    );
    h.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("generate"),
    );
    let streams = camcraft_app::posting::op_streams(&world).expect("streams");
    let records: Vec<cnc::Record> = streams.into_iter().flat_map(|(_, r)| r).collect();
    let out = engine::run(
        &read("assets/posts/fanuc.cps"),
        &mill(),
        &cnc::serialize(&records),
    )
    .expect("posts");
    for want in [
        "(SETUP: Setup1)",
        "(OPERATION: Face1 - FACE - G54)",
        "(T01 EM-0400-2FL - FLAT END MILL D4)",
    ] {
        assert!(
            out.program.lines().any(|l| l == want),
            "no {want:?} in\n{}",
            out.program
        );
    }
}

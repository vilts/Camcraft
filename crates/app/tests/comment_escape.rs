//! #594: every text a post writes inside a comment - the machine's
//! name, the setup, the operation, the tool - is cleaned to the
//! dialect's comment-legal characters, so a name holding `(`, `)`, `;`,
//! `*` or a LinuxCNC active-comment keyword (`MSG,` and kin) never
//! ends, nests or arms a comment. Names without such characters post
//! byte-identical (the conformance and post goldens lock that).
//! Provenance: our own fixtures; comment forms from RS-274 / ISO 6983
//! `( )`, DIN 66025 / Sinumerik `;`, the TNC ISO manual's `; ..*`
//! blocks, and the LinuxCNC G-code manual's active comments.
//!
//! Gate map (#594): covers post::post (the seven assets/posts/*.cps
//! header and comment helpers, the PRELUDE's writeComment default).
//! Lane shape (M4.4): all tests here run in milliseconds.

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::geometry::Point3;
use camcraft_app::machine::{parse_mch, Coolant, Machine};
use camcraft_app::post::{cnc, dump, engine, machine, Dump};
use serde_json::json;
use std::path::Path;

/// A machine name opening with a LinuxCNC active keyword, holding both
/// parentheses, the DIN line comment and the TNC block end.
const MACHINE: &str = "MSG, Mill (A); B*";
const OP: &str = "Op (1); debug*";
const TOOL: &str = "EM(6);*";
const SETUP: &str = "Set(up);1";

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn mill(name: &str) -> serde_json::Value {
    let mut m: serde_json::Value =
        serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    m["name"] = json!(name);
    m
}

fn path() -> Toolpath {
    let p = |x, y, z| Point3 { x, y, z };
    Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(0.0, 0.0, 10.0), p(5.0, 5.0, 10.0), None),
        Move::new(MoveKind::Plunge, p(5.0, 5.0, 10.0), p(5.0, 5.0, -1.0), Some(200.0)),
        Move::new(MoveKind::Feed, p(5.0, 5.0, -1.0), p(25.0, 5.0, -1.0), Some(800.0)),
        Move::new(MoveKind::Rapid, p(25.0, 5.0, -1.0), p(25.0, 5.0, 10.0), None),
    ])
}

fn mill_stream(rpm: f64) -> String {
    let d = Dump {
        op_name: OP,
        op_kind: "2D Contour",
        setup_name: SETUP,
        tool_number: 3,
        tool_name: TOOL,
        tool_kind: "flat",
        tool_diameter: 6.0,
        rpm,
        clearance_z: 10.0,
        retract_z: 5.0,
        ..Default::default()
    };
    cnc::serialize(&dump(&path(), &d))
}

fn post(stem: &str, m: &serde_json::Value, stream: &str) -> String {
    let out = engine::run(&read(&format!("assets/posts/{stem}.cps")), m, stream)
        .unwrap_or_else(|e| panic!("{stem}: must post: {e}"));
    assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
    out.program
}

/// A `( )` dialect's line: at most one comment, opened before it closes,
/// running to the end of the line, and no `;` anywhere.
fn assert_paren_line(stem: &str, l: &str) {
    let (o, c) = (l.matches('(').count(), l.matches(')').count());
    assert!(o == c && o <= 1, "{stem}: unbalanced or nested comment {l:?}");
    if let (Some(a), Some(b)) = (l.find('('), l.find(')')) {
        assert!(a < b && b + 1 == l.len(), "{stem}: comment not closed at the end {l:?}");
    }
    assert!(!l.contains(';') && l.is_ascii(), "{stem}: stray delimiter {l:?}");
}

/// LinuxCNC's active comments: a comment opening with one of these
/// keywords is a command, not a note.
fn assert_inert(stem: &str, l: &str) {
    if let Some(i) = l.find('(') {
        let text = l[i + 1..].trim_start().to_ascii_uppercase();
        for k in ["MSG", "DEBUG", "PRINT", "LOG", "PROBEOPEN", "PROBECLOSE", "ABORT"] {
            let armed = text
                .strip_prefix(k)
                .is_some_and(|r| !r.starts_with(|ch: char| ch.is_ascii_alphanumeric()));
            assert!(!armed, "{stem}: active comment {l:?}");
        }
    }
}

#[test]
fn hostile_names_post_well_formed_comments_in_every_mill_dialect() {
    let stream = mill_stream(8000.0);
    let m = mill(MACHINE);
    for stem in ["fanuc", "haas", "mazak", "tormach"] {
        let nc = post(stem, &m, &stream);
        for l in nc.lines() {
            assert_paren_line(stem, l);
            if stem == "tormach" {
                assert_inert(stem, l);
            }
        }
        let header = match stem {
            "fanuc" | "mazak" => "O0001 (MSG, Mill A B*)",
            "haas" => "(MSG, Mill A B*)",
            _ => "(- MSG, Mill A B*)",
        };
        assert!(nc.lines().any(|l| l == header), "{stem}: no {header:?}\n{nc}");
        assert!(nc.contains("OPERATION: Op 1 debug*"), "{stem}:\n{nc}");
        assert!(nc.contains("SETUP: Set up 1"), "{stem}:\n{nc}");
        assert!(nc.contains(" EM 6 *"), "{stem}: the tool's name, cleaned\n{nc}");
    }

    let nc = post("sinumerik", &m, &stream);
    assert!(nc.lines().any(|l| l == "; MSG, Mill A B*"), "{nc}");
    for l in nc.lines().filter(|l| l.contains(';')) {
        // A line comment: one `;`, and nothing a reader takes for code.
        let b = l.split_once(' ').filter(|(n, _)| n.starts_with('N')).map_or(l, |(_, r)| r);
        assert!(b.starts_with("; ") && b.matches(';').count() == 1, "sinumerik: {l:?}");
        assert!(!b.contains('(') && !b.contains(')') && b.is_ascii(), "sinumerik: {l:?}");
    }

    let nc = post("heidenhain", &m, &stream);
    for l in nc.lines() {
        // Every TNC block ends at its one `*`; a comment opens at one `;`.
        assert!(l.ends_with('*') && l.matches('*').count() == 1, "heidenhain: {l:?}");
        assert!(l.matches(';').count() <= 1 && l.is_ascii(), "heidenhain: {l:?}");
        assert!(!l.contains('(') && !l.contains(')'), "heidenhain: {l:?}");
    }
    assert!(nc.starts_with("%MSG__Mill__A___B_ G71 *\n"), "{nc}");
}

#[test]
fn hostile_names_post_well_formed_comments_on_the_lathe() {
    let m = machine::mirror(&Machine {
        model: MACHINE.into(),
        kind: Some("lathe".into()),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    });
    let mv = |kind, x, z, feed| {
        cnc::Record::Move(cnc::MoveRec {
            kind, x, y: 0.0, z, feed, center: None, ccw: false, plane: Default::default(), abc: None,
        })
    };
    let records = vec![
        cnc::Record::Param { name: "SetupName".into(), value: json!(SETUP) },
        cnc::Record::Param { name: "OperationName".into(), value: json!(OP) },
        cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(1000.0) },
        cnc::Record::Param { name: "WorkOffset".into(), value: json!("G54") },
        cnc::Record::Tool { number: 1, name: TOOL.into() },
        cnc::Record::Section { begin: true },
        mv(MoveKind::Rapid, 20.0, 5.0, None),
        mv(MoveKind::Feed, 20.0, -10.0, Some(200.0)),
        cnc::Record::Section { begin: false },
    ];
    let nc = post("fanuc-lathe", &m, &cnc::serialize(&records));
    for l in nc.lines() {
        assert_paren_line("fanuc-lathe", l);
    }
    assert!(nc.lines().any(|l| l == "O0001 (MSG, Mill A B*)"), "{nc}");
    assert!(nc.contains("(OPERATION: Op 1 debug*"), "{nc}");
    assert!(nc.contains("(T0101 EM 6 *"), "{nc}");

    // The lathe voices a warning through the PRELUDE's writeComment: the
    // (refused) program still parses, its parentheses gone.
    let mut warned = records.clone();
    warned.insert(5, cnc::Record::Warn { text: "check (this); now".into() });
    let out = engine::run(&read("assets/posts/fanuc-lathe.cps"), &m, &cnc::serialize(&warned))
        .expect("runs");
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    for l in out.program.lines() {
        let (o, c) = (l.matches('(').count(), l.matches(')').count());
        assert!(o == c && o <= 1, "fanuc-lathe warning echo: {l:?}");
    }
    assert!(out.program.contains("(check  this ; now)"), "{}", out.program);
}

#[test]
fn every_bundled_machine_name_posts_a_single_header_comment() {
    // Shipped files name machines `Tormach 770MX (ML)`, `... (Series I)`.
    // 2000 rpm: inside every Tormach's spindle range.
    let stream = mill_stream(2000.0);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines");
    let mut seen = 0;
    for e in std::fs::read_dir(&dir).expect("machines dir").flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("mch") {
            continue;
        }
        let text = std::fs::read_to_string(&p).expect("reads");
        let Ok(mch) = parse_mch(&text) else { continue };
        if mch.kind.as_deref() != Some("mill") || !mch.model.starts_with("Tormach") {
            continue;
        }
        let nc = post("tormach", &machine::mirror(&mch), &stream);
        let first = nc.lines().next().unwrap_or_default();
        assert_paren_line(&mch.model, first);
        assert!(first.starts_with("(Tormach "), "{}: {first:?}", mch.model);
        seen += 1;
    }
    assert!(seen >= 10, "the Tormach library posts: {seen}");
}

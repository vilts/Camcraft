//! Provenance: original Explain invariants over our own posts and recorded CNC vocabulary (#1388).
//! Gate map (M4.4): covers assets/posts/fanuc.cps, assets/posts/haas.cps,
//! assets/posts/mazak.cps, assets/posts/doosan-dnm.cps,
//! assets/posts/fanuc-lathe.cps, camcraft_post::post::engine.

use camcraft_post::post::engine;
use serde_json::{json, Value};
use std::path::Path;

const LINE_MAX: usize = 80;
const COMMENT_MAX: usize = 60;

fn script(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../assets/posts/{name}.cps")),
    )
    .expect("shipped post")
}

fn input(lathe: bool) -> &'static str {
    if lathe {
        "PARAM SurfaceSpeed 200\nPARAM SpindleSpeed 1500\nPARAM WorkOffset 1\nTOOL 1 TURN\nSECTION\nRapid 22000 0 2000\nFeed 20000 0 -10000 F300000\nENDSECTION\n"
    } else {
        "PARAM SpindleSpeed 6000\nPARAM WorkOffset 1\nPARAM ClearancePlane 10\nPARAM RetractPlane 5\nTOOL 3 END MILL\nSECTION\nRapid 10000 20000 5000\nFeed 10000 20000 -1000 F300000\nFeed 20000 20000 -1000 F300000\nRapid 20000 20000 10000\nENDSECTION\n"
    }
}

fn machine(lathe: bool) -> Value {
    if lathe {
        json!({"name": "EXPLAIN LATHE", "spindleMax": 4000})
    } else {
        Value::Null
    }
}

/// Removing only the suffix added by Explain must recover every original
/// line, including its existing notes and any executable macro comments.
fn comments_only(on: &str, off: &str) {
    let on: Vec<_> = on.lines().collect();
    let off: Vec<_> = off.lines().collect();
    assert_eq!(on.len(), off.len(), "Explain preserves the block count");
    let mut stripped = Vec::new();
    for (explained, plain) in on.iter().zip(&off) {
        assert!(
            explained.len() <= LINE_MAX,
            "line exceeds {LINE_MAX}: {explained}"
        );
        if explained == plain {
            stripped.push(*explained);
            continue;
        }
        let comment = explained
            .strip_prefix(plain)
            .expect("same executable words");
        assert!(
            comment.starts_with(" (") && comment.ends_with(')'),
            "one trailing comment: {comment}"
        );
        assert!(comment[2..comment.len() - 1]
            .chars()
            .all(|c| c.is_ascii_graphic() || c == ' '));
        assert!(
            !comment[2..comment.len() - 1].contains(['(', ')', ';', '%']),
            "inert comment: {comment}"
        );
        assert!(comment.len() - 1 <= COMMENT_MAX, "comment cap: {comment}");
        stripped.push(*plain);
    }
    assert_eq!(stripped.join("\n"), off.join("\n"), "strip-and-compare");
}

fn pin(name: &str) {
    let lathe = name == "fanuc-lathe";
    let cps = script(name);
    let plain = engine::run_with(&cps, &machine(lathe), input(lathe), &json!({}))
        .expect("Explain off posts");
    assert!(plain.warnings.is_empty(), "{name}: {:?}", plain.warnings);
    let off = engine::run_with(
        &cps,
        &machine(lathe),
        input(lathe),
        &json!({"Explain": false}),
    )
    .expect("explicit off");
    assert_eq!(plain.program, off.program);
    let on = engine::run_with(
        &cps,
        &machine(lathe),
        input(lathe),
        &json!({"Explain": true}),
    )
    .expect("Explain on posts");
    assert!(on.warnings.is_empty(), "{name}: {:?}", on.warnings);
    assert_ne!(on.program, plain.program, "Explain adds comments");
    comments_only(&on.program, &plain.program);
    assert!(
        on.program.contains("(RAPID MOVE)"),
        "{name}: {}",
        on.program
    );
    assert!(
        on.program.contains("(FEED MOVE, FEED RATE"),
        "{name}: {}",
        on.program
    );
    assert!(
        on.program.contains("(PROGRAM END, BACK TO START)"),
        "{name}: {}",
        on.program
    );
    if lathe {
        assert!(
            on.program.contains("T0101 (TOOL 1, OFFSET 1)"),
            "{}",
            on.program
        );
        assert!(
            on.program.contains("G50 S4000 (SPINDLE SPEED LIMIT 4000)"),
            "{}",
            on.program
        );
        assert!(
            !on.program.contains("CYCLE RETURNS"),
            "lathe feed modes: {}",
            on.program
        );
    } else {
        assert!(on.program.contains("TOOL 3, TOOL CHANGE"), "{}", on.program);
    }
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/golden/explain/{name}.nc"));
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        // The review of a regenerated golden is its git diff.
        std::fs::write(&path, &on.program).expect("write the Explain golden");
    }
    assert_eq!(
        on.program,
        std::fs::read_to_string(path).expect("Explain golden"),
        "{name} golden"
    );
}

#[test]
fn fanuc_explain_golden_preserves_program() {
    pin("fanuc");
}

#[test]
fn haas_explain_golden_preserves_program() {
    pin("haas");
}

#[test]
fn mazak_explain_golden_preserves_program() {
    pin("mazak");
}

#[test]
fn doosan_explain_golden_preserves_program() {
    pin("doosan-dnm");
}

#[test]
fn fanuc_lathe_explain_golden_preserves_program() {
    pin("fanuc-lathe");
}

fn block_budget(name: &str) {
    // Probe the real block(), without replacing it. Long executable
    // lines stay intact; Explain must never truncate a coordinate.
    let words = [
        "G1 X1".to_string(),
        format!("G1 X{}", "1".repeat(64)),
        format!("G1 X{}", "1".repeat(65)),
        format!("G1 X{}", "1".repeat(69)),
        format!("G1 X{}", "1".repeat(74)),
        format!("G1 X{}", "1".repeat(75)),
        format!("G1 X{}", "1".repeat(90)),
    ];
    let cps = format!("{}\nfunction onOpen() {{ var ls = {}; for (var i=0;i<ls.length;i++) block([ls[i]]); }}\nfunction onClose() {{}}\n", script(name), json!(words));
    let on = engine::run_with(&cps, &Value::Null, "", &json!({"Explain": true}))
        .expect("block probe posts")
        .program;
    let off = engine::run_with(&cps, &Value::Null, "", &json!({}))
        .expect("plain block probe posts")
        .program;
    assert_eq!(off, words.join("\n") + "\n");
    let lines: Vec<_> = on.lines().collect();
    assert_eq!(lines.len(), words.len());
    assert_eq!(lines[0], "G1 X1 (FEED MOVE)", "{name}: {on}");
    assert_eq!(
        lines[1],
        format!("{} (FEED MOVE)", words[1]),
        "{name}: exact fit gets a comment"
    );
    assert_eq!(
        lines[2], words[2],
        "{name}: one character over omits the comment"
    );
    for (line, executable) in lines.iter().zip(&words) {
        assert!(
            line.starts_with(executable),
            "{name}: executable words preserved: {line}"
        );
        assert!(
            line.len() <= executable.len().max(LINE_MAX),
            "{name}: no explanation overflows: {line}"
        );
        if executable.len() >= LINE_MAX - 1 {
            assert_eq!(*line, executable, "{name}: no room for a useful comment");
        }
    }
}

#[test]
fn fanuc_explain_respects_block_budget() {
    block_budget("fanuc");
}
#[test]
fn haas_explain_respects_block_budget() {
    block_budget("haas");
}
#[test]
fn mazak_explain_respects_block_budget() {
    block_budget("mazak");
}
#[test]
fn doosan_explain_respects_block_budget() {
    block_budget("doosan-dnm");
}
#[test]
fn fanuc_lathe_explain_respects_block_budget() {
    block_budget("fanuc-lathe");
}

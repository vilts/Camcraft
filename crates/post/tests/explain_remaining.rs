//! Provenance: original Explain invariants for our own remaining post dialects (#1389).
//! Gate map (M4.4): covers assets/posts/sinumerik.cps, assets/posts/heidenhain.cps,
//! assets/posts/grblhal.cps, assets/posts/tinyg.cps, assets/posts/carvera.cps,
//! assets/posts/tormach-plasma.cps, assets/posts/wire-iso.cps,
//! assets/posts/sinker-iso.cps, assets/posts/sodick-wire.cps,
//! assets/posts/tormach-lathe.cps, camcraft_post::post::engine.

use camcraft_post::post::engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn read(path: &str) -> String {
    std::fs::read_to_string(repo(path)).expect(path)
}

fn script(name: &str) -> String {
    read(&format!("assets/posts/{name}.cps"))
}

fn fixture(name: &str) -> (Value, String) {
    match name {
        "tormach-plasma" => {
            let text = read("assets/machines/tormach-1300pl.mch");
            let parsed = camcraft_post::machine::parse_mch(&text).expect("plasma fixture");
            (camcraft_post::post::machine::mirror(&parsed), read("crates/app/tests/golden/plasma_plate_with_holes.cl"))
        }
        "wire-iso" => (
            serde_json::from_str(&read("crates/app/tests/edm/wire-iso-fixture.mch")).expect("wire fixture"),
            "PARAM OperationName Punch1\nPARAM EdmCondition C110\nPARAM WireSide punch\nSECTION\nRapid -30000 0 0\nFeed -20000 0 0 F2000\nENDSECTION\n".into(),
        ),
        "sinker-iso" => (
            serde_json::from_str(&read("crates/app/tests/edm/sinker-iso-dialect.mch")).expect("sinker fixture"),
            format!("PARAM OperationName Burn1\nPARAM OperationKind Sinker Burn\nPARAM ToolKind sinker electrode\nPARAM SpindleSpeed 0\nPARAM WorkOffset G54\nPARAM SinkerCondition S1\nPARAM ElectrodePickup 15\nTOOL 1 E-10x6\nSECTION\n{}ENDSECTION\n", read("crates/app/tests/golden/sinker_burn_cavity.cl")),
        ),
        "tormach-lathe" => (
            json!({"name": "EXPLAIN LATHE", "spindleMax": 4000}),
            "PARAM StockXYLeft -20\nPARAM StockXYRight 20\nPARAM StockXYFront -20\nPARAM StockXYBack 20\nPARAM StockZTop 0\nPARAM StockTurnRadius 20\nPARAM ClearancePlane 5\nPARAM SpindleSpeed 1500\nPARAM WorkOffset 1\nTOOL 1 TURN\nSECTION\nRapid 22000 0 2000\nFeed 20000 0 -10000 F300000\nENDSECTION\n".into(),
        ),
        _ => (Value::Null,
            "PARAM SpindleSpeed 6000\nPARAM WorkOffset 1\nPARAM ClearancePlane 10\nPARAM RetractPlane 5\nTOOL 3 END MILL\nSECTION\nRapid 10000 20000 5000\nFeed 10000 20000 -1000 F300000\nFeed 20000 20000 -1000 F300000\nRapid 20000 20000 10000\nENDSECTION\n".into()),
    }
}

fn semi(name: &str) -> bool {
    matches!(name, "sinumerik" | "heidenhain")
}

fn comments_only(name: &str, on: &str, off: &str) {
    let on: Vec<_> = on.lines().collect();
    let off: Vec<_> = off.lines().collect();
    assert_eq!(on.len(), off.len(), "Explain preserves block count");
    let mut stripped = String::new();
    for (explained, plain) in on.iter().zip(off.iter().copied()) {
        if *explained != plain {
            assert!(explained.len() <= 80, "{name}: line cap: {explained}");
            let suffix = explained
                .strip_prefix(plain)
                .expect("same executable words");
            let text = if semi(name) {
                suffix
                    .strip_prefix(" ;")
                    .expect("trailing semicolon comment")
            } else {
                suffix
                    .strip_prefix(" (")
                    .and_then(|s| s.strip_suffix(')'))
                    .expect("trailing parentheses comment")
            };
            assert!(text.bytes().all(|b| (0x20..=0x7e).contains(&b)));
            assert!(!text.contains(['(', ')', ';', '%']));
            assert!(suffix.len() - 1 <= 60, "comment cap: {suffix}");
        }
        stripped.push_str(plain);
        stripped.push('\n');
    }
    assert_eq!(stripped, off_string(off), "strip-and-compare");
}

fn off_string(lines: Vec<&str>) -> String {
    lines.join("\n") + "\n"
}

fn pin(name: &str) {
    let cps = script(name);
    let (machine, input) = fixture(name);
    let absent = engine::run_with(&cps, &machine, &input, &json!({})).expect("plain posts");
    let off =
        engine::run_with(&cps, &machine, &input, &json!({"Explain": false})).expect("off posts");
    assert!(off.warnings.is_empty(), "{name}: {:?}", off.warnings);
    assert_eq!(absent.program, off.program);
    let on = engine::run_with(&cps, &machine, &input, &json!({"Explain": true}))
        .expect("Explain on posts");
    assert_eq!(on.warnings, off.warnings, "warnings stay unchanged");
    assert_eq!(on.swallowed, off.swallowed);
    assert_ne!(on.program, off.program, "Explain adds comments");
    comments_only(name, &on.program, &off.program);
    assert!(on.program.contains("RAPID MOVE"), "{name}: {}", on.program);
    if name == "tormach-lathe" {
        assert!(
            on.program.contains("T0101 (TOOL 1, OFFSET 1)"),
            "{}",
            on.program
        );
        assert!(!on.program.contains("CYCLE RETURNS"), "{}", on.program);
    }
    let path = repo(&format!("crates/post/tests/golden/explain/{name}.nc"));
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(&path, &on.program).expect("write golden");
    }
    assert_eq!(
        on.program,
        std::fs::read_to_string(path).expect("Explain golden")
    );
}

fn block_budget(name: &str) {
    let words: Vec<_> = [1, 50, 64, 65, 74, 75, 90]
        .into_iter()
        .map(|n| format!("G1 X{}", "1".repeat(n)))
        .collect();
    let cps = format!("{}\nfunction onOpen() {{ var ls = {}; for (var i=0;i<ls.length;i++) block([ls[i]]); }}\nfunction onClose() {{}}\n", script(name), json!(words));
    let on = engine::run_with(&cps, &Value::Null, "", &json!({"Explain": true}))
        .expect("budget probe")
        .program;
    let off = engine::run_with(&cps, &Value::Null, "", &json!({}))
        .expect("plain probe")
        .program;
    comments_only(name, &on, &off);
    assert!(
        on.contains("FEED MOVE"),
        "short blocks get explanations: {name}: {on}"
    );
    for (line, plain) in on.lines().zip(off.lines()) {
        assert!(
            line.starts_with(plain),
            "{name}: executable block stays intact"
        );
        assert!(line.len() <= plain.len().max(80), "{name}: {line}");
    }
}

#[test]
fn sinumerik_explain_golden_preserves_program() {
    pin("sinumerik");
    block_budget("sinumerik");
}
#[test]
fn heidenhain_explain_is_refused_by_name() {
    refused("heidenhain", "Heidenhain");
}
#[test]
fn grblhal_explain_golden_preserves_program() {
    pin("grblhal");
    block_budget("grblhal");
}
#[test]
fn tinyg_explain_golden_preserves_program() {
    pin("tinyg");
    block_budget("tinyg");
}
#[test]
fn carvera_explain_golden_preserves_program() {
    pin("carvera");
    block_budget("carvera");
}
#[test]
fn plasma_explain_golden_preserves_program() {
    pin("tormach-plasma");
    block_budget("tormach-plasma");
}
#[test]
fn wire_explain_golden_preserves_program() {
    pin("wire-iso");
    block_budget("wire-iso");
}
#[test]
fn sinker_explain_golden_preserves_program() {
    pin("sinker-iso");
    block_budget("sinker-iso");
}
#[test]
fn tormach_lathe_explain_golden_preserves_program() {
    pin("tormach-lathe");
    block_budget("tormach-lathe");
}

#[test]
fn sodick_explain_is_refused_by_name() {
    refused("sodick-wire", "Sodick");
}

#[test]
fn wire_explain_never_assigns_milling_meanings_to_machine_codes() {
    let (mut machine, input) = fixture("wire-iso");
    machine["edm"]["codes"]["thread"] = json!("T91");
    machine["edm"]["codes"]["cut"] = json!("T90");
    machine["edm"]["codes"]["tip_disposal"] = json!("M3");
    machine["edm"]["codes"]["condition"] = json!("M4 {id}");
    machine["edm"]["codes"]["comp_off"] = json!("G99");
    let cps = script("wire-iso");
    let off = engine::run_with(&cps, &machine, &input, &json!({})).expect("machine codes post");
    let on = engine::run_with(&cps, &machine, &input, &json!({"Explain": true}))
        .expect("Explain machine codes");
    assert!(on.warnings.is_empty(), "{:?}", on.warnings);
    comments_only("wire-iso", &on.program, &off.program);
    for word in ["T91", "T90", "M3", "M4 C110", "G99"] {
        assert!(
            on.program.lines().any(|line| line == word),
            "opaque code stays unexplained: {word}: {}",
            on.program
        );
    }
    assert!(!on.program.contains("TOOL 91"));
    assert!(!on.program.contains("SPINDLE ON"));
    assert!(!on.program.contains("CYCLE RETURNS"));
}

#[test]
fn sinumerik_explain_uses_control_units_and_offsets() {
    for (word, meaning) in [
        ("G700", "INCH LENGTHS AND FEEDS"),
        ("G710", "MM LENGTHS AND FEEDS"),
        ("G505", "WORK OFFSET 5"),
        ("G506", "WORK OFFSET 6"),
    ] {
        let cps = format!(
            "{}\nfunction onOpen() {{ block([\"{word}\"]); }}\nfunction onClose() {{}}\n",
            script("sinumerik")
        );
        let out = engine::run_with(&cps, &Value::Null, "", &json!({"Explain": true}))
            .expect("Sinumerik word");
        assert_eq!(out.program, format!("N10 {word} ; {meaning}\n"));
    }
    let cps = format!("{}\nfunction onOpen() {{ block([\"G58\"]); block([\"G59\"]); }}\nfunction onClose() {{}}\n", script("sinumerik"));
    let out = engine::run_with(&cps, &Value::Null, "", &json!({"Explain": true}))
        .expect("unknown offset meanings");
    assert_eq!(out.program, "N10 G58\nN20 G59\n");
}

#[test]
fn plasma_explain_uses_torch_meanings() {
    for (word, meaning) in [
        ("G15", "PROBE PLATE"),
        ("G16", "TORCH ON"),
        ("M200", "LOAD PLASMA SETTINGS"),
        ("M205", "TORCH OFF"),
        ("M207 P1", "PIERCE HEIGHT 1"),
        ("M208 P2", "CUT HEIGHT 2"),
        ("M209 P3", "PIERCE DELAY 3"),
    ] {
        let cps = format!(
            "{}\nfunction onOpen() {{ block([\"{word}\"]); }}\nfunction onClose() {{}}\n",
            script("tormach-plasma")
        );
        let out = engine::run_with(&cps, &Value::Null, "", &json!({"Explain": true}))
            .expect("plasma word");
        assert_eq!(out.program, format!("{word} ({meaning})\n"));
    }
}

#[test]
fn tormach_lathe_explain_uses_extended_offsets() {
    for (word, meaning) in [
        ("G59.1", "WORK OFFSET 7"),
        ("G59.2", "WORK OFFSET 8"),
        ("G59.3", "WORK OFFSET 9"),
    ] {
        let cps = format!(
            "{}\nfunction onOpen() {{ block([\"{word}\"]); }}\nfunction onClose() {{}}\n",
            script("tormach-lathe")
        );
        let out = engine::run_with(&cps, &Value::Null, "", &json!({"Explain": true}))
            .expect("lathe offset word");
        assert_eq!(out.program, format!("{word} ({meaning})\n"));
    }
}

fn refused(name: &str, description: &str) {
    let err = engine::run_with(&script(name), &Value::Null, "", &json!({"Explain": true}))
        .expect_err("unsupported EDM T-code meanings must refuse")
        .to_string();
    assert!(err.contains(description), "{err}");
    assert!(err.contains("Explain"), "{err}");
}

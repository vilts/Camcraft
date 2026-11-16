//! Explain G-code on a lathe (#1399): the PRELUDE's word table is the
//! mill reading, so a control that reads a word differently states its
//! own meaning in its manifest (`explainWords`, `explainTool`). Posted
//! through the shipped fanuc-lathe.cps with Explain on, G98 / G99 are the
//! feed modes (never the canned-cycle returns), T0101 is tool 1, offset 1
//! (never tool 101) and G50 S is the spindle speed limit (never a plain
//! speed); the comments stay inert - with Explain off the program is
//! byte-identical to the shipped post's, and stripped of them the
//! Explain-on program is too. A malformed table is refused by name.
//!
//! Gate map (M4.4): covers camcraft_post::post::explain, ::post::engine
//! (on fanuc-lathe.cps and tormach-lathe.cps).
//! Lane shape (M4.4): all tests here run in milliseconds.

use camcraft_post::cl::MoveKind;
use camcraft_post::machine::{AxisLimits, Coolant, LiveToolCodes, Machine};
use camcraft_post::post::{cnc, engine, PostError};
use serde_json::{json, Value};

const FANUC_LATHE: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");
const TORMACH_LATHE: &str = include_str!("../../../assets/posts/tormach-lathe.cps");

/// The shipped post, opted into Explain the way #1386 means a dialect
/// to: a comment form, and a `block()` that appends `explain()` of its
/// words after them (the later declaration wins in the one script).
fn explaining(cps: &str) -> String {
    format!(
        r#"{cps}
explains = "paren";
function block(words) {{
  var a = [];
  for (var i = 0; i < words.length; i++) {{
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }}
  if (a.length === 0) return;
  var c = explain(a);
  writeln(c === "" ? a.join(" ") : a.join(" ") + " " + c);
}}
"#
    )
}

/// A mill-turn with a spindle limit and live-tool words (placeholders).
fn mill_turn() -> Value {
    camcraft_post::post::machine::mirror(&Machine {
        model: "TestBench mill-turn".into(),
        kind: Some("mill-turn".into()),
        spindle_max: Some(4000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        axes: vec![
            AxisLimits { name: "X".into(), ..Default::default() },
            AxisLimits { name: "Z".into(), ..Default::default() },
            AxisLimits { name: "C".into(), rotary: true, axis: Some([0.0, 0.0, 1.0]), ..Default::default() },
        ],
        live_tool: Some(LiveToolCodes {
            c_axis_on: "M1001".into(),
            c_axis_off: "M1002".into(),
            spindle_on: "M1003".into(),
            spindle_off: "M1004".into(),
        }),
        ..Default::default()
    })
}

fn mv(kind: MoveKind, x: f64, z: f64, feed: Option<f64>) -> cnc::Record {
    cnc::Record::Move(cnc::MoveRec { kind, x, y: 0.0, z, feed, center: None, ccw: false, plane: Default::default(), abc: None })
}

/// The same move in a C-axis section, at C0.
fn live(r: cnc::Record) -> cnc::Record {
    match r {
        cnc::Record::Move(m) => cnc::Record::Move(cnc::MoveRec { abc: Some([0.0, 0.0, 0.0]), ..m }),
        other => other,
    }
}

/// A turning section under constant surface speed (T0101, G50 S, G99),
/// then a live-tool section (T0202, G98 for its feed per minute, G99
/// after it).
fn stream() -> String {
    cnc::serialize(&[
        cnc::Record::Param { name: "SurfaceSpeed".into(), value: json!(200.0) },
        cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(1500.0) },
        cnc::Record::Param { name: "WorkOffset".into(), value: json!("G54") },
        cnc::Record::Tool { number: 1, name: "TURN".into() },
        cnc::Record::Section { begin: true },
        mv(MoveKind::Rapid, 22.0, 2.0, None),
        mv(MoveKind::Feed, 20.0, -10.0, Some(300.0)),
        cnc::Record::Section { begin: false },
        cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(3000.0) },
        cnc::Record::Param { name: "WorkOffset".into(), value: json!("G54") },
        cnc::Record::Param { name: "LiveTool".into(), value: json!("radial") },
        cnc::Record::Tool { number: 2, name: "EM".into() },
        cnc::Record::Section { begin: true },
        live(mv(MoveKind::Rapid, 10.0, 1.0, None)),
        live(mv(MoveKind::Feed, 10.0, -2.0, Some(200.0))),
        cnc::Record::Section { begin: false },
    ])
}

fn post(cps: &str, options: &Value) -> Result<engine::PostOutput, PostError> {
    engine::run_with(cps, &mill_turn(), &stream(), options)
}

/// Every Explain-on line is its plain line, or that line with one
/// trailing parenthesised comment - never a changed or moved word.
fn only_comments_added(on: &str, plain: &str) {
    let (a, b): (Vec<&str>, Vec<&str>) = (on.lines().collect(), plain.lines().collect());
    assert_eq!(a.len(), b.len(), "same blocks:\n{on}");
    for (l, p) in a.iter().zip(&b) {
        let ok = l == p || l.strip_prefix(p).is_some_and(|c| c.starts_with(" (") && c.ends_with(')') && c[2..].find(['(', ')']) == Some(c.len() - 3));
        assert!(ok, "{l:?} is not {p:?} plus one comment");
    }
    assert_eq!(on.ends_with('\n'), plain.ends_with('\n'));
}

#[test]
fn fanuc_lathe_explains_its_own_feed_modes_tool_word_and_speed_limit() {
    let plain = post(FANUC_LATHE, &json!({})).expect("the shipped post posts");
    assert!(plain.warnings.is_empty(), "{:?}", plain.warnings);
    let cps = explaining(FANUC_LATHE);
    // Explain off: byte-identical to the shipped post, manifest and all.
    let off = post(&cps, &json!({})).expect("explain off posts");
    assert_eq!(off.program, plain.program, "explain off changes nothing");
    let on = post(&cps, &json!({ "Explain": true })).expect("explain on posts").program;
    only_comments_added(&on, &plain.program);

    let line = |head: &str| on.lines().find(|l| l.starts_with(head)).map(str::to_owned).unwrap_or_default();
    // The lathe reading, word by word.
    assert_eq!(line("T0101"), "T0101 (TOOL 1, OFFSET 1)", "{on}");
    assert_eq!(line("T0202"), "T0202 (TOOL 2, OFFSET 2)", "{on}");
    assert_eq!(line("G50 S"), "G50 S4000 (SPINDLE SPEED LIMIT 4000)", "{on}");
    // The opening block's G99 falls past the 60-character cap: whole
    // meanings up to it, never the mill reading in its place.
    assert_eq!(line("G21 G18"), "G21 G18 G40 G80 G99 (MM UNITS, XZ PLANE, RADIUS COMP CANCEL, CANCEL DRILL CYCLE)", "{on}");
    assert_eq!(line("G98"), "G98 (FEED PER MINUTE)", "{on}");
    assert_eq!(line("G99"), "G99 (FEED PER REVOLUTION)", "{on}");
    // The mill readings never appear.
    for wrong in ["CYCLE RETURNS", "TOOL 101", "TOOL 202", "(SPEED 4000"] {
        assert!(!on.contains(wrong), "{wrong:?} is the mill reading:\n{on}");
    }
}

/// What `explain()` says of `words` on a script, Explain on.
fn says(cps: &str, words: &[&str]) -> String {
    let probe = format!(
        "{cps}\nexplains = \"paren\";\nfunction onOpen() {{ writeln(\"[\" + explain({}) + \"]\"); }}\n",
        json!(words)
    );
    let out = engine::run_with(&probe, &mill_turn(), &cnc::serialize(&[]), &json!({ "Explain": true }))
        .expect("the probe posts")
        .program;
    out.lines().find(|l| l.starts_with('[')).unwrap_or_default().to_owned()
}

#[test]
fn a_lathe_word_without_its_reading_is_left_unexplained_never_guessed() {
    // G50 without S is the coordinate setting: no meaning stated, none said.
    assert_eq!(says(FANUC_LATHE, &["G50", "X40", "Z100"]), "[]");
    // Offset 0 is no offset; a T word outside the Txxyy form says nothing.
    assert_eq!(says(FANUC_LATHE, &["T0100"]), "[(TOOL 1, NO OFFSET)]");
    assert_eq!(says(FANUC_LATHE, &["T1"]), "[]");
    // The PathPilot lathe reads T0303 the same way.
    assert_eq!(says(TORMACH_LATHE, &["T0303"]), "[(TOOL 3, OFFSET 3)]");
    // A mill post keeps the mill table: G99 is the R-height return there.
    assert_eq!(says("", &["G99", "T101"]), "[(CYCLE RETURNS TO R HEIGHT, TOOL 101)]");
}

#[test]
fn a_malformed_word_table_is_refused_by_name_only_when_explain_is_on() {
    let cases = [
        ("explainTool = \"two-digit\";", "explainTool"),
        ("explainWords = [\"G99\"];", "not a table"),
        ("explainWords = { G099: \"X\" };", "G099"),
        ("explainWords = { G50: { said: \"LIMIT\" } };", "G50"),
    ];
    for (manifest, named) in cases {
        let cps = format!("description = \"Odd Lathe (2-axis)\";\nexplains = \"paren\";\n{manifest}\nfunction onOpen() {{ writeln(\"%\"); }}\n");
        match engine::run_with(&cps, &Value::Null, &cnc::serialize(&[]), &json!({ "Explain": true })) {
            Err(PostError::Refused(why)) => {
                assert!(why.contains("Odd Lathe") && why.contains(named), "{manifest}: {why}");
            }
            other => panic!("{manifest}: expected a refusal by name, got {other:?}"),
        }
        let off = engine::run_with(&cps, &Value::Null, &cnc::serialize(&[]), &json!({})).expect("off posts");
        assert_eq!(off.program, "%\n", "{manifest}: off never reads the table");
    }
}

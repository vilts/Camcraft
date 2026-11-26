//! #2769 follow-up to #3307: what PR #3306 adds to the Fanuc post's
//! Manual Guide i blank. The writer and its `StockShape` PARAM contract
//! are #3307's (pinned in `fanuc_mgi_blank.rs`). This file pins:
//! - the option's help names the Series 30i background-simulation unit
//!   limit;
//! - the shape PARAMs never move the viewer's line map or trace record
//!   indices off the input stream (the review finding on #3306);
//! - a bar whose top face is below the work origin (a negative K) posts
//!   no G1900, with the work origin above the whole bar.
//!
//! Provenance: FANUC MANUAL GUIDE i operator's manual (B-63874EN), blank
//! figure section - behaviour only; our own fixtures.
//!
//! Gate map (M4.4): covers assets/posts/fanuc.cps,
//! camcraft_post::post::engine, camcraft_post::post::options.

use camcraft_post::cl::MoveKind;
use camcraft_post::post::cnc::{self, Record};
use camcraft_post::post::engine;
use serde_json::{json, Value};
use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// One face pass over a 50 mm bar, Z -40..0, with the work origin on its
/// axis at `origin_z`.
fn bar_stream(origin_z: f64) -> String {
    format!(
        "; HSMW-CL v1 scale=0.001mm moves=3
PARAM WorkOffset 1
PARAM OriginX 0
PARAM OriginY 0
PARAM OriginZ {origin_z}
PARAM StockXYLeft -25
PARAM StockXYRight 25
PARAM StockXYFront -25
PARAM StockXYBack 25
PARAM StockZTop 0
PARAM StockShape cylinder-z
PARAM StockShapeBottom -40
PARAM StockShapeDiameter 50
TOOL 3 \"EM D10\"
PARAM SpindleSpeed 6000
PARAM OperationName Face
PARAM ClearancePlane 50
PARAM RetractPlane 10
SECTION
Rapid 0 0 10000
Feed 0 0 -1000 F300000
Rapid 0 0 10000
ENDSECTION
"
    )
}

fn post(s: &str, options: &Value) -> engine::PostOutput {
    let mch: Value = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    let out = engine::run_with(&read("assets/posts/fanuc.cps"), &mch, s, options).expect("posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    out
}

#[test]
fn the_blank_option_explains_the_series_30i_simulation_unit_limit() {
    let options = camcraft_post::post::options::declared_options(&read("assets/posts/fanuc.cps")).expect("options");
    let option = options.iter().find(|o| o.key == "manualGuideBlank").expect("blank option");
    for want in ["Series 30i", "ignores G20/G21", "control unit setting"] {
        assert!(option.description.contains(want), "{want}: {}", option.description);
    }
}

#[test]
fn the_shape_params_keep_the_line_map_and_trace_on_the_input_records() {
    let source = bar_stream(-1.0);
    let records = cnc::parse(&source).expect("records");
    let feed = records
        .iter()
        .position(|r| matches!(r, Record::Move(m) if m.kind == MoveKind::Feed))
        .expect("feed");
    for options in [Value::Null, json!({ "manualGuideBlank": true })] {
        let out = post(&source, &options);
        let line = out.program.lines().position(|l| l.starts_with("G01")).expect("feed line");
        let mapped = out.line_map.iter().find(|m| m.line == line).expect("mapped line");
        assert_eq!(mapped.record, Some(feed), "{options}: line map\n{}", out.program);
        let block = out.blocks.iter().find(|b| b.first <= line && line < b.last).expect("feed block");
        assert_eq!(block.record, Some(feed), "{options}: trace block");
    }
}

#[test]
fn a_bar_with_the_origin_above_its_top_refuses_the_blank() {
    let out = post(&bar_stream(5.0), &json!({ "manualGuideBlank": true }));
    assert!(!out.program.contains("G1900"), "negative K must not reach the control:\n{}", out.program);
    assert!(
        out.program.contains("(NO BLANK BLOCK - BAR END FACE BELOW THE WORK ORIGIN)"),
        "{}",
        out.program
    );
    // The same bar with the origin on its top face posts the block.
    let ok = post(&bar_stream(0.0), &json!({ "manualGuideBlank": true }));
    assert!(ok.program.lines().any(|l| l == "G1900 D50. L40. K0."), "{}", ok.program);
}

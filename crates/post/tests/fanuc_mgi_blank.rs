//! #2769: the Fanuc post's Manual Guide i blank option. Off by default;
//! on, the setup's stock posts as one blank block (G1902 for a box, G1900
//! for a round bar along Z) after G20 / G21 and before the first G90 /
//! G54 and tool change. A stock no block describes posts a comment.
//!
//! Provenance: Fanuc's public MANUAL GUIDE i operator's manual
//! (B-63874EN), blank-figure addresses as recorded on issue #2769;
//! our own fixtures. No other CAM's post or output consulted.
//!
//! Gate map: covers assets/posts/fanuc.cps, camcraft_post::post::engine.

use camcraft_post::post::engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn read(rel: &str) -> String {
    let p: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// A 100 x 60 x 30 block from (-50, -30, -30) to (50, 30, 0), G54 at its
/// top centre; `extra` adds or overrides PARAMs ahead of the TOOL.
fn stream(extra: &str) -> String {
    format!(
        "; HSMW-CL v1 scale=0.001mm moves=3
PARAM WorkOffset 1
PARAM OriginX 0
PARAM OriginY 0
PARAM OriginZ 0
PARAM StockXYLeft -50
PARAM StockXYRight 50
PARAM StockXYFront -30
PARAM StockXYBack 30
PARAM StockZTop 0
{extra}TOOL 3 \"EM D10\"
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

const BOX: &str = "PARAM StockShape box\nPARAM StockShapeBottom -30\n";
const ON: &str = r#"{ "manualGuideBlank": true }"#;

fn post(s: &str, options: &str) -> (String, Vec<String>) {
    let opts: Value = serde_json::from_str(options).expect("options json");
    let out = engine::run_with(&read("assets/posts/fanuc.cps"), &json!({}), s, &opts).expect("posts");
    (out.program, out.warnings)
}

fn line_of(program: &str, pat: &str) -> Option<usize> {
    program.lines().position(|l| l.contains(pat))
}

#[test]
fn off_by_default_no_blank_and_no_comment() {
    let (plain, w) = post(&stream(BOX), "{}");
    assert!(w.is_empty(), "{w:?}");
    assert!(!plain.contains("G190"), "no blank block: {plain}");
    assert!(!plain.contains("BLANK"), "no blank comment: {plain}");
    // The shape PARAMs alone change nothing: the same text as without them.
    let (bare, _) = post(&stream(""), "{}");
    assert_eq!(plain, bare);
}

#[test]
fn a_box_posts_g1902_before_the_first_g90_and_tool_change() {
    let (p, w) = post(&stream(BOX), ON);
    assert!(w.is_empty(), "{w:?}");
    let at = line_of(&p, "G1902").unwrap_or_else(|| panic!("a G1902 block: {p}"));
    assert_eq!(p.lines().nth(at), Some("G1902 B100. D60. H30. I50. J30. K30."), "{p}");
    assert_eq!(p.matches("G1902").count(), 1, "one block: {p}");
    let unit = line_of(&p, "G21").expect("G21");
    let g90 = line_of(&p, "G90").expect("G90");
    let tool = line_of(&p, "M6").expect("M6");
    assert!(unit < at && at < g90 && at < tool, "after G21, before G90 and T M6: {p}");
}

#[test]
fn a_box_with_an_offset_origin_posts_signed_distances() {
    // The origin at the block's front-left bottom corner plus 10 in X, and
    // a work origin below the block in Z: I/J/K from the minimum corner.
    let s = stream(BOX).replace("PARAM OriginX 0\n", "PARAM OriginX -40\n").replace("PARAM OriginY 0\n", "PARAM OriginY -30\n");
    let s = s.replace("PARAM OriginZ 0\n", "PARAM OriginZ -35\n");
    let (p, _) = post(&s, ON);
    assert!(p.lines().any(|l| l == "G1902 B100. D60. H30. I10. J0. K-5."), "{p}");
}

#[test]
fn an_inch_program_writes_the_blank_in_inch() {
    let (p, _) = post(&stream(BOX), r#"{ "manualGuideBlank": true, "Units": "inch" }"#);
    assert!(p.lines().any(|l| l == "G1902 B3.937 D2.3622 H1.1811 I1.9685 J1.1811 K1.1811"), "{p}");
}

#[test]
fn a_z_bar_on_the_origin_posts_g1900() {
    let s = stream("PARAM StockShape cylinder-z\nPARAM StockShapeBottom -30\nPARAM StockShapeDiameter 60\n")
        .replace("PARAM StockXYLeft -50\n", "PARAM StockXYLeft -30\n")
        .replace("PARAM StockXYRight 50\n", "PARAM StockXYRight 30\n");
    let (p, w) = post(&s, ON);
    assert!(w.is_empty(), "{w:?}");
    assert!(p.lines().any(|l| l == "G1900 D60. L30. K0."), "{p}");
    assert!(line_of(&p, "G1900") < line_of(&p, "G90"), "{p}");
}

#[test]
fn a_z_bar_with_the_origin_below_its_top_posts_the_end_face_distance() {
    // Top face Z12, origin Z5: K is 7, independent of the lower end Z-18.
    let s = stream(
        "PARAM StockShape cylinder-z\nPARAM StockShapeBottom -18\nPARAM StockShapeDiameter 60\n",
    )
    .replace("PARAM StockXYLeft -50\n", "PARAM StockXYLeft -30\n")
    .replace("PARAM StockXYRight 50\n", "PARAM StockXYRight 30\n")
    .replace("PARAM StockZTop 0\n", "PARAM StockZTop 12\n")
    .replace("PARAM OriginZ 0\n", "PARAM OriginZ 5\n");
    let (p, w) = post(&s, ON);
    assert!(w.is_empty(), "{w:?}");
    assert!(p.lines().any(|l| l == "G1900 D60. L30. K7."), "{p}");
}

#[test]
fn a_z_bar_requiring_negative_k_posts_a_comment() {
    let s = stream(
        "PARAM StockShape cylinder-z\nPARAM StockShapeBottom -30\nPARAM StockShapeDiameter 60\n",
    )
    .replace("PARAM StockXYLeft -50\n", "PARAM StockXYLeft -30\n")
    .replace("PARAM StockXYRight 50\n", "PARAM StockXYRight 30\n")
    .replace("PARAM OriginZ 0\n", "PARAM OriginZ 5\n");
    let (p, w) = post(&s, ON);
    assert!(w.is_empty(), "{w:?}");
    assert!(!p.contains("G1900"), "{p}");
    let at = line_of(&p, "(NO BLANK BLOCK - BAR END FACE BELOW THE WORK ORIGIN)")
        .unwrap_or_else(|| panic!("a refusal comment: {p}"));
    assert!(Some(at) < line_of(&p, "G90"), "{p}");
}

#[test]
fn a_z_bar_off_the_origin_posts_a_comment() {
    let s = stream("PARAM StockShape cylinder-z\nPARAM StockShapeBottom -30\nPARAM StockShapeDiameter 60\n")
        .replace("PARAM StockXYLeft -50\n", "PARAM StockXYLeft -20\n")
        .replace("PARAM StockXYRight 50\n", "PARAM StockXYRight 40\n");
    let (p, _) = post(&s, ON);
    assert!(!p.contains("G1900"), "{p}");
    assert!(p.contains("(NO BLANK BLOCK - BAR AXIS NOT ON THE WORK ORIGIN)"), "{p}");
}

#[test]
fn a_z_bar_without_its_diameter_posts_a_comment() {
    let s = stream("PARAM StockShape cylinder-z\nPARAM StockShapeBottom -30\n")
        .replace("PARAM StockXYLeft -50\n", "PARAM StockXYLeft -30\n")
        .replace("PARAM StockXYRight 50\n", "PARAM StockXYRight 30\n");
    let (p, _) = post(&s, ON);
    assert!(!p.contains("G1900"), "{p}");
    assert!(p.contains("(NO BLANK BLOCK - BAR DIAMETER NOT STATED)"), "{p}");
}

#[test]
fn a_stock_no_block_describes_posts_a_comment_not_a_block() {
    let cases = [
        ("", "NO STOCK SHAPE STATED"),
        ("PARAM StockShape from-model\nPARAM StockShapeBottom -30\n", "STOCK FROM A MODEL"),
        ("PARAM StockShape from-previous\nPARAM StockShapeBottom -30\n", "STOCK FROM THE SETUP BEFORE"),
        ("PARAM StockShape rotated\nPARAM StockShapeBottom -30\n", "STOCK OFF THE MACHINE AXES"),
        ("PARAM StockShape cylinder-x\nPARAM StockShapeBottom -30\nPARAM StockShapeDiameter 60\n", "BAR ALONG X - NOT SUPPORTED YET"),
        ("PARAM StockShape cylinder-y\nPARAM StockShapeBottom -30\nPARAM StockShapeDiameter 60\n", "BAR ALONG Y"),
        ("PARAM StockShape box\n", "STOCK BOUNDS NOT STATED"),
        ("PARAM StockShape box\nPARAM StockShapeBottom 0\n", "STOCK HAS NO SIZE"),
        ("PARAM StockShape cone\nPARAM StockShapeBottom -30\n", "UNKNOWN STOCK SHAPE CONE"),
        ("PARAM StockShape box\nPARAM StockShapeBottom -30\nPARAM WcsYaw 30\n", "WCS ROTATED ABOUT Z"),
    ];
    for (extra, why) in cases {
        let (p, w) = post(&stream(extra), ON);
        assert!(w.is_empty(), "{why}: a comment, not a warning: {w:?}");
        assert!(!p.contains("G190"), "{why}: no block: {p}");
        let want = format!("(NO BLANK BLOCK - {why})");
        let at = line_of(&p, &want).unwrap_or_else(|| panic!("{why}: the comment: {p}"));
        assert!(Some(at) < line_of(&p, "G90"), "{why}: before the first G90: {p}");
    }
}

#[test]
fn two_operations_write_one_blank() {
    let one = stream(BOX);
    let second = one.split_once("TOOL 3").map(|(_, t)| format!("TOOL 4{t}")).expect("a tool record");
    let (p, _) = post(&format!("{one}{}", second.replace("PARAM OperationName Face", "PARAM OperationName Edge")), ON);
    assert_eq!(p.matches("G1902").count(), 1, "{p}");
}

/// The Robodrill tester kit (docs/tester-kits/fanuc-mgi-blank) is exactly
/// what the post writes for the box and the Z bar of the pins above.
#[test]
fn the_tester_kit_is_what_the_post_writes() {
    let bar = stream("PARAM StockShape cylinder-z\nPARAM StockShapeBottom -30\nPARAM StockShapeDiameter 60\n")
        .replace("PARAM StockXYLeft -50\n", "PARAM StockXYLeft -30\n")
        .replace("PARAM StockXYRight 50\n", "PARAM StockXYRight 30\n");
    for (file, s) in [("mgi_blank_box.nc", stream(BOX)), ("mgi_blank_bar.nc", bar)] {
        let (p, _) = post(&s, ON);
        assert_eq!(p, read(&format!("docs/tester-kits/fanuc-mgi-blank/{file}")), "{file}");
    }
}

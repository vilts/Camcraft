//! Issue #3035: an Adaptive operation whose planner hands an area to the
//! Offset rings says so as a toolpath warning, not only in the notes. The
//! op reports `warnings` > 0, the text counts the areas, and the post is
//! not held (the rings are verified like any other path). The fixture is
//! the cheap channel of `shoulder_adaptive`: the band round the user's
//! "Milling-test" part, which the planner refuses as too narrow and long.
//!
//! Gate map: covers app::operation (generate), sim::verify (the
//! StrategyFallback warning).
//! Provenance: our own regression of the QA report; no vendor material
//! consulted.

mod area_support;

use area_support::*;
use camcraft_app::operation::HeightReference;

fn channel_job() -> (Toolpath, Vec<camcraft_app::verify::Warning>) {
    let world = new_world();
    let mut h = History::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let imp = ImportModel::new(&path).expect("the user's STEP imports");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let mut p = operation::retype_params(&pocket(20.0, -5.0), OpType::Adaptive2D.ordinal());
    if let OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. } = &mut p {
        *top_z = Height::fixed(20.0);
        *bottom_z = Height { reference: HeightReference::BottomOfSelection, offset: 0.0 };
        *stepdown = 25.0; // one level: the debug lane stays fast
    }
    let src = sel(&name, top_face(&world, &name, -5.0));
    let a = AreaControls { containment: Some(Containment::Outside), extension: 4.0, ..Default::default() };
    operation::generate(&world, &op(src, p, a), "").expect("generates")
}

#[test]
fn an_adaptive_fallback_to_offset_rings_is_a_warning() {
    let (t, warns) = channel_job();
    let fell = t.notes.iter().filter(|n| n.contains(" - Offset rings cut ") && n.ends_with(" instead")).count();
    assert_eq!(fell, 1, "the fixture falls back once: {:?}", t.notes);
    let texts: Vec<String> = warns.iter().map(ToString::to_string).collect();
    let fallback: Vec<&camcraft_app::verify::Warning> =
        warns.iter().filter(|w| w.to_string().contains("Offset rings")).collect();
    assert_eq!(fallback.len(), 1, "one fallback warning: {texts:?}\nnotes {:?}", t.notes);
    let text = fallback[0].to_string();
    assert!(text.contains("1 area") && text.contains("Adaptive"), "{text}");
    // #3542: the Adaptive text does not change, byte for byte.
    let at = t.moves.iter().position(|m| m.kind != camcraft_app::cl::MoveKind::Rapid).unwrap_or(0);
    assert_eq!(
        text,
        format!("Adaptive fell back to Offset rings on 1 area from move {at}: the notes name each one and why (the post is not held: the rings are verified)")
    );
    assert!(!fallback[0].blocks_post(), "the rings are verified; the post is not held: {text}");
    assert_eq!(warns.len(), 1, "no other warning: {texts:?}");
}

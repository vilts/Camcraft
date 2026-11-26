//! #2987: the first traverse of a 3+2 operation against the machine
//! table. The tool starts at an unknown X/Y at the tool-change height
//! (the Z travel top; decision premise (b)). When the head tilts, the
//! move along the tool axis to the clearance plane goes down and out; if
//! it can end under the table top, the post is held by name. A
//! horizontal axis (B 90) keeps the Z and never reads the table, and an
//! axis that points down toward the table gets a note. When only the work
//! tilts (a rotary that turns the part), the table stays under the work
//! and is never the higher top; the posed law (#3100) reads the work at
//! every pose the rotaries can leave. A head-tilt machine that states no travel gives a
//! note that names the table as not judged - never a silent pass.
//!
//! Provenance: our own machine files and fixtures; the shop rule "never
//! traverse through uncut stock or steel" (machine-tool handbooks). No
//! vendor material.
//!
//! Gate map (#2987): covers app::operation (first_traverse, generate),
//! app::posting, sim::verify (first_traverse).

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, LoadMachine, SetStock, SetWcsOrigin};
use camcraft_app::geometry::{Point3, IDENTITY_Q};
use camcraft_app::machine::parse_mch;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef, Tilt};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::{model, posting};
use flecs_ecs::prelude::World;

/// The B 30 deg plane through the block's top centre (Z 20).
const B_DEG: f64 = 30.0;
const ORIGIN_Z: f64 = 20.0;

/// How the machine reaches the tilt.
#[derive(Clone, Copy)]
enum Rig {
    /// A and B ride the head (a head hangs below them).
    Head,
    /// A and B turn the work (no head below them).
    Work,
    /// A and B ride the head; the file states no linear travel.
    HeadNoTravel,
}

/// A small 5-axis mill: X travel +-`x_half` mm, Y travel +-100 mm, Z
/// travel 0..300 mm, so the table top is Z 0 - the bottom of the block -
/// and the tool-change height is Z 300.
fn machine_text(rig: Rig, x_half: f64) -> String {
    let t = |lo: f64, hi: f64| match rig {
        Rig::HeadNoTravel => String::new(),
        _ => format!(r#", "travel": {{ "min": {lo}, "max": {hi} }}"#),
    };
    let rotaries = |inner: &str| {
        format!(
            r#"{{ "id": "ra", "name": "A", "type": "rotary", "axis": [1.0, 0.0, 0.0], "travel": {{ "min": -120.0, "max": 120.0 }}, "parts": [
                 {{ "id": "rb", "name": "B", "type": "rotary", "axis": [0.0, 1.0, 0.0], "travel": {{ "min": -120.0, "max": 120.0 }}{inner} }} ] }}"#
        )
    };
    let head = r#"{ "id": "hed", "name": "head", "type": "head", "spindle": { "min_speed": 50.0, "max_speed": 15000.0 } }"#;
    let (z_parts, base_extra) = match rig {
        Rig::Work => (format!(r#", "parts": [ {head} ]"#), format!(", {}", rotaries(""))),
        _ => (format!(r#", "parts": [ {} ]"#, rotaries(&format!(r#", "parts": [ {head} ]"#))), String::new()),
    };
    format!(
        r#"{{
  "general": {{ "model": "TEST 5-AXIS", "vendor": "Camcraft", "kind": "mill" }},
  "kinematics": {{ "default": {{
    "units": {{ "length": "mm", "angle": "degrees" }},
    "parts": [
      {{ "id": "base", "name": "table", "type": "table", "parts": [
        {{ "id": "crl", "name": "X", "type": "linear"{x}, "parts": [
          {{ "id": "sld", "name": "Y", "type": "linear"{y} }} ] }},
        {{ "id": "col", "name": "Z", "type": "linear"{z}, "safe_retract": 0.0{z_parts} }}{base_extra}
      ] }}
    ] }} }},
  "controller": {{ "synced_configuration": {{ "parts": {{
    "X": {{ "coordinate": 0 }}, "Y": {{ "coordinate": 1 }}, "Z": {{ "coordinate": 2 }},
    "A": {{ "coordinate": 3 }}, "B": {{ "coordinate": 4 }}
  }} }} }},
  "tooling": {{ "number_of_tools": 24 }}
}}"#,
        x = t(-x_half, x_half),
        y = t(-100.0, 100.0),
        z = t(0.0, 300.0),
    )
}

/// A 60 x 60 x 20 block (x -30..30, z 0..20) on `rig`, a B `b_deg` face
/// with its clearance plane at world `clear` over the tilt origin.
fn generate(rig: Rig, b_deg: f64, clear: f64) -> (World, OpStatus, Vec<Warning>, Vec<String>) {
    generate_on(rig, 100.0, b_deg, clear)
}

/// [`generate`] on a machine with X travel +-`x_half` mm.
fn generate_on(rig: Rig, x_half: f64, b_deg: f64, clear: f64) -> (World, OpStatus, Vec<Warning>, Vec<String>) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let m = parse_mch(&machine_text(rig, x_half)).unwrap_or_else(|e| panic!("test machine: {e}"));
    h.execute(&world, LoadMachine::new(&world, "test-5axis.mch".into(), Some(m)).expect("machine"));
    if matches!(rig, Rig::Work) {
        // #3100: the posed law reads the work about the WCS origin.
        h.execute(&world, SetWcsOrigin::new(&world, Location { pos: Point3::ZERO, rot: IDENTITY_Q }).expect("G54 on the lines"));
    }
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", t).expect("tool"));
    let p = OpParams::Face {
        holes: Default::default(),
        tilt: Some(Tilt { basis_version: 1, a_deg: 0.0, b_deg, origin: [0.0, 0.0, ORIGIN_Z] }),
        blend_radius: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(clear),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, p).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    let e = model::find_by_name(&world, "Face1").expect("op");
    let status = camcraft_app::operation::status_of(&world, &e);
    let cached = model::toolpath_of(&world, &e).expect("cached path");
    let (warns, notes) = (cached.warnings.clone(), cached.path.notes.clone());
    (world, status, warns, notes)
}

fn low(warns: &[Warning]) -> Option<(f64, f64, bool)> {
    warns.iter().find_map(|w| match *w {
        Warning::FirstTraverseLow { height_mm, top_mm, fixture, .. } => Some((height_mm, top_mm, fixture)),
        _ => None,
    })
}

/// The wide machine's X travel half-width, mm.
const WIDE: f64 = 400.0;

/// The lowest plane over the tilt origin, along the B `b_deg` axis, that
/// a move from every tool-change start (x +-`x_half`, Z 300) reaches over
/// the table top (Z 0): the worst start (x `x_half`, Z 300) reads
/// `x_half` sin B + 280 cos B, and the move ends at Z 300 - (that - h)
/// cos B. B 30: 100 mm wide gives -53.9 (under the block's 15: never
/// the higher top); 400 mm wide gives 96.08.
fn table_clear(x_half: f64, b_deg: f64) -> f64 {
    let b = b_deg.to_radians();
    x_half * b.sin() + (300.0 - ORIGIN_Z) * b.cos() - 300.0 / b.cos()
}

#[test]
fn a_head_tilt_plane_that_the_tool_change_height_clears_passes() {
    // Z 40: 20 mm over the origin - over the block (15). From Z 300 the
    // move along the B 30 axis ends at Z 64 or higher (x +-100): over the
    // table top. Premise (b) passes the plane that #2987 first refused.
    assert!(table_clear(100.0, B_DEG) < 15.0);
    let (_, status, warns, notes) = generate(Rig::Head, B_DEG, 40.0);
    assert_eq!(status, OpStatus::Ok);
    assert_eq!(low(&warns), None, "{warns:?}");
    assert!(!notes.iter().any(|n| n.contains("machine table is not judged")), "{notes:?}");
}

#[test]
fn a_head_tilt_plane_that_a_move_from_the_tool_change_height_takes_under_the_table_is_refused_by_name() {
    // X +-400: from (400, y, 300) the move along the B 30 axis to a plane
    // 20 mm over the origin ends at Z 300 - (442.49 - 20) cos 30 = -48.6,
    // under the table top. Refused by name, with the height to clear.
    // #2991: the verdict gives the two heights in the frame the user sets
    // them in (the local height plus the origin Z), and its text names the
    // raise, not a Z value.
    let want = table_clear(WIDE, B_DEG);
    assert!((want - 96.077).abs() < 1e-3, "{want}");
    let (world, status, warns, _) = generate_on(Rig::Head, WIDE, B_DEG, 40.0);
    assert_eq!(status, OpStatus::Ok);
    let (h_mm, top_mm, fixture) = low(&warns).unwrap_or_else(|| panic!("no first-traverse verdict: {warns:?}"));
    assert!((h_mm - 40.0).abs() < 1e-6, "the clearance height the user set: {h_mm}");
    assert!((top_mm - (want + ORIGIN_Z)).abs() < 1e-6, "the height to clear, in the user's frame: {top_mm}, want {}", want + ORIGIN_Z);
    assert!(fixture, "the table is steel, not stock");
    let err = posting::post_setup_unwritten(&world).expect_err("the program must not post");
    let raise = format!("raise the clearance height by at least {:.3} mm", want - 20.0);
    for text in ["Face1", "first rapid (move 0)", "measured along the tilted tool axis", &raise] {
        assert!(err.contains(text), "{text}: {err}");
    }
}

#[test]
fn a_head_tilt_plane_over_the_height_to_clear_passes() {
    // X +-400, a plane 100 mm over the origin: over the 96.08 to clear.
    let (_, status, warns, notes) = generate_on(Rig::Head, WIDE, B_DEG, ORIGIN_Z + 100.0);
    assert_eq!(status, OpStatus::Ok);
    assert_eq!(low(&warns), None, "{warns:?}");
    assert!(!notes.iter().any(|n| n.contains("machine table is not judged")), "{notes:?}");
    // The narrow machine at Z 60 passes too (its slab, 1e6 mm deep, never counts).
    let (_, _, warns, _) = generate(Rig::Head, B_DEG, 60.0);
    assert_eq!(low(&warns), None, "{warns:?}");
}

#[test]
fn a_work_turning_rotary_keeps_the_table_under_the_work() {
    // The same plane and machine as the refused head-tilt case: the work
    // tilts, and the table stays fixed under it - never the higher top.
    // #3100: the X/Y traverse runs before the rotary words, at the pose
    // the previous program left, so the block reads on its sphere about
    // the WCS origin: the corner (30, 30, 20) stands sqrt(2200) from it,
    // 29.584 over the plane origin along B 30. At the op's own pose the
    // block's top (15) was the only top. A plane 20 over the origin is
    // held by the stock, not the table; 30 over it passes.
    let want = 2200f64.sqrt() - ORIGIN_Z * B_DEG.to_radians().cos();
    let (_, status, warns, notes) = generate_on(Rig::Work, WIDE, B_DEG, 40.0);
    assert_eq!(status, OpStatus::Ok);
    let (h_mm, top_mm, fixture) = low(&warns).unwrap_or_else(|| panic!("no first-traverse verdict: {warns:?}"));
    // #2991: the heights in the user's frame (local + origin Z 20).
    assert!((h_mm - 40.0).abs() < 1e-6, "the clearance the user set: {h_mm}");
    assert!((top_mm - (want + ORIGIN_Z)).abs() < 1e-6 && !fixture, "the block on its sphere: {top_mm}, want {}", want + ORIGIN_Z);
    assert!(warns.iter().any(|w| matches!(w, Warning::FirstTraverseLow { tilted: true, .. })), "a 3+2 verdict is tilted: {warns:?}");
    assert!(!notes.iter().any(|n| n.contains("machine table is not judged")), "{notes:?}");
    let (_, status, warns, _) = generate_on(Rig::Work, WIDE, B_DEG, ORIGIN_Z + 30.0);
    assert_eq!(status, OpStatus::Ok);
    assert_eq!(low(&warns), None, "{warns:?}");
}

#[test]
fn a_head_tilt_machine_with_no_travel_names_the_table_unjudged() {
    let (_, status, warns, notes) = generate(Rig::HeadNoTravel, B_DEG, 40.0);
    assert_eq!(status, OpStatus::Ok);
    assert_eq!(low(&warns), None, "no table box, so no table verdict: {warns:?}");
    let note = notes.iter().find(|n| n.contains("machine table is not judged")).unwrap_or_else(|| panic!("no note: {notes:?}"));
    assert!(note.contains("does not state all of the X, Y and Z travel"), "{note}");
    assert!(note.contains("judged against the stock and the fixtures only"), "{note}");
}

fn under(warns: &[Warning]) -> Option<(f64, f64)> {
    warns.iter().find_map(|w| match *w {
        Warning::FirstTraverseUnderTable { height_mm, limit_mm, .. } => Some((height_mm, limit_mm)),
        _ => None,
    })
}

/// The highest plane over the tilt origin, along a B `b_deg` axis past
/// 90 deg (it points down toward the table), that a move from every
/// tool-change start (x +-`x_half`, Z 300) reaches over the table top
/// (Z 0): the worst start (x -`x_half`, Z 300) reads `-x_half` sin B +
/// 280 cos B, and the move from it drops (h - that) * -cos B. B 120 on
/// the wide machine: -486.41 + 300 / 0.5 = 113.59.
fn table_limit(x_half: f64, b_deg: f64) -> f64 {
    let b = b_deg.to_radians();
    -x_half * b.sin() + (300.0 - ORIGIN_Z) * b.cos() + 300.0 / -b.cos()
}

#[test]
fn a_head_tilt_past_90_under_the_table_is_refused_with_lower() {
    // B 120: the axis (sin 120, 0, cos 120) points down toward the table,
    // so a HIGHER plane along it stands LOWER in the world. At 150 (over
    // the 113.59 limit) the move from the tool-change height can go under
    // the table top: a verdict refuses the post and says "lower", never
    // "raise" (#3081). The stock top along the axis (35.98) stays under
    // the plane, so no FirstTraverseLow verdict comes with it.
    let want = table_limit(WIDE, 120.0);
    assert!((want - 113.590).abs() < 1e-3, "{want}");
    let (world, _, warns, notes) = generate_on(Rig::Head, WIDE, 120.0, ORIGIN_Z + 150.0);
    assert_eq!(low(&warns), None, "{warns:?}");
    let (h_mm, limit_mm) = under(&warns).unwrap_or_else(|| panic!("no table verdict past 90 deg: {warns:?}"));
    // #3148: the heights in the user's frame (local + origin Z 20): the
    // plane the user set (170) and the limit 133.590, not the local 150
    // and 113.590.
    let user_limit = want + ORIGIN_Z;
    assert!((h_mm - (ORIGIN_Z + 150.0)).abs() < 1e-6, "the clearance the user set: {h_mm}");
    assert!((limit_mm - user_limit).abs() < 1e-6, "the plane to lower to: {limit_mm}, want {user_limit}");
    assert!((user_limit - 133.590).abs() < 1e-3, "{user_limit}");
    let verdict = warns.iter().find(|w| matches!(w, Warning::FirstTraverseUnderTable { .. })).copied().unwrap_or_else(|| panic!("{warns:?}"));
    assert!(verdict.blocks_post(), "the table verdict must hold the post");
    assert!(!notes.iter().any(|n| n.contains("machine table is not judged")), "a verdict, not a note: {notes:?}");
    // The message names the repair: lower, never raise, in the user's
    // values along the tilted axis, never as a Z value.
    let text = verdict.to_string();
    for want_text in [
        "first rapid (move 0)",
        "points down toward the machine table",
        "clearance plane at 170.000 mm",
        "lower the clearance plane to 133.590 mm or under it",
        "measured along the tilted tool axis",
    ] {
        assert!(text.contains(want_text), "{want_text}: {text}");
    }
    assert!(!text.contains("raise"), "a down-pointing axis must not say raise: {text}");
    assert!(!text.contains(" Z1"), "a height along a tilted axis is no Z value: {text}");
    // This rig's B 120 face also cuts under the Z stroke, so the machine
    // limit law refuses the post first; the post must not pass either way.
    posting::post_setup_unwritten(&world).expect_err("the program must not post");
}

#[test]
fn a_head_tilt_past_90_plane_under_the_limit_passes() {
    // B 120, X +-400: planes at 100 and 113 (under the 113.59 limit) keep
    // the move from the tool-change height over the table top.
    for h in [100.0, 113.0] {
        let (_, _, warns, notes) = generate_on(Rig::Head, WIDE, 120.0, ORIGIN_Z + h);
        assert_eq!(low(&warns), None, "{h}: {warns:?}");
        assert_eq!(under(&warns), None, "{h}: {warns:?}");
        assert!(!notes.iter().any(|n| n.contains("machine table is not judged")), "{h}: {notes:?}");
    }
}

#[test]
fn a_horizontal_head_axis_does_not_read_the_far_table_edge() {
    // B 90: the axis is world +X. The table's far edge (x 400) reads 400
    // along it, but a move along a horizontal axis keeps the world Z, so
    // the tool never goes under the table top. A plane 40 mm over the
    // origin clears the block (30 along X) and passes, with no note.
    let (_, _, warns, notes) = generate_on(Rig::Head, WIDE, 90.0, ORIGIN_Z + 40.0);
    assert_eq!(low(&warns), None, "the table's far edge was read along a horizontal axis: {warns:?}");
    assert!(!notes.iter().any(|n| n.contains("machine table is not judged")), "{notes:?}");
}

#[test]
fn a_near_horizontal_head_axis_passes_from_the_tool_change_height() {
    // B 80: the axis has a small up component (cos 80 = 0.17). From Z 300
    // the move along it to a plane 40 mm over the origin drops at most
    // (400 sin 80 + 280 cos 80 - 40) cos 80 = 69.9 mm: it ends near Z 230,
    // far over the table top. A near-horizontal axis needs a 1/cos B
    // longer move to reach the table, so the table does not count.
    assert!(table_clear(WIDE, 80.0) < 0.0);
    let (_, _, warns, notes) = generate_on(Rig::Head, WIDE, 80.0, ORIGIN_Z + 40.0);
    assert_eq!(low(&warns), None, "{warns:?}");
    assert!(!notes.iter().any(|n| n.contains("machine table is not judged")), "{notes:?}");
}

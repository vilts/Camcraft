//! #532 (part of #290): the Wire Contour operation through the
//! commands - the wire EDM's family door both ways, the condition door
//! at command creation (a missing condition, gap or wire diameter is
//! refused by name; no library tool rides the op), a die and a punch
//! on one profile standing apart by exactly the kerf, the corner
//! minimum and the start hole refused by name at generation, the
//! stamp reading the machine's condition, undo / redo, the machine
//! check, the post's refusal, and the project round trip.
//!
//! Gate map (#532): covers app::operation (wire_op, kinds registry,
//! family gate, stamp, stamp_tool, tool_gate), app::commands (operation
//! create/edit/generate gates), app::machine::wire_edm, app::project
//! (wire_edm_dto, validate), app::posting (family door), motion::wire_edm.
//! Provenance: our own tests over our own fixtures. The conditions are
//! SYNTHETIC - invented numbers of a plausible size, from no machine's
//! manual (#290 D4, D10); the wire rules they pin are cited in
//! `crates/motion/src/wire_edm/mod.rs`.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    CreateOperation, EditOperationLinks, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{self, check_document, parse_mch, Kind};
use camcraft_app::model;
use camcraft_app::operation::{
    self, Family, Height, KindInfo, OpParams, OpStatus, OpType, Section, SrcRef, WireContourOp, WireSide,
};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::world::new_world;

const WIRE: &str = include_str!("edm/wire-edm.mch");

/// Synthetic: a complete rough condition, one with no gap, one with no
/// wire diameter.
const TABLE: &str = r#"[
    { "id": "C110", "name": "rough", "gap": 0.04, "wire_diameter": 0.25, "role": "rough" },
    { "id": "330", "wire_diameter": 0.25, "role": "finish" },
    { "id": "S9", "gap": 0.02 }
]"#;
/// Wire radius 0.125 + gap 0.04.
const K: f64 = 0.165;

fn machine_text(conds: &str) -> String {
    let mut v: serde_json::Value = serde_json::from_str(WIRE).expect("template json");
    v["edm"]["conditions"] = serde_json::from_str(conds).expect("conditions json");
    v.to_string()
}

fn mount(world: &World, h: &mut History, text: &str) {
    let m = parse_mch(text).expect("parses");
    h.execute(world, LoadMachine::new(world, "wire.mch".into(), Some(m)).expect("mount"));
}

/// A `w` x `h` outline centred on the origin, corners of radius `r` in
/// 3-degree facets (`r` = 0: four sharp corners), counter-clockwise.
fn outline(w: f64, h: f64, r: f64) -> Vec<(f64, f64)> {
    let (hx, hy) = (w * 0.5 - r, h * 0.5 - r);
    let mut pts = Vec::new();
    for (cx, cy, a0) in [(hx, hy, 0.0_f64), (-hx, hy, 90.0), (-hx, -hy, 180.0), (hx, -hy, 270.0)] {
        if r == 0.0 {
            pts.push((cx, cy));
            continue;
        }
        for s in 0..=30 {
            let t = (a0 + 3.0 * f64::from(s)).to_radians();
            pts.push((cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    pts
}

/// The outline extruded from z = 0 to z = 12: a slug whose
/// through-silhouette is the outline.
fn prism(pts: &[(f64, f64)]) -> Mesh {
    let (z0, z1) = (0.0, 12.0);
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut f = Vec::new();
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        f.push(Triangle::new(Point3::new(0.0, 0.0, z1), p(a, z1), p(b, z1)));
        f.push(Triangle::new(Point3::new(0.0, 0.0, z0), p(b, z0), p(a, z0)));
        f.push(Triangle::new(p(a, z0), p(b, z0), p(b, z1)));
        f.push(Triangle::new(p(a, z0), p(b, z1), p(a, z1)));
    }
    Mesh::from_facets(f)
}

fn wire_op(side: WireSide, start: Option<[f64; 2]>) -> WireContourOp {
    WireContourOp {
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side,
        start,
        tabs: None,
        capture: false,
        taper_deg: 0.0,
        four_axis: None,
        burnout: None,
        clearance: operation::DEFAULT_CLEARANCE,
        skims: Vec::new(),
    }
}

/// A wire EDM document: the template machine with the synthetic table
/// mounted, a 40 x 20 slug with 2 mm corners, and a sharp 40 x 20 one.
fn wire_doc() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    mount(&world, &mut h, &machine_text(TABLE));
    h.execute(&world, ImportModel::from_mesh("slug", prism(&outline(40.0, 20.0, 2.0))));
    h.execute(&world, ImportModel::from_mesh("sharp", prism(&outline(40.0, 20.0, 0.0))));
    (world, h)
}

fn create(world: &World, name: &str, model: &str, op: WireContourOp) -> Result<CreateOperation, String> {
    CreateOperation::new(world, name, "", SrcRef::Entity(model.into()), OpParams::WireContour(op)).map_err(|e| e.to_string())
}

fn path(world: &World, name: &str) -> camcraft_app::cl::Toolpath {
    let e = model::find_by_name(world, name).expect("op");
    model::toolpath_of(world, &e).expect("generated").path
}

fn status(world: &World, name: &str) -> OpStatus {
    operation::status_of(world, &model::find_by_name(world, name).expect("op"))
}

/// Where the path crosses x = 0 highest.
fn top_at_x0(p: &camcraft_app::cl::Toolpath) -> f64 {
    p.moves
        .iter()
        .filter_map(|m| {
            let (a, b) = (m.from, m.to);
            ((a.x <= 0.0) != (b.x <= 0.0)).then(|| a.y + (b.y - a.y) * (0.0 - a.x) / (b.x - a.x))
        })
        .fold(f64::NEG_INFINITY, f64::max)
}

#[test]
fn the_wire_family_lives_on_a_wire_edm_only_and_mounts_no_tool() {
    let info = OpType::WireContour.info();
    assert_eq!((info.family(), info.section, info.label), (Family::WireEdm, Section::WireEdm, "Wire Contour"));
    assert!(info.tool_kinds.is_empty() && !info.mounts_tool(), "the wire is the machine's condition, not a library tool");
    assert_eq!(KindInfo::ALL[41].ty, OpType::WireContour, "appended: stored kind ids never move");
    assert_eq!(operation::shown_for(Kind::WireEdm), vec![Section::WireEdm]);
    assert_eq!(Section::WireEdm.kinds_for(Kind::WireEdm), vec![OpType::WireContour.ordinal()]);
    for m in [Kind::Mill, Kind::Lathe, Kind::MillTurn, Kind::Plasma, Kind::SinkerEdm] {
        assert!(!operation::shown_for(m).contains(&Section::WireEdm), "{m:?}");
        assert!(!info.admits(m), "{m:?}");
    }
    // At the door: a wire cut on a mill (no machine mounted) is refused by name.
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("slug", prism(&outline(40.0, 20.0, 2.0))));
    let err = create(&world, "W", "slug", wire_op(WireSide::Die, Some([0.0, 0.0]))).err().expect("refused on a mill");
    assert!(err.contains("wire EDM work") && err.contains("mount a wire EDM first"), "{err}");
    // And a library tool named on a wire cut is refused: no second source for the wire.
    let (world, mut h) = wire_doc();
    let em = camcraft_app::tools::catalog().remove(0);
    h.execute(&world, camcraft_app::commands::AddTool::new(&world, em.0, em.1).expect("tool"));
    let err = CreateOperation::new(&world, "W", em.0, SrcRef::Entity("slug".into()), OpParams::WireContour(wire_op(WireSide::Die, None)))
        .err()
        .expect("a tool on a wire cut")
        .to_string();
    assert!(err.contains("mounts no library tool"), "{err}");
    h.execute(&world, create(&world, "W", "slug", wire_op(WireSide::Die, None)).expect("no tool, a stated condition"));
    let err = EditOperationLinks::new(&world, "W", Some(em.0), None).err().expect("no tool can be given later").to_string();
    assert!(err.contains("mounts no library tool"), "{err}");
    let e = model::find_by_name(&world, "W").expect("op");
    assert_eq!(operation::tool_gate::tool_problem_of(&world, &e), None, "naming no tool is no problem for a wire cut");
}

/// The issue's refusal at command creation: a missing condition, gap or
/// wire diameter never reaches a generator.
#[test]
fn a_missing_condition_gap_or_wire_diameter_is_refused_at_command_creation() {
    // The template machine states no condition at all.
    let world = new_world();
    let mut h = History::default();
    mount(&world, &mut h, WIRE);
    h.execute(&world, ImportModel::from_mesh("slug", prism(&outline(40.0, 20.0, 2.0))));
    let err = create(&world, "W", "slug", wire_op(WireSide::Die, None)).err().expect("no table");
    assert!(err.contains("states no EDM condition \"C110\"") && err.contains("it states none"), "{err}");

    let (world, mut h) = wire_doc();
    let with = |id: &str| WireContourOp { condition: id.into(), ..wire_op(WireSide::Die, Some([0.0, 0.0])) };
    let none = create(&world, "W", "slug", with("")).err().expect("no condition named");
    assert!(none.contains("names no EDM condition") && none.contains("it states C110, 330, S9"), "{none}");
    let unknown = create(&world, "W", "slug", with("C999")).err().expect("no such condition");
    assert!(unknown.contains("states no EDM condition \"C999\"") && unknown.contains("C110, 330, S9"), "{unknown}");
    let gapless = create(&world, "W", "slug", with("330")).err().expect("no gap");
    assert!(gapless.contains("\"330\" states no gap"), "{gapless}");
    let wireless = create(&world, "W", "slug", with("S9")).err().expect("no wire diameter");
    assert!(wireless.contains("\"S9\" states no wire_diameter"), "{wireless}");
    assert!(model::find_by_name(&world, "W").is_none(), "nothing entered the document");
    // The same door on an edit, and on the parameter set's own sanity.
    h.execute(&world, create(&world, "W", "slug", with("C110")).expect("a complete condition"));
    let err = EditOperationParams::new(&world, "W", OpParams::WireContour(with("330"))).err().expect("edit refused").to_string();
    assert!(err.contains("states no gap"), "{err}");
    let nan = WireContourOp { start: Some([f64::NAN, 0.0]), ..with("C110") };
    assert!(EditOperationParams::new(&world, "W", OpParams::WireContour(nan)).is_err());
}

/// Done means: a die and a punch on the same profile offset to opposite
/// sides by exactly the kerf.
#[test]
fn a_die_and_a_punch_on_one_profile_stand_apart_by_exactly_the_kerf() {
    let (world, mut h) = wire_doc();
    h.execute(&world, create(&world, "Die1", "slug", wire_op(WireSide::Die, Some([0.0, 0.0]))).expect("die"));
    h.execute(&world, create(&world, "Punch1", "slug", wire_op(WireSide::Punch, Some([-30.0, 0.0]))).expect("punch"));
    for name in ["Die1", "Punch1"] {
        h.execute(&world, GenerateOperation::new(&world, name).expect("generates"));
        assert_eq!(status(&world, name), OpStatus::Ok);
    }
    let (die, punch) = (path(&world, "Die1"), path(&world, "Punch1"));
    let (yd, yp) = (top_at_x0(&die), top_at_x0(&punch));
    assert!((yd - (10.0 - K)).abs() < 1e-6, "the die's wire runs inside, at {yd}");
    assert!((yp - (10.0 + K)).abs() < 1e-6, "the punch's wire runs outside, at {yp}");
    assert!(((yp - yd) - 0.33).abs() < 1e-6, "apart by the kerf width, 0.25 + 2 x 0.04: {}", yp - yd);
    // D2: the lower programmed plane, no pose, no feed - it posts as XY.
    for p in [&die, &punch] {
        assert!(p.is_prismatic() && p.moves.iter().all(|m| m.axis.is_none() && m.feed.is_none()));
        assert!(p.moves.iter().all(|m| m.from.z == 0.0 && m.to.z == 0.0), "the workpiece's bottom");
    }
    assert!(die.notes.iter().any(|n| n.contains("die side") && n.contains("condition C110")), "{:?}", die.notes);
    assert!(punch.notes.iter().any(|n| n.contains("punch side") && n.contains("kerf width 0.3300")), "{:?}", punch.notes);
    // The machine check judges them on the wire EDM: its own work, and said so.
    let (_, m) = machine::of_setup(&world, None);
    let c = check_document(&world, &m.expect("the wire EDM")).expect("checks");
    assert!(c.problems.is_empty(), "{:?}", c.problems);
    assert!(c.notes.iter().any(|n| n.contains("judged at the post by the wire laws")), "{:?}", c.notes);
    // #533: each streams as a wire section, its condition riding for
    // the wire post (which alone voices it - `wire_post.rs`); the wire
    // laws (#534) stand between the stream and NC text (`wire_verify.rs`).
    let streams = posting::op_streams(&world).expect("the wire sections stream");
    let cond = |r: &camcraft_app::post::cnc::Record| {
        matches!(r, camcraft_app::post::cnc::Record::Param { name, value } if name == "EdmCondition" && value == "C110")
    };
    assert!(streams.iter().any(|(n, rs)| n == "Die1" && rs.iter().any(cond)), "{streams:?}");
}

/// Done means: the too-tight corner and the missing start hole are
/// refused by name.
#[test]
fn a_too_tight_corner_and_a_missing_start_hole_are_refused_by_name() {
    let (world, mut h) = wire_doc();
    // A sharp 40 x 20 opening: four inside corners under wire radius + gap.
    h.execute(&world, create(&world, "Sharp", "sharp", wire_op(WireSide::Die, Some([0.0, 0.0]))).expect("op"));
    let err = GenerateOperation::new(&world, "Sharp").err().expect("refused").to_string();
    assert!(err.contains("inside corner") && err.contains("minimum inside radius is 0.1650 mm"), "{err}");
    assert!(err.contains("wire radius 0.1250 + spark gap 0.0400"), "{err}");
    // The same sharp profile as a punch has only outside corners: cut.
    h.execute(&world, create(&world, "SharpPunch", "sharp", wire_op(WireSide::Punch, Some([-30.0, 0.0]))).expect("op"));
    h.execute(&world, GenerateOperation::new(&world, "SharpPunch").expect("a sharp punch is cut"));
    // No start hole stated: refused by name, for either side.
    for (name, side) in [("NoHoleDie", WireSide::Die), ("NoHolePunch", WireSide::Punch)] {
        h.execute(&world, create(&world, name, "slug", wire_op(side, None)).expect("op"));
        let err = GenerateOperation::new(&world, name).err().expect("refused").to_string();
        assert!(err.contains("no start hole is stated") && err.contains("threaded through solid material"), "{err}");
    }
    // A start hole on the product's side is refused too.
    h.execute(&world, create(&world, "Outside", "slug", wire_op(WireSide::Die, Some([-30.0, 0.0]))).expect("op"));
    let err = GenerateOperation::new(&world, "Outside").err().expect("refused").to_string();
    assert!(err.contains("lies outside the profile"), "{err}");
}

/// The wire and the gap are the machine's: an edited condition table
/// re-marks the cut Stale, and a removed condition says why.
#[test]
fn the_stamp_reads_the_machines_condition() {
    let (world, mut h) = wire_doc();
    h.execute(&world, create(&world, "Die1", "slug", wire_op(WireSide::Die, Some([0.0, 0.0]))).expect("die"));
    h.execute(&world, GenerateOperation::new(&world, "Die1").expect("generates"));
    assert_eq!(status(&world, "Die1"), OpStatus::Ok);
    // The same id, a wider gap: the path on screen is no longer this cut.
    mount(&world, &mut h, &machine_text(r#"[ { "id": "C110", "gap": 0.05, "wire_diameter": 0.25 } ]"#));
    assert_eq!(status(&world, "Die1"), OpStatus::Stale);
    h.undo(&world);
    assert_eq!(status(&world, "Die1"), OpStatus::Ok, "the old table back, the path is the cut again");
    // The condition gone from the table: Stale, by name, and no regenerate.
    mount(&world, &mut h, &machine_text(r#"[ { "id": "C200", "gap": 0.04, "wire_diameter": 0.25 } ]"#));
    let e = model::find_by_name(&world, "Die1").expect("op");
    let why = operation::stale_reason(&world, &e).expect("stale");
    assert!(why.contains("states no EDM condition \"C110\"") && why.contains("C200"), "{why}");
    let err = GenerateOperation::new(&world, "Die1").err().expect("refused").to_string();
    assert!(err.contains("states no EDM condition \"C110\""), "{err}");
}

#[test]
fn create_and_edit_undo_and_redo() {
    let (world, mut h) = wire_doc();
    let p0 = OpParams::WireContour(wire_op(WireSide::Die, None));
    h.execute(&world, create(&world, "W", "slug", wire_op(WireSide::Die, None)).expect("op"));
    let p1 = OpParams::WireContour(wire_op(WireSide::Punch, Some([-30.0, 0.0])));
    h.execute(&world, EditOperationParams::new(&world, "W", p1.clone()).expect("edit"));
    let params = |w: &World| model::find_by_name(w, "W").and_then(|e| model::operation_of(w, &e)).map(|o| o.params);
    assert_eq!(params(&world), Some(p1.clone()));
    assert!(h.undo(&world));
    assert_eq!(params(&world), Some(p0.clone()));
    assert!(h.undo(&world));
    assert_eq!(params(&world), None, "the create is undone");
    assert!(h.redo(&world) && h.redo(&world));
    assert_eq!(params(&world), Some(p1));
    let op = model::find_by_name(&world, "W").and_then(|e| model::operation_of(&world, &e)).expect("op");
    assert_eq!(op.tool, "", "a wire cut names no tool");
}

/// The op rides the project file; its side is never defaulted by a
/// reader, and a wire cut that names a tool is a corrupt file.
#[test]
fn the_op_round_trips_and_its_side_is_never_defaulted() {
    let (world, mut h) = wire_doc();
    h.execute(&world, create(&world, "Punch1", "slug", wire_op(WireSide::Punch, Some([-30.0, 0.0]))).expect("punch"));
    h.execute(&world, GenerateOperation::new(&world, "Punch1").expect("generates"));
    let file = std::env::temp_dir().join(format!("camcraft-532-{}.ccam", std::process::id()));
    project::save(&world, &file).expect("save");
    let back = new_world();
    let loaded = project::load(&back, &file);
    let _ = std::fs::remove_file(&file);
    loaded.expect("load");
    let op = model::find_by_name(&back, "Punch1").and_then(|e| model::operation_of(&back, &e)).expect("op");
    assert_eq!(op.params, OpParams::WireContour(wire_op(WireSide::Punch, Some([-30.0, 0.0]))));
    assert_eq!(op.tool, "");
    assert_eq!(path(&back, "Punch1").moves, path(&world, "Punch1").moves);
    assert_eq!(status(&back, "Punch1"), OpStatus::Ok, "the loaded machine states the same condition");
    // The JSON door the file and the MCP patch share.
    let json = project::params_to_json(&op.params);
    assert_eq!(json["wire_contour"]["side"], "punch");
    assert_eq!(json["wire_contour"]["condition"], "C110");
    assert_eq!(project::params_from_json(&json).expect("reads back"), op.params);
    let mut sideless = json.clone();
    sideless["wire_contour"].as_object_mut().expect("fields").remove("side");
    let err = project::params_from_json(&sideless).expect_err("the side is stated, never defaulted");
    assert!(err.contains("side"), "{err}");
    let mut startless = json;
    startless["wire_contour"].as_object_mut().expect("fields").remove("start");
    let OpParams::WireContour(w) = project::params_from_json(&startless).expect("an unstated start hole reads") else {
        panic!("not a wire contour");
    };
    assert_eq!(w.start, None);
}

/// #1109: the open run's fields - the kerf side left / right of travel
/// and the run-out clearance - ride EditOperationParams (undoable) and
/// the JSON door the file and the MCP patch share; a closed profile
/// with a left / right side is refused by name at generation, and an
/// insane clearance at the command door.
#[test]
fn the_open_run_fields_ride_the_commands_and_the_json_door() {
    let (world, mut h) = wire_doc();
    let p0 = wire_op(WireSide::Die, Some([0.0, 0.0]));
    h.execute(&world, create(&world, "W", "slug", p0.clone()).expect("op"));
    let p1 = WireContourOp { side: WireSide::Right, clearance: 2.5, ..p0.clone() };
    h.execute(&world, EditOperationParams::new(&world, "W", OpParams::WireContour(p1.clone())).expect("edit"));
    let params = |w: &World| model::find_by_name(w, "W").and_then(|e| model::operation_of(w, &e)).map(|o| o.params);
    assert_eq!(params(&world), Some(OpParams::WireContour(p1.clone())));
    let why = GenerateOperation::new(&world, "W").err().expect("refused").to_string();
    assert!(why.contains("profile is closed") && why.contains("die or punch"), "{why}");
    assert!(h.undo(&world));
    assert_eq!(params(&world), Some(OpParams::WireContour(p0.clone())));
    let bad = WireContourOp { clearance: f64::NAN, ..p1.clone() };
    assert!(EditOperationParams::new(&world, "W", OpParams::WireContour(bad)).is_err(), "a NaN clearance is refused");
    let json = project::params_to_json(&OpParams::WireContour(p1.clone()));
    assert_eq!(json["wire_contour"]["side"], "right");
    assert_eq!(json["wire_contour"]["clearance"], 2.5);
    assert_eq!(project::params_from_json(&json).expect("reads back"), OpParams::WireContour(p1));
    // A closed cut's file names no clearance: it reads and writes as before.
    let closed = project::params_to_json(&OpParams::WireContour(p0));
    assert!(closed["wire_contour"].get("clearance").is_none(), "{closed}");
}

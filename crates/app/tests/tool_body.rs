//! #368 acceptance: a tool body (a holder mesh placed in the tool frame)
//! is judged by its revolved envelope. A body wider than the holder
//! cylinder at one band turns a move the cylinder passed into a named
//! fixture collision; no body is byte-identical to before; the body
//! saves, loads, undoes, redoes, dies with its tool, and refuses
//! nonsense by name.
//!
//! Gate map (M4.4): covers app::commands, app::tool_body, app::operation, app::posting, app::project, app::schema, app::fixture, sim::fixture, sim::verify, tools::tools.

use camcraft_app::commands::{
    AddTool, ChangeTool, CommandError, CreateOperation, DeleteEntity, DeleteTool, GenerateOperation, History,
    SetStock, SetToolBody, TagFixture, ToolBodySpec,
};
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tool_body;
use camcraft_app::tools::{catalog, Tool, ToolKind};
use camcraft_app::verify::{FixturePart, Warning};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-0400-2FL";
const CUT_Z: f64 = 19.5;

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A closed `n`-gon prism of circumradius `r` about the model Z axis over
/// `[z0, z1]` - our own test holder.
fn prism(r: f64, z0: f64, z1: f64, n: usize) -> Vec<Triangle> {
    let q = |k: usize, z: f64| {
        let a = std::f64::consts::TAU * k as f64 / n as f64;
        p(r * a.cos(), r * a.sin(), z)
    };
    let mut v = Vec::new();
    for k in 0..n {
        let (a0, a1, b0, b1) = (q(k, z0), q(k + 1, z0), q(k, z1), q(k + 1, z1));
        v.push(Triangle::new(a0, a1, b1));
        v.push(Triangle::new(a0, b1, b0));
        v.push(Triangle::new(p(0.0, 0.0, z0), a1, a0));
        v.push(Triangle::new(p(0.0, 0.0, z1), b0, b1));
    }
    v
}

/// The shank's top above the tip for the catalog tool (flute + shank).
fn shank_top() -> f64 {
    let t = catalog_tool(TOOL);
    t.flute_len + t.shank_len
}

/// A holder body: a 40 mm-radius collar over the holder band, wider
/// than the tool's own 22 mm holder cylinder there.
fn wide_body() -> ToolBodySpec {
    let z0 = shank_top();
    ToolBodySpec { source: "collar.stl".into(), facets: prism(40.0, z0, z0 + 35.0, 32), axis: [0.0, 0.0, 1.0], offset: 0.0 }
}

/// Stock, the catalog end mill and a generated face op over it.
fn face_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"),
    );
    history.execute(&world, AddTool::new(&world, TOOL, catalog_tool(TOOL)).expect("tool"));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Face1",
            TOOL,
            SrcRef::Stock,
            OpParams::Face {
                holes: Default::default(),
                tilt: None,
                blend_turns: false,
                stepover: 3.0,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(CUT_Z),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        )
        .expect("face op"),
    );
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, history)
}

/// A post standing 30 mm off the face path's +X edge, up in the holder
/// band: outside the 22 mm holder cylinder, inside the 40 mm collar.
fn plant_post(world: &World, history: &mut History) {
    plant_post_at(world, history, CUT_Z + shank_top() + 10.0, 10.0);
}

/// [`plant_post`] at world z `zb` up, `h` tall.
fn plant_post_at(world: &World, history: &mut History, zb: f64, h: f64) {
    let op = model::find_by_name(world, "Face1").expect("Face1");
    let cache = model::toolpath_of(world, &op).expect("generated");
    let xmax = cache.path.moves.iter().flat_map(|m| [m.from.x, m.to.x]).fold(f64::NEG_INFINITY, f64::max);
    let (y0, y1) = cache
        .path
        .moves
        .iter()
        .flat_map(|m| [m.from.y, m.to.y])
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), y| (a.min(y), b.max(y)));
    let (min, max) = (p(xmax + 30.0, y0, zb), p(xmax + 34.0, y1, zb + h));
    let e = model::spawn(world, "Post1");
    model::set_facets(world, &e, vec![Triangle::new(min, max, p(min.x, max.y, max.z))].into());
    model::set_rev(world, &e, 1);
    history.execute(world, TagFixture::new(world, "Post1", None).expect("tagged"));
}

fn holder_hits(world: &World) -> usize {
    let op = model::find_by_name(world, "Face1").expect("Face1");
    let cache = model::toolpath_of(world, &op).expect("generated");
    cache
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::FixtureHit { fixture_part: FixturePart::Holder, .. }))
        .count()
}

fn status(world: &World) -> OpStatus {
    operation::status_of(world, &model::find_by_name(world, "Face1").expect("Face1"))
}

fn stamp(world: &World) -> Option<u64> {
    let op = model::operation_of(world, &model::find_by_name(world, "Face1").expect("Face1")).expect("an op");
    operation::current_stamp(world, &op.tool, &op.src, &op.params, "Face1", op.setup.as_deref()).map(|g| g.0)
}

fn tool_entity(world: &World) -> flecs_ecs::prelude::Entity {
    model::find_by_name(world, TOOL).expect("tool")
}

#[test]
fn a_collar_wider_than_the_holder_cylinder_turns_a_clear_move_into_a_named_collision() {
    let (world, mut history) = face_world();
    plant_post(&world, &mut history);
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    assert_eq!(holder_hits(&world), 0, "the 22 mm holder cylinder passes 30 mm off the post");
    assert!(
        !posting::verification_gate(&world).iter().any(|s| s.contains("strikes a fixture")),
        "the gate passes the cylinder"
    );

    history.execute(&world, SetToolBody::new(&world, TOOL, Some(wide_body())).expect("a sane body"));
    assert_eq!(status(&world), OpStatus::Stale, "a body re-marks its operations");
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    assert!(holder_hits(&world) > 0, "the 40 mm collar strikes the post");
    let gate = posting::verification_gate(&world);
    assert!(
        gate.iter().any(|s| s.starts_with("Face1:") && s.contains("holder") && s.contains("strikes a fixture")),
        "the gate names op and holder: {gate:?}"
    );

    // Undo the generation and the body: the cylinder's verdict returns.
    assert!(history.undo(&world) && history.undo(&world));
    assert!(tool_body::body_of(&world, &tool_entity(&world)).is_none());
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    assert_eq!(holder_hits(&world), 0, "undo restores the cylinder");
}

#[test]
fn the_fixture_laws_read_the_bands_not_the_enclosing_cylinder() {
    // A 15 mm nose over the first 20 mm of the holder band, a 40 mm
    // flange above it; the post stands 30 mm off the path beside the
    // nose only. The bands pass it; the one-cylinder stand-in (40 mm
    // the whole way) would not - the profile is what the laws judge.
    let (world, mut history) = face_world();
    plant_post_at(&world, &mut history, CUT_Z + shank_top() + 2.0, 6.0);
    let z0 = shank_top();
    let mut facets = prism(15.0, z0, z0 + 20.0, 24);
    facets.extend(prism(40.0, z0 + 20.0, z0 + 35.0, 24));
    let stepped = ToolBodySpec { source: "stepped.step".into(), facets, axis: [0.0, 0.0, 1.0], offset: 0.0 };
    history.execute(&world, SetToolBody::new(&world, TOOL, Some(stepped)).expect("a sane body"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    assert_eq!(holder_hits(&world), 0, "the nose band passes the post");
    let (judged, _) = tool_body::judged(&world, &tool_entity(&world), catalog_tool(TOOL)).expect("fits");
    assert!(judged.holder_d >= 80.0, "while the enclosing cylinder is the flange's: {}", judged.holder_d);
}

#[test]
fn no_body_reads_byte_identical_and_clearing_a_body_restores_the_stamp() {
    let (world, mut history) = face_world();
    let before = stamp(&world);
    let path_before = model::toolpath_of(&world, &model::find_by_name(&world, "Face1").expect("op")).expect("path");
    assert_eq!(status(&world), OpStatus::Ok);
    history.execute(&world, SetToolBody::new(&world, TOOL, Some(wide_body())).expect("a sane body"));
    assert_ne!(stamp(&world), before, "a body moves the stamp");
    history.execute(&world, SetToolBody::new(&world, TOOL, None).expect("clear"));
    assert_eq!(stamp(&world), before, "no body, no digest bytes");
    assert_eq!(status(&world), OpStatus::Ok);
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    let path_after = model::toolpath_of(&world, &model::find_by_name(&world, "Face1").expect("op")).expect("path");
    assert_eq!(path_before.path.moves, path_after.path.moves);
    assert_eq!(path_before.warnings, path_after.warnings);
}

#[test]
fn undo_restores_and_redo_reapplies_the_body() {
    let (world, mut history) = face_world();
    history.execute(&world, SetToolBody::new(&world, TOOL, Some(wide_body())).expect("a sane body"));
    let set = tool_body::body_of(&world, &tool_entity(&world)).expect("landed");
    assert_eq!(set.source, "collar.stl");
    assert!(set.profile().expect("a spindle body").reach_over(shank_top() + 10.0, shank_top() + 10.0) >= 40.0 * (std::f64::consts::PI / 32.0).cos());
    // Replace with a slimmer one, then walk the history both ways.
    let z0 = shank_top();
    let slim = ToolBodySpec { source: "slim.step".into(), facets: prism(15.0, z0, z0 + 20.0, 16), axis: [0.0, 0.0, 1.0], offset: 0.0 };
    history.execute(&world, SetToolBody::new(&world, TOOL, Some(slim)).expect("a sane body"));
    let b = |w: &World| tool_body::body_of(w, &tool_entity(w)).map(|b| b.source);
    assert_eq!(b(&world).as_deref(), Some("slim.step"));
    assert!(history.undo(&world));
    assert_eq!(tool_body::body_of(&world, &tool_entity(&world)), Some(set.clone()));
    assert!(history.undo(&world));
    assert_eq!(b(&world), None);
    assert!(history.redo(&world));
    assert_eq!(b(&world).as_deref(), Some("collar.stl"));
    assert!(history.redo(&world));
    assert_eq!(b(&world).as_deref(), Some("slim.step"));
}

#[test]
fn the_body_round_trips_through_the_project_file() {
    let (world, mut history) = face_world();
    history.execute(&world, SetToolBody::new(&world, TOOL, Some(wide_body())).expect("a sane body"));
    let path = std::env::temp_dir().join(format!("camcraft_tool_body_{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "save-load-save is byte-identical");
    assert!(first.contains("\"body\""));
    assert_eq!(
        tool_body::body_of(&world2, &tool_entity(&world2)),
        tool_body::body_of(&world, &tool_entity(&world)),
        "facets, placement and the re-derived envelope all agree"
    );
    // A tool without a body writes no key at all.
    history.execute(&world, SetToolBody::new(&world, TOOL, None).expect("clear"));
    let mut text = serde_json::to_string(&project::collect(&world)).expect("json");
    assert!(!text.contains("\"body\""));
    // A file whose body is no body refuses by name.
    let bad = first.replacen("\"source\": \"collar.stl\"", "\"source\": \"collar.stl\", \"axis_unused\": 0", 1);
    assert!(project::open(&new_world(), &bad).is_ok(), "unknown keys inside the body are ignored like everywhere");
    let v: serde_json::Value = serde_json::from_str(&first).expect("json");
    let mut v2 = v.clone();
    for e in v2["entities"].as_array_mut().expect("entities") {
        if let Some(b) = e.get_mut("tool").and_then(|t| t.get_mut("body")) {
            b["facets"] = serde_json::json!([]);
        }
    }
    text = serde_json::to_string(&v2).expect("json");
    let err = project::open(&new_world(), &text).expect_err("an empty body is refused");
    assert!(err.to_string().contains("no facets"), "{err}");
}

#[test]
fn deleting_the_tool_frees_the_body_and_undo_brings_it_back() {
    let (world, mut history) = face_world();
    history.execute(&world, SetToolBody::new(&world, TOOL, Some(wide_body())).expect("a sane body"));
    let body = tool_body::body_of(&world, &tool_entity(&world));
    history.execute(&world, DeleteTool::new(&world, TOOL).expect("delete"));
    assert!(model::find_by_name(&world, TOOL).is_none(), "the entity and every component go");
    assert!(history.undo(&world));
    assert_eq!(tool_body::body_of(&world, &tool_entity(&world)), body);
    history.execute(&world, DeleteEntity::new(&world, TOOL).expect("tree delete"));
    assert!(model::find_by_name(&world, TOOL).is_none());
    assert!(history.undo(&world));
    assert_eq!(tool_body::body_of(&world, &tool_entity(&world)), body);
}

#[test]
fn nonsense_bodies_refuse_by_name_before_the_history_sees_them() {
    let (world, mut history) = face_world();
    let refuse = |spec: ToolBodySpec, says: &str| match SetToolBody::new(&world, TOOL, Some(spec)) {
        Err(CommandError::Invalid(m)) => assert!(m.contains(says), "{m:?} should say {says:?}"),
        Err(e) => panic!("wrong refusal {e}"),
        Ok(_) => panic!("accepted a body that should say {says:?}"),
    };
    let base = wide_body;
    refuse(ToolBodySpec { facets: Vec::new(), ..base() }, "no facets");
    let mut nan = base();
    nan.facets[0].a.x = f64::NAN;
    refuse(nan, "non-finite");
    refuse(ToolBodySpec { axis: [0.0, 0.0, 0.0], ..base() }, "axis direction");
    refuse(ToolBodySpec { offset: f64::INFINITY, ..base() }, "tip offset");
    let shift = |q: Point3| p(q.x + 100.0, q.y, q.z);
    let off: Vec<Triangle> = base().facets.iter().map(|t| Triangle::new(shift(t.a), shift(t.b), shift(t.c))).collect();
    refuse(ToolBodySpec { facets: off, ..base() }, "does not surround the tool axis");
    refuse(ToolBodySpec { offset: -shank_top() - 1.0, ..base() }, "below the tool tip");
    refuse(ToolBodySpec { offset: -shank_top() + 5.0, ..base() }, "reaches into the flutes");
    refuse(ToolBodySpec { axis: [1.0, 0.0, 0.0], ..base() }, "below the tool tip");
    let flat = vec![Triangle::new(p(-1.0, -1.0, 60.0), p(1.0, -1.0, 60.0), p(0.0, 1.0, 60.0))];
    refuse(ToolBodySpec { facets: flat, ..base() }, "no length along the tool axis");
    let line = vec![Triangle::new(p(-1.0, -1.0, 60.0), p(0.0, 0.0, 70.0), p(1.0, 1.0, 80.0))];
    refuse(ToolBodySpec { facets: line, ..base() }, "degenerate");
    assert!(matches!(SetToolBody::new(&world, "nope", Some(base())), Err(CommandError::NotFound(_))));
    assert!(matches!(SetToolBody::new(&world, "Face1", Some(base())), Err(CommandError::Invalid(_))));
    // A lathe insert takes no body in this slice.
    let insert = catalog().into_iter().find(|(_, t)| t.kind == ToolKind::Turning).map(|(_, t)| t).expect("an insert");
    history.execute(&world, AddTool::new(&world, "T1", insert).expect("insert"));
    let r = SetToolBody::new(&world, "T1", Some(base()));
    assert!(matches!(r, Err(CommandError::Invalid(ref m)) if m.contains("lathe holders")), "{:?}", r.err());

    // And an edit that would stand the body over the flutes is refused.
    history.execute(&world, SetToolBody::new(&world, TOOL, Some(base())).expect("a sane body"));
    let longer = Tool { flute_len: shank_top() + 1.0, ..catalog_tool(TOOL) };
    let r = ChangeTool::new(&world, TOOL, longer);
    assert!(matches!(r, Err(CommandError::Invalid(ref m)) if m.contains("reaches into the flutes")), "{:?}", r.err());
}

#[test]
fn a_body_reads_from_an_stl_and_is_placed_by_axis_and_offset() {
    // Modelled lying along model +X from x = 0 to 30, its origin to stand
    // 60 mm above the tip: in the tool frame it runs z 60..90.
    let tris: Vec<Triangle> = prism(12.0, 0.0, 30.0, 12)
        .into_iter()
        .map(|t| {
            let m = |q: Point3| p(q.z, q.y, -q.x);
            Triangle::new(m(t.a), m(t.b), m(t.c))
        })
        .collect();
    let mut stl = String::from("solid body\n");
    for t in &tris {
        stl.push_str("facet normal 0 0 0\nouter loop\n");
        for v in [t.a, t.b, t.c] {
            stl.push_str(&format!("vertex {} {} {}\n", v.x, v.y, v.z));
        }
        stl.push_str("endloop\nendfacet\n");
    }
    stl.push_str("endsolid body\n");
    let path = std::env::temp_dir().join(format!("camcraft_tool_body_{}.stl", std::process::id()));
    std::fs::write(&path, stl).expect("write stl");
    let (world, mut history) = face_world();
    let cmd = SetToolBody::from_file(&world, TOOL, &path, [1.0, 0.0, 0.0], 60.0);
    let _ = std::fs::remove_file(&path);
    history.execute(&world, cmd.expect("a sane body"));
    let b = tool_body::body_of(&world, &tool_entity(&world)).expect("landed");
    assert!(b.source.ends_with(".stl"));
    let profile = b.profile().expect("a spindle body");
    assert!((profile.bottom() - 60.0).abs() < 1e-6 && (profile.top() - 90.0).abs() < 1e-6, "{profile:?}");
    assert!(profile.bands().iter().all(|band| band.r <= 12.0 + 1e-5));
    // The enclosing cylinder every one-cylinder law reads.
    let (judged, _) = tool_body::judged(&world, &tool_entity(&world), catalog_tool(TOOL)).expect("fits");
    assert!((judged.holder_len - (90.0 - shank_top())).abs() < 1e-9);
    assert!(judged.holder_d >= 24.0 - 1e-9 && judged.holder_d <= 24.0 + 1e-4);
}

fn testdata(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models")
        .join(name)
}

#[test]
fn step_with_tool_coordinate_systems_aligns_automatically() {
    let path = testdata("tool_csys_box_iso13399.step");
    let spec = ToolBodySpec::from_step_auto(&path).expect("spec from step with CIP");
    assert_eq!(spec.axis, [0.0, 0.0, 1.0]);
    assert!((spec.offset - 75.0).abs() < 1e-9, "offset is 75 mm from CIP origin (0, 0, -75)");

    let (world, mut history) = face_world();
    let cmd = SetToolBody::from_step_auto(&world, TOOL, &path).expect("SetToolBody from step auto");
    history.execute(&world, cmd);

    let b = tool_body::body_of(&world, &tool_entity(&world)).expect("body landed on tool");
    assert_eq!(b.source, "tool_csys_box_iso13399.step");
    let profile = b.profile().expect("profile envelope");
    // Model box z was [-40, 0], with offset 75 => tool frame z is [35, 75].
    assert!((profile.bottom() - 35.0).abs() < 1e-6, "foot at 35 mm: {}", profile.bottom());
    assert!((profile.top() - 75.0).abs() < 1e-6, "top at 75 mm: {}", profile.top());

    // Undo removes the body, redo restores it.
    history.undo(&world);
    assert!(tool_body::body_of(&world, &tool_entity(&world)).is_none());
    history.redo(&world);
    assert!(tool_body::body_of(&world, &tool_entity(&world)).is_some());
}

#[test]
fn step_without_tool_coordinate_systems_falls_back_by_name() {
    let path = testdata("box.step");
    let err = ToolBodySpec::from_step_auto(&path).expect_err("no csys falls back");
    assert_eq!(err, "the file names no coordinate systems");

    let (world, _) = face_world();
    let cmd_err = SetToolBody::from_step_auto(&world, TOOL, &path).expect_err("command refuses by name");
    match cmd_err {
        CommandError::Invalid(msg) => assert!(msg.contains("the file names no coordinate systems"), "{msg}"),
        other => panic!("expected CommandError::Invalid, got {other:?}"),
    }
}

#[test]
fn step_with_degenerate_coordinate_systems_falls_back_by_name() {
    let text = std::fs::read_to_string(testdata("tool_csys_box_iso13399.step"))
        .expect("fixture reads")
        .replace(
            "#220 = AXIS2_PLACEMENT_3D('CIP', #221, #202, #203);",
            "#220 = AXIS2_PLACEMENT_3D('CIP', #221, #300, #203);\n#300 = DIRECTION('', (0.0, 0.0, 0.0));",
        );
    let path = std::env::temp_dir().join(format!("camcraft_degenerate_{}.step", std::process::id()));
    std::fs::write(&path, text).expect("write temp step");
    let err = ToolBodySpec::from_step_auto(&path).expect_err("degenerate csys falls back");
    let _ = std::fs::remove_file(&path);
    assert!(
        err.contains("axis has no direction") || err.contains("every ISO 13399 coordinate system was refused"),
        "{err}"
    );
}

#[test]
fn tool_body_view_load_aligned_handles_step_csys_and_fallback() {
    use camcraft_app::tool_body_view;

    let path_aligned = testdata("tool_csys_box_iso13399.step");
    let loaded = tool_body_view::load_aligned(&path_aligned).expect("loads");
    assert_eq!(loaded.source, "tool_csys_box_iso13399.step");
    assert!(!loaded.facets.is_empty());
    let (axis, offset) = loaded.alignment.expect("aligned");
    assert_eq!(axis, [0.0, 0.0, 1.0]);
    assert!((offset - 75.0).abs() < 1e-9);

    let path_none = testdata("box.step");
    let loaded_none = tool_body_view::load_aligned(&path_none).expect("loads");
    let err = loaded_none.alignment.expect_err("no csys fallback");
    assert_eq!(err, "the file names no coordinate systems");
}

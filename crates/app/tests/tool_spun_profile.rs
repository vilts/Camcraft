//! #93 acceptance: a milling tool's holder drawn as a spun (revolved)
//! profile lands as the tool's body through one undoable command: its
//! bands are exactly the drawn steps, a drawn collar is judged by the
//! fixture laws, it saves/loads/undoes, refuses nonsense by name, and
//! the parametric profile changes nothing for an existing tool.
//!
//! Gate map (M4.4): covers app::commands, app::tool_body, app::tool_body_view, app::project, app::schema, tools::tools.

use camcraft_app::commands::{AddTool, CommandError, CreateOperation, GenerateOperation, History, SetStock, SetToolBody, TagFixture};
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::spun::{SpunPoint, SpunProfile};
use camcraft_app::stock::StockShape;
use camcraft_app::tool_body;
use camcraft_app::tool_body_view;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::verify::{FixturePart, Warning};
use camcraft_app::world::new_world;
use camcraft_tools::holder_profile::{HolderBand, HolderStack};
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-0400-2FL";
const CUT_Z: f64 = 19.5;

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

fn shank_top() -> f64 {
    let t = catalog_tool(TOOL);
    t.flute_len + t.shank_len
}

fn pt(z: f64, r: f64) -> SpunPoint {
    SpunPoint::line(z, r)
}

/// A drawn holder: a 15 mm nose over 20 mm, then a 40 mm flange over
/// 15 mm, on the shank's top.
fn drawn_collar() -> Vec<SpunPoint> {
    let z0 = shank_top();
    vec![pt(z0, 0.0), pt(z0, 15.0), pt(z0 + 20.0, 15.0), pt(z0 + 20.0, 40.0), pt(z0 + 35.0, 40.0), pt(z0 + 35.0, 0.0)]
}

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

fn tool_entity(world: &World) -> flecs_ecs::prelude::Entity {
    model::find_by_name(world, TOOL).expect("tool")
}

fn stamp(world: &World) -> Option<u64> {
    let op = model::operation_of(world, &model::find_by_name(world, "Face1").expect("Face1")).expect("an op");
    operation::current_stamp(world, &op.tool, &op.src, &op.params, "Face1", op.setup.as_deref()).map(|g| g.0)
}

/// A post 30 mm off the path's +X edge, `zb` up, `h` tall.
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
    let (min, max) = (Point3::new(xmax + 30.0, y0, zb), Point3::new(xmax + 34.0, y1, zb + h));
    let e = model::spawn(world, "Post1");
    model::set_facets(world, &e, vec![Triangle::new(min, max, Point3::new(min.x, max.y, max.z))].into());
    model::set_rev(world, &e, 1);
    history.execute(world, TagFixture::new(world, "Post1", None).expect("tagged"));
}

fn holder_hits(world: &World) -> usize {
    let op = model::find_by_name(world, "Face1").expect("Face1");
    let cache = model::toolpath_of(world, &op).expect("generated");
    cache.warnings.iter().filter(|w| matches!(w, Warning::FixtureHit { fixture_part: FixturePart::Holder, .. })).count()
}

#[test]
fn a_drawn_holder_lands_with_its_bands_exact() {
    let (world, mut history) = face_world();
    history.execute(&world, SetToolBody::spun(&world, TOOL, Some(drawn_collar())).expect("a sane profile"));
    let body = tool_body::body_of(&world, &tool_entity(&world)).expect("landed");
    assert_eq!(body.source, tool_body::SPUN_SOURCE);
    assert_eq!(body.spun.as_ref().map(|s| s.points().to_vec()), Some(drawn_collar()));
    let z0 = shank_top();
    let profile = body.profile().expect("a spindle-frame body");
    assert_eq!(
        profile.bands(),
        &[HolderBand { above: z0, len: 20.0, r: 15.0 }, HolderBand { above: z0 + 20.0, len: 15.0, r: 40.0 }]
    );
    let g = catalog_tool(TOOL).geom();
    assert_eq!(tool_body::stack(&g, Some(profile)), HolderStack { bands: profile.bands().to_vec(), top: z0 + 35.0, neck: Vec::new() });
    // The display assembly carries the revolve of the drawn outline.
    let asm = tool_body_view::assembly(&catalog_tool(TOOL), &body);
    let top = asm.iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).fold(0.0f64, f64::max);
    assert!((top - (z0 + 35.0)).abs() < 1e-9, "the assembly tops out at the drawn top: {top}");
}

#[test]
fn a_drawn_flange_is_judged_by_the_fixture_laws() {
    let (world, mut history) = face_world();
    // Beside the flange (z0 + 20 .. z0 + 35), 30 mm off the path: the
    // 22 mm holder cylinder passes, the drawn 40 mm flange strikes.
    plant_post_at(&world, &mut history, CUT_Z + shank_top() + 25.0, 5.0);
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    assert_eq!(holder_hits(&world), 0, "the cylinder passes");
    history.execute(&world, SetToolBody::spun(&world, TOOL, Some(drawn_collar())).expect("a sane profile"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    assert!(holder_hits(&world) > 0, "the drawn flange strikes the post");
}

#[test]
fn the_parametric_profile_changes_nothing() {
    let (world, mut history) = face_world();
    let before = stamp(&world);
    // Parametric = the tool's own holder cylinder: its drawn profile
    // stacks bit-identically, and Apply in Parametric mode is a clear,
    // which leaves no digest bytes behind.
    let t = catalog_tool(TOOL);
    let h = SpunProfile::holder_of(&t).expect("the catalog tool has a holder");
    assert_eq!(h.holder_profile().stack(&t.geom()), HolderStack::cylinder(&t.geom()));
    history.execute(&world, SetToolBody::spun(&world, TOOL, Some(drawn_collar())).expect("custom"));
    assert_ne!(stamp(&world), before, "a custom profile moves the stamp");
    history.execute(&world, SetToolBody::spun(&world, TOOL, None).expect("back to parametric"));
    assert_eq!(stamp(&world), before, "parametric again: byte-identical stamp");
    assert!(tool_body::body_of(&world, &tool_entity(&world)).is_none());
}

/// A flange drawn out to r 35 and straight back to the axis at the top
/// (no height of its own) is judged at r 35: the envelope the laws read
/// is never smaller than the drawn outline (#475 hands-on finding).
#[test]
fn a_flat_top_flange_is_judged_at_its_full_radius() {
    let (world, mut history) = face_world();
    let z0 = shank_top();
    let pts = vec![pt(z0, 0.0), pt(z0, 25.0), pt(z0 + 35.0, 25.0), pt(z0 + 35.0, 35.0), pt(z0 + 35.0, 0.0)];
    history.execute(&world, SetToolBody::spun(&world, TOOL, Some(pts)).expect("a sane profile"));
    let (judged, body) = tool_body::judged(&world, &tool_entity(&world), catalog_tool(TOOL)).expect("judged");
    let body = body.expect("a spindle-frame body");
    assert!(judged.holder_d >= 70.0 - 1e-9, "the judged holder is Ø{} - the drawn flange is Ø70", judged.holder_d);
    let wide = tool_body::stack(&catalog_tool(TOOL).geom(), Some(&body)).bands.iter().fold(0.0f64, |m, b| m.max(b.r));
    assert!(wide >= 35.0 - 1e-9, "the swept stack reaches r {wide} - the drawn flange is r 35");
}

#[test]
fn undo_and_redo_walk_the_drawn_profile() {
    let (world, mut history) = face_world();
    let b = |w: &World| tool_body::body_of(w, &tool_entity(w)).and_then(|b| b.spun).map(|s| s.points().len());
    history.execute(&world, SetToolBody::spun(&world, TOOL, Some(drawn_collar())).expect("custom"));
    let mut slim = drawn_collar();
    slim.drain(3..5);
    history.execute(&world, SetToolBody::spun(&world, TOOL, Some(slim)).expect("slimmer"));
    assert_eq!(b(&world), Some(4));
    assert!(history.undo(&world));
    assert_eq!(b(&world), Some(6));
    assert!(history.undo(&world));
    assert_eq!(b(&world), None);
    assert!(history.redo(&world) && history.redo(&world));
    assert_eq!(b(&world), Some(4));
}

#[test]
fn nonsense_profiles_refuse_by_name_at_the_command() {
    let (world, _history) = face_world();
    let refuse = |pts: Vec<SpunPoint>, says: &str| match SetToolBody::spun(&world, TOOL, Some(pts)) {
        Err(CommandError::Invalid(m)) => assert!(m.contains(says), "{m:?} should say {says:?}"),
        Err(e) => panic!("wrong refusal {e}"),
        Ok(_) => panic!("accepted a profile that should say {says:?}"),
    };
    let mut open = drawn_collar();
    open.pop();
    refuse(open, "must end on the axis");
    let mut back = drawn_collar();
    back[2].z = shank_top() - 5.0;
    refuse(back, "point 2 runs back down");
    let mut nan = drawn_collar();
    nan[3].r = f64::NAN;
    refuse(nan, "point 4 is not a finite");
    // Into the flutes: a holder whose foot is below the flute top.
    refuse(vec![pt(1.0, 0.0), pt(1.0, 10.0), pt(30.0, 10.0), pt(30.0, 0.0)], "reaches into the flutes");
    // Turning tools are not spun profiles.
    let world2 = new_world();
    let lathe = catalog().into_iter().find(|(_, t)| t.kind == camcraft_app::tools::ToolKind::Turning).expect("an insert");
    let mut h = History::default();
    h.execute(&world2, AddTool::new(&world2, lathe.0, lathe.1.clone()).expect("tool"));
    match SetToolBody::spun(&world2, lathe.0, Some(drawn_collar())) {
        Err(CommandError::Invalid(m)) => assert!(m.contains("lathe"), "{m}"),
        _ => panic!("a turning tool takes no spun holder"),
    }
}

#[test]
fn the_drawn_profile_round_trips_and_an_old_reader_over_reports() {
    let (world, mut history) = face_world();
    let mut pts = drawn_collar();
    pts[1].arc = 30.0; // a convex flank on the nose
    history.execute(&world, SetToolBody::spun(&world, TOOL, Some(pts)).expect("custom"));
    let path = std::env::temp_dir().join(format!("camcraft_spun_{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert!(first.contains("\"spun\""));
    assert_eq!(first, second, "save-load-save is byte-identical");
    let landed = tool_body::body_of(&world, &tool_entity(&world)).expect("landed");
    assert_eq!(tool_body::body_of(&world2, &tool_entity(&world2)).as_ref(), Some(&landed));
    // A reader that drops `spun` judges the written facets: their ring
    // vertices sit on the (conservative) outline, so the envelope never
    // reads less than the drawn outline at any of its heights.
    let mut v: serde_json::Value = serde_json::from_str(&first).expect("json");
    for e in v["entities"].as_array_mut().expect("entities") {
        if let Some(b) = e.get_mut("tool").and_then(|t| t.get_mut("body")).and_then(|b| b.as_object_mut()) {
            b.remove("spun");
        }
    }
    let world3 = new_world();
    project::open(&world3, &serde_json::to_string(&v).expect("json")).expect("facets alone load");
    let old = tool_body::body_of(&world3, &tool_entity(&world3)).expect("a body");
    assert!(old.spun.is_none());
    let outline = landed.spun.as_ref().expect("drawn").outline();
    let old_profile = old.profile().expect("a spindle-frame body");
    for &(z, r) in &outline {
        assert!(old_profile.reach_over(z, z) >= r - 1e-6, "at {z}: {} < {r}", old_profile.reach_over(z, z));
    }
}

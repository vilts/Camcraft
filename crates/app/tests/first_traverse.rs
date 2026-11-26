//! #2901: the first traverse of a milling operation comes from an X/Y
//! the machine does not know after a tool change, so its clearance
//! plane must stand over the whole start stock and every fixture, in the
//! operation's own frame. A clamp that stands nowhere near the path but
//! over the plane holds the post by name; the same clamp under the plane
//! posts; a 3+2 plane reads the stock's top along its tilted axis; and
//! the simulation opens over the entry on the clearance plane, never at
//! the WCS origin.
//!
//! The stock case (a clearance plane Z 8 under a stock top Z 10) is
//! pinned by `stale_rapid_law::regenerating_the_old_cache_judges_it_under_the_new_law`.
//!
//! Provenance: our own fixtures; the shop rule "never traverse through
//! uncut stock or steel" (machine-tool handbooks). No vendor material.
//!
//! Gate map (#2901): covers app::operation (first_traverse, generate),
//! app::posting, sim::verify (first_traverse), sim::sim.

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock, TagFixture};
use camcraft_app::geometry::{cube, IDENTITY_Q, Point3};
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef, Tilt};
use camcraft_app::schema::Location;
use camcraft_app::sim::{script, Traverse};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::{model, posting};
use flecs_ecs::prelude::World;

const CLEAR: f64 = 30.0;

fn face(clear: f64, tilt: Option<Tilt>, cut: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt,
        blend_radius: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(clear),
        cut_z: Height::fixed(cut),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

/// A 40 x 40 x 10 block centred on (60, 40), a 4 mm end mill.
fn world() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let at = Location { pos: Point3::new(60.0, 40.0, 0.0), rot: IDENTITY_Q };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 10.0 }, at).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", t).expect("tool"));
    (world, h)
}

fn generate(world: &World, h: &mut History, p: OpParams) -> (OpStatus, Toolpath, Vec<Warning>) {
    h.execute(world, CreateOperation::new(world, "Face1", "EM-0400-2FL", SrcRef::Stock, p).expect("create"));
    h.execute(world, GenerateOperation::new(world, "Face1").expect("generate"));
    let e = model::find_by_name(world, "Face1").expect("op");
    let cached = model::toolpath_of(world, &e).expect("cached path");
    (camcraft_app::operation::status_of(world, &e), cached.path.clone(), cached.warnings.clone())
}

/// A clamp of `top` mm far off the stock and off every move: the old
/// seat's line, the path and the links never come near it.
fn far_clamp(world: &World, h: &mut History, top: f64) {
    let clamp = model::spawn(world, "Clamp1");
    model::set_facets(world, &clamp, cube(Point3::new(150.0, 150.0, 0.0), Point3::new(160.0, 160.0, top)).facets.to_vec().into());
    model::set_rev(world, &clamp, 1);
    h.execute(world, TagFixture::new(world, "Clamp1", None).expect("tagged"));
}

fn post(world: &World, tag: &str) -> Result<String, String> {
    let out = std::env::temp_dir().join(format!("camcraft_first_traverse_{tag}_{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let r = posting::post_setup(world, &out).map(|p| p.program);
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    if r.is_err() {
        assert!(!wrote, "{tag}: a refused post writes nothing");
    }
    r
}

#[test]
fn a_far_clamp_over_the_clearance_plane_holds_the_post_by_name() {
    let (world, mut h) = world();
    far_clamp(&world, &mut h, 40.0);
    let (status, path, warns) = generate(&world, &mut h, face(CLEAR, None, 9.0));
    assert_eq!(status, OpStatus::Ok);
    let m = path.moves[0];
    assert!((m.from.x - 150.0).abs() > 20.0 && m.from.x < 100.0, "the path stays far from the clamp: {m:?}");
    let want = Warning::FirstTraverseLow { at: 0, height_mm: CLEAR, top_mm: 40.0, fixture: true, tilted: false };
    assert_eq!(warns, vec![want], "only the first-traverse verdict: no struck move, no routed link");
    let err = posting::post_setup_unwritten(&world).expect_err("the program must not post");
    for text in ["Face1", "clearance height Z30.000", "under the fixture top at Z40.000", "raise the clearance height above Z40.000"] {
        assert!(err.contains(text), "{text}: {err}");
    }
    assert!(post(&world, "far_clamp").is_err());
}

#[test]
fn the_same_clamp_under_the_clearance_plane_posts() {
    let (world, mut h) = world();
    let (_, bare_path, _) = generate(&world, &mut h, face(CLEAR, None, 9.0));
    let bare = post(&world, "bare").unwrap_or_else(|e| panic!("the bare program posts: {e}"));
    far_clamp(&world, &mut h, 25.0);
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    let e = model::find_by_name(&world, "Face1").expect("op");
    let cached = model::toolpath_of(&world, &e).expect("cached path");
    assert!(cached.warnings.is_empty(), "{:?}", cached.warnings);
    assert_eq!(cached.path.moves, bare_path.moves);
    let clamped = post(&world, "low_clamp").unwrap_or_else(|e| panic!("the program posts under the low clamp: {e}"));
    assert_eq!(clamped, bare, "a low clamp changed the program");
}

/// A 60 x 60 x 20 block (x -30..30) indexed B 10 deg about its top centre: along the
/// tilted axis its high corner stands 30 sin 10 = 5.209 mm over the
/// plane's zero. A clearance 5 mm over that zero (Z 25) is under it.
/// #2991: the verdict names the clearance the user set (25, not the
/// local 5) and the corner in the same frame (25.209), says that both
/// are measured along the tilted tool axis, and names the raise - no Z
/// value the user cannot find on the operation.
#[test]
fn a_3_2_plane_reads_the_stock_top_along_its_tool_axis() {
    let tilt = Tilt { basis_version: 1, a_deg: 0.0, b_deg: 10.0, origin: [0.0, 0.0, 20.0] };
    let top = 30.0 * 10f64.to_radians().sin();
    for (clear, refused) in [(25.0, true), (30.0, false)] {
        let world = camcraft_app::world::new_world();
        let mut h = History::default();
        h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"));
        let b = model::stock_world_aabb_for(&world, None).expect("stock box");
        assert!((b.min.x + 30.0).abs() < 1e-9 && (b.max.x - 30.0).abs() < 1e-9 && (b.max.z - 20.0).abs() < 1e-9, "{b:?}");
        let t = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
        h.execute(&world, AddTool::new(&world, "EM-0400-2FL", t).expect("tool"));
        let (status, path, warns) = generate(&world, &mut h, face(clear, Some(tilt), 19.5));
        assert_eq!(status, OpStatus::Ok);
        let low = warns.iter().find_map(|w| match *w {
            Warning::FirstTraverseLow { at, height_mm, top_mm, fixture, tilted } => Some((at, height_mm, top_mm, fixture, tilted)),
            _ => None,
        });
        if refused {
            let (at, h_mm, top_mm, fixture, tilted) = low.unwrap_or_else(|| panic!("clearance Z{clear}: no verdict in {warns:?}; the path opens {:?}", path.moves.first()));
            assert_eq!((at, fixture, tilted), (0, false, true));
            assert!((h_mm - clear).abs() < 1e-6, "the clearance the user set: {h_mm}");
            assert!((top_mm - (top + 20.0)).abs() < 1e-6, "the tilted block's high corner in the same frame: {top_mm}, want {}", top + 20.0);
            let text = warns.iter().find(|w| matches!(w, Warning::FirstTraverseLow { .. })).map(ToString::to_string).unwrap_or_default();
            assert!(text.contains("at the clearance height 25.000 mm, under the stock top at 25.209 mm"), "{text}");
            assert!(text.contains("measured along the tilted tool axis"), "{text}");
            assert!(text.ends_with("raise the clearance height by at least 0.209 mm"), "{text}");
            assert!(!text.contains('Z') && !text.contains("X/Y"), "no G-code words: {text}");
            let refusal = posting::post_setup_unwritten(&world).expect_err("the low first traverse holds the post");
            assert!(refusal.to_string().contains("raise the clearance height by at least 0.209 mm"), "{refusal}");
        } else {
            assert_eq!(low, None, "clearance Z{clear}: {warns:?}");
        }
    }
}

/// The simulation's first tool position is the path's own start: over
/// the entry, on the clearance plane - not the WCS origin, not the frame
/// zero the generator seeds at.
#[test]
fn the_simulation_opens_over_the_entry_on_the_clearance_plane() {
    let (world, mut h) = world();
    let (_, path, _) = generate(&world, &mut h, face(CLEAR, None, 9.0));
    let entry = path.moves[0].to;
    let sc = script([("Face1", &path)], &Traverse::default());
    let at = sc.at(0.0).pos;
    assert!((at.x - entry.x).abs() < 1e-9 && (at.y - entry.y).abs() < 1e-9 && (at.z - CLEAR).abs() < 1e-9, "the sim opens at {at:?}, entry {entry:?}");
    assert!(at.x.abs() > 1.0 || at.y.abs() > 1.0, "the sim opens at the frame zero: {at:?}");
}

//! #316: the rigid transform between setups is derived from their part
//! alignments (#326) - one transform per setup, never a second one - and
//! a setup may be placed relative to another ("flip over X from
//! Setup1"), so a later change of that setup's alignment carries into it
//! in the same undo unit, or is refused by name. The work offset is never
//! read or touched; links round-trip through save/load (v56).
//! Provenance: our own fixtures (a 40 x 40 x 20 block) over plain
//! re-clamp practice; no vendor material consulted.
//!
//! Gate map (#316): covers app::setup_relative, app::commands
//! (SetSetupRelative, AlignPartToSetup, CreateSetup, DeleteSetup),
//! app::project (save, load, open, setup_links).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::core::EntityViewGet;
use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AlignPartToSetup, CreateSetup, DeleteSetup, History, ImportModel, SetSetupRelative, SetStock,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::schema::{Location, SetupDef};
use camcraft_app::setup_align::{self, AlignPreset};
use camcraft_app::setup_relative::{self, carry_point, transform_between};
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;
use camcraft_app::{model, project, setup};

/// A corner of the part's top face; the part's centre is (0, 0, 10).
const CORNER: Point3 = Point3 { x: -20.0, y: -20.0, z: 20.0 };

/// The block, a 60 x 60 x 22 stock, Setup1 (materialized) and Setup2,
/// Setup3 (datum transfers from Setup1, own stocks).
fn document() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let part = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 20.0));
    h.execute(&world, ImportModel::from_mesh("part", part));
    let at = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 22.0 }, at).expect("stock"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup3"));
    (world, h)
}

fn close(a: Point3, b: Point3) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9 && (a.z - b.z).abs() < 1e-9
}

fn carry(world: &World, from: &str, to: &str, p: Point3) -> Point3 {
    carry_point(world, from, to, p).expect("rigid setups")
}

fn preset(world: &World, h: &mut History, s: &str, p: AlignPreset) {
    h.execute(world, AlignPartToSetup::preset(world, s, p).expect("align"));
}

fn link(world: &World, h: &mut History, s: &str, parent: &str, deg: [f64; 3]) {
    h.execute(world, SetSetupRelative::link(world, s, parent, deg).expect("link"));
}

fn link_of(world: &World, s: &str) -> Option<String> {
    setup_relative::relative_to(world, s)
}

#[test]
fn transform_between_reads_identity_flip_and_quarter_turn() {
    let (world, mut h) = document();
    assert!(setup_relative::is_still(transform_between(&world, "Setup1", "Setup1").expect("self")));
    // A datum transfer starts in the source's frame.
    assert!(setup_relative::is_still(transform_between(&world, "Setup1", "Setup2").expect("clone")));
    preset(&world, &mut h, "Setup2", AlignPreset::FlipX);
    preset(&world, &mut h, "Setup3", AlignPreset::TurnZ90);
    let flip = transform_between(&world, "Setup1", "Setup2").expect("flip");
    assert_eq!(setup_align::preset_of(flip.rot), Some(AlignPreset::FlipX));
    // Flip over X about the centre: (x, y, z) -> (x, -y, 20 - z).
    assert!(close(carry(&world, "Setup1", "Setup2", CORNER), Point3::new(-20.0, 20.0, 0.0)));
    // A quarter turn about Z: (x, y) -> (-y, x), Z kept.
    assert!(close(carry(&world, "Setup1", "Setup3", CORNER), Point3::new(20.0, -20.0, 20.0)));
    // The work offsets were never read nor moved: still G54, still zero.
    for s in ["Setup1", "Setup2", "Setup3"] {
        assert_eq!(setup::wcs_slot(&world, Some(s)), 0, "{s}");
    }
}

#[test]
fn transforms_compose_and_invert() {
    let (world, mut h) = document();
    preset(&world, &mut h, "Setup1", AlignPreset::TurnZ90);
    preset(&world, &mut h, "Setup2", AlignPreset::FlipX);
    h.execute(&world, AlignPartToSetup::angles(&world, "Setup3", [0.0, 30.0, 45.0]).expect("custom"));
    let p = Point3::new(3.0, -7.0, 11.0);
    let direct = carry(&world, "Setup1", "Setup3", p);
    let via = carry(&world, "Setup2", "Setup3", carry(&world, "Setup1", "Setup2", p));
    assert!(close(direct, via), "A->C equals A->B then B->C: {direct:?} vs {via:?}");
    let back = carry(&world, "Setup2", "Setup1", carry(&world, "Setup1", "Setup2", p));
    assert!(close(back, p), "B->A undoes A->B: {back:?}");
    let ab = transform_between(&world, "Setup1", "Setup2").expect("ab");
    let ba = transform_between(&world, "Setup2", "Setup1").expect("ba");
    assert!(setup_relative::same(Some(ab), Some(setup_align::inverse(ba))));
}

#[test]
fn transform_between_refuses_by_name() {
    let (world, _h) = document();
    let err = transform_between(&world, "Setup1", "Nope").expect_err("missing");
    assert!(err.contains("no setup named 'Nope'"), "{err}");
    let err = transform_between(&world, "part", "Setup1").expect_err("a model");
    assert!(err.contains("'part' is not a setup"), "{err}");
    // A stretched rotation cannot arise through the commands or the loader;
    // poked in directly, it is refused, never read as the identity.
    let e = model::find_by_name(&world, "Setup2").expect("Setup2");
    let mut def = e.entity_view(&world).try_get::<&SetupDef>(Clone::clone).expect("def");
    def.part_alignment = Some(Location { pos: Point3::ZERO, rot: [2.0, 0.0, 0.0, 0.0] });
    e.entity_view(&world).set(def);
    let err = transform_between(&world, "Setup1", "Setup2").expect_err("not rigid");
    assert!(err.contains("not a rigid transform"), "{err}");
}

#[test]
fn a_linked_setup_resolves_its_turn_into_its_alignment_and_undoes() {
    let (world, mut h) = document();
    link(&world, &mut h, "Setup2", "Setup1", [180.0, 0.0, 0.0]);
    assert_eq!(link_of(&world, "Setup2").as_deref(), Some("Setup1"));
    assert_eq!(setup_relative::describe(&world, "Setup2").as_deref(), Some("Flip over X from Setup1"));
    assert_eq!(setup_align::preset_of(setup_align::stored(&world, "Setup2").expect("aligned").rot), Some(AlignPreset::FlipX));
    assert!(close(carry(&world, "Setup1", "Setup2", CORNER), Point3::new(-20.0, 20.0, 0.0)));
    // Setup2's own stock followed the flip; Setup1's did not move.
    let s2 = model::find_by_name(&world, "Setup2/Stock").expect("own stock");
    assert_eq!(model::transform_of(&world, &s2).map(|l| l.pos.z), Some(0.0));
    let again = SetSetupRelative::link(&world, "Setup2", "Setup1", [180.0, 0.0, 0.0]);
    assert!(again.is_err(), "the same link and turn is no change");
    assert!(h.undo(&world));
    assert_eq!(link_of(&world, "Setup2"), None);
    assert_eq!(setup_align::stored(&world, "Setup2"), None);
    assert!(h.redo(&world));
    assert_eq!(link_of(&world, "Setup2").as_deref(), Some("Setup1"));
    // "Same as" links with no turn; unlinking keeps the part where it is.
    link(&world, &mut h, "Setup3", "Setup1", [0.0; 3]);
    assert_eq!(setup_relative::describe(&world, "Setup3").as_deref(), Some("Same as Setup1"));
    let held = setup_align::stored(&world, "Setup2");
    h.execute(&world, SetSetupRelative::unlink(&world, "Setup2").expect("unlink"));
    assert_eq!(link_of(&world, "Setup2"), None);
    assert_eq!(setup_align::stored(&world, "Setup2"), held, "unlinking moves nothing");
    assert!(SetSetupRelative::unlink(&world, "Setup2").is_err(), "nothing to unlink");
}

#[test]
fn a_change_of_the_parent_carries_into_linked_setups_in_one_undo_unit() {
    let (world, mut h) = document();
    link(&world, &mut h, "Setup2", "Setup1", [180.0, 0.0, 0.0]);
    link(&world, &mut h, "Setup3", "Setup2", [0.0, 0.0, 90.0]);
    let t12 = transform_between(&world, "Setup1", "Setup2").expect("t12");
    let t23 = transform_between(&world, "Setup2", "Setup3").expect("t23");
    let (a2, a3) = (setup_align::stored(&world, "Setup2"), setup_align::stored(&world, "Setup3"));
    let cmd = AlignPartToSetup::preset(&world, "Setup1", AlignPreset::TurnZ90).expect("parent turns");
    assert_eq!(cmd.carried_count(), 2, "the child and the grandchild follow");
    h.execute(&world, cmd);
    assert!(!setup_relative::same(setup_align::stored(&world, "Setup2"), a2), "Setup2 moved with Setup1");
    let t12_now = transform_between(&world, "Setup1", "Setup2").expect("t12'");
    let t23_now = transform_between(&world, "Setup2", "Setup3").expect("t23'");
    assert!(setup_relative::same(Some(t12), Some(t12_now)), "the flip between them is kept");
    assert!(setup_relative::same(Some(t23), Some(t23_now)), "the turn between them is kept");
    assert!(h.undo(&world), "one undo unit");
    assert!(setup_relative::same(setup_align::stored(&world, "Setup2"), a2));
    assert!(setup_relative::same(setup_align::stored(&world, "Setup3"), a3));
    assert_eq!(setup_align::stored(&world, "Setup1"), None);
    assert!(h.redo(&world));
    assert!(setup_relative::same(Some(t12), transform_between(&world, "Setup1", "Setup2").ok()));
    // An unlinked setup does not follow.
    h.execute(&world, SetSetupRelative::unlink(&world, "Setup2").expect("unlink"));
    let held = setup_align::stored(&world, "Setup2");
    preset(&world, &mut h, "Setup1", AlignPreset::FlipY);
    assert_eq!(setup_align::stored(&world, "Setup2"), held);
}

#[test]
fn a_parent_alignment_makes_a_linked_childs_toolpath_stale_and_undo_restores_it() {
    use camcraft_app::commands::{ActivateSetup, AddTool, CreateOperation, GenerateOperation};
    use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
    use camcraft_app::tools::catalog;

    let (world, mut h) = document();
    link(&world, &mut h, "Setup2", "Setup1", [180.0, 0.0, 0.0]);
    h.execute(&world, ActivateSetup::new(&world, Some("Setup2")).expect("activate child"));
    let tool_name = "EM-0400-2FL";
    let tool = catalog().into_iter().find(|(n, _)| *n == tool_name).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, tool_name, tool).expect("add tool"));
    // The flipped stock extends to Z22 above the part's Z20 top. Face
    // only that cap, with fixed heights and a square stock: a Z quarter
    // turn changes the child's alignment, not its stock or cut heights.
    let params = OpParams::Face {
        holes: Default::default(), tilt: None, blend_turns: false, blend_radius: None,
        stepover: 3.0, clearance_z: Height::fixed(25.0), cut_z: Height::fixed(21.5),
        top_z: None, stepdown: None, feed: 1200.0, plunge_feed: 300.0, climb: true,
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "Face1", tool_name, SrcRef::Stock, params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate in linked child"));
    let op = model::find_by_name(&world, "Face1").expect("op");
    assert_eq!(model::operation_of(&world, &op).expect("operation").setup.as_deref(), Some("Setup2"));
    assert_eq!(link_of(&world, "Setup2").as_deref(), Some("Setup1"));
    assert!(!model::toolpath_of(&world, &op).expect("generated path").path.moves.is_empty());
    assert_eq!(operation::status_of(&world, &op), OpStatus::Ok);
    let before = setup_align::stored(&world, "Setup2");
    let stock = model::find_by_name(&world, "Setup2/Stock").expect("child stock");
    let stock_before = (model::stock_def_of(&world, &stock), model::transform_of(&world, &stock));
    let child_point = || {
        let a = setup_align::stored(&world, "Setup2").expect("child alignment");
        camcraft_app::geometry::transform_point(CORNER, a.pos, a.rot)
    };
    let point_before = child_point();

    let turn = AlignPartToSetup::preset(&world, "Setup1", AlignPreset::TurnZ90).expect("parent turns");
    assert_eq!(turn.carried_count(), 1);
    h.execute(&world, turn);
    let moved = setup_align::stored(&world, "Setup2");
    assert!(!setup_relative::same(moved, before), "the child followed its parent");
    assert!(!close(child_point(), point_before));
    assert_eq!((model::stock_def_of(&world, &stock), model::transform_of(&world, &stock)), stock_before);
    assert_eq!(operation::status_of(&world, &op), OpStatus::Stale);

    assert!(h.undo(&world), "undo only the parent's alignment command");
    assert_eq!(setup_align::stored(&world, "Setup1"), None);
    assert!(setup_relative::same(setup_align::stored(&world, "Setup2"), before));
    assert!(close(child_point(), point_before));
    assert_eq!(operation::status_of(&world, &op), OpStatus::Ok, "fresh without regenerating");
    assert!(h.redo(&world));
    assert!(setup_relative::same(setup_align::stored(&world, "Setup2"), moved));
    assert_eq!(operation::status_of(&world, &op), OpStatus::Stale);
}

#[test]
fn a_linked_setup_that_cannot_follow_refuses_the_parent_change_by_name() {
    let (world, mut h) = document();
    // Setup3 is active (the last created): give it a cylinder stock.
    let cyl = StockShape::Cylinder { diameter: 70.0, height: 22.0 };
    let at = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, cyl, at).expect("cylinder"));
    link(&world, &mut h, "Setup3", "Setup1", [0.0; 3]);
    let err = AlignPartToSetup::angles(&world, "Setup1", [90.0, 0.0, 0.0]).err().expect("refused");
    let msg = err.to_string();
    assert!(msg.contains("'Setup3' is placed relative to 'Setup1'") && msg.contains("on its side"), "{msg}");
    assert_eq!(setup_align::stored(&world, "Setup1"), None, "nothing written");
}

#[test]
fn links_refuse_self_loops_and_missing_setups_and_guard_delete() {
    let (world, mut h) = document();
    let err = SetSetupRelative::link(&world, "Setup2", "Setup2", [0.0; 3]).err().expect("self");
    assert!(err.to_string().contains("relative to itself"), "{err}");
    let err = SetSetupRelative::link(&world, "Setup2", "Nope", [0.0; 3]).err().expect("missing");
    assert!(err.to_string().contains("Nope"), "{err}");
    let err = SetSetupRelative::link(&world, "part", "Setup1", [0.0; 3]).err().expect("a model");
    assert!(err.to_string().contains("not a setup"), "{err}");
    let err = SetSetupRelative::link(&world, "Setup2", "Setup1", [f64::NAN, 0.0, 0.0]).err().expect("nan");
    assert!(err.to_string().contains("finite"), "{err}");
    link(&world, &mut h, "Setup3", "Setup2", [0.0; 3]);
    link(&world, &mut h, "Setup2", "Setup1", [0.0; 3]);
    let err = SetSetupRelative::link(&world, "Setup1", "Setup3", [0.0; 3]).err().expect("loop");
    assert!(err.to_string().contains("loop"), "{err}");
    let err = DeleteSetup::new(&world, "Setup2").err().expect("a parent is not deletable");
    assert!(err.to_string().contains("'Setup3' is placed relative to 'Setup2'"), "{err}");
    h.execute(&world, SetSetupRelative::unlink(&world, "Setup3").expect("unlink"));
    h.execute(&world, DeleteSetup::new(&world, "Setup3").expect("a leaf deletes"));
}

fn temp(tag: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-relative-{tag}-{}.ccam", std::process::id()));
    path
}

#[test]
fn links_round_trip_and_older_or_broken_files_are_read_by_name() {
    let (world, mut h) = document();
    link(&world, &mut h, "Setup2", "Setup1", [180.0, 0.0, 0.0]);
    let path = temp("rt");
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"relative_to\": \"Setup1\""), "{first}");
    assert!(first.contains(&format!("\"version\": {}", project::PROJECT_VERSION)));
    // Links arrived at v56 (#316); a later rung must not break them, so
    // pin the floor, not the exact number (cf. #344).
    const _: () = assert!(project::PROJECT_VERSION >= 56, "setup links need project v56 or later");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    assert_eq!(link_of(&world2, "Setup2").as_deref(), Some("Setup1"));
    assert_eq!(setup_align::stored(&world2, "Setup2"), setup_align::stored(&world, "Setup2"));
    project::save(&world2, &path).expect("resave");
    assert_eq!(first, std::fs::read_to_string(&path).expect("reread"), "byte-identical round trip");
    // The loaded link still carries a parent change.
    let cmd = AlignPartToSetup::preset(&world2, "Setup1", AlignPreset::TurnZ180).expect("turn");
    assert_eq!(cmd.carried_count(), 1);
    let _ = std::fs::remove_file(&path);

    // A v55 file (no key) links nothing.
    let old = r#"{"version": 55, "entities": [
        {"name": "Setup1", "translation": [0.0, 0.0, 0.0], "kind": "setup",
         "setup": {"stock": "Stock", "wcs": "G54"}},
        {"name": "Setup2", "translation": [0.0, 0.0, 0.0], "kind": "setup",
         "setup": {"stock": "Setup2/Stock", "wcs": "Setup2/WCS"}}]}"#;
    let w = new_world();
    project::open(&w, old).expect("a v55 file reads");
    assert_eq!(link_of(&w, "Setup2"), None);
    let now = old.replace("\"version\": 55", &format!("\"version\": {}", project::PROJECT_VERSION));
    let dangling = now.replace("\"wcs\": \"Setup2/WCS\"", "\"wcs\": \"Setup2/WCS\", \"relative_to\": \"Gone\"");
    let err = project::open(&new_world(), &dangling).expect_err("a link to no setup");
    assert!(err.to_string().contains("'Gone', which is not a setup"), "{err}");
    let looped = dangling
        .replace("\"relative_to\": \"Gone\"", "\"relative_to\": \"Setup1\"")
        .replace("\"wcs\": \"G54\"", "\"wcs\": \"G54\", \"relative_to\": \"Setup2\"");
    let err = project::open(&new_world(), &looped).expect_err("a loop of links");
    assert!(err.to_string().contains("loop"), "{err}");
}

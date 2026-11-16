//! The operation's generated path as its own undoable verb
//! (`ClearToolpath`), and the pocket family's flat-area refusal raised at
//! the pick and creation doors instead of only at Generate.
//! Provenance: our own tests, specced from the operation lifecycle and
//! the hand-test report (vertical-wall rim picked for a 2D Pocket).
//!
//! Gate map (M4.4): covers app::commands, app::operation, app::model, app::schema,
//! app::setup_wcs_tilt (#1510).

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, ChangeOperationKind, ClearToolpath, CommandError, CreateOperation, DeleteOperation,
    EditOperationLinks, GenerateOperation, History, ImportModel, MoveModel, SetOperationGeometry, SetStock,
    SetWcsOrigin,
};
use camcraft_app::geometry::feature::{self, FaceKind};
use camcraft_app::geometry::{Point3, cube, quat_from_euler};
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::{Location, Toolpath};
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0600-4FL";

fn world_with_part() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let tool = camcraft_app::tools::catalog()
        .into_iter()
        .find(|(n, _)| *n == TOOL)
        .map(|(_, t)| t)
        .expect("catalog tool");
    history.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box { dx: 40.0, dy: 40.0, dz: 10.0 },
            Location { pos: Point3::new(-15.0, -15.0, 0.0), ..Location::default() },
        )
        .expect("stock"),
    );
    (world, history)
}

fn face_id(world: &World, vertical: bool) -> u64 {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    f.faces
        .iter()
        .filter(|fa| matches!(fa.kind, FaceKind::Planar))
        .filter(|fa| (fa.normal[2].abs() < 0.5) == vertical)
        .max_by(|a, b| a.offset.total_cmp(&b.offset))
        .map(|fa| fa.id)
        .expect("a planar face")
}

fn pick(id: u64) -> GeoSel {
    GeoSel { entity: "part".into(), feature: FeatureSel::Face(id) }
}

fn pocket() -> OpParams {
    OpParams::Pocket {
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(15.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(5.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: camcraft_app::machining2d::PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    }
}

fn facing() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(15.0),
        cut_z: Height::fixed(5.0),
        top_z: None,
        stepdown: None,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        blend_turns: false,
    }
}

fn cache(world: &World, name: &str) -> Option<Toolpath> {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e))
}

fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("op");
    operation::status_of(world, &e)
}

fn refusal_names_the_fix(r: Result<impl Sized, CommandError>) {
    let Err(err) = r else {
        panic!("a vertical wall pick must be refused at the door");
    };
    let msg = err.to_string();
    assert!(msg.contains("projects to a line from above"), "{msg}");
    assert!(msg.contains("select a horizontal face or a closed edge chain"), "{msg}");
}

/// A generated pocket on the part's top face.
fn generated_pocket(world: &World, history: &mut History) {
    let top = pick(face_id(world, false));
    let src = SrcRef::Selection(vec![top]);
    history.execute(world, CreateOperation::new(world, "P1", TOOL, src, pocket()).expect("create"));
    history.execute(world, GenerateOperation::new(world, "P1").expect("generate"));
}

#[test]
fn clear_toolpath_keeps_the_operation_and_undo_restores_the_cache_exactly() {
    let (world, mut history) = world_with_part();
    generated_pocket(&world, &mut history);
    let before = cache(&world, "P1").expect("generated");
    let op_before = model::operation_of(&world, &model::find_by_name(&world, "P1").expect("P1"));
    assert_eq!(status(&world, "P1"), OpStatus::Ok);

    history.execute(&world, ClearToolpath::new(&world, "P1").expect("clear"));
    assert_eq!(cache(&world, "P1"), None, "the path is gone");
    assert!(model::find_by_name(&world, "P1").is_some(), "the operation is kept");
    assert_eq!(status(&world, "P1"), OpStatus::Never);
    let kept = model::operation_of(&world, &model::find_by_name(&world, "P1").expect("P1"))
        .expect("op");
    let prev = op_before.clone().expect("op");
    assert_eq!((kept.params, kept.src, kept.tool), (prev.params, prev.src, prev.tool), "definition untouched");

    assert!(history.undo(&world));
    let back = cache(&world, "P1").expect("undo restores the cache");
    assert_eq!(common::gen_to_cltext(&back.path), common::gen_to_cltext(&before.path), "byte-identical path");
    assert_eq!(back, before);
    assert_eq!(model::operation_of(&world, &model::find_by_name(&world, "P1").expect("P1")), op_before);
    assert_eq!(status(&world, "P1"), OpStatus::Ok, "the stamp came back with it");

    assert!(history.redo(&world));
    assert_eq!(cache(&world, "P1"), None, "redo clears again");
}

#[test]
fn clearing_nothing_is_refused_and_delete_still_deletes() {
    let (world, mut history) = world_with_part();
    let src = SrcRef::Selection(vec![pick(face_id(&world, false))]);
    history.execute(&world, CreateOperation::new(&world, "P1", TOOL, src, pocket()).expect("create"));
    assert!(ClearToolpath::new(&world, "P1").is_err(), "never generated: nothing to clear");
    assert!(ClearToolpath::new(&world, "nope").is_err());

    history.execute(&world, GenerateOperation::new(&world, "P1").expect("generate"));
    let before = cache(&world, "P1").expect("cache");
    history.execute(&world, DeleteOperation::new(&world, "P1").expect("delete"));
    assert!(model::find_by_name(&world, "P1").is_none(), "delete removes the operation");
    assert!(history.undo(&world));
    assert_eq!(cache(&world, "P1"), Some(before), "undo of delete brings op and path back");
}

#[test]
fn a_wall_pick_for_a_pocket_is_refused_at_every_door() {
    let (world, mut history) = world_with_part();
    let wall = SrcRef::Selection(vec![pick(face_id(&world, true))]);
    // Creation.
    refusal_names_the_fix(CreateOperation::new(&world, "W", TOOL, wall.clone(), pocket()));
    // The pick itself, onto a live pocket.
    generated_pocket(&world, &mut history);
    let path = cache(&world, "P1");
    refusal_names_the_fix(SetOperationGeometry::new(&world, "P1", vec![pick(face_id(&world, true))]));
    // The links door (the Source combo, chip removal).
    refusal_names_the_fix(EditOperationLinks::new(&world, "P1", None, Some(wall.clone())));
    // Nothing was written by any refusal.
    assert_eq!(cache(&world, "P1"), path);
    assert_eq!(status(&world, "P1"), OpStatus::Ok);
}

#[test]
fn a_rotated_face_pick_for_facing_is_refused_before_generation() {
    let (world, mut history) = world_with_part();
    let top = pick(face_id(&world, false));
    history.execute(
        &world,
        CreateOperation::new(&world, "F1", TOOL, SrcRef::Stock, facing()).expect("facing op"),
    );
    history.execute(
        &world,
        MoveModel::new(
            &world,
            "part",
            Location { rot: quat_from_euler(std::f64::consts::FRAC_PI_2, 0.0, 0.0), ..Location::default() },
        )
        .expect("turn part"),
    );
    let err = SetOperationGeometry::new(&world, "F1", vec![top.clone()])
        .err()
        .expect("vertical face is refused at the pick");
    assert!(err.to_string().contains("not horizontal"), "{err}");
    let (_, refusal) = operation::selection_account(&world, None, &[top], &facing());
    assert!(refusal.is_some_and(|msg| msg.contains("not horizontal")));
}

/// #1510: a wall the tilted WCS calls level - the reporter's pick - says
/// the WCS is why, and that the part is turned with its alignment.
#[test]
fn a_wall_level_only_in_a_tilted_wcs_names_the_wcs() {
    let (world, mut history) = world_with_part();
    history.execute(
        &world,
        CreateOperation::new(&world, "F1", TOOL, SrcRef::Stock, facing()).expect("facing op"),
    );
    let tilted = Location { rot: quat_from_euler(std::f64::consts::FRAC_PI_2, 0.0, 0.0), ..Location::default() };
    history.execute(&world, SetWcsOrigin::new(&world, tilted).expect("tilt the WCS"));
    let err = SetOperationGeometry::new(&world, "F1", vec![pick(face_id(&world, true))])
        .err()
        .expect("a machine-vertical face is refused at the pick");
    let msg = err.to_string();
    assert!(
        msg.contains("not horizontal")
            && msg.contains("the WCS (G54) has its Z axis tilted 90.000 deg off the machine's Z")
            && msg.contains("Part alignment"),
        "{msg}"
    );
}

#[test]
fn the_door_is_the_pocket_familys_and_the_horizontal_pick_passes() {
    let (world, _history) = world_with_part();
    let wall = SrcRef::Selection(vec![pick(face_id(&world, true))]);
    assert!(
        operation::selection_refusal(&world, None, &wall, &pocket())
            .is_some_and(|m| m.contains("projects to a line")),
    );
    assert_eq!(operation::selection_refusal(&world, None, &SrcRef::Stock, &pocket()), None);
    let top = SrcRef::Selection(vec![pick(face_id(&world, false))]);
    assert_eq!(operation::selection_refusal(&world, None, &top, &pocket()), None);
    // Not a pocket-family kind: the door stays out of it (a contour's
    // wall refusal remains Generate's, pinned in tests/operations.rs).
    let contour = (0..operation::KindInfo::ALL.len())
        .map(|k| operation::retype_params(&pocket(), k))
        .find(|p| matches!(p, OpParams::Contour { .. }))
        .expect("a contour kind");
    assert_eq!(operation::selection_refusal(&world, None, &wall, &contour), None);
    // The page's account carries the same sentence beside its notes.
    let SrcRef::Selection(items) = &wall else { return };
    let (_, refusal) = operation::selection_account(&world, None, items, &pocket());
    assert!(refusal.is_some_and(|m| m.contains("select a horizontal face")));
}

#[test]
fn retyping_onto_a_wall_pick_meets_the_pocket_door() {
    let (world, mut history) = world_with_part();
    let wall = SrcRef::Selection(vec![pick(face_id(&world, true))]);
    // A Face op cannot hold a wall either; a 2D Contour can be created on
    // one (its refusal stays Generate's), so retype from a contour.
    let pocket_ordinal = pocket().kind().ordinal();
    let contour_kind = (0..operation::KindInfo::ALL.len())
        .map(|k| operation::retype_params(&pocket(), k))
        .find(|p| matches!(p, OpParams::Contour { .. }))
        .expect("a contour kind");
    match CreateOperation::new(&world, "C1", TOOL, wall, contour_kind) {
        Ok(cmd) => history.execute(&world, cmd),
        Err(_) => return, // the contour's own doors refuse a wall: nothing to retype
    }
    refusal_names_the_fix(ChangeOperationKind::new(&world, "C1", pocket_ordinal));
}

/// The edge run of the part whose every point passes `on` (`lo`/`hi` are
/// the part's bounds): the pick a viewport click on that edge makes.
fn edge_run(world: &World, on: impl Fn(&[f64; 3], [f64; 3], [f64; 3]) -> bool) -> GeoSel {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for v in &f.verts {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    let run = feature::edge_runs(&f)
        .into_iter()
        .find(|c| feature::chain_points(&f, c).iter().all(|p| on(p, lo, hi)))
        .expect("an edge run");
    GeoSel { entity: "part".into(), feature: FeatureSel::Chain(run.id) }
}

/// #1726: an edge picked one at a time is an open run mid-pick. Two runs
/// that only coincide seen from above (a top edge and the bottom edge
/// under it), or one vertical edge, stitch into a "ring" with no area;
/// the pick door used to refuse that pick, so Ctrl+click added nothing.
/// The door takes the picks; Generate keeps the refusal.
#[test]
fn edge_picks_that_meet_only_from_above_pass_the_pick_door() {
    let (world, mut history) = world_with_part();
    generated_pocket(&world, &mut history);
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let top = edge_run(&world, |p, lo, hi| near(p[1], lo[1]) && near(p[2], hi[2]));
    let under = edge_run(&world, |p, lo, _| near(p[1], lo[1]) && near(p[2], lo[2]));
    let upright = edge_run(&world, |p, lo, _| near(p[0], lo[0]) && near(p[1], lo[1]));
    for items in [vec![upright.clone()], vec![top.clone(), upright], vec![top.clone(), under.clone()]] {
        let n = items.len();
        let cmd = match SetOperationGeometry::new(&world, "P1", items.clone()) {
            Ok(cmd) => cmd,
            Err(err) => panic!("{n} edge pick(s) refused at the pick door: {err}"),
        };
        history.execute(&world, cmd);
        let op = model::operation_of(&world, &model::find_by_name(&world, "P1").expect("P1")).expect("op");
        assert_eq!(op.src, SrcRef::Selection(items.clone()), "the picks are stored");
        // The page still says what Generate will say, and Generate refuses.
        let (_, said) = operation::selection_account(&world, None, &items, &pocket());
        assert!(said.is_some_and(|m| m.contains("projects to a line from above")));
        let refused = GenerateOperation::new(&world, "P1").err().map(|e| e.to_string()).unwrap_or_default();
        assert!(refused.contains("projects to a line from above"), "Generate is the backstop: {refused:?}");
    }
    // A wall FACE, and a closed chain, still meet the door (pinned above).
    refusal_names_the_fix(SetOperationGeometry::new(&world, "P1", vec![pick(face_id(&world, true))]));
}

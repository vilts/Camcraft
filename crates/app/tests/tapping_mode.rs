//! Issue #51: tapping intent must survive the command, generated cycle,
//! project cache and intermediate stream. `Tap` remains the rigid legacy
//! cycle; `FloatingTap` is explicit and keeps its own dwell.
//!
//! Gate map: covers app::commands (EditOperationParams + History),
//! app::operation (drill generation), app::project (params + cached cycle),
//! app::posting / post::cnc (dump + parse).
//!
//! Provenance: regression contract from issue #51 and our own toolpath and
//! stream laws; no vendor material consulted.

use flecs_ecs::prelude::World;

use camcraft_app::cl::{CycleKind, MoveKind, Toolpath};
use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel,
};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::cnc::{self, Record};
use camcraft_app::post::engine;
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOP: f64 = 20.0;
const FLOOR: f64 = -5.0;
const OP: &str = "Tap1";
const TOOL: &str = "TAP-M4";

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// A 40 x 40 block with a 6 mm square, 25 mm deep existing hole.
/// This is the established drill-reach fixture shape, kept local to the
/// tapping regression rather than expanding shared test fixtures.
fn block_with_hole(h: f64) -> Mesh {
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, -10.0), Point3::new(20.0, 20.0, TOP));
    f.extend(outer.facets[4..12].iter().cloned());
    f.extend(outer.facets[0..2].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let i = [(-h, -h), (h, -h), (h, h), (-h, h)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, TOP);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    let inner = cube(Point3::new(-h, -h, FLOOR), Point3::new(h, h, TOP));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

fn fresh_tap_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let tap = catalog()
        .into_iter()
        .find(|(name, _)| *name == TOOL)
        .map(|(_, tool)| tool)
        .expect("catalog TAP-M4");
    history.execute(&world, AddTool::new(&world, TOOL, tap).expect("tap tool"));
    history.execute(&world, ImportModel::from_mesh("block", block_with_hole(3.0)));
    (world, history)
}

fn params(cycle: CycleChoice, dwell: f64) -> OpParams {
    OpParams::Drill {
        break_through: None,
        break_through_from: Default::default(),
        clearance_z: Height::fixed(TOP + 10.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        cycle,
        peck: 0.0,
        dwell,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![Hole {
            xy: Point2::new(0.0, 0.0),
            radius: 3.0,
            entry_z: TOP,
            floor_z: FLOOR,
            through: false,
            stepped: false,
        }],
        cross: vec![],
        picks: Default::default(),
    }
}

fn create_and_generate(world: &World, history: &mut History, cycle: CycleChoice, dwell: f64) {
    history.execute(
        world,
        CreateOperation::new(world, OP, TOOL, SrcRef::Entity("block".into()), params(cycle, dwell))
            .expect("create tap operation"),
    );
    history.execute(world, GenerateOperation::new(world, OP).expect("generate tap operation"));
}

fn operation(world: &World) -> camcraft_app::schema::Operation {
    let entity = model::find_by_name(world, OP).expect("tap operation entity");
    model::operation_of(world, &entity).expect("operation definition")
}

fn path(world: &World) -> Toolpath {
    let entity = model::find_by_name(world, OP).expect("tap operation entity");
    model::toolpath_of(world, &entity).expect("generated toolpath").path
}

fn assert_generated(world: &World, want: CycleKind, dwell: f64) -> Toolpath {
    let path = path(world);
    assert_eq!(path.cycles.len(), 1, "one record for the tapped hole");
    let cycle = path.cycles[0];
    assert_eq!(cycle.kind, want, "assert intent before dumping the path");
    assert_eq!(cycle.dwell, dwell, "cycle dwell");

    let tool_entity = model::find_by_name(world, TOOL).expect("tap tool entity");
    let tool = model::tool_of(world, &tool_entity).expect("tap tool definition");
    let pitch_feed = tool.pitch * tool.rpm;
    assert!(pitch_feed.is_finite() && pitch_feed > 0.0, "catalog tap has a positive pitch x rpm feed");
    let cycle_moves = &path.moves[cycle.first_move..];
    let feed_moves: Vec<_> = cycle_moves
        .iter()
        .take_while(|m| m.kind == MoveKind::Feed)
        .collect();
    assert_eq!(feed_moves.len(), 2, "tap feeds down and reverses out");
    assert!(
        feed_moves.iter().all(|m| m.feed == Some(pitch_feed)),
        "both tapping directions use pitch x rpm = {pitch_feed}: {feed_moves:?}"
    );
    path
}

/// Exercise the real operation-to-stream dump and the public stream parser.
/// The generated cycle kind is checked first, so a dump/parser agreement
/// cannot conceal an upstream mode collapse.
fn assert_stream_cycle(world: &World, want: CycleKind, marker: &str) -> String {
    let records = posting::stream_records(world).expect("generated operation dumps to records");
    let stream = cnc::serialize(&records);
    assert!(stream.contains(marker), "stream keeps its explicit cycle word: {stream}");
    let parsed = cnc::parse(&stream).expect("serialized operation stream parses");
    let cycle = parsed
        .into_iter()
        .find_map(|record| match record {
            Record::Cycle(cycle) => Some(cycle),
            _ => None,
        })
        .expect("stream has one canned cycle");
    assert_eq!(cycle.kind, want, "parser preserves the cycle word");
    stream
}

fn temp_project(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-tapping-{tag}-{}.camproj", std::process::id()))
}

#[test]
fn floating_mode_is_distinct_editable_undoable_and_round_trips_its_cached_cycle() {
    let (world, mut history) = fresh_tap_world();
    create_and_generate(&world, &mut history, CycleChoice::Tap, 1.5);

    // The established `Tap` choice remains rigid and preserves its
    // legacy no-dwell generator behavior even when a dwell was requested.
    let rigid = assert_generated(&world, CycleKind::Tap, 0.0);
    assert_stream_cycle(&world, CycleKind::Tap, "CYCLE TAP ");
    assert_eq!(rigid.cycles[0].kind, CycleKind::Tap);

    let floating_edit = EditOperationParams::new(&world, OP, params(CycleChoice::FloatingTap, 1.5))
        .expect("edit to floating tap");
    history.execute(&world, floating_edit);
    let OpParams::Drill { cycle, .. } = operation(&world).params else {
        panic!("drill parameters remain a drill operation");
    };
    assert_eq!(cycle, CycleChoice::FloatingTap, "edit stores the explicit mode");
    history.execute(&world, GenerateOperation::new(&world, OP).expect("generate floating tap"));

    assert_generated(&world, CycleKind::FloatingTap, 1.5);
    let floating_stream = assert_stream_cycle(&world, CycleKind::FloatingTap, "CYCLE FLOATINGTAP ");

    // Both the operation's choice and the independently cached cycle kind
    // survive the actual project serializer and loader.
    let project_path = temp_project("floating-roundtrip");
    project::save(&world, &project_path).expect("save floating tap project");
    let saved = std::fs::read_to_string(&project_path).expect("read saved project");
    assert!(saved.contains("\"cycle\": \"floating_tap\""), "explicit params spelling: {saved}");
    assert!(saved.contains("\"kind\": \"floating_tap\""), "cached CycleDto spelling: {saved}");
    let loaded = new_world();
    project::load(&loaded, &project_path).expect("load floating tap project");
    let _ = std::fs::remove_file(&project_path);
    let OpParams::Drill { cycle, .. } = operation(&loaded).params else {
        panic!("loaded parameters remain a drill operation");
    };
    assert_eq!(cycle, CycleChoice::FloatingTap);
    assert_generated(&loaded, CycleKind::FloatingTap, 1.5);
    assert_stream_cycle(&loaded, CycleKind::FloatingTap, "CYCLE FLOATINGTAP ");
    assert_eq!(floating_stream, assert_stream_cycle(&world, CycleKind::FloatingTap, "CYCLE FLOATINGTAP "));

    // Undo the regeneration, then the edit: history restores the previous
    // mode and its still-cached rigid path. Redo restores floating intent.
    assert!(history.undo(&world), "undo floating generation");
    assert!(history.undo(&world), "undo floating-mode edit");
    let OpParams::Drill { cycle, .. } = operation(&world).params else {
        panic!("undone parameters remain a drill operation");
    };
    assert_eq!(cycle, CycleChoice::Tap);
    assert_generated(&world, CycleKind::Tap, 0.0);
    assert!(history.redo(&world), "redo floating-mode edit");
    let OpParams::Drill { cycle, .. } = operation(&world).params else {
        panic!("redone parameters remain a drill operation");
    };
    assert_eq!(cycle, CycleChoice::FloatingTap);
    history.execute(&world, GenerateOperation::new(&world, OP).expect("regenerate after redo"));
    assert_generated(&world, CycleKind::FloatingTap, 1.5);
}

#[test]
fn legacy_tap_spelling_and_stream_are_stable_and_unknown_kinds_are_rejected() {
    let (world, mut history) = fresh_tap_world();
    create_and_generate(&world, &mut history, CycleChoice::Tap, 1.5);
    assert_generated(&world, CycleKind::Tap, 0.0);
    let stream = assert_stream_cycle(&world, CycleKind::Tap, "CYCLE TAP ");

    let project_path = temp_project("legacy-tap");
    project::save(&world, &project_path).expect("save legacy tap project");
    let saved = std::fs::read_to_string(&project_path).expect("read saved project");
    assert!(saved.contains("\"cycle\": \"tap\""), "legacy choice remains `tap`: {saved}");
    assert!(saved.contains("\"kind\": \"tap\""), "legacy cached kind remains `tap`: {saved}");
    let loaded = new_world();
    project::load(&loaded, &project_path).expect("load legacy tap project");
    let _ = std::fs::remove_file(&project_path);
    let OpParams::Drill { cycle, .. } = operation(&loaded).params else {
        panic!("loaded parameters remain a drill operation");
    };
    assert_eq!(cycle, CycleChoice::Tap);
    assert_generated(&loaded, CycleKind::Tap, 0.0);

    let mut unknown_choice: serde_json::Value = serde_json::from_str(&saved).expect("valid saved project JSON");
    let entities = unknown_choice["entities"].as_array_mut().expect("project entities");
    let entity = entities
        .iter_mut()
        .find(|entity| entity["name"] == OP)
        .expect("saved operation entity");
    entity["operation"]["params"]["cycle"] = serde_json::json!("unknown_tapping_mode");
    let bad_choice = serde_json::to_string(&unknown_choice).expect("serialize unknown choice");
    assert!(project::open(&new_world(), &bad_choice).is_err(), "unknown parameter mode is refused");

    let mut unknown_cache: serde_json::Value = serde_json::from_str(&saved).expect("valid saved project JSON");
    let entities = unknown_cache["entities"].as_array_mut().expect("project entities");
    let entity = entities
        .iter_mut()
        .find(|entity| entity["name"] == OP)
        .expect("saved operation entity");
    entity["operation"]["cache"]["cycles"][0]["kind"] = serde_json::json!("unknown_cycle_kind");
    let bad_cache = serde_json::to_string(&unknown_cache).expect("serialize unknown cache kind");
    assert!(project::open(&new_world(), &bad_cache).is_err(), "unknown cached cycle kind is refused");

    let unknown_stream = stream.replacen("CYCLE TAP ", "CYCLE UNKNOWNCYCLE ", 1);
    assert_ne!(unknown_stream, stream, "the unknown cycle word fixture changed");
    assert!(cnc::parse(&unknown_stream).is_err(), "unknown stream cycle words are refused");
}

#[test]
fn generated_floating_tap_posts_as_one_native_cycle_with_its_safe_lift() {
    let (world, mut history) = fresh_tap_world();
    create_and_generate(&world, &mut history, CycleChoice::FloatingTap, 0.25);
    let generated = assert_generated(&world, CycleKind::FloatingTap, 0.25);
    let generated_cycle = generated.cycles[0];
    assert!(generated_cycle.first_move > 0, "the tap approach precedes its first cutting move");
    assert_eq!(generated.moves[generated_cycle.first_move - 1].kind, MoveKind::Rapid);
    assert_eq!(generated.moves[generated_cycle.first_move].kind, MoveKind::Feed);

    let records = posting::stream_records(&world).expect("generated operation dumps to actual posting records");
    let cycle_at = records
        .iter()
        .position(|record| matches!(record, Record::Cycle(_)))
        .expect("posting records contain the generated tap cycle");
    let cycle = match &records[cycle_at] {
        Record::Cycle(cycle) => cycle,
        _ => unreachable!(),
    };
    assert_eq!(cycle.kind, CycleKind::FloatingTap);
    assert_eq!(cycle.dwell, 0.25);
    assert_eq!(cycle.first_move, generated_cycle.first_move);
    let moves_before_cycle = records[..cycle_at]
        .iter()
        .filter(|record| matches!(record, Record::Move(_)))
        .count();
    assert_eq!(moves_before_cycle, cycle.first_move, "first_move counts the actual preceding move records");
    assert!(cycle_at > 0);
    assert!(matches!(&records[cycle_at - 1], Record::Move(m) if m.kind == MoveKind::Rapid), "the actual rapid approach precedes Cycle");
    assert!(matches!(records.get(cycle_at + 1), Some(Record::Move(m)) if m.kind == MoveKind::Feed), "first_move is the first cutting feed after Cycle");

    let stream = cnc::serialize(&records);
    let machine = serde_json::from_str(include_str!("../../../assets/machines/3axis-vmc.mch"))
        .expect("built-in VMC machine JSON");
    let out = engine::run(include_str!("../../../assets/posts/heidenhain.cps"), &machine, &stream)
        .expect("post the generated operation's actual record stream");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(out.program.matches("G206").count(), 1, "one native floating cycle definition");
    assert_eq!(out.program.matches("M99").count(), 1, "one call for the generated hole");
    let posted_feed: f64 = out.program.split_whitespace()
        .find_map(|word| word.strip_prefix("Q206="))
        .expect("native tapping feed").parse().expect("numeric Q206");
    assert!((posted_feed - cycle.feed.expect("generated cycle feed")).abs() < 0.000001, "posted feed agrees with the actual generated tool feed");
    assert!(out.program.contains("Q211=+0.25"), "{}", out.program);
    assert!(!out.program.contains("Q239="), "floating cycle does not emit rigid pitch");
    assert!(!out.program.contains("G01"), "native tapping replaces both expanded axial cuts");
    assert!(
        out.program.lines().any(|line| line.contains("G00") && line.contains("Z+30")),
        "the generator's higher safe lift remains explicit: {}",
        out.program,
    );
}

#[test]
fn generated_legacy_tap_posts_as_rigid_g207() {
    let (world, mut history) = fresh_tap_world();
    create_and_generate(&world, &mut history, CycleChoice::Tap, 0.25);
    assert_generated(&world, CycleKind::Tap, 0.0);
    assert_stream_cycle(&world, CycleKind::Tap, "CYCLE TAP ");

    let records = posting::stream_records(&world).expect("generated legacy tap dumps to actual posting records");
    let stream = cnc::serialize(&records);
    let machine = serde_json::from_str(include_str!("../../../assets/machines/3axis-vmc.mch"))
        .expect("built-in VMC machine JSON");
    let out = engine::run(include_str!("../../../assets/posts/heidenhain.cps"), &machine, &stream)
        .expect("post the generated legacy tap's actual record stream");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(out.program.matches("G207").count(), 1);
    assert_eq!(out.program.matches("M99").count(), 1);
    assert!(out.program.contains("Q239=+0.7"), "catalog pitch and rpm resolve into rigid Q239");
    assert!(!out.program.contains("G206"), "legacy TAP stays rigid");
    assert!(!out.program.contains("G01"), "native rigid tapping replaces its expanded axial cuts");
}

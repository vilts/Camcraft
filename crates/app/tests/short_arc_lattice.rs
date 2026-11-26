//! #3442 through the app's own posting paths: a generator arc with a
//! chord under one micrometre (a 0.09 um fillet on R50, the size that
//! #3463 names) used to reach the stream as a record whose end equals its
//! start - the stream's full-turn form - and every post wrote a full
//! circle of R50 through the part. The stream writer
//! (`post::cnc_arc_lattice`) now writes it as the straight move it is.
//! The whole-job, per-operation and per-mount paths are pinned, in mm and
//! in inch.
//!
//! Provenance: our own invariants; the posting fixture of
//! `control_comp.rs` (a contour on a 10 x 10 box).
//!
//! Gate map (M4.4): covers camcraft_app::posting,
//! camcraft_post::post::cnc, camcraft_post::post::cnc_arc_lattice,
//! assets/posts/fanuc.cps.

use flecs_ecs::prelude::World;

use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock, SetUnits};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::links::Links;
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::units::Unit;

fn fanuc() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/posts/fanuc.cps");
    std::fs::read_to_string(path).expect("post script")
}

fn contour() -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Linear { length: 6.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        blend_radius: None,
        links: Links::PLANAR,
        chain_overrides: Vec::new(),
        picks: Default::default(),
    }
}

/// The contour fixture, with a 0.09 um R50 fillet arc spliced in after
/// its first cutting move at constant Z. `inch` sets the document unit
/// through its command.
fn world_with_fillet(inch: bool) -> World {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let cmd = SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 100.0, dz: 5.0 }, Default::default());
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let cmd = CreateOperation::new(&world, "Contour1", "EM-0600-4FL", SrcRef::Entity("part".into()), contour());
    history.execute(&world, cmd.expect("create contour"));
    history.execute(&world, GenerateOperation::new(&world, "Contour1").expect("generate"));
    if inch {
        history.execute(&world, SetUnits::new(&world, Unit::Inches).expect("inch document"));
    }
    let e = model::find_by_name(&world, "Contour1").expect("op");
    let mut c = model::toolpath_of(&world, &e).expect("cache");
    let k = c
        .path
        .moves
        .iter()
        .position(|m| m.kind == MoveKind::Feed && m.arc.is_none() && m.from.z == m.to.z && m.from.x != m.to.x)
        .expect("a cutting move at constant Z");
    let p = c.path.moves[k].to;
    let (r, t): (f64, f64) = (50.0, 0.00009 / 50.0);
    // About a centre 50 mm off in -Y, turning CW from the top of the circle.
    let center = Point2::new(p.x, p.y - r);
    let end = Point3::new(p.x + r * t.sin(), p.y - r + r * t.cos(), p.z);
    let fillet = Move::new(MoveKind::Feed, p, end, c.path.moves[k].feed).with_arc(center, false);
    c.path.moves.insert(k + 1, fillet);
    c.path.moves[k + 2].from = end;
    let warnings = c.warnings.clone();
    e.entity_view(&world).set(camcraft_app::schema::Toolpath::new(c.path, warnings));
    world
}

/// The G02/G03 blocks of a Fanuc program whose written X and Y are the
/// position before them: each a full circle on the control.
fn coincident(program: &str) -> Vec<String> {
    let (mut x, mut y, mut out) = (None::<String>, None::<String>, Vec::new());
    for line in program.lines() {
        let word = |l: char| line.split_whitespace().find_map(|w| w.strip_prefix(l)).map(str::to_string);
        let (nx, ny) = (word('X'), word('Y'));
        if line.contains("G53") || line.contains("G28") {
            continue;
        }
        let arc = line.split_whitespace().any(|w| w == "G02" || w == "G03");
        if arc && nx.is_some() && nx == x && ny == y {
            out.push(line.to_string());
        }
        if nx.is_some() {
            x = nx;
        }
        if ny.is_some() {
            y = ny;
        }
    }
    out
}

fn tmp(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft_short_arc_{tag}_{}", std::process::id()))
}

#[test]
fn a_sub_micrometre_fillet_posts_no_full_circle_through_the_whole_job_path() {
    for inch in [false, true] {
        let world = world_with_fillet(inch);
        let path = tmp(&format!("job_{inch}.nc"));
        let posted = posting::post_script(&world, &path, &fanuc()).expect("the gate releases it");
        let _ = std::fs::remove_file(&path);
        assert!(coincident(&posted.program).is_empty(), "inch={inch}: full circles {:?}\n{}", coincident(&posted.program), posted.program);
        assert!(posted.program.contains(if inch { "G20" } else { "G21" }), "inch={inch}: the unit word");
    }
}

#[test]
fn a_sub_micrometre_fillet_posts_no_full_circle_per_operation_and_per_mount() {
    for inch in [false, true] {
        let world = world_with_fillet(inch);
        let dir = tmp(&format!("per_op_{inch}"));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let files = posting::post_per_op_script(&world, &dir, "short-arc", &fanuc()).expect("per-op posts");
        assert!(!files.is_empty(), "one file per operation");
        for f in &files {
            assert!(coincident(&f.posted.program).is_empty(), "per-op inch={inch}:\n{}", f.posted.program);
        }
        let _ = std::fs::remove_dir_all(&dir);
        let path = tmp(&format!("mounts_{inch}.nc"));
        let mounts = posting::post_mounts_script(&world, &path, &fanuc()).expect("per-mount posts");
        assert!(!mounts.is_empty(), "one program per mount");
        for m in &mounts {
            assert!(coincident(&m.posted.program).is_empty(), "per-mount inch={inch}:\n{}", m.posted.program);
            let _ = std::fs::remove_file(&m.path);
        }
    }
}

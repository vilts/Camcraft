//! Suppression against the program ahead (#110 review B1): a later op
//! is generated against every earlier same-setup cut, suppressed ones
//! included (rest masks, the rapid verdicts' in-process stock). Posting
//! it without the suppressed cut would rapid and slot into stock
//! nothing judged, so the post refuses by name; suppressing an op that
//! nothing posted after it depends on still posts.
//!
//! Provenance: our own design (`app::rest`, `app::posting`); RS-274
//! program-order semantics for "ahead in the program".
//!
//! Gate map (M4.4): covers app::posting (verification_report,
//! post_script, post_per_op), app::rest (suppressed_cuts_ahead),
//! app::commands (EditOperationParams::with_suppressed).

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

/// A 40x40x20 plate with a 20x20 pocket sunk to z=10 (remachining.rs's
/// part): the pocket void is the job.
fn plate_with_pocket() -> Mesh {
    let mut facets = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 20.0));
    facets.extend(outer.facets[0..2].iter().cloned());
    facets.extend(outer.facets[4..12].iter().cloned());
    let inner = cube(Point3::new(-10.0, -10.0, 10.0), Point3::new(10.0, 10.0, 20.0));
    facets.extend(inner.facets[0..2].iter().cloned());
    facets.extend(inner.facets[4..12].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)].map(|(x, y)| Point3::new(x, y, 20.0));
    let i = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)].map(|(x, y)| Point3::new(x, y, 20.0));
    for k in 0..4 {
        let (a, b, c, d) = (o[k], o[(k + 1) % 4], i[(k + 1) % 4], i[k]);
        facets.push(Triangle::new(a, b, c));
        facets.push(Triangle::new(a, c, d));
    }
    Mesh::from_facets(facets)
}

fn pocket(bottom_z: f64, stepover: f64, rest_rough: bool) -> OpParams {
    OpParams::Pocket3D {
        links: camcraft_app::links::Links::ROUGH3D,
        stepover,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(bottom_z),
        stepdown: 5.0,
        feed: 1500.0,
        plunge_feed: 400.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        rest_rough,
        optimal_load: camcraft_app::load::load_of_degrees(60.0),
        feed_opt: false,
        shell: false,
    }
}

/// The review's scenario: a rough with the big tool, then a rest pass
/// with the small one - the second generated against the first's cut.
fn rough_then_rest() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    for t in ["EM-0800-4FL", "EM-0400-2FL"] {
        let tool = catalog().into_iter().find(|(n, _)| *n == t).map(|(_, t)| t).expect("catalog entry");
        history.execute(&world, AddTool::new(&world, t, tool).expect("catalog tool"));
    }
    let stock = SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 }, Location::default());
    history.execute(&world, stock.expect("box stock"));
    history.execute(&world, ImportModel::from_mesh("plate", plate_with_pocket()));
    for (name, tool, params) in [
        ("Rough1", "EM-0800-4FL", pocket(12.0, 4.0, false)),
        ("Rest2", "EM-0400-2FL", pocket(10.0, 2.0, true)),
    ] {
        let cmd = CreateOperation::new(&world, name, tool, SrcRef::Entity("plate".into()), params);
        history.execute(&world, cmd.expect("create"));
        history.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
    }
    (world, history)
}

fn fanuc() -> &'static str {
    posting::builtin_post_text("Fanuc").expect("the Fanuc built-in")
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-suppressed-rest-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

#[test]
fn suppressing_a_cut_a_later_op_was_generated_against_refuses_by_name() {
    let (world, mut history) = rough_then_rest();
    assert!(posting::verification_report(&world).problems.is_empty(), "the whole program posts");
    history.execute(&world, EditOperationParams::with_suppressed(&world, "Rough1", true).expect("suppress"));

    let want = "operation Rest2 was generated against the cut of suppressed Rough1; unsuppress Rough1 or suppress Rest2 too";
    let problems = posting::verification_report(&world).problems;
    assert!(problems.iter().any(|p| p == want), "{problems:?}");

    let dir = scratch("refuse");
    let out = dir.join("job.nc");
    let err = posting::post_script(&world, &out, fanuc()).expect_err("the post refuses");
    assert!(err.contains(want), "{err}");
    assert!(!out.exists(), "a refused post writes nothing");
    // One file per operation runs the same gate.
    let err = posting::post_per_op_script(&world, &dir, "job", fanuc()).expect_err("per-op refuses too");
    assert!(err.contains(want), "{err}");
    assert_eq!(std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0), 0, "nothing written");

    // Suppressing the dependent too is the stated way out.
    history.execute(&world, EditOperationParams::with_suppressed(&world, "Rest2", true).expect("suppress"));
    assert!(
        !posting::verification_report(&world).problems.iter().any(|p| p.contains("generated against")),
        "nothing posted depends on a suppressed cut"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn suppressing_an_op_nothing_depends_on_still_posts() {
    let (world, mut history) = rough_then_rest();
    // The last op: nothing posted after it was generated against its cut.
    history.execute(&world, EditOperationParams::with_suppressed(&world, "Rest2", true).expect("suppress"));
    assert!(posting::verification_report(&world).problems.is_empty());
    let dir = scratch("posts");
    let out = dir.join("job.nc");
    let posted = posting::post_script(&world, &out, fanuc()).expect("the rough alone posts");
    assert_eq!(posted.ops, 1);
    assert!(out.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

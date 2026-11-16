//! Issue #323: contour turning leaves a cut inside a longer bar at feed.
//! Provenance: the stock/rapid law and command fixtures from turning_axial_rapids.
//! Gate map (M4.4): covers app::operation (turning_stock, generate, turning_rapids), app::posting.
//! Lane shape (M4.4): small revolved fixtures; all tests run in seconds.

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;

fn revolve(meridian: &[(f64, f64)]) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
            let (a, b, c, d) = (
                v(j, ra, za),
                v(j + 1, ra, za),
                v(j, rb, zb),
                v(j + 1, rb, zb),
            );
            if ra <= 1e-12 {
                facets.push(Triangle::new(a, d, c));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a, b, d));
            } else {
                facets.push(Triangle::new(a, b, d));
                facets.push(Triangle::new(a, d, c));
            }
        }
    }
    Mesh::from_facets(facets)
}

fn assert_long_bar_exit(tag: &str, mesh: Mesh, diameter: f64, params: OpParams, top: f64) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let stock = SetStock::new(
        &world,
        StockShape::Cylinder {
            diameter,
            height: 64.0,
        },
        Location {
            pos: Point3::new(0.0, 0.0, -16.0),
            ..Location::default()
        },
    )
    .expect("bar to Z48");
    history.execute(&world, stock);
    history.execute(
        &world,
        LoadMachine::new(
            &world,
            "lathe.mch".into(),
            Some(Machine {
                model: "TestBench lathe".into(),
                kind: Some("lathe".into()),
                ..Default::default()
            }),
        )
        .expect("lathe"),
    );
    history.execute(&world, camcraft_app::commands::SelectPost::new(&world, camcraft_app::schema::PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    let name = "TN-1600-0R8";
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("insert");
    history.execute(&world, AddTool::new(&world, name, tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", mesh));
    history.execute(
        &world,
        CreateOperation::new(&world, tag, name, SrcRef::Entity("part".into()), params)
            .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, tag).expect("generate"),
    );
    let e = model::find_by_name(&world, tag).expect("operation");
    let c = model::toolpath_of(&world, &e).expect("cached path");
    // Establish the consumer's actual end and retained stock, not just authored heights.
    let ends: Vec<_> = c
        .path
        .moves
        .iter()
        .filter(|m| {
            (m.from.z - top).abs() < 1e-8
                && (m.to.z - top).abs() < 1e-8
                && m.to.x.hypot(m.to.y) > m.from.x.hypot(m.from.y) + 1e-6
        })
        .collect();
    assert!(!ends.is_empty(), "the generated pass must end at Z{top}");
    assert!(
        top < 48.0
            && ends
                .iter()
                .any(|m| m.from.x.hypot(m.from.y) < diameter * 0.5),
        "exit begins inside the uncut bar"
    );
    let out = std::env::temp_dir().join(format!("camcraft-323-{tag}-{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    eprintln!(
        "{tag}: exits={ends:?}; warnings={:?}; posted={posted:?}",
        c.warnings
    );
    assert!(
        ends.iter()
            .all(|m| m.kind == MoveKind::Feed && m.feed == Some(50.0)),
        "exit through retained stock must use the plunge feed"
    );
    assert!(
        !c.warnings
            .iter()
            .any(|w| matches!(w, Warning::RapidInStock { .. })),
        "{:?}",
        c.warnings
    );
    assert_eq!(
        camcraft_app::operation::status_of(&world, &e),
        OpStatus::Ok,
        "{:?}",
        c.warnings
    );
    assert!(posted.is_ok(), "{posted:?}");
}

#[test]
fn a_multipass_in_a_longer_bar_exits_at_feed_and_posts() {
    let part = revolve(&[
        (0.0, -16.0),
        (6.0, -16.0),
        (6.0, 2.0),
        (3.5, 2.0),
        (3.5, 40.0),
        (0.0, 40.0),
    ]);
    assert_long_bar_exit(
        "multi",
        part,
        12.0,
        OpParams::MultiPassTurning {
            tolerance: 0.05,
            clearance_z: Height::fixed(56.0),
            top_z: Height::fixed(40.0),
            bottom_z: Height::fixed(-16.0),
            stepdown: 0.5,
            stock_to_leave: 0.25,
            passes: 3,
            feed: 200.0,
            plunge_feed: 50.0,
        },
        40.0,
    );
}

#[test]
fn a_chamfer_in_a_longer_bar_exits_at_feed_and_posts() {
    let part = revolve(&[
        (0.0, -16.0),
        (10.0, -16.0),
        (10.0, 38.0),
        (8.0, 40.0),
        (0.0, 40.0),
    ]);
    assert_long_bar_exit(
        "chamfer",
        part,
        20.0,
        OpParams::Chamfering {
            tolerance: 0.05,
            clearance_z: Height::fixed(56.0),
            top_z: Height::fixed(40.0),
            bottom_z: Height::fixed(38.0),
            angle_deg: 45.0,
            feed: 200.0,
            plunge_feed: 50.0,
        },
        40.0,
    );
}

#[test]
fn a_taper_in_a_longer_bar_exits_at_feed_and_posts() {
    let part = revolve(&[(0.0, -16.0), (10.0, -16.0), (8.0, 40.0), (0.0, 40.0)]);
    assert_long_bar_exit(
        "taper",
        part,
        20.0,
        OpParams::TaperTurning {
            tolerance: 0.05,
            clearance_z: Height::fixed(56.0),
            top_z: Height::fixed(40.0),
            bottom_z: Height::fixed(-16.0),
            bottom_dia: 20.0,
            top_dia: 16.0,
            feed: 200.0,
            plunge_feed: 50.0,
        },
        40.0,
    );
}

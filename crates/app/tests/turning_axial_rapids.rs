//! Issue #103: a turned path's axial rapids judged in the meridian
//! plane against the revolved stock; issue #130: the rough reads that
//! stock, so in a bar wider than the part it approaches from the bar
//! and posts clean, as it does beside a bar the part's own size. The
//! law's own convictions are pinned in turning_rapids_tests.
//!
//! Gate map (M4.4): covers app::operation (turning_rapids, generate's
//! rapid law wiring, job's prior plan for the turned family),
//! app::posting (the verification gate on a lathe document).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;

const TAU: f64 = std::f64::consts::TAU;

fn revolve(meridian: &[(f64, f64)], segs: usize) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..segs {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

fn lathe() -> Machine {
    Machine {
        model: "TestBench lathe".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("lathe".into()),
        spindle_min: Some(25.0),
        spindle_max: Some(12000.0),
        ..Default::default()
    }
}

/// The stage-B flanged bar (r6, neck r3.5 over Z 2) in a round bar of
/// `diameter` from Z -16 to 16, roughed by R1.
fn rough_in_bar(diameter: f64) -> (World, Vec<Warning>, Vec<camcraft_app::cl::Move>) {
    rough_in(diameter, PART_TOP)
}

/// The part's free end (Z): the bar of [`rough_in_bar`] ends here too.
const PART_TOP: f64 = 16.0;

/// [`rough_in_bar`] in a bar that runs from Z -16 to `bar_top`: past
/// the part's free end when `bar_top` > [`PART_TOP`] (#229).
fn rough_in(diameter: f64, bar_top: f64) -> (World, Vec<Warning>, Vec<camcraft_app::cl::Move>) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
    let cmd = SetStock::new(&world, StockShape::Cylinder { diameter, height: bar_top + 16.0 }, at);
    history.execute(&world, cmd.expect("bar stock"));
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe())).expect("lathe"));
    history.execute(&world, camcraft_app::commands::SelectPost::new(&world, camcraft_app::schema::PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    let name = "TN-1600-0R8";
    let tool = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, name, tool).expect("tool"));
    let bar = revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (3.5, 2.0), (3.5, 16.0), (0.0, 16.0)], 16);
    history.execute(&world, ImportModel::from_mesh("bar", bar));
    let rough = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(-16.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let cmd = CreateOperation::new(&world, "R1", name, SrcRef::Entity("bar".into()), rough);
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "R1").expect("generate"));
    let e = model::find_by_name(&world, "R1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("cache");
    (world, cache.warnings.clone(), cache.path.moves.clone())
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-axial-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
fn a_rough_in_a_wider_bar_approaches_from_the_bar_and_posts_clean() {
    // Issue #130 (was #103's refusal pin): a 20 mm bar roughed down to
    // the 12 mm flanged part. The kernel reads the revolved stock: the
    // approach stands clear of the bar's r 10 by the retract clearance,
    // the ladder starts at the bar's skin - a pass over the flange whose
    // nose bites the outermost stepdown of the bar - and steps down to
    // the part plus the leave. The meridian law then has nothing to
    // name and the post writes the program.
    let (world, warns, moves) = rough_in_bar(20.0);
    let (nose, stepdown, leave) = (0.8, 0.8, 0.25);
    let rho = |q: Point3| q.x.hypot(q.y);
    let m0 = moves.first().expect("a path");
    assert!(m0.kind == MoveKind::Rapid && rho(m0.to) >= 10.0 + nose + 1.0 - 1e-9, "the approach clears the bar's OD: {m0:?}");
    for m in moves.iter().filter(|m| m.kind == MoveKind::Rapid && (m.from.z - m.to.z).abs() > 1e-9) {
        assert!(rho(m.from) >= 10.0 + nose + 1.0 - 1e-9, "an axial rapid inside the bar: {m:?}");
    }
    let flange: Vec<f64> = moves.iter().filter(|m| m.kind == MoveKind::Feed && m.to.z < 1.0).map(|m| rho(m.to) - nose).collect();
    assert!(flange.iter().any(|&r| r < 10.0 && r >= 10.0 - stepdown - 1e-6), "no pass starts at the bar's skin: {flange:?}");
    let floor = flange.iter().copied().fold(f64::INFINITY, f64::min);
    assert!((floor - (6.0 + leave)).abs() < 0.05, "the ladder ends at the part plus the leave: {floor}");
    assert!(!warns.iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{warns:?}");
    let out = tmp_nc("wide");
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    assert!(posted.is_ok(), "{posted:?}");
}

#[test]
fn the_same_approach_beside_a_bar_the_parts_size_is_clean() {
    let (world, warns, _) = rough_in_bar(12.0);
    assert!(!warns.iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{warns:?}");
    let out = tmp_nc("snug");
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    assert!(posted.is_ok(), "{posted:?}");
}

/// #229: a bar longer than the part. Each rough pass ends at the part's
/// free end, inside the bar's uncut end; the move out of the cut there
/// must not be a rapid through that stock, and the job posts.
#[test]
fn a_rough_in_a_bar_longer_than_the_part_posts_clean() {
    let (world, warns, _) = rough_in(20.0, PART_TOP + 8.0);
    assert!(!warns.iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{warns:?}");
    let out = tmp_nc("long");
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    assert!(posted.is_ok(), "{posted:?}");
}

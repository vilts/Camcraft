//! The lathe lift (#19, M13.1 owed): turning programs simulate on the
//! FIELD engine through the document's own walk (`stock_view`'s field
//! eye, `plan_program` carrying the insert's nose and the op's fitted
//! spindle), and the machined stock answers the meridian law - the
//! revolved nose-centre stadium the turning kernels and
//! `turning::revolve` reason in (M5.2 E) - at every azimuth.
//!
//! The parity law, written here independently of the engine: a cell
//! centre of the stock is air iff some FEED chord's nose stadium in
//! the (radius, station) half-plane about the fixture's own axis
//! (world Z - every fixture is revolved about it) holds it. The field
//! reads its cut layer in closed form at cell centres, so the sign
//! must match everywhere off the law's surface and off the stock's
//! skin - zero misses, the sim suite's `stock_lathe.rs` law carried
//! onto real generated programs. A grooving blade (#21) answers its
//! own swept rectangle the same way; the column verdict refuses every
//! lathe program by name.
//!
//! Provenance: our own engines and the M5.2 meridian law; nothing from
//! vendor binaries.
//!
//! Gate map (M4.4): covers app::stock_sim, sim::stock_sim (plan_program's lathe clause,
//! stock_sim::lathe), app::stock_view (field_view_with's lathe arm),
//! app::operation (source_facets).
//! Lane shape (M4.4): all tests here run in seconds in debug.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock, SetWcsOrigin};
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock_sim::LATHE_VERDICT_REFUSAL;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
/// The display budget the scans run at: a ~0.5 mm cell on the 40 mm
/// stocks - fine enough to read every rung, seconds in debug.
const TEXELS: f64 = 512_000.0;

/// A closed solid of revolution about world Z, 16 facets round.
fn revolve(meridian: &[(f64, f64)]) -> Mesh {
    revolve_at(meridian, (0.0, 0.0))
}

/// The same about the vertical line through `(ox, oy)`.
fn revolve_at(meridian: &[(f64, f64)], (ox, oy): (f64, f64)) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(ox + r * th.cos(), oy + r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
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

/// A lathe document: `dz`-tall box stock, the lathe mounted, the
/// house insert, boring bar and grooving blade on the rack, `mesh`
/// imported as "bar".
fn lathe_world(dz: f64, mesh: Mesh) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let shape = camcraft_app::stock::StockShape::Box { dx: 40.0, dy: 40.0, dz };
    history.execute(&world, SetStock::new(&world, shape, Location::default()).expect("box stock"));
    let mch = Machine { model: "TestBench lathe".into(), kind: Some("lathe".into()), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(mch)).expect("lathe"));
    for name in ["TN-1600-0R8", "BR-1000", "GT-0200"] {
        let tool = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry");
        history.execute(&world, AddTool::new(&world, name, tool).expect("catalog tool"));
    }
    history.execute(&world, ImportModel::from_mesh("bar", mesh));
    (world, history)
}

fn generate(world: &World, history: &mut History, name: &str, tool: &str, params: OpParams) {
    let op = CreateOperation::new(world, name, tool, SrcRef::Entity("bar".into()), params).expect("create");
    history.execute(world, op);
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op exists");
    let st = camcraft_app::operation::status_of(world, &e);
    assert_eq!(st, OpStatus::Ok, "{name} generated");
}

/// Every FEED chord of the named ops as a meridian segment about
/// the vertical line through `(ox, oy)`, the second end signed
/// against the first's side.
fn chords(world: &World, ops: &[&str], (ox, oy): (f64, f64)) -> Vec<(f64, f64, f64, f64)> {
    let mut out = Vec::new();
    for name in ops {
        let e = model::find_by_name(world, name).expect("op exists");
        let path = model::toolpath_of(world, &e).expect("cached").path;
        for m in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
            assert!(m.arc.is_none(), "{name}: the nose kernels write straight chords");
            let (ax, ay, bx, by) = (m.from.x - ox, m.from.y - oy, m.to.x - ox, m.to.y - oy);
            let (r0, r1) = (ax.hypot(ay), bx.hypot(by));
            let side = ax * bx + ay * by;
            out.push((r0, m.from.z, if side < 0.0 { -r1 } else { r1 }, m.to.z));
        }
    }
    out
}

/// The meridian law: signed distance to the nearest nose stadium.
fn law(chords: &[(f64, f64, f64, f64)], nose: f64, rho: f64, z: f64) -> f64 {
    let seg = |p: f64, (ax, az, bx, bz): (f64, f64, f64, f64)| {
        let (dx, dz) = (bx - ax, bz - az);
        let dd = dx * dx + dz * dz;
        let t = if dd > 0.0 { (((p - ax) * dx + (z - az) * dz) / dd).clamp(0.0, 1.0) } else { 0.0 };
        (p - ax - t * dx).hypot(z - az - t * dz)
    };
    chords.iter().map(|c| seg(rho, *c).min(seg(-rho, *c))).fold(f64::INFINITY, f64::min) - nose
}

/// The stock's world box, read off the stock entity's own mesh.
fn stock_box(world: &World) -> (Point3, Point3) {
    let e = model::find_by_name(world, &camcraft_app::setup::stock_entity_name(world, None)).expect("stock");
    let tris = model::world_facets(world, &e).expect("stock mesh");
    let mut lo = Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut hi = Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for v in tris.iter().flat_map(|t| [t.a, t.b, t.c]) {
        lo = Point3::new(lo.x.min(v.x), lo.y.min(v.y), lo.z.min(v.z));
        hi = Point3::new(hi.x.max(v.x), hi.y.max(v.y), hi.z.max(v.z));
    }
    (lo, hi)
}

/// Walk the document through the field eye and scan every cell
/// centre two cells inside the stock box: (checked, cut, misses,
/// worst |law| at a miss).
fn meridian_parity(world: &World, ops: &[&str], nose: f64, axis: (f64, f64)) -> (usize, usize, usize, f64) {
    let view = camcraft_app::stock_view::field_view_with(world, TEXELS).expect("the field eye cuts the lathe program");
    let cs = chords(world, ops, axis);
    assert!(!cs.is_empty(), "fixture: the ops cut something");
    let (lo, hi) = stock_box(world);
    let c = view.cell;
    let idx = |v: f64| (v / c).floor() as i64;
    let (mut checked, mut cut, mut misses, mut worst) = (0usize, 0usize, 0usize, 0.0f64);
    for k in idx(lo.z + 2.0 * c)..idx(hi.z - 2.0 * c) {
        let z = (k as f64 + 0.5) * c;
        for j in idx(lo.y + 2.0 * c)..idx(hi.y - 2.0 * c) {
            let y = (j as f64 + 0.5) * c;
            for i in idx(lo.x + 2.0 * c)..idx(hi.x - 2.0 * c) {
                let x = (i as f64 + 0.5) * c;
                let l = law(&cs, nose, (x - axis.0).hypot(y - axis.1), z);
                if l.abs() < 1e-6 {
                    continue; // on the law's own surface: either sign is honest
                }
                let air = view.stock.distance(Point3::new(x, y, z)) > 0.0;
                checked += 1;
                cut += usize::from(l < 0.0);
                if air != (l < 0.0) {
                    misses += 1;
                    worst = worst.max(l.abs());
                }
            }
        }
    }
    (checked, cut, misses, worst)
}

/// The blade law (#21): every FEED chord of `op` is radial or axial
/// (the grooving kernel's plunges and steps), so the sharp blade's
/// rectangle swept along it is the rectangle spanning both ends -
/// `width` across the station, tip to `height` outward - and the cell
/// centre's sign is that union's, read in its meridian both signs.
fn blade_parity(world: &World, op: &str, width: f64, height: f64) -> (usize, usize, usize, f64) {
    let view = camcraft_app::stock_view::field_view_with(world, TEXELS).expect("the field eye cuts the blade program");
    let rects: Vec<(f64, f64, f64, f64)> = chords(world, &[op], (0.0, 0.0))
        .into_iter()
        .map(|(r0, z0, r1, z1)| {
            assert!((r0 - r1).abs() < 1e-9 || (z0 - z1).abs() < 1e-9, "fixture: a groove chord is radial or axial");
            (r0.min(r1), r0.max(r1) + height, z0.min(z1) - 0.5 * width, z0.max(z1) + 0.5 * width)
        })
        .collect();

    let sd = |rho: f64, z: f64| {
        rects.iter().map(|&(ra, rb, za, zb)| {
            let one = |p: f64| {
                let (qx, qz) = ((p - 0.5 * (ra + rb)).abs() - 0.5 * (rb - ra), (z - 0.5 * (za + zb)).abs() - 0.5 * (zb - za));
                qx.max(0.0).hypot(qz.max(0.0)) + qx.max(qz).min(0.0)
            };
            one(rho).min(one(-rho))
        }).fold(f64::INFINITY, f64::min)
    };
    let (lo, hi) = stock_box(world);
    let c = view.cell;
    let idx = |v: f64| (v / c).floor() as i64;
    let (mut checked, mut cut, mut misses, mut worst) = (0usize, 0usize, 0usize, 0.0f64);
    for k in idx(lo.z + 2.0 * c)..idx(hi.z - 2.0 * c) {
        let z = (k as f64 + 0.5) * c;
        for j in idx(lo.y + 2.0 * c)..idx(hi.y - 2.0 * c) {
            let y = (j as f64 + 0.5) * c;
            for i in idx(lo.x + 2.0 * c)..idx(hi.x - 2.0 * c) {
                let x = (i as f64 + 0.5) * c;
                let l = sd(x.hypot(y), z);
                if l.abs() < 1e-6 {
                    continue;
                }
                let air = view.stock.distance(Point3::new(x, y, z)) > 0.0;
                checked += 1;
                cut += usize::from(l < 0.0);
                if air != (l < 0.0) {
                    misses += 1;
                    worst = worst.max(l.abs());
                }
            }
        }
    }
    (checked, cut, misses, worst)
}

fn assert_parity(label: &str, world: &World, ops: &[&str], nose: f64) {
    assert_parity_about(label, world, ops, nose, (0.0, 0.0));
}

fn assert_parity_about(label: &str, world: &World, ops: &[&str], nose: f64, axis: (f64, f64)) {
    let (checked, cut, misses, worst) = meridian_parity(world, ops, nose, axis);
    println!("{label}: meridian parity over {checked} cell centres ({cut} cut): {misses} sign misses, worst |law| at a miss {worst:.3e}");
    assert!(cut > 100, "{label}: fixture - the program must cut a measurable volume ({cut})");
    assert_eq!(misses, 0, "{label}: the field disagrees with the meridian law");
}

/// The stage-A bar: wall r6 to z 2, neck r5 to the top, z -16..16.
fn bar_mesh() -> Mesh {
    revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)])
}

#[test]
fn facing_simulates_on_the_field_with_meridian_parity() {
    let (world, mut history) = lathe_world(40.0, bar_mesh());
    let p = OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    generate(&world, &mut history, "F1", "TN-1600-0R8", p);
    assert_parity("facing", &world, &["F1"], 0.8);
}

#[test]
fn a_bar_off_the_origin_is_turned_about_its_own_fitted_axis() {
    // The plan's frame is the op's FITTED spindle, not world Z through
    // the origin: the bar stands about (3, -2), and the field must
    // revolve the facing ladder about that line - an origin-axis sweep
    // would stand the whole cut 3.6 mm off it. The setup's work offset
    // sits ON the bar's axis, so the machine spindle the lathe post
    // turns about (world Z through G54) IS the fitted axis - the only
    // case generation lets through (#62 refuses an op whose fitted
    // axis is not the spindle).
    let (world, mut history) = lathe_world(40.0, revolve_at(&[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)], (3.0, -2.0)));
    let on_axis = Location { pos: Point3::new(3.0, -2.0, 0.0), rot: IDENTITY_Q };
    history.execute(&world, SetWcsOrigin::new(&world, on_axis).expect("G54 on the bar's axis"));
    let p = OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    generate(&world, &mut history, "F1", "TN-1600-0R8", p);
    assert_parity_about("facing off the origin", &world, &["F1"], 0.8, (3.0, -2.0));
}

#[test]
fn profile_rough_and_finish_simulate_with_meridian_parity() {
    let flanged = revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (3.5, 2.0), (3.5, 16.0), (0.0, 16.0)]);
    let (world, mut history) = lathe_world(40.0, flanged);
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
    let finish = OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(2.0),
        feed: 120.0,
        plunge_feed: 60.0,
    };
    // Names are the program order's tie-break for ops created without an
    // explicit position (`setup::operation_order`), so they must sort
    // rough first: "PF1" sorted ahead of "PR1" and slipped the finish in
    // front of the already-generated rough, which (#109: the rough reads
    // the program ahead) correctly read Stale (#186). Same as lathe_post.
    generate(&world, &mut history, "R1", "TN-1600-0R8", rough);
    generate(&world, &mut history, "S1", "TN-1600-0R8", finish);
    let order: Vec<String> = camcraft_app::setup::operation_order(&world).into_iter().map(|(n, _)| n).collect();
    assert_eq!(order, ["R1", "S1"], "fixture: the rough machines ahead of the finish");
    assert_parity("profile rough+finish", &world, &["R1", "S1"], 0.8);
}

#[test]
fn multi_pass_contour_simulates_with_meridian_parity() {
    let stepped = revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)]);
    let (world, mut history) = lathe_world(40.0, stepped);
    let p = OpParams::MultiPassTurning {
        tolerance: 0.01,
        clearance_z: Height::fixed(30.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(-16.0),
        stepdown: 0.5,
        stock_to_leave: 0.25,
        passes: 3,
        feed: 200.0,
        plunge_feed: 50.0,
    };
    generate(&world, &mut history, "MP1", "TN-1600-0R8", p);
    assert_parity("multi-pass contour", &world, &["MP1"], 0.8);
}

#[test]
fn boring_simulates_inside_the_bore_with_meridian_parity() {
    let bored = revolve(&[(0.0, -50.0), (10.0, -50.0), (10.0, 50.0), (6.0, 50.0), (6.0, -20.0), (0.0, -20.0)]);
    let (world, mut history) = lathe_world(110.0, bored);
    let p = OpParams::BoringTurning {
        tolerance: 0.05,
        clearance_z: Height::fixed(60.0),
        top_z: Height::fixed(40.0),
        bottom_z: Height::fixed(-10.0),
        feed: 120.0,
        plunge_feed: 40.0,
    };
    generate(&world, &mut history, "BT1", "BR-1000", p);
    let nose = catalog().into_iter().find(|(n, _)| *n == "BR-1000").map(|(_, t)| t.nose_radius).expect("bar");
    assert_parity("boring", &world, &["BT1"], nose);
}

#[test]
fn a_grooving_blade_cuts_its_rectangle_and_the_verdict_refuses_the_lathe() {
    // #21: the blade sweeps its own meridian rectangle - `width` along
    // the axis centred on the move, `flute_len` outward of its tip -
    // never the nose disc, which would under-claim the kerf.
    // The groove stands inside the 0..40 box stock: the kerf is steel.
    let grooved = revolve(&[
        (0.0, -16.0),
        (6.0, -16.0),
        (6.0, 13.2),
        (4.0, 13.2),
        (4.0, 16.4),
        (6.0, 16.4),
        (6.0, 28.0),
        (0.0, 28.0),
    ]);
    let (world, mut history) = lathe_world(40.0, grooved);
    let g = OpParams::GrooveRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(20.0),
        top_z: Height::fixed(16.4),
        bottom_z: Height::fixed(13.2),
        stepdown: 0.5,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 60.0,
    };
    generate(&world, &mut history, "G1", "GT-0200", g);
    let blade = catalog().into_iter().find(|(n, _)| *n == "GT-0200").map(|(_, t)| t).expect("blade");
    assert_eq!(blade.corner_r, 0.0, "fixture: the house blade is sharp");
    let (checked, cut, misses, worst) = blade_parity(&world, "G1", blade.width, blade.flute_len);
    println!("groove: blade parity over {checked} cell centres ({cut} cut): {misses} sign misses, worst |law| at a miss {worst:.3e}");
    assert!(cut > 100, "fixture: the groove must cut a measurable volume ({cut})");
    assert_eq!(misses, 0, "the field disagrees with the blade's swept rectangle");

    // A nose program the eye cuts is still not the column verdict's
    // to judge: refused by name, never a silent pass.
    let (world, mut history) = lathe_world(40.0, bar_mesh());
    let p = OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    generate(&world, &mut history, "F1", "TN-1600-0R8", p);
    let e = camcraft_app::stock_sim::simulate_document(&world).expect_err("the column verdict has no revolved reading");
    assert!(e.contains("F1") && e.contains(LATHE_VERDICT_REFUSAL), "{e}");
}

//! #2436: whole-model 2D Contour on a part whose outline grows outward
//! below Top Z (a chamfered island on a plate, an open-ended slot
//! floor closing into the plate perimeter) must continue down to Bottom Z
//! using each level's part outline with tabs written, or refuse by name
//! with the stopped level rather than dropping lower levels silently.
//!
//! Gate map (#2436): covers app::operation (generate, 2D Contour whole model),
//! app::commands (GenerateOperation), app::posting.
//!
//! Provenance: our own grid-matched watertight mesh of a stepped and
//! slotted flange; no vendor material consulted.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle, slice};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-1000-4FL";
const EPS: f64 = 1e-3;

/// Grid-matched watertight shell of a 100 x 60 x 10 plate (z 0..10)
/// centred at XY (0, 0), with an open-ended slot (x 35..45, y 15..30,
/// floor z 7) and a 60 x 30 x 5 island on top (z 10..15) with a 1 x 45
/// deg chamfer on its top edges (z 14..15 to 58 x 28).
fn stepped_slotted_flange() -> Mesh {
    let xs = [-50.0, -30.0, 30.0, 35.0, 45.0, 50.0];
    let ys = [-30.0, -15.0, 15.0, 30.0];
    let mut f = Vec::new();
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let quad = |f: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, d: Point3| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };

    // Full bottom face at z = 0 (-z normal) and full cell tops/floors.
    for i in 0..5 {
        for j in 0..3 {
            let (x0, x1, y0, y1) = (xs[i], xs[i + 1], ys[j], ys[j + 1]);
            // Bottom cap (-z): CW seen from +z.
            quad(&mut f, p(x0, y0, 0.0), p(x0, y1, 0.0), p(x1, y1, 0.0), p(x1, y0, 0.0));
            let top_z = if i == 3 && j == 2 {
                7.0
            } else if i == 1 && j == 1 {
                continue; // closed later at the chamfered island top
            } else {
                10.0
            };
            // Top face (+z): CCW seen from +z.
            quad(&mut f, p(x0, y0, top_z), p(x1, y0, top_z), p(x1, y1, top_z), p(x0, y1, top_z));
        }
    }

    // Perimeter walls at outer box [-50..50] x [-30..30].
    let edge_wall = |f: &mut Vec<Triangle>, ax: f64, ay: f64, bx: f64, by: f64, z0: f64, z1: f64| {
        quad(f, p(ax, ay, z0), p(bx, by, z0), p(bx, by, z1), p(ax, ay, z1));
    };
    for i in 0..5 {
        edge_wall(&mut f, xs[i], ys[0], xs[i + 1], ys[0], 0.0, 10.0);
        let z_north = if i == 3 { 7.0 } else { 10.0 };
        edge_wall(&mut f, xs[i + 1], ys[3], xs[i], ys[3], 0.0, z_north);
    }
    for j in 0..3 {
        edge_wall(&mut f, xs[5], ys[j], xs[5], ys[j + 1], 0.0, 10.0);
        edge_wall(&mut f, xs[0], ys[j + 1], xs[0], ys[j], 0.0, 10.0);
    }

    // Slot side and back walls (x 35..45, y 15..30, z 7..10).
    edge_wall(&mut f, 35.0, 30.0, 35.0, 15.0, 7.0, 10.0);
    edge_wall(&mut f, 35.0, 15.0, 45.0, 15.0, 7.0, 10.0);
    edge_wall(&mut f, 45.0, 15.0, 45.0, 30.0, 7.0, 10.0);

    // Island walls (z 10..14), 1 x 45 deg chamfer (z 14..15), top cap (z 15).
    let base = [(-30.0, -15.0), (30.0, -15.0), (30.0, 15.0), (-30.0, 15.0)];
    let top = [(-29.0, -14.0), (29.0, -14.0), (29.0, 14.0), (-29.0, 14.0)];
    for k in 0..4 {
        let n = (k + 1) % 4;
        edge_wall(&mut f, base[k].0, base[k].1, base[n].0, base[n].1, 10.0, 14.0);
        quad(
            &mut f,
            p(base[k].0, base[k].1, 14.0),
            p(base[n].0, base[n].1, 14.0),
            p(top[n].0, top[n].1, 15.0),
            p(top[k].0, top[k].1, 15.0),
        );
    }
    quad(
        &mut f,
        p(top[0].0, top[0].1, 15.0),
        p(top[1].0, top[1].1, 15.0),
        p(top[2].0, top[2].1, 15.0),
        p(top[3].0, top[3].1, 15.0),
    );

    let mesh = Mesh::from_facets(f);
    for z in [0.5, 5.0, 7.5, 9.5, 11.5, 14.5] {
        assert!(slice::slice(&mesh.facets, z).is_ok(), "manifold fixture at z {z}");
    }
    mesh
}

fn world_with(mesh: Mesh, oversized_stock: bool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("flange", mesh));
    let (dx, dy, x0, y0) = if oversized_stock { (120.0, 80.0, -60.0, -40.0) } else { (100.0, 60.0, -50.0, -30.0) };
    let loc = Location { pos: Point3::new(x0, y0, 0.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx, dy, dz: 17.0 }, loc).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, t).expect("tool"));
    (world, h)
}

fn contour_params(top_z: Height, stepdown: f64, tab_width: f64, tab_height: f64) -> OpParams {
    OpParams::Contour {
        side: Side::Auto,
        stock_to_leave: 0.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        retract_z: Height::of(HeightReference::TopOfStock, 10.0),
        top_z,
        bottom_z: Height::of(HeightReference::BottomOfModel, -0.5),
        stepdown,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        lead: Lead::Linear { length: 8.0 },
        tolerance: 0.01,
        tab_count: 4,
        tab_width,
        tab_height,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
    }
}

fn run(world: &World, h: &mut History, params: OpParams) -> (Toolpath, Vec<Warning>) {
    let create = CreateOperation::new(world, "C1", TOOL, SrcRef::Entity("flange".into()), params);
    h.execute(world, create.expect("create"));
    h.execute(world, GenerateOperation::new(world, "C1").expect("generate"));
    let e = model::find_by_name(world, "C1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("toolpath");
    (tp.path.clone(), tp.warnings.clone())
}

fn deepest_feed_z(t: &Toolpath) -> f64 {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < 1e-9)
        .map(|m| m.to.z)
        .fold(f64::INFINITY, f64::min)
}

fn feeds_at_z(t: &Toolpath, z: f64) -> Vec<(f64, f64)> {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - z).abs() < EPS && (m.to.z - z).abs() < EPS)
        .flat_map(|m| [(m.from.x, m.from.y), (m.to.x, m.to.y)])
        .collect()
}

/// Interpolated `y` where planar feed moves at `z` cross `x = x_probe` on the +Y side.
fn north_crossings_at_x(t: &Toolpath, z: f64, x_probe: f64) -> Vec<f64> {
    let mut ys = Vec::new();
    for m in &t.moves {
        if m.kind != MoveKind::Feed || (m.from.z - z).abs() >= EPS || (m.to.z - z).abs() >= EPS {
            continue;
        }
        let (ax, bx) = (m.from.x, m.to.x);
        if (ax <= x_probe && bx >= x_probe) || (bx <= x_probe && ax >= x_probe) {
            let y = if (bx - ax).abs() < 1e-12 {
                0.5 * (m.from.y + m.to.y)
            } else {
                let u = (x_probe - ax) / (bx - ax);
                m.from.y + u * (m.to.y - m.from.y)
            };
            if y > 0.0 {
                ys.push(y);
            }
        }
    }
    ys
}

/// Number of separate planar cutting windows at level `z` (for 4 tabs on
/// one closed loop there are 4 feed runs separated by tab hops).
fn cut_windows_at_z(t: &Toolpath, z: f64) -> usize {
    let mut windows = 0;
    let mut in_window = false;
    for m in &t.moves {
        let on_z = m.kind == MoveKind::Feed && (m.from.z - z).abs() < EPS && (m.to.z - z).abs() < EPS;
        if on_z {
            if !in_window {
                windows += 1;
                in_window = true;
            }
        } else if m.kind == MoveKind::Rapid || m.to.z > z + EPS {
            in_window = false;
        }
    }
    windows
}

fn assert_full_plate_outline(t: &Toolpath, z: f64, tag: &str) {
    let pts = feeds_at_z(t, z);
    assert!(!pts.is_empty(), "{tag}: no moves at level {z}");
    let min_x = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let max_x = pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let min_y = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_y = pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    assert!(
        min_x <= -55.0 + 0.05 && max_x >= 55.0 - 0.05 && min_y <= -35.0 + 0.05 && max_y >= 35.0 - 0.05,
        "{tag}: must trace 100x60 plate offset by r=5 ([-55..55] x [-35..35]), got x [{min_x:.3}..{max_x:.3}] y [{min_y:.3}..{max_y:.3}]"
    );
    // Across the closed slot mouth (x = 40, y > 0), the path stays out on the plate line y = 35, never dipping into the slot.
    let mouth_ys = north_crossings_at_x(t, z, 40.0);
    assert!(!mouth_ys.is_empty() && mouth_ys.iter().all(|&y| (y - 35.0).abs() < 0.05), "{tag}: closed slot mouth {mouth_ys:?}");
}

#[test]
fn case1_chamfered_island_top_contours_plate_to_bottom_with_tabs() {
    for oversized in [false, true] {
        let (world, mut h) = world_with(stepped_slotted_flange(), oversized);
        let params = contour_params(Height::of(HeightReference::TopOfModel, 0.0), 3.5, 5.0, 1.0);
        let (t, warns) = run(&world, &mut h, params);
        assert!(
            !warns.iter().any(|w| w.blocks_post()),
            "oversized={oversized}: blocking warnings {warns:?} / {:?}",
            t.notes
        );
        assert!(
            (deepest_feed_z(&t) - (-0.5)).abs() < EPS,
            "oversized={oversized}: deepest z {} != -0.5; notes: {:?}",
            deepest_feed_z(&t),
            t.notes
        );
        assert_full_plate_outline(&t, -0.5, "Case 1 bottom (-0.5)");
        assert_eq!(
            cut_windows_at_z(&t, -0.5),
            4,
            "Case 1 must write 4 tab windows at z -0.5; notes: {:?}",
            t.notes
        );
        let nc = std::env::temp_dir().join(format!("camcraft-2436-c1-{oversized}-{}.nc", std::process::id()));
        let _ = std::fs::remove_file(&nc);
        let posted = posting::post_setup(&world, &nc);
        let _ = std::fs::remove_file(&nc);
        assert!(posted.is_ok(), "oversized={oversized}: post blocked: {:?}", posted.err());
    }
}

#[test]
fn case2_plate_top_with_open_slot_contours_below_slot_floor_to_bottom_with_tabs() {
    for oversized in [false, true] {
        let (world, mut h) = world_with(stepped_slotted_flange(), oversized);
        let params = contour_params(Height::of(HeightReference::BottomOfModel, 10.0), 2.0, 6.0, 1.5);
        let (t, warns) = run(&world, &mut h, params);
        assert!(
            !warns.iter().any(|w| w.blocks_post()),
            "oversized={oversized}: blocking warnings {warns:?} / {:?}",
            t.notes
        );
        // Upper levels cut into the open slot mouth (y < 30 near x 40).
        let notch_pts = feeds_at_z(&t, 8.0);
        assert!(
            notch_pts.iter().any(|&(x, y)| x > 34.0 && x < 46.0 && y < 29.0),
            "Case 2 z 8 should follow the open slot notch: {notch_pts:?}"
        );
        // Levels below the z 7 slot floor close across the plate perimeter down to Bottom Z (-0.5) with 4 tabs on z 0 and z -0.5.
        assert!(
            (deepest_feed_z(&t) - (-0.5)).abs() < EPS,
            "oversized={oversized}: deepest z {} != -0.5; notes: {:?}",
            deepest_feed_z(&t),
            t.notes
        );
        assert_full_plate_outline(&t, 6.0, "Case 2 z 6.0");
        assert_full_plate_outline(&t, 0.0, "Case 2 z 0.0");
        assert_full_plate_outline(&t, -0.5, "Case 2 bottom (-0.5)");
        assert_eq!(cut_windows_at_z(&t, 0.0), 4, "4 tabs at z 0.0");
        assert_eq!(cut_windows_at_z(&t, -0.5), 4, "4 tabs at z -0.5");
        let nc = std::env::temp_dir().join(format!("camcraft-2436-c2-{oversized}-{}.nc", std::process::id()));
        let _ = std::fs::remove_file(&nc);
        let posted = posting::post_setup(&world, &nc);
        let _ = std::fs::remove_file(&nc);
        assert!(posted.is_ok(), "oversized={oversized}: post blocked: {:?}", posted.err());
    }
}

#[test]
fn whole_model_contour_that_cannot_reach_bottom_is_refused_with_the_stopped_level() {
    // A non-manifold open sliver below the slot floor prevents slicing
    // at z = 4.0 while upper levels [10, 8, 7] still slice cleanly: the op
    // must refuse by name with the failed lower level rather than silently
    // posting a partial path stopped at z 7.
    let mut facets = stepped_slotted_flange().facets.to_vec();
    // Open dangling triangle spanning z [2.0, 5.0] breaks slices in (2, 5).
    facets.push(Triangle::new(
        Point3::new(-5.0, -5.0, 2.0),
        Point3::new(5.0, -5.0, 2.0),
        Point3::new(0.0, 5.0, 5.0),
    ));
    let (world, mut h) = world_with(Mesh::from_facets(facets), true);
    let params = contour_params(Height::of(HeightReference::BottomOfModel, 10.0), 2.0, 6.0, 1.5);
    h.execute(
        &world,
        CreateOperation::new(&world, "C1", TOOL, SrcRef::Entity("flange".into()), params).expect("create"),
    );
    let res = GenerateOperation::new(&world, "C1");
    let err = match res {
        Ok(cmd) => {
            h.execute(&world, cmd);
            let e = model::find_by_name(&world, "C1").expect("op");
            let tp = model::toolpath_of(&world, &e);
            panic!(
                "partial whole-model contour must not succeed cleanly; deepest={}, notes={:?}",
                tp.as_ref().map_or(f64::NAN, |c| deepest_feed_z(&c.path)),
                tp.as_ref().map(|c| &c.path.notes)
            );
        }
        Err(e) => e.to_string(),
    };
    assert!(
        err.contains("4.000") || err.contains("7.000"),
        "refusal should name the stopped or failed level: {err}"
    );
}

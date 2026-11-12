//! A typed Slot whose cutter rides its round caps verifies clean
//! (issue #510): the region the gouge law judges a Slot against is the
//! stadium with its caps CIRCUMSCRIBED about the true arcs, so a cutter
//! tangent to a cap is not convicted by chord error, while a path driven
//! past the cap is still a Gouge and never posts. Also pins where the
//! level ladder starts: the first level is the authored Top (the shared
//! 2D ladder convention, see `cut_levels`).
//!
//! Provenance: our own fixtures, specced from the shop rule of holding
//! the cutter radius to the finish wall (machine-tool handbooks) and
//! elementary polygon/circle geometry; no vendor material.
//!
//! Gate map (#510): covers app::operation (material stadium, region_of,
//! generate), sim::verify (section law b), app::posting.

use camcraft_app::cl::{Move, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::{ToolGeom, VerifyContext, Warning, verify};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-0600-4FL";
/// The 6 mm end mill's radius.
const R: f64 = 3.0;

fn slot(width: f64) -> OpParams {
    OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: Point2::new(-15.0, 0.0),
        b: Point2::new(15.0, 0.0),
        width,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(10.0),
        stepdown: 5.0,
        stepover: 2.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
    }
}

/// The finished part a typed slot of `width` leaves: an 80 x 60 x
/// [0, 20] block with the stadium channel (a = (-15, 0), b = (15, 0))
/// sunk from the top to z 10. Each cap is 256 chords on the true arc
/// (sagitta under 0.1 um). The top face fans out to the block's rim
/// along rays from the slot's center. Outward-wound.
fn slotted_block(width: f64) -> Mesh {
    let (h, caps) = (width * 0.5, 256usize);
    let mut ring: Vec<(f64, f64)> = Vec::with_capacity(2 * caps + 2);
    for (cx, t0) in [(15.0, -std::f64::consts::FRAC_PI_2), (-15.0, std::f64::consts::FRAC_PI_2)] {
        for i in 0..=caps {
            let t = t0 + std::f64::consts::PI * i as f64 / caps as f64;
            ring.push((cx + h * t.cos(), h * t.sin()));
        }
    }
    let (ex, ey) = (40.0, 30.0);
    // Where the ray from the origin through `(x, y)` leaves the block.
    let rim = |(x, y): (f64, f64)| {
        let k = (ex / x.abs().max(1e-12)).min(ey / y.abs().max(1e-12));
        (x * k, y * k)
    };
    let corners = [(ex, ey), (-ex, ey), (-ex, -ey), (ex, -ey)];
    let ang = |(x, y): (f64, f64)| y.atan2(x);
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let block = cube(Point3::new(-ex, -ey, 0.0), Point3::new(ex, ey, 20.0));
    let mut f: Vec<Triangle> = block.facets[0..2].to_vec(); // bottom
    f.extend(block.facets[4..12].iter().cloned()); // outer walls
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        // The channel wall faces the void (left of the CCW ring edge).
        f.push(Triangle::new(p(a, 10.0), p(b, 20.0), p(b, 10.0)));
        f.push(Triangle::new(p(a, 10.0), p(a, 20.0), p(b, 20.0)));
        // The channel floor, up.
        f.push(Triangle::new(p((0.0, 0.0), 10.0), p(a, 10.0), p(b, 10.0)));
        // The top between the two rays, up: a fan from `a` over the
        // rim chain (with any block corner between the rays).
        let (ta, mut tb) = (ang(a), ang(b));
        if tb < ta {
            tb += std::f64::consts::TAU;
        }
        let mut chain = vec![rim(a)];
        let mut cs: Vec<(f64, (f64, f64))> = corners
            .iter()
            .map(|&c| {
                let mut t = ang(c);
                if t < ta {
                    t += std::f64::consts::TAU;
                }
                (t, c)
            })
            .filter(|(t, _)| *t > ta && *t < tb)
            .collect();
        cs.sort_by(|x, y| x.0.total_cmp(&y.0));
        chain.extend(cs.into_iter().map(|(_, c)| c));
        chain.push(rim(b));
        for w in chain.windows(2) {
            f.push(Triangle::new(p(a, 20.0), p(w[0], 20.0), p(w[1], 20.0)));
        }
        f.push(Triangle::new(p(a, 20.0), p(rim(b), 20.0), p(b, 20.0)));
    }
    Mesh::from_facets(f)
}

/// An 80 x 60 x 20 stock box (top at z 20); with `part`, the finished
/// part (`slotted_block`) standing in it is the op's source, else the
/// stock is.
fn job(width: f64, part: bool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    if part {
        h.execute(&world, ImportModel::from_mesh("part", slotted_block(width)));
    }
    let stock = SetStock::new(&world, StockShape::Box { dx: 80.0, dy: 60.0, dz: 20.0 }, Location::default());
    h.execute(&world, stock.expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let src = if part { SrcRef::Entity("part".into()) } else { SrcRef::Stock };
    h.execute(&world, CreateOperation::new(&world, "Slot1", TOOL, src, slot(width)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Slot1").expect("generate"));
    (world, h)
}

fn cached(world: &World) -> (Toolpath, Vec<Warning>) {
    let e = model::find_by_name(world, "Slot1").expect("op");
    let c = model::toolpath_of(world, &e).expect("cached path");
    (c.path, c.warnings)
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

fn nc_path(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-slot-caps-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

/// The section law (b) against the op's own region (the one the
/// generator verified against), for a path the test edits.
fn judge(world: &World, path: &Toolpath) -> Vec<Warning> {
    let e = model::find_by_name(world, "Slot1").expect("op");
    let op = model::operation_of(world, &e).expect("operation");
    let region = operation::region_of(world, &op).expect("stadium region");
    let geom = ToolGeom {
        radius: R,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: R,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    verify(
        path,
        &VerifyContext {
            region: &region,
            stock: None,
            model: None,
            tool: &geom,
            cutter: None,
            mode: op.params.kind().info().mode,
            stock_top_z: 20.0,
            clearance_z: 30.0,
            stock_to_leave: 0.0,
            gouge_tol: 0.01,
            z_tol: 1e-6,
            finish_tol: None,
            fixtures: &[],
            spindle: None,
            ..Default::default()
        },
    )
}

#[test]
fn a_slot_riding_its_round_caps_verifies_clean_and_posts() {
    // Width 6: one pass on the centerline, the cutter tangent to both
    // caps at the ends. Width 10: rings at +-2, tangent to the walls and
    // the caps. Before #510 every plunge/feed here was a Gouge.
    for part in [false, true] {
        for width in [6.0, 10.0] {
            let (world, _h) = job(width, part);
            let (path, warns) = cached(&world);
            assert!(!path.moves.is_empty());
            assert_eq!(gouges(&warns), 0, "width {width}, part {part}: {warns:?}");
            assert!(!warns.iter().any(Warning::blocks_post), "width {width}, part {part}: {warns:?}");
            let nc = nc_path(&format!("{width}-{part}"));
            posting::post_setup(&world, &nc).expect("a clean slot posts");
            assert!(nc.exists(), "the program is written");
            let _ = std::fs::remove_file(&nc);
        }
    }
}

#[test]
fn a_slot_driven_past_its_cap_is_still_a_gouge() {
    for width in [6.0, 10.0] {
        let (world, _h) = job(width, false);
        let (path, _) = cached(&world);
        assert_eq!(gouges(&judge(&world, &path)), 0, "the harness reads the generated path clean");
        // Every move pushed 0.05 mm along +X: the far cap is over-cut by
        // 0.05 mm, five times the gouge tolerance.
        let shift = |p: Point3| Point3::new(p.x + 0.05, p.y, p.z);
        let moves = path
            .moves
            .iter()
            .map(|m| {
                let mut s: Move = *m;
                s.from = shift(m.from);
                s.to = shift(m.to);
                if let Some(arc) = s.arc.as_mut() {
                    arc.center = Point2::new(arc.center.x + 0.05, arc.center.y);
                }
                s
            })
            .collect();
        let pushed = Toolpath { moves, ..path.clone() };
        let w = judge(&world, &pushed);
        assert!(gouges(&w) > 0, "width {width}: a 0.05 mm over-cut of the cap convicts: {w:?}");
    }
}

#[test]
fn the_stadium_region_hugs_the_true_caps() {
    // The cap polygon contains the true stadium (every point on the arc
    // is in) and stands no more than a micron outside it (a point 2 um
    // past the arc is out), at angles across the cap - corners included.
    let (world, _h) = job(10.0, false);
    let e = model::find_by_name(&world, "Slot1").expect("op");
    let op = model::operation_of(&world, &e).expect("operation");
    let region = operation::region_of(&world, &op).expect("region");
    for k in 0..=720 {
        let t = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * f64::from(k) / 720.0;
        let (c, s) = (t.cos(), t.sin());
        let at = |rho: f64| Point2::new(15.0 + rho * c, rho * s);
        assert!(region.contains(at(5.0 - 1e-6)), "true arc inside at {t}");
        assert!(!region.contains(at(5.002)), "within 2 um of the arc at {t}");
        let at_a = |rho: f64| Point2::new(-15.0 - rho * c, rho * s);
        assert!(region.contains(at_a(5.0 - 1e-6)), "true arc inside at {t} (a end)");
        assert!(!region.contains(at_a(5.002)), "within 2 um of the arc at {t} (a end)");
    }
}

#[test]
fn the_slot_ladder_starts_at_the_authored_top() {
    // The 2D ladders' shared convention (`cut_levels`): the first level
    // IS Top, so a Top set on the material's top surface opens with a
    // zero-depth pass there, then steps down by `stepdown` to Bottom.
    let (world, _h) = job(6.0, false);
    let (path, _) = cached(&world);
    let mut levels: Vec<f64> = Vec::new();
    for m in path.moves.iter().filter(|m| m.kind == camcraft_app::cl::MoveKind::Feed) {
        if levels.last().is_none_or(|z| (z - m.to.z).abs() > 1e-9) {
            levels.push(m.to.z);
        }
    }
    assert_eq!(levels, vec![20.0, 15.0, 10.0]);
}

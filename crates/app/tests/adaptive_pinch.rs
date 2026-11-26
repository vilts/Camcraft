//! Issue #1620: a whole-model 2D Adaptive with default settings cut the
//! user's `Milling-test.step` 0.94 mm into its D wall at every level.
//! The part pinches the stock's band shut: the cutter-centre area is
//! pieces whose walls lie on one line (the stock's offset edge), and the
//! clearing joined two pieces with a feed along that line, across the
//! gap where the part stands. The chord law
//! (`machining2d::chord_clears`) read the graze along the second piece's
//! wall and passed the whole chord on its midpoint; it now judges every
//! piece of the chord between boundary touches.
//!
//! Gate map (#1620): covers app::operation (generate, area),
//! app::stock_sim, motion machining2d (chord_clear, waterscan, pocket_hsm).
//! Provenance: our own regression of the reviewer's reproduction; no
//! vendor material consulted.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::{Point2, Region};
use camcraft_app::geometry::{Mesh, Point3, cylinder, quat_from_yaw, slice};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpType, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-0600-4FL";
const R: f64 = 3.0;

/// `part` in a `side` x `side` box stock standing on Z `z0` .. `z0 + dz`,
/// aligned or turned a quarter about Z (the same footprint). `flutes`
/// lengthens the end mill's flutes when the part is deeper than the
/// catalog's 15 mm (the shank law is not this issue's).
fn job(part: Mesh, side: f64, z0: f64, dz: f64, turned: bool, flutes: Option<f64>) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", part));
    let rot = if turned { quat_from_yaw(std::f64::consts::FRAC_PI_2) } else { Location::default().rot };
    let at = Location { pos: Point3::new(0.0, 0.0, z0), rot };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: side, dy: side, dz }, at).expect("stock"));
    let mut tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    if let Some(f) = flutes {
        tool.flute_len = f;
    }
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    (world, h)
}

fn generated(world: &World, h: &mut History, src: &str, params: OpParams) -> (Toolpath, usize) {
    h.execute(world, CreateOperation::new(world, "Op1", TOOL, SrcRef::Entity(src.into()), params).expect("create"));
    h.execute(world, GenerateOperation::new(world, "Op1").expect("generate"));
    let e = model::find_by_name(world, "Op1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("the op generates");
    (tp.path.clone(), tp.warnings.len())
}

/// The least distance any cutting move's centre keeps from the Z axis
/// below `top`, read along arcs on the arc.
fn closest_to_axis(t: &Toolpath, top: f64) -> (f64, Point3) {
    let mut worst = (f64::INFINITY, Point3::new(0.0, 0.0, 0.0));
    for m in t.moves.iter().filter(|m| m.kind != MoveKind::Rapid && m.from.z.min(m.to.z) < top - 1e-6) {
        for (a, b) in m.chords(0.01) {
            for k in 0..=16 {
                let s = f64::from(k) / 16.0;
                let (x, y) = (a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s);
                if x.hypot(y) < worst.0 {
                    worst = (x.hypot(y), Point3::new(x, y, m.from.z.min(m.to.z)));
                }
            }
        }
    }
    worst
}

fn retyped(kind: OpType, top: f64, bottom: f64) -> OpParams {
    let mut p = operation::seed_params(kind.ordinal());
    match &mut p {
        OpParams::Adaptive2D { top_z, bottom_z, optimal_load, .. } => {
            *top_z = Height::fixed(top);
            *bottom_z = Height::fixed(bottom);
            // Issue #1716: 40% of D keeps the HSM plan fast in debug. The
            // chord law judges every link between pieces at any ring step.
            *optimal_load = 0.4;
        }
        OpParams::Offset2D { top_z, bottom_z, .. } => {
            *top_z = Height::fixed(top);
            *bottom_z = Height::fixed(bottom);
        }
        _ => panic!("a pocket-family kind"),
    }
    p
}

/// A 20 mm disc standing in a 31 mm stock: the stock's band around it
/// is 5.5 mm, under the 6 mm cutter, so the cutter-centre area is the four
/// corners, whose walls lie on the stock's offset edges. Every cutting
/// move keeps the cutter off the disc - on the aligned stock and the
/// turned one, under 2D Adaptive and 2D Offset (both ride the rings).
#[test]
fn a_whole_model_clearing_never_joins_two_corners_across_the_part_that_pinches_them() {
    // 64 facets: the circumradius is 10, every edge at least 9.99 out.
    let rim = 10.0 * (std::f64::consts::PI / 64.0).cos();
    for kind in [OpType::Adaptive2D, OpType::Offset2D] {
        for turned in [false, true] {
            let (world, mut h) = job(cylinder(20.0, 10.0, 64), 31.0, 0.0, 10.0, turned, None);
            let (t, warns) = generated(&world, &mut h, "part", retyped(kind, 10.0, 5.0));
            let tag = format!("{kind:?} turned {turned}");
            assert!(t.moves.iter().any(|m| m.kind == MoveKind::Feed && m.to.z < 10.0 - 1e-6), "{tag}: the corners are cut");
            let (d, at) = closest_to_axis(&t, 10.0);
            assert!(d >= rim + R - 0.02, "{tag}: a cut at ({:.3}, {:.3}, {:.3}) stands {d:.3} from the axis - {:.3} into the disc", at.x, at.y, at.z, rim + R - d);
            // #3035: the corners ride the rings - Adaptive says so with one
            // advisory warning; nothing else is flagged.
            let fell = usize::from(camcraft_app::machining2d::rings_fallbacks(&t.notes) > 0);
            assert_eq!(warns, fell, "{tag}: the op verifies clean");
        }
    }
}

fn milling_test() -> Mesh {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let imp = ImportModel::new(&path).expect("the user's STEP imports");
    let world = new_world();
    let mut h = History::default();
    let name = imp.name.clone();
    h.execute(&world, imp);
    let e = model::find_by_name(&world, &name).expect("the part");
    Mesh::from_facets(model::world_facets(&world, &e).expect("facets"))
}

fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}

/// How far a cutter of radius `R` centred at `p` reaches into `section`.
fn reach_into(section: &Region, p: Point2) -> f64 {
    let mut d = f64::INFINITY;
    for l in section.loops() {
        let q = l.points();
        for i in 0..q.len() {
            d = d.min(seg_dist(p, q[i], q[(i + 1) % q.len()]));
        }
    }
    if section.contains(p) { R + d } else { R - d }
}

/// The issue's own job: the user's part (a D whose round side is
/// tangent to Y -30 near X 0, Z -5 .. 20) in the 70 x 70 x 27 box on
/// Z -7 that covers it, the kind's seed params (Top/Bottom of model,
/// 5 mm steps, helix entry) and the 6 mm end mill with 30 mm flutes.
/// (A box with its top at Z 10, as first reported, leaves the part
/// 10 mm proud of the stock: the sim reads that as a 10 mm "gouge" at
/// the first part column, (-1.44, -29.94), whatever the path does.)
/// Before #1620 the path fed along Y -32 from (-8.05, -32) to (32, -32)
/// at every level, 0.94 mm into the D. Read against the part's own section
/// at each move's Z; the 0.06 allowance is the arc fit on the faceted
/// round side (0.045 at worst), far under that cut.
#[test]
#[ignore = "heavy lane (release): the HSM plan grinds its budget on the user's part in debug (~5 min for both stocks)"]
fn the_users_part_with_default_adaptive_keeps_the_cutter_off_it() {
    let part = milling_test();
    let mut sections: Vec<(i64, Region)> = Vec::new();
    for turned in [false, true] {
        let (world, mut h) = job(part.clone(), 70.0, -7.0, 27.0, turned, Some(30.0));
        let (t, warns) = generated(&world, &mut h, "part", operation::seed_params(OpType::Adaptive2D.ordinal()));
        let mut worst = (0.0f64, Point3::new(0.0, 0.0, 0.0));
        for m in t.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
            let z = m.from.z.min(m.to.z);
            let key = (z * 1000.0).round() as i64;
            if !sections.iter().any(|(k, _)| *k == key) {
                sections.push((key, slice::material_above(&part.facets, z).expect("the part sections")));
            }
            let Some((_, sec)) = sections.iter().find(|(k, _)| *k == key) else { continue };
            for (a, b) in m.chords(0.01) {
                for k in 0..=8 {
                    let s = f64::from(k) / 8.0;
                    let p = Point2::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s);
                    let r = reach_into(sec, p);
                    if r > worst.0 {
                        worst = (r, Point3::new(p.x, p.y, z));
                    }
                }
            }
        }
        let (r, at) = worst;
        assert!(r < 0.06, "turned {turned}: a cut at ({:.3}, {:.3}, {:.3}) reaches {r:.3} into the part", at.x, at.y, at.z);
        // #3035: a fallback is advisory; every other warning still fails.
        let areas = camcraft_app::machining2d::rings_fallbacks(&t.notes);
        assert_eq!(warns, usize::from(areas > 0), "turned {turned}: only the fallback is reported");
        let e = model::find_by_name(&world, "Op1").expect("op");
        let cached = model::toolpath_of(&world, &e).expect("cached path");
        assert!(cached.warnings.iter().all(|w| {
            matches!(w, camcraft_app::verify::Warning::StrategyFallback { areas: n, .. } if *n == areas)
                && !w.blocks_post()
        }), "turned {turned}: unexpected warnings: {:?}", cached.warnings);
        if !turned {
            // The program sim reads the same job (it judges the aligned
            // stock; the turned one rests on the op law above).
            let sim = simulate_document(&world).expect("the program sim runs");
            assert_eq!(
                sim.dev.gouge_cells, 0,
                "the sim reads a gouge, worst {:.3} mm at {:?}",
                sim.dev.max_gouge, sim.dev.max_gouge_at
            );
        }
    }
}

//! Issue #1749: a whole-model 3D Pocket on a notch open to the part's
//! outside clears the notch out to the part's edge. The silhouette's
//! outside is air, so the cutter's centre rides up to the silhouette
//! edge; before the fix it stayed a radius inside it and left a ridge a
//! radius wide, the full stock height tall, along the open edges - and
//! the op said ok. The setup's other parts stay out of the cutter's
//! reach.
//!
//! #1774: stock reaching far past the silhouette is cleared out to the
//! stock's outline, and the open-side law's tilted branch is pinned.
//!
//! Gate map (#1749, #1774): covers app::operation (generate's Pocket3D
//! arm, open_side), motion::machining3d (open_side).

mod area_support;

use area_support::*;
use camcraft_app::geometry::{Mesh, Triangle};
use camcraft_app::links::Links;

/// The notch floor and the part's top.
const FLOOR: f64 = 20.0;
const TOP: f64 = 30.0;
/// The stock's top: the ladder's first plane.
const STOCK_TOP: f64 = 32.0;
/// The chord sag the offset rings' corner arcs may show.
const CHORD: f64 = 0.01;

/// Solid cells of a grid, meshed by their boundary faces: every face
/// lies on the grid, so no face has a T-junction. Outward-wound.
fn voxel_mesh(xs: &[f64], ys: &[f64], zs: &[f64], solid: impl Fn(usize, usize, usize) -> bool) -> Mesh {
    let (nx, ny, nz) = (xs.len() - 1, ys.len() - 1, zs.len() - 1);
    let at = |i: isize, j: isize, k: isize| {
        let inside = |v: isize, n: usize| v >= 0 && (v as usize) < n;
        inside(i, nx) && inside(j, ny) && inside(k, nz) && solid(i as usize, j as usize, k as usize)
    };
    let mut f = Vec::new();
    let mut quad = |c: [Point3; 4], n: [f64; 3]| {
        let (a, b, d) = (c[0], c[1], c[2]);
        let u = [b.x - a.x, b.y - a.y, b.z - a.z];
        let v = [d.x - a.x, d.y - a.y, d.z - a.z];
        let cr = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let c = if cr[0] * n[0] + cr[1] * n[1] + cr[2] * n[2] > 0.0 { c } else { [c[3], c[2], c[1], c[0]] };
        f.push(Triangle::new(c[0], c[1], c[2]));
        f.push(Triangle::new(c[0], c[2], c[3]));
    };
    for i in 0..nx {
        for j in 0..ny {
            for k in 0..nz {
                if !solid(i, j, k) {
                    continue;
                }
                let (ii, jj, kk) = (i as isize, j as isize, k as isize);
                let (x0, x1, y0, y1, z0, z1) = (xs[i], xs[i + 1], ys[j], ys[j + 1], zs[k], zs[k + 1]);
                let p = Point3::new;
                if !at(ii - 1, jj, kk) {
                    quad([p(x0, y0, z0), p(x0, y1, z0), p(x0, y1, z1), p(x0, y0, z1)], [-1.0, 0.0, 0.0]);
                }
                if !at(ii + 1, jj, kk) {
                    quad([p(x1, y0, z0), p(x1, y1, z0), p(x1, y1, z1), p(x1, y0, z1)], [1.0, 0.0, 0.0]);
                }
                if !at(ii, jj - 1, kk) {
                    quad([p(x0, y0, z0), p(x1, y0, z0), p(x1, y0, z1), p(x0, y0, z1)], [0.0, -1.0, 0.0]);
                }
                if !at(ii, jj + 1, kk) {
                    quad([p(x0, y1, z0), p(x1, y1, z0), p(x1, y1, z1), p(x0, y1, z1)], [0.0, 1.0, 0.0]);
                }
                if !at(ii, jj, kk - 1) {
                    quad([p(x0, y0, z0), p(x1, y0, z0), p(x1, y1, z0), p(x0, y1, z0)], [0.0, 0.0, -1.0]);
                }
                if !at(ii, jj, kk + 1) {
                    quad([p(x0, y0, z1), p(x1, y0, z1), p(x1, y1, z1), p(x0, y1, z1)], [0.0, 0.0, 1.0]);
                }
            }
        }
    }
    Mesh::from_facets(f)
}

/// A 30 mm block [-15, 15]^2 x [0, 30] with the +X +Y quadrant notched
/// down to Z 20: the notch is open on its +X and +Y sides.
fn notched_block() -> Mesh {
    let g = [-15.0, 0.0, 15.0];
    voxel_mesh(&g, &g, &[0.0, FLOOR, TOP], |i, j, k| !(i == 1 && j == 1 && k == 1))
}

fn world(neighbour: Option<Mesh>) -> World {
    world_on(neighbour, 30.0)
}

/// The part on a square stock `side` mm across, centred on the part.
fn world_on(neighbour: Option<Mesh>, side: f64) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", notched_block()));
    if let Some(m) = neighbour {
        h.execute(&world, ImportModel::from_mesh("other", m));
    }
    // The stock fits the part in XY (the user's case): only the notch
    // and the 2 mm above the top are to cut.
    let stock = StockShape::Box { dx: side, dy: side, dz: STOCK_TOP };
    h.execute(&world, SetStock::new(&world, stock, Location::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    world
}

/// The user's 3D Pocket: offset rings, shell on, no stock to leave.
fn pocket3d() -> OpParams {
    OpParams::Pocket3D {
        pass_start: Default::default(),
        links: Links::ROUGH3D,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(STOCK_TOP),
        bottom_z: Height::fixed(FLOOR),
        stepdown: 3.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::OffsetRings,
        entry: Entry3D::Plunge,
        helix_radius: 2.0,
        rest_rough: false,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        feed_opt: false,
        shell: true,
        method: camcraft_app::machining2d::AdaptiveMethod::Spiral,
        picks: Default::default(),
    }
}

/// Every feed's tip, sampled at most 0.25 mm apart (an arc on the arc).
fn feed_samples(t: &Toolpath) -> Vec<Point3> {
    let mut out = Vec::new();
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        let len = ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2) + (m.to.z - m.from.z).powi(2)).sqrt();
        let n = ((len / 0.25).ceil() as usize).clamp(1, 4000);
        for k in 0..=n {
            let t = k as f64 / n as f64;
            let q = m.arc.and_then(|a| a.point_at(m.from, m.to, t)).unwrap_or_else(|| {
                Point3::new(
                    m.from.x + (m.to.x - m.from.x) * t,
                    m.from.y + (m.to.y - m.from.y) * t,
                    m.from.z + (m.to.z - m.from.z) * t,
                )
            });
            out.push(q);
        }
    }
    out
}

/// The stock the path leaves standing over (x, y): the lowest tip whose
/// flat-end disc covers the column, or the stock top where none does.
fn carried(samples: &[Point3], x: f64, y: f64) -> f64 {
    samples
        .iter()
        .filter(|q| (q.x - x).hypot(q.y - y) <= R - 1e-6)
        .map(|q| q.z)
        .fold(STOCK_TOP, f64::min)
}

/// Distance from (x, y) to the axis box [x0, x1] x [y0, y1].
fn to_box(x: f64, y: f64, b: [f64; 4]) -> f64 {
    let dx = (b[0] - x).max(x - b[1]).max(0.0);
    let dy = (b[2] - y).max(y - b[3]).max(0.0);
    dx.hypot(dy)
}

/// No feed's tool disc enters the part's material at its Z: the full
/// square below the floor, the L above it.
fn assert_no_gouge(samples: &[Point3]) {
    for q in samples {
        if q.z < FLOOR - EPS {
            let d = to_box(q.x, q.y, [-15.0, 15.0, -15.0, 15.0]);
            assert!(d >= R - CHORD, "feed at ({:.3}, {:.3}, {:.3}) is {d:.3} from the block below the floor", q.x, q.y, q.z);
        }
        if q.z < TOP - EPS {
            let d = to_box(q.x, q.y, [-15.0, 0.0, -15.0, 15.0]).min(to_box(q.x, q.y, [0.0, 15.0, -15.0, 0.0]));
            assert!(d >= R - CHORD, "feed at ({:.3}, {:.3}, {:.3}) is {d:.3} from the L standing above the floor", q.x, q.y, q.z);
        }
    }
}

/// The issue's reading: along both open edges of the notch, the stock the
/// pocket leaves is at the floor (no stock to leave), not the stock top.
/// Before the fix the centre stayed 5 mm inside the silhouette, so the
/// band within 5 mm of each open edge kept the stock's full 12 mm.
#[test]
fn an_open_sided_notch_is_cleared_to_the_parts_edge() {
    let world = world(None);
    let (lo, hi) = stock_box(&world);
    assert!((lo.x + 15.0).abs() < 1e-6 && (hi.x - 15.0).abs() < 1e-6 && (hi.z - STOCK_TOP).abs() < 1e-6, "stock {lo:?} {hi:?}");
    let o = op(SrcRef::Entity("part".into()), pocket3d(), AreaControls::default());
    let (path, warns) = run(&world, &o).unwrap_or_else(|e| panic!("the notch generates: {e}"));
    assert_eq!(warns, 0, "verify stays clean");
    let s = feed_samples(&path);
    assert_no_gouge(&s);
    // The open edges, from the corner the walls leave (one radius off
    // the inside corner at the origin is the cutter's own fillet) to the
    // part's outer corner.
    let mut worst = (f64::NEG_INFINITY, 0.0, 0.0);
    for k in 0..=50 {
        let a = R + (15.0 - R) * k as f64 / 50.0;
        for (x, y) in [(14.95, a.min(14.95)), (a.min(14.95), 14.95)] {
            let z = carried(&s, x, y);
            if z > worst.0 {
                worst = (z, x, y);
            }
        }
    }
    assert!(
        worst.0 <= FLOOR + 1e-3,
        "the open edge keeps stock to Z {:.3} at ({:.2}, {:.2}); the floor is Z {FLOOR}",
        worst.0,
        worst.1,
        worst.2
    );
}

/// A second part 1 mm past the notch's +X open edge: the cutter clears
/// the notch but never reaches into the neighbour.
#[test]
fn an_open_side_never_reaches_into_a_neighbouring_part() {
    let other = [16.0, 26.0, -15.0, 15.0];
    let neighbour = voxel_mesh(&[other[0], other[1]], &[other[2], other[3]], &[0.0, TOP], |_, _, _| true);
    let world = world(Some(neighbour));
    let o = op(SrcRef::Entity("part".into()), pocket3d(), AreaControls::default());
    let (path, warns) = run(&world, &o).unwrap_or_else(|e| panic!("the notch generates beside a neighbour: {e}"));
    assert_eq!(warns, 0, "verify stays clean");
    let s = feed_samples(&path);
    assert_no_gouge(&s);
    for q in s.iter().filter(|q| q.z < TOP - EPS) {
        let d = to_box(q.x, q.y, other);
        assert!(d >= R - CHORD, "feed at ({:.3}, {:.3}, {:.3}) is {d:.3} from the neighbouring part", q.x, q.y, q.z);
    }
    // The +Y open edge, away from the neighbour, is still cleared.
    let z = carried(&s, 7.0, 14.95);
    assert!(z <= FLOOR + 1e-3, "the +Y open edge keeps stock to Z {z:.3}");
}

/// The solid law is the second net (#1749 turns it on for the 3D
/// Pocket): a second part standing on the notch floor, inside the
/// model's own silhouette, is no island of the model's ladder - the
/// cutter goes through it, and the op says so instead of reading ok.
#[test]
fn a_part_standing_in_the_notch_is_named_by_the_solid_law() {
    let inner = voxel_mesh(&[5.0, 10.0], &[5.0, 10.0], &[FLOOR, TOP], |_, _, _| true);
    let world = world(Some(inner));
    let o = op(SrcRef::Entity("part".into()), pocket3d(), AreaControls::default());
    let (_, warns) = run(&world, &o).unwrap_or_else(|e| panic!("the notch generates: {e}"));
    assert!(warns > 0, "the cut through the second part is warned, never ok");
}

/// #1774: a stock reaching 20 mm past the part on every side, four tool
/// radii - far past the #1749 band, which stopped about two radii out.
/// The whole-model pocket clears it out to the stock's outline, to the
/// floor, without a cutter position inside the part. Before the fix the
/// columns past the silhouette by more than two radii kept the stock's
/// full height, and the op read ok.
#[test]
fn stock_far_past_the_silhouette_is_cleared_to_the_stock_outline() {
    let world = world_on(None, 70.0);
    let (lo, hi) = stock_box(&world);
    assert!((lo.x + 35.0).abs() < 1e-6 && (hi.y - 35.0).abs() < 1e-6, "stock {lo:?} {hi:?}");
    let o = op(SrcRef::Entity("part".into()), pocket3d(), AreaControls::default());
    let (path, warns) = run(&world, &o).unwrap_or_else(|e| panic!("the far stock generates: {e}"));
    assert_eq!(warns, 0, "verify stays clean");
    let s = feed_samples(&path);
    assert_no_gouge(&s);
    let mut worst = (f64::NEG_INFINITY, 0.0, 0.0);
    for k in 0..=70 {
        let a = -34.9 + 69.8 * k as f64 / 70.0;
        // The stock's edge, and a ring between it and the #1749 band.
        for (x, y) in [(34.9, a), (-34.9, a), (a, 34.9), (a, -34.9), (28.0, a.clamp(-28.0, 28.0)), (-28.0, a.clamp(-28.0, 28.0))] {
            let z = carried(&s, x, y);
            if z > worst.0 {
                worst = (z, x, y);
            }
        }
    }
    assert!(
        worst.0 <= FLOOR + 1e-3,
        "the stock past the silhouette keeps Z {:.3} at ({:.2}, {:.2}); the floor is Z {FLOOR}",
        worst.0,
        worst.1,
        worst.2
    );
}

/// #1774: the 3D Pocket (like its stamped twin, `OpParams::tilt`'s
/// exhaustive match) carries no 3+2 index, so the open-side law is never read in a tilted frame; the
/// law's tilted branch refuses by name (`open_side`'s unit test). Should
/// either kind gain a tilt, this premise fails first.
#[test]
fn no_3d_pocket_kind_carries_a_tilt() {
    assert!(pocket3d().tilt().is_none(), "a 3D Pocket carries no 3+2 index");
}

/// A cached 3D cut must be checked against parts added after generation,
/// even when the stock turned off the axes prevents the program
/// simulation from running (#2177: a quarter turn no longer does).
fn added_part_blocks_live_post(world: World, params: OpParams, tool_name: &str, tag: &str, inner: impl FnOnce(&Toolpath) -> Mesh) {
    use camcraft_app::commands::GenerateOperation;
    use camcraft_app::operation::OpStatus;
    use camcraft_app::posting;

    let mut h = History::default();
    if tool_name != TOOL {
        let tool = catalog().into_iter().find(|(n, _)| *n == tool_name).map(|(_, t)| t).expect("catalog tool");
        h.execute(&world, AddTool::new(&world, tool_name, tool).expect("tool"));
    }
    h.execute(
        &world,
        CreateOperation::new(&world, tag, tool_name, SrcRef::Entity("part".into()), params).expect("create"),
    );
    h.execute(&world, GenerateOperation::new(&world, tag).expect("generate"));
    let e = model::find_by_name(&world, tag).expect("operation");
    let cached = model::toolpath_of(&world, &e).expect("cached path");
    assert!(!cached.path.moves.is_empty(), "the clean cut exists");
    assert!(!cached.warnings.iter().any(|w| w.blocks_post()), "clean generation: {:?}", cached.warnings);
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok);

    let out = std::env::temp_dir().join(format!("camcraft-live-3d-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    posting::post_setup(&world, &out).unwrap_or_else(|err| panic!("the clean path posts: {err}"));
    assert!(out.exists(), "the clean post writes an NC file");
    std::fs::remove_file(&out).expect("remove clean output");

    let inner = inner(&cached.path);
    h.execute(&world, ImportModel::from_mesh("inner", inner));
    assert_eq!(
        operation::status_of(&world, &e),
        OpStatus::Ok,
        "adding a neighbour leaves the cached path fresh"
    );
    let result = posting::post_setup(&world, &out);
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    let err = result.expect_err("a cached cut through an added part must never post");
    assert!(
        err.contains(&format!("{tag} now cuts part 'inner'")),
        "refuse by operation and part name: {err}"
    );
    assert!(err.contains("regenerate"), "{err}");
    assert!(oblique_refused(&world), "the device holds: the program sim does not judge this stock");
    assert!(!wrote, "the refused post writes no NC file");
}

#[test]
fn pocket3d_post_refuses_a_part_added_after_clean_generation_on_rotated_stock() {
    let world = world(None);
    let mut h = History::default();
    let at = Location { rot: stock_turn(true), ..Location::default() };
    h.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 30.0,
                dy: 30.0,
                dz: STOCK_TOP,
            },
            at,
        )
        .expect("turned stock"),
    );
    added_part_blocks_live_post(world, pocket3d(), TOOL, "pocket-live", |_| {
        voxel_mesh(&[5.0, 10.0], &[5.0, 10.0], &[FLOOR, TOP], |_, _, _| true)
    });
}

#[test]
fn multiaxis_rough_post_refuses_a_part_added_after_clean_generation_on_rotated_stock() {
    let world = new_world();
    let mut h = History::default();
    let facets = cube(Point3::new(-20.0, -20.0, -20.0), Point3::new(20.0, 20.0, -10.0)).facets.to_vec();
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(facets)));
    let at = Location {
        pos: Point3::new(0.0, 0.0, -20.0),
        rot: stock_turn(true),
    };
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 }, at).expect("turned stock"),
    );
    let params = OpParams::MultiAxisRough {
        links: Links::ROUGH3D,
        tolerance: 0.1,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(5.0),
        retract_z: None,
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-5.0),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 1.0,
        shank_avoid: false,
        picks: Default::default(),
    };
    added_part_blocks_live_post(world, params, "BN-0600", "multi-live", |path| {
        let p = path
            .moves
            .iter()
            .find(|m| m.kind == MoveKind::Feed && m.to.z < 0.0)
            .expect("a feed below the stock top")
            .to;
        Mesh::from_facets(
            cube(Point3::new(p.x - 1.0, p.y - 1.0, p.z - 1.0), Point3::new(p.x + 1.0, p.y + 1.0, p.z + 1.0))
                .facets
                .to_vec(),
        )
    });
}

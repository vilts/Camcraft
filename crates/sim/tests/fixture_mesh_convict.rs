//! Can a real mesh penetration pass the fixture box laws (#339 slice
//! 2)? A meshed fixture's box is the exact AABB of the very vertices its
//! mesh judges (`app::model::entity_world_aabb`), so the mesh lies
//! inside it; every z silence of the box laws (the strict-dip rule:
//! flush contact with a box top or bottom, a retract off a mounting
//! face) only ever spares an overlap of float dust with the BOX, so it
//! can spare no more than that with the mesh. These pins hold the
//! claim on the shapes that look like they might break it: a rotated
//! fixture whose top is an edge, an overhanging clamp arm, a ramp and
//! an upright rapid under steel (#424/#425 with a closed mesh), and a
//! seeded sweep of straight moves - many flush with a face - asserting
//! that wherever the box laws stay silent the true swept stack stands
//! clear of the mesh.
//!
//! Gate map (M4.4): covers sim::verify::laws, sim::verify::stack_span,
//! sim::verify::mesh_steel, sim::fixture, sim::fixture_mesh.
//! Provenance: our own fixtures; the law is the Prime Directive's.

use std::sync::Arc;

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::fixture::FixtureBox;
use camcraft_sim::fixture_mesh::MeshedFixture;
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::verify::{
    bands_vs_workholding, mesh_refined, rapids_vs_workholding, steel_stack, FixturePart, MeshSteel, SteelBand,
    ToolGeom, Warning, Workholding,
};
use camcraft_tools::holder_profile::HolderStack;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Flute 0..20 r3, shank 20..50 r5, holder 50..90 r20 above the tip:
/// the #424/#425 repros' tool.
fn tool() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 20.0,
        shank_radius: 5.0,
        shank_len: 30.0,
        holder_radius: 20.0,
        holder_len: 40.0,
    }
}

/// A closed, outward box: 12 facets sharing 8 exact corners.
fn cube(lo: Point3, hi: Point3) -> Vec<Triangle> {
    let c = |i: usize| {
        p(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z })
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    quads
        .iter()
        .flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))])
        .collect()
}

/// `tris` turned by `deg` about the unit axis `k` through the origin
/// (Rodrigues), then moved by `d`. A shared vertex maps to the same
/// bits every time, so a closed mesh stays closed.
fn turned(tris: &[Triangle], k: [f64; 3], deg: f64, d: Point3) -> Vec<Triangle> {
    let l = (k[0] * k[0] + k[1] * k[1] + k[2] * k[2]).sqrt();
    let k = [k[0] / l, k[1] / l, k[2] / l];
    let (s, c) = deg.to_radians().sin_cos();
    let f = |v: Point3| {
        let w = [v.x, v.y, v.z];
        let kx = [k[1] * w[2] - k[2] * w[1], k[2] * w[0] - k[0] * w[2], k[0] * w[1] - k[1] * w[0]];
        let e = (k[0] * w[0] + k[1] * w[1] + k[2] * w[2]) * (1.0 - c);
        p(
            w[0] * c + kx[0] * s + k[0] * e + d.x,
            w[1] * c + kx[1] * s + k[1] * e + d.y,
            w[2] * c + kx[2] * s + k[2] * e + d.z,
        )
    };
    tris.iter().map(|t| Triangle::new(f(t.a), f(t.b), f(t.c))).collect()
}

/// The exact AABB of the vertices - what `entity_world_aabb` reports.
fn aabb(tris: &[Triangle]) -> FixtureBox {
    let mut lo = p(f64::MAX, f64::MAX, f64::MAX);
    let mut hi = p(f64::MIN, f64::MIN, f64::MIN);
    for q in tris.iter().flat_map(|t| [t.a, t.b, t.c]) {
        lo = p(lo.x.min(q.x), lo.y.min(q.y), lo.z.min(q.z));
        hi = p(hi.x.max(q.x), hi.y.max(q.y), hi.z.max(q.z));
    }
    FixtureBox { min: lo, max: hi }
}

fn fixture(tris: Vec<Triangle>) -> MeshedFixture {
    MeshedFixture::new("Fixture1", aabb(&tris), Arc::<[Triangle]>::from(tris))
}

fn stack(t: &ToolGeom) -> Vec<SteelBand> {
    steel_stack(t, &HolderStack::cylinder(t), None)
}

/// Both laws (c2)/(d2), box-only and mesh-refined, for one fixture -
/// the app's own composition (`posting::fixture_problems`).
fn judge(path: &Toolpath, f: &MeshedFixture) -> (Vec<Warning>, Vec<Warning>) {
    let boxes = [f.aabb];
    let wh = Workholding { turning: &boxes, fixed: &[], axis: None };
    let t = tool();
    let st = stack(&t);
    let steel = MeshSteel { fixtures: std::slice::from_ref(f), stack: &st, chord_tol: 0.01 };
    let mut boxed = bands_vs_workholding(path, &t, &wh, None, 0.01);
    boxed.extend(rapids_vs_workholding(path, &t, &wh, None));
    let mut meshed = mesh_refined(path, &wh, &steel, false, |w| bands_vs_workholding(path, &t, w, None, 0.01));
    meshed.extend(mesh_refined(path, &wh, &steel, true, |w| rapids_vs_workholding(path, &t, w, None)));
    (boxed, meshed)
}

fn hit_at(ws: &[Warning], i: usize) -> Option<FixturePart> {
    ws.iter().find_map(|w| match w {
        Warning::FixtureHit { at, fixture_part } if *at == i => Some(*fixture_part),
        _ => None,
    })
}

fn rapid(a: Point3, b: Point3) -> Move {
    Move::new(MoveKind::Rapid, a, b, None)
}

fn feed(a: Point3, b: Point3) -> Move {
    Move::new(MoveKind::Feed, a, b, Some(500.0))
}

/// Does the TRUE swept stack - every band shrunk by `d` on every side,
/// so flush contact does not count - reach into the mesh along the
/// straight move? (A facet met, or a band standing inside the solid.)
fn penetrates(f: &MeshedFixture, m: &Move, d: f64) -> bool {
    let Ok(mesh) = f.mesh() else { return false };
    stack(&tool()).iter().filter(|b| b.hi - b.lo > 2.0 * d && b.r > d).any(|b| {
        mesh.band_meets(m.from, m.to, b.lo + d, b.hi - d, b.r - d, 0.0)
            || mesh.contains(p(m.from.x, m.from.y, m.from.z + 0.5 * (b.lo + b.hi)))
    })
}

#[test]
fn a_mesh_top_face_keeps_the_strict_dip_and_a_dip_convicts() {
    let f = fixture(cube(p(-30.0, -30.0, 0.0), p(30.0, 30.0, 20.0)));
    let path = Toolpath::new(vec![
        rapid(p(0.0, 0.0, 20.0), p(0.0, 0.0, 80.0)), // retract off the face
        rapid(p(-50.0, 0.0, 20.0), p(50.0, 0.0, 20.0)), // traverse riding it
        feed(p(-50.0, 0.0, 20.0), p(50.0, 0.0, 20.0)), // a cut riding it
        rapid(p(0.0, 0.0, 19.999), p(0.0, 0.0, 80.0)), // a thousandth in
        feed(p(-50.0, 0.0, 19.999), p(50.0, 0.0, 19.999)),
    ]);
    let (boxed, meshed) = judge(&path, &f);
    for (i, m) in path.moves.iter().enumerate().take(3) {
        assert_eq!(hit_at(&boxed, i), None, "flush is silent ({i}): {boxed:?}");
        assert!(!penetrates(&f, m, 1e-6), "and the mesh agrees nothing goes in ({i})");
    }
    assert_eq!(hit_at(&meshed, 3), Some(FixturePart::Rapid), "{meshed:?}");
    assert_eq!(hit_at(&meshed, 4), Some(FixturePart::Flute), "{meshed:?}");
    assert_eq!(boxed, meshed, "the mesh confirms every box hit here");
}

#[test]
fn a_rotated_fixtures_top_edge_is_its_boxs_top() {
    // A block turned 45 deg about X then 30 deg about Z: its top is one
    // edge, the box's top plane runs through it exactly (the AABB of
    // the turned vertices). Riding that plane is flush with the edge;
    // a thousandth below it crosses the edge and convicts through the
    // box - the mesh confirms.
    let base = turned(&cube(p(-20.0, -10.0, -10.0), p(20.0, 10.0, 10.0)), [1.0, 0.0, 0.0], 45.0, p(0.0, 0.0, 0.0));
    let f = fixture(turned(&base, [0.0, 0.0, 1.0], 30.0, p(0.0, 0.0, 30.0)));
    let top = f.aabb.max.z;
    let path = Toolpath::new(vec![
        rapid(p(-60.0, 0.0, top), p(60.0, 0.0, top)),
        feed(p(0.0, -60.0, top), p(0.0, 60.0, top)),
        rapid(p(-60.0, 0.0, top - 1e-3), p(60.0, 0.0, top - 1e-3)),
        feed(p(0.0, -60.0, top - 1e-3), p(0.0, 60.0, top - 1e-3)),
    ]);
    let (boxed, meshed) = judge(&path, &f);
    assert_eq!(hit_at(&boxed, 0), None, "{boxed:?}");
    assert_eq!(hit_at(&boxed, 1), None, "{boxed:?}");
    assert!(!penetrates(&f, &path.moves[0], 1e-6) && !penetrates(&f, &path.moves[1], 1e-6));
    assert_eq!(hit_at(&meshed, 2), Some(FixturePart::Rapid), "{meshed:?}");
    assert_eq!(hit_at(&meshed, 3), Some(FixturePart::Flute), "{meshed:?}");
}

/// A clamp: a post x[-5,5] y[-5,5] z[0,40] and an arm x[-5,80]
/// y[-5,5] z[40,50] reaching out over the work - two closed shells,
/// touching. The box spans z 0..50 over the whole arm's reach.
fn clamp() -> Vec<Triangle> {
    let mut t = cube(p(-5.0, -5.0, 0.0), p(5.0, 5.0, 40.0));
    t.extend(cube(p(-5.0, -5.0, 40.0), p(80.0, 5.0, 50.0)));
    t
}

#[test]
fn an_overhanging_arm_convicts_the_stack_under_it_and_its_top_stays_flush() {
    let f = fixture(clamp());
    let path = Toolpath::new(vec![
        // Under the arm, tip at z 0: the shank (20..50) runs into it.
        feed(p(60.0, -30.0, 0.0), p(60.0, 30.0, 0.0)),
        // Off the arm's end, the holder's rim (r20) reaches it.
        feed(p(95.0, -30.0, -15.0), p(95.0, 30.0, -15.0)),
        // Retract off the arm's top, and a traverse riding it.
        rapid(p(60.0, 0.0, 50.0), p(60.0, 0.0, 120.0)),
        rapid(p(60.0, -30.0, 50.0), p(60.0, 30.0, 50.0)),
    ]);
    let (boxed, meshed) = judge(&path, &f);
    assert_eq!(hit_at(&meshed, 0), Some(FixturePart::Shank), "{meshed:?}");
    assert_eq!(hit_at(&meshed, 1), Some(FixturePart::Holder), "{meshed:?}");
    assert_eq!(hit_at(&boxed, 2), None, "{boxed:?}");
    assert_eq!(hit_at(&boxed, 3), None, "{boxed:?}");
    assert!(!penetrates(&f, &path.moves[2], 1e-6) && !penetrates(&f, &path.moves[3], 1e-6));
}

#[test]
fn the_424_and_425_repros_convict_on_a_closed_mesh() {
    // #424: a vertical retract z 0 -> 50 beside a bolt 15 mm off-axis
    // standing to z 60 - only the holder (r20, 50 above the tip)
    // reaches it. #425: a ramp z 0 -> 40 under an arm at z 100..120
    // over its high end; the holder's top (130) is there.
    let bolt = fixture(cube(p(13.0, -2.0, 0.0), p(17.0, 2.0, 60.0)));
    let retract = Toolpath::new(vec![rapid(p(0.0, 0.0, 0.0), p(0.0, 0.0, 50.0))]);
    let (boxed, meshed) = judge(&retract, &bolt);
    assert_eq!(hit_at(&boxed, 0), Some(FixturePart::Holder), "{boxed:?}");
    assert_eq!(hit_at(&meshed, 0), Some(FixturePart::Holder), "{meshed:?}");

    let arm = fixture(cube(p(90.0, -10.0, 100.0), p(110.0, 10.0, 120.0)));
    let ramp = Toolpath::new(vec![feed(p(0.0, 0.0, 0.0), p(100.0, 0.0, 40.0))]);
    let (boxed, meshed) = judge(&ramp, &arm);
    assert_eq!(hit_at(&boxed, 0), Some(FixturePart::Holder), "{boxed:?}");
    assert_eq!(hit_at(&meshed, 0), Some(FixturePart::Holder), "{meshed:?}");
}

/// xorshift64*: a fixed seed, the same sweep on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
    fn pick(&mut self, v: &[f64]) -> f64 {
        v.get((self.next() * v.len() as f64) as usize).copied().unwrap_or(0.0)
    }
}

#[test]
fn no_straight_move_the_box_laws_clear_penetrates_the_mesh() {
    // Fixtures: a turned block, the clamp, a block turned about a
    // skew axis. Moves: feeds, rapids, ramps, plunges and retracts,
    // their z drawn mostly from the planes that matter - each vertex z
    // (flush with a top), each vertex z less a band's top (a band flush
    // under a bottom) - with jitter of a thousandth either way.
    let t = tool();
    let shapes = [
        turned(&cube(p(-20.0, -10.0, -10.0), p(20.0, 10.0, 10.0)), [1.0, 0.0, 0.0], 45.0, p(0.0, 0.0, 30.0)),
        clamp(),
        turned(&cube(p(-15.0, -15.0, -8.0), p(15.0, 15.0, 8.0)), [1.0, 2.0, 3.0], 37.0, p(5.0, -5.0, 40.0)),
    ];
    let tops: Vec<f64> = stack(&t).iter().flat_map(|b| [b.lo, b.hi]).collect();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (mut silent, mut flush, mut total) = (0usize, 0usize, 0usize);
    for tris in shapes {
        let f = fixture(tris.clone());
        let zs: Vec<f64> = tris.iter().flat_map(|q| [q.a.z, q.b.z, q.c.z]).collect();
        let mut planes: Vec<f64> = zs.clone();
        planes.extend(zs.iter().flat_map(|z| tops.iter().map(move |h| z - h)));
        for _ in 0..4000 {
            let z = |rng: &mut Rng| {
                let base = if rng.next() < 0.8 { rng.pick(&planes) } else { rng.range(-100.0, 120.0) };
                base + rng.pick(&[0.0, 0.0, 0.0, 1e-3, -1e-3])
            };
            let (z0, x0, y0) = (z(&mut rng), rng.range(-110.0, 110.0), rng.range(-60.0, 60.0));
            let to = match (rng.next() * 3.0) as u8 {
                0 => p(x0, y0, z(&mut rng)), // plunge or retract
                1 => p(rng.range(-110.0, 110.0), rng.range(-60.0, 60.0), z0), // level
                _ => p(rng.range(-110.0, 110.0), rng.range(-60.0, 60.0), z(&mut rng)), // ramp
            };
            let from = p(x0, y0, z0);
            let m = if rng.next() < 0.5 { rapid(from, to) } else { feed(from, to) };
            let path = Toolpath::new(vec![m]);
            let (boxed, meshed) = judge(&path, &f);
            total += 1;
            if boxed.is_empty() {
                silent += 1;
                assert!(meshed.is_empty(), "the mesh never adds a hit: {m:?}");
                assert!(!penetrates(&f, &m, 1e-6), "box laws silent, mesh penetrated: {m:?}");
                flush += usize::from(penetrates(&f, &m, -1e-6));
            }
        }
    }
    assert_eq!(total, 12_000);
    assert!(silent > 1000, "the sweep must exercise the silences: {silent}");
    eprintln!("silent {silent} flush {flush}");
    assert!(flush > 50, "and flush contact among them: {flush}");
}

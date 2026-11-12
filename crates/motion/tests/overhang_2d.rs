//! The 2D contour's ring ladder under material that hangs over it: a
//! post with a thin slanted slab overhanging the ring around it, and the
//! blind-hole block of `blind_floor.rs`, at random stepdowns, leaves and
//! Bottoms. A ring is walked at a level only where nothing of the part
//! stands above it within the tool's reach - section AND shadow
//! (`slice::material_overhead`'s law) - checked here against each
//! fixture's analytic solid, never against the law that built the path.
//!
//! Provenance: our own design (the shadowed-islands follow-up of the
//! blind-floor fix), specced from the Prime Directive; no vendor material.
//!
//! Gate map (shadowed islands): covers camcraft_motion::machining2d
//! (contour_depth, contour), camcraft_geometry::slice (shadow_above,
//! material_overhead), camcraft_motion::verify.

use camcraft_motion::geo2d::{Point2, Region};
use camcraft_motion::geometry::slice;
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::{ContourParams, Lead, Side, contour, contour_on_part};
use camcraft_motion::machining3d::Entry3D;
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

const R: f64 = 3.0;
/// Penetration granted the analytic check: the arc fitter's chord sag
/// and the ring-legality slack, nothing a cutter could remove.
const TOL: f64 = 0.011;

/// An affine hexahedron: corner `i` (bits x, y, z) at `at(ix, iy, iz)`.
/// Outward-wound for any orientation-preserving affine image of a box.
fn hexa(at: impl Fn(usize, usize, usize) -> Point3) -> Vec<Triangle> {
    let v = |i: usize| at(i & 1, (i >> 1) & 1, (i >> 2) & 1);
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6));
    t.extend(quad(0, 2, 3, 1));
    t.extend(quad(0, 1, 5, 4));
    t.extend(quad(2, 6, 7, 3));
    t.extend(quad(1, 3, 7, 5));
    t.extend(quad(2, 0, 4, 6));
    t
}

fn boxf(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    hexa(|ix, iy, iz| Point3::new([min[0], max[0]][ix], [min[1], max[1]][iy], [min[2], max[2]][iz]))
}

/// The slab: x in [X0, X1], y in [-Y, Y], underside `u(x)` rising from
/// `z0` at a shallow slope, `T` thick. Its plane sections at its own
/// vertex heights are bands at its two ends, far from the post - only
/// its shadow covers the ring.
const X0: f64 = -20.0;
const X1: f64 = 30.0;
const Y: f64 = 30.0;
const SLOPE: f64 = 0.05;
const T: f64 = 0.5;

fn u(x: f64, z0: f64) -> f64 {
    z0 + SLOPE * (x - X0)
}

/// A post `[0, 5] x [-5, 5] x [0, 20]` and the slab over it at `z0`.
fn post_and_slab(z0: f64) -> Vec<Triangle> {
    let mut f = boxf([0.0, -5.0, 0.0], [5.0, 5.0, 20.0]);
    f.extend(hexa(|ix, iy, iz| {
        let x = [X0, X1][ix];
        Point3::new(x, [-Y, Y][iy], u(x, z0) + T * iz as f64)
    }));
    f
}

/// Does a flat cutter of radius `R` (less `tol`) at `q`, shank standing
/// up from its tip, share volume with the post or the slab?
fn hits_post_or_slab(q: Point3, z0: f64, tol: f64) -> bool {
    let rr = R - tol;
    let post = (q.x.clamp(0.0, 5.0) - q.x).hypot(q.y.clamp(-5.0, 5.0) - q.y) < rr && q.z < 20.0 - tol;
    let (xa, xb) = ((q.x - rr).max(X0), (q.x + rr).min(X1));
    let over = xa < xb && q.y.abs() < Y + rr && u(xb, z0) + T > q.z + tol;
    post || over
}

/// The blind-hole block of `blind_floor.rs`: top Z 0, underside `ZB`,
/// a `[-A, A]^2` hole to `-d`, built of overlapping boxes.
const W: f64 = 25.0;
const A: f64 = 8.0;
const ZB: f64 = -15.0;

/// The block's boxes: (XY min, XY max, top Z) - the union is the solid.
fn block_boxes(d: f64) -> [([f64; 2], [f64; 2], f64); 5] {
    [
        ([-W, -W], [W, W], -d),
        ([-W, -W], [-A, W], 0.0),
        ([A, -W], [W, W], 0.0),
        ([-A - 1.0, -W + 0.5], [A + 1.0, -A], 0.0),
        ([-A - 1.0, A], [A + 1.0, W - 0.5], 0.0),
    ]
}

fn block(d: f64) -> Vec<Triangle> {
    block_boxes(d).iter().flat_map(|&(lo, hi, top)| {
        let z0 = if top == 0.0 { -d } else { ZB };
        boxf([lo[0], lo[1], z0], [hi[0], hi[1], top])
    }).collect()
}

/// Does a cutter at `q` (shank up from its tip) share volume with any of
/// the block's boxes? Every box stands from under the tip's reach (the
/// tool comes from above), so only its top and XY matter.
fn hits_block(q: Point3, d: f64, tol: f64) -> bool {
    let rr = R - tol;
    block_boxes(d).iter().any(|&(lo, hi, top)| {
        (q.x.clamp(lo[0], hi[0]) - q.x).hypot(q.y.clamp(lo[1], hi[1]) - q.y) < rr && q.z < top - tol
    })
}

/// The first sampled tool position `hit` convicts, or None.
fn first_hit(t: &camcraft_motion::cl::Toolpath, hit: impl Fn(Point3) -> bool) -> Option<(usize, Point3)> {
    for (i, m) in t.moves.iter().enumerate() {
        for (a, b) in m.segments3(0.005) {
            let n = ((b.x - a.x).hypot(b.y - a.y).max((b.z - a.z).abs()) / 0.1).ceil().clamp(1.0, 10_000.0) as usize;
            for k in 0..=n {
                let s = k as f64 / n as f64;
                let q = Point3::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s);
                if hit(q) {
                    return Some((i, q));
                }
            }
        }
    }
    None
}

fn params(top: f64, bottom: f64, stepdown: f64, leave: f64) -> ContourParams {
    ContourParams {
        tool_diameter: 2.0 * R,
        stock_to_leave: leave,
        clearance_z: top + 10.0,
        retract_z: top + 5.0,
        top_z: top,
        bottom_z: bottom,
        stepdown,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_motion::links::Links::PLANAR,
    }
}

fn depth_verify(t: &camcraft_motion::cl::Toolpath, region: &Region, f: &[Triangle], top: f64, leave: f64) -> Vec<Warning> {
    let tool = ToolGeom {
        radius: R,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: R,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    verify(
        t,
        &VerifyContext {
            region,
            stock: None,
            model: Some(f),
            tool: &tool,
            cutter: None,
            mode: Mode::Boundary,
            stock_top_z: top,
            clearance_z: top + 10.0,
            stock_to_leave: leave,
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
fn the_slab_hangs_over_the_post_ring_yet_no_plane_section_sees_it() {
    // The fixture's claim: at a level under the slab the plain section
    // is the post alone, and the slab's own vertex-plane sections are
    // bands at its ends - only the shadow covers the ring at x = -3..8.
    let f = post_and_slab(12.5);
    let ring = [Point2::new(-3.0, 0.0), Point2::new(8.0, 0.0), Point2::new(2.5, 8.0)];
    for z in [12.0, 13.0 + 1e-6, 15.0 + 1e-6] {
        let plain = slice::material_above(&f, z).expect("section");
        assert!(ring.iter().all(|&p| !plain.contains(p)), "z {z}: the plain section misses the slab over the ring");
    }
    let all = slice::material_overhead(&f, 12.0).expect("overhead");
    assert!(ring.iter().all(|&p| all.contains(p)), "the shadow stands over the whole ring");
    assert!(slice::shadow_above(&f, 15.0 + 1e-3).expect("shadow").is_empty(), "nothing hangs over 15");
}

#[test]
fn a_ring_under_an_overhang_stops_above_it() {
    // Pre-fix the ring was walked at 12, 8, 4 and 0 with the slab over
    // it: the shank ran through the slab, and the plain-section verify
    // was silent. Now the ring ends above the slab.
    let f = post_and_slab(12.5);
    let region = slice::slice(&f, 20.0 - 1e-3).expect("top section");
    // The pre-fix answer, byte for byte: every plain section left the
    // ring legal, so it ran the flat ladder - through the slab. The
    // per-plane verifier (plain section at each move's depth) was silent
    // on it; the solid law (b2) reads the slab itself and convicts the
    // moves under it - and only those (the first hit is one of them).
    let old = contour(&region, &params(20.0, 0.0, 4.0, 0.0)).expect("flat contour");
    let (hit, _) = first_hit(&old, |q| hits_post_or_slab(q, 12.5, TOL)).expect("the flat ladder runs through the slab");
    let w = depth_verify(&old, &region, &f, 20.0, 0.0);
    assert!(w.contains(&Warning::Gouge { at: hit }), "the solid law sees the slab: {w:?}");
    for wn in &w {
        let (Warning::Gouge { at } | Warning::RapidInStock { at }) = wn else { panic!("{wn:?}") };
        assert!(old.moves[*at].from.z.min(old.moves[*at].to.z) < u(X1, 12.5) + T, "only moves under the slab: {:?}", old.moves[*at]);
    }
    let t = contour_on_part(&region, &f, &params(20.0, 0.0, 4.0, 0.0)).expect("contour");
    assert!(t.all_finite() && t.is_chained());
    if let Some((i, q)) = first_hit(&t, |q| hits_post_or_slab(q, 12.5, TOL)) {
        panic!("move {i} cuts the post or the slab at {q:?} ({:?}) / {:?}", t.moves[i], t.notes);
    }
    let deepest = t.moves.iter().map(|m| m.from.z.min(m.to.z)).fold(f64::INFINITY, f64::min);
    assert!(deepest >= u(8.0 + R, 12.5) + T - 1e-9, "the ring stays over the slab: {deepest} / {:?}", t.notes);
    let notes = t.notes.join(" | ");
    assert!(notes.contains("levels below skipped (12.000, 8.000, 4.000, 0.000)"), "{notes}");
    assert_eq!(depth_verify(&t, &region, &f, 20.0, 0.0), Vec::new(), "{notes}");
}

#[test]
fn random_ladders_never_walk_a_ring_into_hanging_or_floor_solid() {
    let mut s: u64 = 0x0_5ad0_ba5e;
    let mut next = || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut runs = 0;
    for case in 0..24 {
        let stepdown = 0.8 + 6.0 * next();
        let leave = if case % 3 == 0 { 0.3 } else { 0.0 };
        let tag = format!("case {case}: stepdown {stepdown:.4} leave {leave}");
        // The slab anywhere from the post's foot to near its top.
        let z0 = 2.0 + 14.0 * next();
        let f = post_and_slab(z0);
        let region = slice::slice(&f, 20.0 - 1e-3).expect("top section");
        let bottom = -3.0 * next();
        let t = contour_on_part(&region, &f, &params(20.0, bottom, stepdown, leave))
            .unwrap_or_else(|e| panic!("{tag} slab {z0:.4}: {e}"));
        if let Some((i, q)) = first_hit(&t, |q| hits_post_or_slab(q, z0, TOL)) {
            panic!("{tag} slab {z0:.4}: move {i} enters solid at {q:?} ({:?})", t.moves[i]);
        }
        assert_eq!(depth_verify(&t, &region, &f, 20.0, leave), Vec::new(), "{tag} slab {z0:.4}: {:?}", t.notes);
        // The blind block, Bottom anywhere down to under its underside.
        let d = 1.0 + 12.0 * next();
        let f = block(d);
        let region = slice::slice(&f, -1e-3).expect("top section");
        let bottom = -d - (ZB.abs() + 4.0 - d) * next();
        let t = contour_on_part(&region, &f, &params(0.0, bottom, stepdown, leave))
            .unwrap_or_else(|e| panic!("{tag} hole {d:.4}: {e}"));
        if let Some((i, q)) = first_hit(&t, |q| hits_block(q, d, TOL)) {
            panic!("{tag} hole {d:.4} bottom {bottom:.4}: move {i} enters solid at {q:?} ({:?})", t.moves[i]);
        }
        assert_eq!(depth_verify(&t, &region, &f, 0.0, leave), Vec::new(), "{tag} hole {d:.4}: {:?}", t.notes);
        runs += 2;
    }
    assert_eq!(runs, 48);
}

//! M2.2a acceptance: the waterline. The construction promises a
//! gouge-free finish (tangent-locus geometry); these tests MEASURE that
//! promise with the envelope kernel, and pin the structural laws the
//! generator states in its module comment - chaining, ring closure, the
//! ladder's snap, the refusal set.
//!
//! Fixtures are analytic: a spherical cap (normals tilt 0..45 deg, the
//! scallop law's home turf), a 45-degree cone (the law's equality case),
//! and a box (pure structure - its flat shelves are the parallel
//! strategy's jurisdiction, so the envelope must stay silent on them).
//!
//! Gate map (M4.4): covers camcraft_motion::cl, camcraft_motion::finishing3d, camcraft_motion::geo2d, camcraft_motion::geometry, camcraft_motion::machining2d, camcraft_motion::stock_sim, camcraft_motion::verify.
//! Lane shape (M4.4): 54 law tests run here in seconds (M2.5 B adds
//! the crease pair, M2.5 D the band, the lofts and the drape); 33 envelope-VERDICT and
//! dome-generation tests are #[ignore]d - run them with
//! `cargo test --release -p camcraft-app -- --ignored` at milestone close.

use camcraft_motion::cl::{lattice, MoveKind, Toolpath};
use camcraft_motion::finishing3d::{
    blend, contour, corner, flat, geodesic, morph, morphed_spiral, parallel, pencil, project,
    radial, scallop_stepdown, scallop_stepover, spiral, waterline, Finish3DParams, FinishPattern,
};
use camcraft_motion::geometry::{slice, Point3, Triangle};
use camcraft_motion::geo2d::Region;
use camcraft_motion::machining2d::GenError;
use camcraft_motion::stock_sim::envelope_scan;
use camcraft_motion::verify::{verify, Mode, ToolGeom, VerifyContext};

const PI: f64 = std::f64::consts::PI;
const TAU: f64 = std::f64::consts::TAU;

/// The tool every fixture is planned with: catalog BN-0600 (r = 3).
const R: f64 = 3.0;
/// The tolerance every fixture is planned with.
const TOL: f64 = 0.1;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A sphere of `radius` centered at the origin, cut at polar angle
/// `polar_max` and closed with a flat disc (or left closed at the pole
/// when `polar_max` reaches it). Band facets plus the cap fan.
fn sphere_cap(radius: f64, polar_max: f64, n_lat: usize, n_lon: usize) -> Vec<Triangle> {
    let mut facets = Vec::new();
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon)
            .map(|j| {
                let th = TAU * j as f64 / n_lon as f64;
                p3(rr * th.cos(), rr * th.sin(), z)
            })
            .collect()
    };
    let apex = p3(0.0, 0.0, radius);
    let first = ring(1);
    for j in 0..n_lon {
        facets.push(Triangle::new(apex, first[j], first[(j + 1) % n_lon]));
    }
    for k in 1..n_lat {
        let ra = ring(k);
        let rb = ring(k + 1);
        for j in 0..n_lon {
            let j2 = (j + 1) % n_lon;
            facets.push(Triangle::new(ra[j], rb[j], rb[j2]));
            facets.push(Triangle::new(ra[j], rb[j2], ra[j2]));
        }
    }
    if polar_max < PI - 1e-9 {
        let rim = ring(n_lat);
        let zb = radius * polar_max.cos();
        let center = p3(0.0, 0.0, zb);
        for j in 0..n_lon {
            facets.push(Triangle::new(center, rim[j], rim[(j + 1) % n_lon]));
        }
    }
    facets
}

/// A solid cone, apex at (0, 0, 20), base disc at z = 0 of radius 20:
/// walls tilted exactly 45 deg - the stepdown law's equality case.
fn cone(n_lon: usize) -> Vec<Triangle> {
    let mut facets = Vec::new();
    let apex = p3(0.0, 0.0, 20.0);
    let base: Vec<Point3> = (0..n_lon)
        .map(|j| {
            let th = TAU * j as f64 / n_lon as f64;
            p3(20.0 * th.cos(), 20.0 * th.sin(), 0.0)
        })
        .collect();
    for j in 0..n_lon {
        facets.push(Triangle::new(apex, base[j], base[(j + 1) % n_lon]));
    }
    let center = p3(0.0, 0.0, 0.0);
    for j in 0..n_lon {
        facets.push(Triangle::new(center, base[(j + 1) % n_lon], base[j]));
    }
    facets
}

/// A solid cone, apex at (0, 0, 34.641), base disc r = 20 at z = 0:
/// walls at 60 deg - STEEPER than the 45° equality case, so the
/// tangent point sits far below the center plane and a plunge column
/// has more finished flank above it to bite. M8.2 B's re-proof stage.
fn steep_cone(n_lon: usize) -> Vec<Triangle> {
    let h = 20.0 * (PI / 3.0).tan(); // h = R·tan(60°): 34.641
    let mut facets = Vec::new();
    let apex = p3(0.0, 0.0, h);
    let base: Vec<Point3> = (0..n_lon)
        .map(|j| {
            let th = TAU * j as f64 / n_lon as f64;
            p3(20.0 * th.cos(), 20.0 * th.sin(), 0.0)
        })
        .collect();
    for j in 0..n_lon {
        facets.push(Triangle::new(apex, base[j], base[(j + 1) % n_lon]));
    }
    let center = p3(0.0, 0.0, 0.0);
    for j in 0..n_lon {
        facets.push(Triangle::new(center, base[(j + 1) % n_lon], base[j]));
    }
    facets
}

/// Distance from a point to a triangle - the same Voronoi-region
/// closest-point law as `finishing3d`'s support scan (Ericson,
/// Real-Time Collision Detection, published geometry); written here
/// so the law test asks its own question of the facets.
fn pt_tri_dist(p: Point3, t: &Triangle) -> f64 {
    let dot = |u: Point3, v: Point3| u.dot(v);
    let nn = |u: Point3| (dot(u, u)).sqrt();
    let lerp3 = |a: Point3, b: Point3, s: f64| {
        Point3::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s)
    };
    let (ab, ac, ap) = (t.b - t.a, t.c - t.a, p - t.a);
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return nn(ap);
    }
    let bp = p - t.b;
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return nn(bp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return nn(p - lerp3(t.a, t.b, d1 / (d1 - d3)));
    }
    let cp = p - t.c;
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return nn(cp);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return nn(p - lerp3(t.a, t.c, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return nn(p - lerp3(t.b, t.c, (d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    let q = Point3::new(
        t.a.x + ab.x * v + ac.x * w,
        t.a.y + ab.y * v + ac.y * w,
        t.a.z + ab.z * v + ac.z * w,
    );
    nn(p - q)
}

/// A 20 x 20 x 10 box centered on the Z axis, z in 0..10.
fn box_facets() -> Vec<Triangle> {
    let q = |sx: f64, sy: f64, z: f64| p3(sx * 10.0, sy * 10.0, z);
    let (a, b, c, d) = (q(-1.0, -1.0, 0.0), q(1.0, -1.0, 0.0), q(1.0, 1.0, 0.0), q(-1.0, 1.0, 0.0));
    let (e, f, g, h) = (q(-1.0, -1.0, 10.0), q(1.0, -1.0, 10.0), q(1.0, 1.0, 10.0), q(-1.0, 1.0, 10.0));
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let mut facets = Vec::new();
    facets.extend(quad([e, f, g, h])); // top
    facets.extend(quad([a, c, b, d])); // bottom (opposite winding)
    facets.extend(quad([a, b, f, e])); // sides
    facets.extend(quad([b, c, g, f]));
    facets.extend(quad([c, d, h, g]));
    facets.extend(quad([d, a, e, h]));
    facets
}

fn ppattern(top_z: f64, bottom_z: f64, pattern: FinishPattern) -> Finish3DParams {
    Finish3DParams { pattern, ..params(top_z, bottom_z) }
}

fn params(top_z: f64, bottom_z: f64) -> Finish3DParams {
    Finish3DParams {
        pattern: FinishPattern::Waterline,
        stepover: None,
        tool_diameter: 2.0 * R,
        tolerance: TOL,
        clearance_z: 25.0,
        retract_z: None,
        top_z,
        bottom_z,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        // The surface-finish defaults state v17's law: the march's
        // own floor, and the 45° the raster's jurisdiction already
        // grants. Every existing test rides these untouched.
        min_step: None,
        ramp_angle: 45.0,
        // M8.2 B: the ring entry keeps its plunge unless asked.
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
    }
}

/// The envelope gate `verify.rs` states: the tolerance plus the scan's
/// own quantization floor (one cell's sagitta of the ball).
fn gate(cell: f64) -> f64 {
    TOL + cell * cell / (2.0 * R) + 1e-9
}

fn envelope(facets: &[Triangle], t: &Toolpath) -> camcraft_motion::stock_sim::EnvelopeReport {
    envelope_scan(facets, &t.moves, R, TOL).expect("fixture-sized grid")
}

/// The scallop ratchet (M13.9): the ridge flank law (`stock_sim::ridge`)
/// reads these fixtures' spacing scallop OVER the gate - a quality
/// defect, not a gouge (RECOMMENDED "M13.8 B" (3)) - and the
/// generator fix is the owed item. Until it lands, every fixture pins
/// the reading it had when the law went live: a rise fails, a fall is
/// free. Fixtures not listed answer to the gate itself. The same
/// pattern as `KINK_RATCHET` in `adaptive_hsm.rs`. The dome waterline
/// left the table with the slope-aware ladder (`finishing3d::ladder`):
/// 0.183 at the 27° crown band became 0 - it answers to the gate. The
/// march's skip-phase stride ladder (M13.9, `march.rs`) took the
/// cos-law rasters' over-reads with it: the blind 4·delta0 probe had
/// dropped whole runs between the tessellation's fold gaps (dome
/// multidirection 0.349 → 0.125, dome scallop and the geodesic to the
/// gate, hill project to 0 with the ring's-own-inside law in
/// `stock_sim::ridge`). The convex folds' edge contact (#36,
/// `finishing3d::edge`) closed the strict tangency's fold annulus
/// (`r·Δm`, 0.47 mm between the hill's 9° bands): every hill entry and
/// the dome multidirection read 0 since. The tilted ramp's 0.1698 at
/// the high rim corner (7.19, -7.81) was a real gap: the fixed-phase
/// line grid's last line with ground contacted 1.2 mm short of the
/// rim; the rim row (#46, `finishing3d::rim`) lays a line where the
/// ground ends and it reads 0. The table is empty - every fixture
/// answers to the gate; it stays as the place a future pin goes.
const SCALLOP_RATCHET: &[(&str, f64)] = &[];

fn scallop_ratchet(fixture: &str, rep: &camcraft_motion::stock_sim::EnvelopeReport) {
    let cap = SCALLOP_RATCHET
        .iter()
        .find(|(f, _)| *f == fixture)
        .map_or(gate(rep.cell), |&(_, c)| c + 1e-3);
    assert!(
        rep.scallop_mm <= cap,
        "{fixture}: scallop {} mm at {:?} rose past its ratchet {cap:.4} (gate {:.4})",
        rep.scallop_mm,
        rep.scallop_at,
        gate(rep.cell)
    );
}

/// The verifier's list without the scallop note (M13.9): the ratchet
/// above judges that reading; every blocking kind must still be absent.
fn sans_scallop(w: Vec<camcraft_motion::verify::Warning>) -> Vec<camcraft_motion::verify::Warning> {
    w.into_iter().filter(|w| w.blocks_post()).collect()
}

/// The full verifier with the envelope law switched on - the shape the
/// operation layer assembles for a Finish3D op. `stock_top` is the
/// fixture's own top height: laws (c)/(d) measure the path against
/// remaining material, and for a finish on a near-net model the
/// honest stand-in for "not yet cut" is the part surface, not the
/// clearance plane (clearance there would make every entry rapid
/// "inside stock").
fn verify_finish(
    facets: &[Triangle],
    t: &Toolpath,
    stock_top: f64,
) -> Vec<camcraft_motion::verify::Warning> {
    let region: Region = slice::project(facets, 64).expect("fixture silhouette");
    let tool = ToolGeom {
        radius: R,
        nose_radius: 0.0,
        flute_len: 10.0,
        shank_radius: 2.9,
        shank_len: 40.0,
        holder_radius: 4.9,
        holder_len: 30.0,
    };
    let cx = VerifyContext {
        region: &region,
        stock: None,
        model: Some(facets),
        tool: &tool,
        cutter: None,
        mode: Mode::Envelope,
        stock_top_z: stock_top,
        clearance_z: 25.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: Some(TOL),
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(t, &cx)
}

// The dome: R = 20, normals tilt 0..45 deg, rim at z = 20/sqrt(2).
fn dome() -> Vec<Triangle> {
    sphere_cap(20.0, PI / 4.0, 12, 96)
}
const RIM_Z: f64 = 20.0 * std::f64::consts::FRAC_1_SQRT_2;

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn dome_waterline_is_envelope_clean() {
    // The headline promise: rings from the tangent-locus scan, walked
    // with the ball, leave the swept envelope within tolerance of the
    // model - measured on the grid, not assumed from the construction.
    let f = dome();
    let t = waterline(&f, &params(18.0, RIM_Z)).expect("dome waterline");
    assert!(!t.moves.is_empty());
    let rep = envelope(&f, &t);
    assert!(
        rep.gouge_mm <= gate(rep.cell),
        "gouged {} mm at {:?} (gate {})",
        rep.gouge_mm,
        rep.at,
        gate(rep.cell)
    );
    assert!(
        rep.rest_mm <= gate(rep.cell),
        "left {} mm of rest at {:?} (gate {})",
        rep.rest_mm,
        rep.at,
        gate(rep.cell)
    );
    scallop_ratchet("dome waterline", &rep);
    assert_eq!(
        sans_scallop(verify_finish(&f, &t, 20.0)),
        Vec::new(),
        "the verifier agrees"
    );
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn cone_rest_hits_the_scallop_budget_not_past_it() {
    // The 45-degree wall is where `Delta z = 2 sqrt(r tol)` is exactly
    // at budget: the measured ridge should be AT the cusp the law
    // promises (tol), never meaningfully above it.
    let f = cone(96);
    let t = waterline(&f, &params(16.0, 0.5)).expect("cone waterline");
    let rep = envelope(&f, &t);
    assert!(
        rep.gouge_mm <= gate(rep.cell),
        "gouged {} mm at {:?}",
        rep.gouge_mm,
        rep.at
    );
    assert!(
        rep.rest_mm <= gate(rep.cell),
        "left {} mm of rest at {:?}; the law's budget is {TOL}",
        rep.rest_mm,
        rep.at
    );
    assert_eq!(verify_finish(&f, &t, 20.0), Vec::new());
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn box_walls_are_pure_structure() {
    // A box has no tangent ring across its flat shelves, and the column
    // model sees no surface on its vertical walls: the VERDICT must be
    // silent (the shelves are M2.2b's job), while the path itself is the
    // offset-ladder structure every other depth-stepping op shares.
    // The scan, though, is honest: on exactly-fitted rings the top
    // corner columns carry two near-vertical band contacts (the x-wall
    // ring and the y-wall ring both graze them), so they read as real
    // ridges with a real - but sub-gate - cusp standing between the two
    // offset ladders. Silence is the verifier's; measurement tells the
    // truth, and the truth here is under budget, not zero. Since the
    // M5.2 E facet-distance law the corner vertex now registers on the
    // GOUGE channel too (0.027 mm at a 0.123 gate) - the old dexel-floor
    // law was blind to a graze with no floor under the wall columns - so
    // the assert is `gouge <= gate`, the same budget the comment names,
    // never the `gouge == 0` the blind law let it stay.
    let f = box_facets();
    let t = waterline(&f, &params(10.0, 0.0)).expect("box waterline");
    assert!(!t.moves.is_empty());
    let rep = envelope(&f, &t);
    assert!(
        rep.gouge_mm <= gate(rep.cell),
        "the tangent ladder cut below the box tops past budget: {:?}",
        rep
    );
    assert!(
        rep.rest_mm <= gate(rep.cell),
        "corner cusp of {} mm at {:?} breaches the gate {}",
        rep.rest_mm,
        rep.at,
        gate(rep.cell)
    );
    assert_eq!(verify_finish(&f, &t, 10.0), Vec::new());
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn overhang_is_caught_by_the_measurement() {
    // The module's entry law, stated twice by its own history. This
    // fixture used to DOCUMENT the violation: a vertical plunge over
    // an overhang breaks the tangent promise, a sphere run past its
    // equator is exactly that, and the assert demanded the envelope
    // CAUGHT the gouge the generator committed (silent gouge being
    // the worst failure mode without exception). M2.5 A's verdict
    // lane fixed the generator - the rings' entry plunge is now
    // column-certified - so the violation never reaches the measure.
    // The law the same assert now protects is stronger: the refused
    // entries speak (a counted note), the cut moves stay inside
    // budget, and the ground under the equator stands as REPORTED
    // rest, never quietly skipped.
    let f = sphere_cap(20.0, 0.9 * PI, 10, 96);
    let t = waterline(&f, &params(16.0, -16.0)).expect("sphere waterline");
    let rep = envelope(&f, &t);
    assert!(
        rep.gouge_mm <= gate(rep.cell),
        "an entry the column law could refuse still gouged: {:?}",
        rep
    );
    assert!(
        t.notes.iter().any(|n| n.contains("plunge column")),
        "the refused entries went quiet: {:?}",
        t.notes
    );
    let warns = verify_finish(&f, &t, 20.0);
    assert!(
        warns
            .iter()
            .any(|w| matches!(w, camcraft_motion::verify::Warning::Envelope { .. })),
        "the verifier's word is missing: {warns:?}"
    );
}

#[test]
#[ignore = "heavy lane: waterline generation over the 12x96 dome (structural expectations ride this fixture's counts); ~10-50s in debug, seconds in release; run at milestone close with -- --ignored"]
fn rings_close_and_the_chain_is_continuous() {
    // Every plunge lands on its ring's first point; every ring's last
    // feed returns to that point; and the whole path chains - no move
    // starts where the previous one did not end.
    let t = waterline(&dome(), &params(18.0, RIM_Z)).expect("dome waterline");
    let mut pl = 0usize;
    let m = &t.moves;
    for i in 0..m.len() {
        if m[i].kind == MoveKind::Plunge {
            pl += 1;
            // Walk forward through the ring's feeds: the last must land
            // on the plunge's XY (the ring closes).
            let start = m[i].to;
            let mut j = i + 1;
            while j < m.len() && m[j].kind == MoveKind::Feed {
                j += 1;
            }
            assert!(j > i + 1, "a plunge with no ring behind it");
            let last = &m[j - 1];
            assert!(
                (last.to.x - start.x).abs() <= 1e-6 && (last.to.y - start.y).abs() <= 1e-6,
                "ring {pl} at {start:?} did not close: ended at {:?}",
                last.to
            );
        }
        if i + 1 < m.len() {
            let (a, b) = (&m[i].to, &m[i + 1].from);
            assert!(
                (a.x - b.x).abs() <= 1e-9 && (a.y - b.y).abs() <= 1e-9 && (a.z - b.z).abs() <= 1e-9,
                "the chain broke at move {i}: {a:?} -> {b:?}"
            );
        }
    }
    assert!(pl >= 4, "expected several rings, saw {pl}");
    // Arc-fitted rings: the close may end exactly on the start or carry
    // a zero-XY closing arc; both chain the same way.
    assert_eq!(t.moves.first().map(|m| m.kind), Some(MoveKind::Rapid));
    let last = t.moves.last().expect("non-empty path");
    assert_eq!(last.kind, MoveKind::Rapid);
    assert_eq!(last.to.z, 25.0, "the path ends at clearance");
}

#[test]
fn the_tangent_plunge_never_cuts_above_its_ring() {
    // M8.2 B re-proves the entry law on a STEEPER flank than the 45°
    // equality cone: at 60° the tangent point sits far below where the
    // level plane crosses the surface, so more finished band stands
    // above every plunge than the old fixture ever presented. The
    // claim is geometric: lift the ball center straight up off a
    // tangent ring's tangent center and it recedes from the surface
    // (the locally convex premise) - so at every sampled center
    // strictly ABOVE a ring's tangent center, the distance to every
    // facet must stay inside the deviation budget the ring law itself
    // promises. The tessellated locus sits tol-sagged inside the true
    // one, so the honest currency here is `tol`, not epsilon: a bite
    // beyond tol is a cut above the ring's level, past what the whole
    // family's finish_tol already licenses. Measured, not assumed.
    let facets = steep_cone(96);
    let t = waterline(&facets, &params(18.0, 4.0)).expect("steep cone waterline");
    let mut rings = 0;
    for m in &t.moves {
        if m.kind != MoveKind::Plunge {
            continue;
        }
        assert!(
            (m.to.x - m.from.x).abs() <= 1e-9 && (m.to.y - m.from.y).abs() <= 1e-9,
            "first contact must be the vertical plunge the law states"
        );
        rings += 1;
        let (x, y) = (m.to.x, m.to.y);
        let mut cz = m.to.z + R + 1e-6;
        while cz <= 25.0 + R + 1e-9 {
            let c = p3(x, y, cz);
            for f in &facets {
                let d = pt_tri_dist(c, f);
                assert!(
                    d > R - TOL,
                    "plunge into the ring ending at tip z {:.4} has center z {:.4} biting \
                     {:.6} mm into the facet past the {} mm budget, {} above the ring's level",
                    m.to.z,
                    cz,
                    R - d,
                    TOL,
                    cz - (m.to.z + R)
                );
            }
            cz += R * 0.5;
        }
    }
    assert!(rings >= 4, "expected several steep-flank rings, saw {rings}");
}

/// M8.2 B's ramp: with `entry_ramp` set, the first contact of a steep
/// ring slides in along the contact normal instead of dropping, and
/// the diagonal's cut is certified by the same probe the vertical
/// plunge just passed - the tangent law holds on the slanted way in.
#[test]
fn the_ramp_enters_a_steep_ring_on_its_contact_normal() {
    let f = steep_cone(96);
    let mut p = params(18.0, 4.0);
    p.entry_ramp = true;
    let t = waterline(&f, &p).expect("ramped cone waterline");
    // The cone's flank (60 deg from horizontal) is inside the steep
    // window, and its tip is a point no facet touches - every entry
    // must certify, silently; a refusal note here means the ramp's own
    // gate mis-reads a flank it was made for.
    for n in &t.notes {
        assert!(!n.contains("refused"), "ramp refused a certifiable flank: {n}");
    }
    let (mut diagonals, mut columns) = (0, 0);
    for m in &t.moves {
        let (adx, ady) = ((m.to.x - m.from.x).abs(), (m.to.y - m.from.y).abs());
        // A ramped entry is a diagonal Feed at plunge feed descending
        // into the ring, preceded by a vertical Plunge column ending
        // one radius below where the diagonal starts.
        let diag = m.kind == MoveKind::Feed && m.feed == Some(100.0) && m.from.z > m.to.z && (adx > 1e-9 || ady > 1e-9);
        let col = m.kind == MoveKind::Plunge;
        if !diag && !col {
            continue;
        }
        if diag {
            diagonals += 1;
        } else {
            columns += 1;
        }
        // The ball center along the entry leg, then straight up off it:
        // the same probe the vertical-only law passes (locally convex
        // flank recedes from a lifted center; tol is the tessellation
        // budget, not epsilon).
        let mut cz = m.to.z + R + 1e-6;
        while cz <= 25.0 + R + 1e-9 {
            let c = p3(m.to.x, m.to.y, cz);
            for facet in &f {
                let d = pt_tri_dist(c, facet);
                assert!(
                    d > R - TOL,
                    "{tag} center ({c:?}) bit {r:.4} mm into a facet above the entry: ({x:?}, {y:?}, {z:?})",
                    tag = if diag { "diagonal" } else { "column" },
                    r = R - d,
                    x = facet.a.x,
                    y = facet.a.y,
                    z = facet.a.z,
                );
            }
            cz += R * 0.5;
        }
        if diag {
            for k in 0..=8 {
                let s = k as f64 / 8.0;
                let c = p3(
                    m.from.x + s * (m.to.x - m.from.x),
                    m.from.y + s * (m.to.y - m.from.y),
                    m.from.z + s * (m.to.z - m.from.z) + R,
                );
                for facet in &f {
                    let d = pt_tri_dist(c, facet);
                    assert!(
                        d > R - TOL,
                        "diagonal ({m:?}) at t {s:.3} bit {r:.4} mm into a facet: ({x:?}, {y:?}, {z:?})",
                        r = R - d,
                        x = facet.a.x,
                        y = facet.a.y,
                        z = facet.a.z,
                    );
                }
            }
        }
    }
    assert!(diagonals >= 4, "expected ramped entries, saw {diagonals}");
    assert_eq!(diagonals, columns, "every diagonal needs its vertical column leg");
}

/// The flag is opt-in: off (the default, every pre-0.5 document) the
/// choreography is the vertical plunge, byte for byte, notes empty.
#[test]
fn the_ramp_is_off_until_asked() {
    let f = steep_cone(96);
    let t = waterline(&f, &params(18.0, 4.0)).expect("default cone waterline");
    assert!(t.notes.is_empty(), "a default run wrote notes: {:?}", t.notes);
    assert!(
        !t.moves
            .iter()
            .any(|m| m.kind == MoveKind::Feed && m.feed == Some(100.0) && (m.from.x != m.to.x || m.from.y != m.to.y)),
        "the default path carries a diagonal entry"
    );
}

/// The wall that cannot lift a ramp: a vertical box flank reads its
/// contact normal at the horizon, and cutting in along it would be a
/// lateral plunge at depth - the ramp refuses itself, notes why, and
/// the honest vertical entry stands unchanged.
#[test]
fn the_ramp_refuses_the_vertical_wall_and_still_cuts() {
    let f = box_facets();
    let mut p = params(8.0, 2.0);
    p.entry_ramp = true;
    let t = waterline(&f, &p).expect("a refusal is a note, never an error");
    assert!(
        t.notes.iter().any(|n| n.contains("ramp entry") && n.contains("refused")),
        "the vertical wall wrote no refusal: {:?}",
        t.notes
    );
    assert!(
        t.moves.iter().any(|m| m.kind == MoveKind::Plunge),
        "the refusal must leave the vertical entry standing"
    );
}

#[test]
#[ignore = "heavy lane: waterline generation over the 12x96 dome (structural expectations ride this fixture's counts); ~10-50s in debug, seconds in release; run at milestone close with -- --ignored"]
fn levels_snap_to_the_bottom_and_obey_the_stepdown() {
    // The ladder's last level is exactly `bottom_z` (shared snap law),
    // and successive plunge planes differ by at most the scallop
    // stepdown (the snap can only shorten the final gap, never widen
    // any gap past dz).
    let (top, bottom) = (18.0, RIM_Z);
    let t = waterline(&dome(), &params(top, bottom)).expect("dome waterline");
    let dz = scallop_stepdown(R, TOL);
    let tip_zs: Vec<f64> = {
        let mut v: Vec<f64> = t
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge)
            .map(|m| m.to.z)
            .collect();
        v.sort_by(|a, b| b.total_cmp(a));
        v.dedup();
        v
    };
    assert_eq!(tip_zs.first(), Some(&top), "first ring is the top");
    assert!(
        tip_zs.iter().any(|&z| (z - bottom).abs() <= 1e-9),
        "no level snapped to the bottom {bottom}: {tip_zs:?}"
    );
    for w in tip_zs.windows(2) {
        assert!(
            w[0] - w[1] > 1e-9 && w[0] - w[1] <= dz + 1e-9,
            "gap {} outside (0, stepdown {dz}]: doubled or missing level",
            w[0] - w[1]
        );
    }
}

#[test]
#[ignore = "heavy lane: waterline generation over the 12x96 dome (structural expectations ride this fixture's counts); ~10-50s in debug, seconds in release; run at milestone close with -- --ignored"]
fn a_hop_plane_replaces_only_the_inter_ring_rapids() {
    // The shared ladder law, finish flavor: rings within one level hop
    // at `retract_z` (clamped to clearance); a level change still
    // climbs to clearance. A single dome has one loop per level, so no
    // inter-ring hop ever exists to check - the fixture is two caps,
    // whose upper levels carry two rings each until the tangent disks
    // merge near the rims.
    let f = twin_dome();
    let plain = waterline(&f, &params(18.0, RIM_Z)).expect("plain");
    let mut p = params(18.0, RIM_Z);
    p.retract_z = Some(20.0);
    let hopped = waterline(&f, &p).expect("hopped");
    let hi_rapids = |t: &Toolpath| -> Vec<f64> {
        t.moves
            .iter()
            .filter(|m| m.kind == MoveKind::Rapid)
            .map(|m| m.to.z)
            .collect::<Vec<_>>()
    };
    let zs = hi_rapids(&hopped);
    assert!(zs.contains(&20.0), "no hop at the retract plane: {zs:?}");
    assert!(
        zs.contains(&25.0),
        "the level change stopped climbing to clearance: {zs:?}"
    );
    assert!(
        hi_rapids(&plain).iter().all(|&z| z == 25.0),
        "without retract_z every rapid goes to clearance"
    );
    assert_eq!(hopped.moves.len(), plain.moves.len(), "same law, same moves");
}

/// Two caps, 30 mm apart: levels where the forbidden disks stay disjoint
/// carry two rings, which is what gives the hop law something to hop
/// between (a single dome has one loop per level - no inter-ring move
/// exists there at all).
fn twin_dome() -> Vec<Triangle> {
    let mut f = dome();
    f.extend(dome().into_iter().map(|t| {
        Triangle::new(
            p3(t.a.x + 30.0, t.a.y, t.a.z),
            p3(t.b.x + 30.0, t.b.y, t.b.z),
            p3(t.c.x + 30.0, t.c.y, t.c.z),
        )
    }));
    f
}

#[test]
#[ignore = "heavy lane: waterline generation over the 12x96 dome (structural expectations ride this fixture's counts); ~10-50s in debug, seconds in release; run at milestone close with -- --ignored"]
fn climb_and_conventional_are_mirror_walks() {
    // The law is the SAME rings walked OPPOSITE ways - not identical
    // move lists. fit_arcs segments a ring from its start vertex, and
    // on the exact circles the region fixes now produce it collapses
    // each ring to a couple of 180-degree arcs (zero sagitta - the arc
    // IS the ring), so a reversed walk legitimately cuts different
    // chords and total move counts need not match (an earlier version
    // of this test asserted they did, and the arc fitter honestly
    // disagreed). What must match: the ring count, the levels, and the
    // footprint; what must flip: the sign of each ring's XY traversal.
    // The footprint is measured over an ARC-AWARE trace - an
    // endpoint-only shoelace of a two-semicircle ring is a sliver
    // between antipodal vertices, not the circle it walks.
    let f = dome();
    let climb = waterline(&f, &params(18.0, RIM_Z)).expect("climb");
    let mut p = params(18.0, RIM_Z);
    p.climb = false;
    let conv = waterline(&f, &p).expect("conventional");
    let ring_trace = |t: &Toolpath| -> Vec<Vec<(f64, f64)>> {
        // One XY polyline per ring, plunge to plunge, arcs sampled at
        // <= 15 degrees so the polyline's area is the ring's footprint
        // to well within a part per thousand.
        let mut out: Vec<Vec<(f64, f64)>> = Vec::new();
        for m in &t.moves {
            match m.kind {
                MoveKind::Plunge => out.push(vec![(m.to.x, m.to.y)]),
                MoveKind::Feed => {
                    let Some(ring) = out.last_mut() else { continue };
                    match m.arc {
                        None => ring.push((m.to.x, m.to.y)),
                        Some(_)
                            if m.from.x == m.to.x && m.from.y == m.to.y =>
                        {
                            ring.push((m.to.x, m.to.y)) // no sweep to sample
                        }
                        Some(arc) => {
                            let (cx, cy) = (arc.center.x, arc.center.y);
                            let a0 = (m.from.y - cy).atan2(m.from.x - cx);
                            let mut da =
                                (m.to.y - cy).atan2(m.to.x - cx) - a0;
                            if arc.ccw {
                                while da <= 0.0 {
                                    da += TAU;
                                }
                            } else {
                                while da >= 0.0 {
                                    da -= TAU;
                                }
                            }
                            let rr = (m.from.x - cx).hypot(m.from.y - cy);
                            let n = (da.abs() / 0.2618).ceil() as usize;
                            for i in 1..=n.max(1) {
                                let a = a0 + da * i as f64 / n.max(1) as f64;
                                ring.push((cx + rr * a.cos(), cy + rr * a.sin()));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        out
    };
    let areas = |rings: Vec<Vec<(f64, f64)>>| -> Vec<f64> {
        rings
            .iter()
            .map(|pts| {
                let mut a = 0.0;
                for i in 0..pts.len() {
                    let (x0, y0) = pts[i];
                    let (x1, y1) = pts[(i + 1) % pts.len()];
                    a += x0 * y1 - x1 * y0;
                }
                a * 0.5
            })
            .collect()
    };
    let ca = areas(ring_trace(&climb));
    let va = areas(ring_trace(&conv));
    assert!(ca.len() >= 4, "expected several rings, saw {}", ca.len());
    assert_eq!(ca.len(), va.len(), "same rings");
    for (a, b) in ca.iter().zip(va.iter()) {
        assert!(a.abs() > 1.0, "a ring traced a sliver, not a footprint: {a}");
        assert!(a * b < 0.0, "walks not opposed: {a} vs {b}");
        let rel = (a.abs() - b.abs()).abs() / a.abs().max(1.0);
        assert!(rel < 0.01, "different footprints: {a} vs {b}");
    }
    let differs = climb
        .moves
        .iter()
        .zip(conv.moves.iter())
        .any(|(a, b)| a.to != b.to);
    assert!(differs, "the XOR did nothing");
}

#[test]
#[ignore = "heavy lane: waterline generation over the 12x96 dome (structural expectations ride this fixture's counts); ~10-50s in debug, seconds in release; run at milestone close with -- --ignored"]
fn the_generator_is_deterministic() {
    let f = dome();
    let p = params(18.0, RIM_Z);
    assert_eq!(
        waterline(&f, &p).expect("first").moves,
        waterline(&f, &p).expect("second").moves,
        "two calls, two paths - a stamp would churn forever"
    );
}

/// The M4.2b law, pinned on one machine: the march quantizes every
/// point it asks the surface about onto the micron lattice, so its
/// halving/accept decision reads the same bits on glibc and the
/// Windows CRT - the wobble that made the dome goldens disagree 560
/// to 558. What a decision consumes, the output exhibits: every
/// stored sample is a lattice point, never a platform-dependent
/// real a hair off one. (A mesh-wide ULP nudge is deliberately NOT
/// asserted - jittering the facet planes moves the model's own
/// knife-edges, a rim sitting exactly on a programmed floor, which
/// is degenerate-fixture territory, not the platform's last bit.)
#[test]
fn every_march_sample_lands_on_the_micron_lattice() {
    let domed = sphere_cap(12.0, PI / 3.0, 6, 24);
    let p = params(12.0, 6.0);
    for (family, path) in [
        ("radial", radial(&domed, &p).expect("radial path")),
        ("project", project(&domed, &p).expect("project path")),
    ] {
        for m in &path.moves {
            for v in [m.to.x, m.to.y] {
                assert_eq!(lattice(v), v, "{family} sample off the lattice: {v}");
            }
        }
    }
}

#[test]
fn refusals_are_clean_errors_never_a_panic() {
    // The empty model.
    let e = waterline(&[], &params(1.0, 0.0)).expect_err("nothing to contour");
    assert!(matches!(e, GenError::Empty(_)), "{e:?}");
    // A non-finite vertex, named as the model's, not as a crash.
    let mut f = dome();
    f[0].b = p3(f64::NAN, 0.0, 0.0);
    let e = waterline(&f, &params(18.0, RIM_Z)).expect_err("corrupt model");
    assert!(matches!(e, GenError::Invalid(_)), "{e:?}");
    assert!(e.to_string().contains("non-finite"), "{e}");
    // Bottom above top.
    let e = waterline(&dome(), &params(RIM_Z, 18.0)).expect_err("inverted ladder");
    assert!(e.to_string().contains("above its top"), "{e}");
    // The level cap: a tolerance so fine the rings number in the
    // hundreds of thousands is REFUSED, never quietly coarsened.
    let mut p = params(100.0, 0.0);
    p.tolerance = 1e-6; // dz = 2 sqrt(3e-6) = 0.0035, span 100 -> 29000 > 8192
    let e = waterline(&cone(96), &p).expect_err("tolerance overruns the cap");
    assert!(e.to_string().contains("8192"), "{e}");
    // No surface anywhere near the requested depths.
    let e = waterline(&box_facets(), &params(-50.0, -51.0)).expect_err("air only");
    assert!(matches!(e, GenError::Empty(_)), "{e:?}");
    assert!(e.to_string().contains("ball radius"), "{e}");
    // Unvalidated parameters refuse at the gate, naming nothing false.
    let mut p = params(18.0, RIM_Z);
    p.tool_diameter = 0.0;
    assert!(matches!(
        waterline(&dome(), &p),
        Err(GenError::Invalid(_))
    ));
    let mut p = params(18.0, RIM_Z);
    p.tolerance = f64::NAN;
    assert!(matches!(
        waterline(&dome(), &p),
        Err(GenError::Invalid(_))
    ));
}

#[test]
fn degenerate_meshes_never_panic() {
    // Zero-area facets, collinear bands, a single dust-mote triangle:
    // the generator answers Ok-or-Err, never a hang or a crash.
    let mote = vec![Triangle::new(
        p3(0.0, 0.0, 1.0),
        p3(0.0, 0.0, 1.0),
        p3(0.0, 0.0, 1.0),
    )];
    let r = waterline(&mote, &params(1.0, 0.9));
    assert!(r.is_ok() || r.is_err());
    let line = vec![Triangle::new(
        p3(-5.0, 0.0, 1.0),
        p3(5.0, 0.0, 1.0),
        p3(0.0, 0.0, 1.0),
    )];
    let r = waterline(&line, &params(1.0, 0.9));
    assert!(r.is_ok() || r.is_err());
    // A grid of points scattered over the band - hulls collapse,
    // region may empty, nothing unwraps.
    let scatter: Vec<Triangle> = (-8..8)
        .flat_map(|i| (-8..8).map(move |j| (i, j)))
        .map(|(i, j)| {
            let x = i as f64;
            let y = j as f64;
            Triangle::new(p3(x, y, 1.0), p3(x, y, 1.0), p3(x, y, 1.0))
        })
        .collect();
    let _ = waterline(&scatter, &params(1.0, 0.0));
}

/// Distinct ring tip-Z levels of a path, top down.
fn ring_levels(t: &Toolpath) -> Vec<f64> {
    let mut v: Vec<f64> = t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).map(|m| m.to.z).collect();
    v.sort_by(|a, b| b.total_cmp(a));
    v.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
    v
}

#[test]
fn the_ladder_reads_the_slope_and_keeps_the_45_degree_law_as_its_ceiling() {
    // M13.9: on the wedge's 20° top (70° from vertical) the waterline's
    // rungs tighten to `√(8·r·tol)·sin 20°` wherever a band touches
    // that face; the 45° law `2√(r·tol)` is the ceiling everywhere.
    let dz45 = scallop_stepdown(R, TOL);
    let dz20 = scallop_stepover(R, TOL) * 20f64.to_radians().sin();
    assert!(dz20 < dz45);
    let f = tilted(20f64.to_radians(), 8.0);
    let (top, bottom) = (12.0, 5.0);
    let wl = ring_levels(&waterline(&f, &params(top, bottom)).expect("wedge waterline"));
    assert_eq!(wl.first(), Some(&top));
    assert!(wl.last().is_some_and(|z| (z - bottom).abs() <= 1e-9), "{wl:?}");
    let old = ((top - bottom) / dz45).ceil() as usize + 1;
    assert!(wl.len() > old, "the slope law did not tighten the ladder: {wl:?}");
    for w in wl.windows(2) {
        let gap = w[0] - w[1];
        assert!(gap > 0.0 && gap <= dz45 + 1e-9, "gap {gap} past the ceiling: {wl:?}");
        // Above the tilted face's lowest tip height the band always
        // touches it and the rung is the 20° law's or tighter.
        if w[1] > 7.0 {
            assert!(gap <= dz20 + 1e-9, "gap {gap} past the 20° law at {}: {wl:?}", w[1]);
        }
    }
    // A pattern with a shallow family keeps the 45° ladder: its raster
    // owns the top, the rings the walls (M13.9 scoping). The raster's
    // own plunges land at surface heights, so the ladder is read by
    // its rungs' presence: every 45° rung is there, none was moved.
    let has = |zs: &[f64], z: f64| zs.iter().any(|q| (q - z).abs() <= 1e-9);
    let walk = |dz: f64| -> Vec<f64> {
        let mut v = Vec::new();
        let mut z = top;
        while z > bottom + 1e-9 {
            v.push(z);
            z -= dz;
        }
        v
    };
    let par = ring_levels(&parallel(&f, &ppattern(top, bottom, FinishPattern::Parallel)).expect("wedge parallel"));
    for z in walk(dz45) {
        assert!(has(&par, z), "the raster's ladder lost its 45° rung {z}: {par:?}");
    }
    assert!(!has(&par, top - dz20), "the raster's ladder tightened: {par:?}");
    // And ground at the 45° line is the ceiling's own: the cone's
    // ladder is the fixed 2√(r·tol) walk, rung for rung.
    let cone_l = ring_levels(&waterline(&cone(96), &params(16.0, 0.5)).expect("cone waterline"));
    for (k, z) in cone_l.iter().enumerate().take(cone_l.len() - 1) {
        assert!((z - (16.0 - k as f64 * dz45)).abs() <= 1e-9, "cone rung {k} moved: {cone_l:?}");
    }
    // A stated flank spacing is never changed by the slope law.
    let ss = ring_levels(
        &parallel(&f, &Finish3DParams { steep_stepover: Some(2.0), ..ppattern(top, bottom, FinishPattern::SteepShallow) })
            .expect("wedge split"),
    );
    for z in walk(2.0) {
        assert!(has(&ss, z), "the stated ladder lost its rung {z}: {ss:?}");
    }
    assert!(!has(&ss, top - dz20) && !has(&ss, top - dz45), "a stated 2.0 was tightened: {ss:?}");
}

#[test]
fn scallop_stepdown_law_is_exact_and_total() {
    assert!((scallop_stepdown(3.0, 0.1) - 2.0 * 0.3f64.sqrt()).abs() <= 1e-12);
    for (r, tol) in [(0.0, 0.1), (3.0, 0.0), (-1.0, 0.1), (3.0, -0.1)] {
        assert_eq!(scallop_stepdown(r, tol), 0.0);
    }
    assert_eq!(scallop_stepdown(f64::NAN, 0.1), 0.0);
    assert_eq!(scallop_stepdown(3.0, f64::INFINITY), 0.0);
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn the_ball_radius_is_the_diameters_half_in_every_law() {
    // A guard against a silent factor-of-two: the path planned with Ø6
    // measures envelope-clean as the radius-3 ball that planned it -
    // and read back as a radius-6 ball (centers that stand 3 mm off
    // the surface, sweeping 6), it buries ~3 mm under where the plan
    // swore to graze. Either half of that law missing would make the
    // other assertion vacuous.
    let f = dome();
    let t = waterline(&f, &params(18.0, RIM_Z)).expect("Ø6 waterline");
    let clean = envelope(&f, &t);
    assert!(clean.gouge_mm <= gate(clean.cell), "{clean:?}");
    let rep = envelope_scan(&f, &t.moves, 2.0 * R, TOL).expect("measured with r=6");
    assert!(rep.gouge_mm > TOL, "the r/2 mixup went unnoticed");
}

// ---------------------------------------------------------------------------
// M2.2b: the parallel finish. The same envelope measurement judges the
// raster's laws; the ring's laws are unchanged and ride along beneath.

/// A four-walled prism with NO bottom facet: a flat shelf at z = 10 on
/// four vertical walls, z 0..10 - the auto-split's two jurisdictions in
/// one fixture (the shelf is the raster's, the walls the rings').
fn plateau() -> Vec<Triangle> {
    let q = |sx: f64, sy: f64, z: f64| p3(sx * 10.0, sy * 10.0, z);
    let (a, b, c, d) = (q(-1.0, -1.0, 0.0), q(1.0, -1.0, 0.0), q(1.0, 1.0, 0.0), q(-1.0, 1.0, 0.0));
    let (e, f, g, h) = (q(-1.0, -1.0, 10.0), q(1.0, -1.0, 10.0), q(1.0, 1.0, 10.0), q(-1.0, 1.0, 10.0));
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let mut facets = Vec::new();
    facets.extend(quad([e, f, g, h])); // top
    facets.extend(quad([a, b, f, e])); // sides
    facets.extend(quad([b, c, g, f]));
    facets.extend(quad([c, d, h, g]));
    facets.extend(quad([d, a, e, h]));
    facets
}

/// A closed wedge: one square top tilted `alpha` from horizontal, its
/// fall along +y (so the x-raster crosses the slope), four walls and a
/// flat base at z = 4. The stepover cos-law's home turf - closed,
/// because an open sheet has a phantom back side a ring can lawfully
/// rest on, and a height-field measure cannot tell that from material.
fn tilted(alpha: f64, half: f64) -> Vec<Triangle> {
    let t = alpha.tan();
    let surf = |_x: f64, y: f64| 10.0 - y * t;
    let q = |p: [(f64, f64, f64); 4]| {
        vec![
            Triangle::new(p3(p[0].0, p[0].1, p[0].2), p3(p[1].0, p[1].1, p[1].2), p3(p[2].0, p[2].1, p[2].2)),
            Triangle::new(p3(p[0].0, p[0].1, p[0].2), p3(p[2].0, p[2].1, p[2].2), p3(p[3].0, p[3].1, p[3].2)),
        ]
    };
    let h = half;
    let zb = 4.0;
    let mut facets = Vec::new();
    facets.extend(q([
        (-h, -h, surf(-h, -h)),
        (h, -h, surf(h, -h)),
        (h, h, surf(h, h)),
        (-h, h, surf(-h, h)),
    ])); // tilted top
    facets.extend(q([(-h, -h, zb), (h, -h, zb), (h, -h, surf(h, -h)), (-h, -h, surf(-h, -h))])); // walls
    facets.extend(q([(-h, h, zb), (h, h, zb), (h, h, surf(h, h)), (-h, h, surf(-h, h))]));
    facets.extend(q([(-h, -h, zb), (-h, h, zb), (-h, h, surf(-h, h)), (-h, -h, surf(-h, -h))]));
    facets.extend(q([(h, -h, zb), (h, h, zb), (h, h, surf(h, h)), (h, -h, surf(h, -h))]));
    facets.extend(q([(-h, -h, zb), (h, -h, zb), (h, h, zb), (-h, h, zb)])); // base
    facets
}

/// A 24 x 24 block, z 0..10 (no bottom), with a 12 x 12 x 5 blind
/// pocket centered on its top: floor z = 5, steep walls everywhere
/// else. The floor-outranks law's fixture: pocket columns answer to
/// their own floor, not to the surrounding shelf.
fn step_pocket() -> Vec<Triangle> {
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let (t, f) = (10.0, 5.0); // top, floor
    let mut facets = Vec::new();
    // Top annulus, four strips, interior (-6..6)^2 left as the mouth.
    facets.extend(quad([p3(-12.0, -12.0, t), p3(12.0, -12.0, t), p3(12.0, -6.0, t), p3(-12.0, -6.0, t)]));
    facets.extend(quad([p3(-12.0, 6.0, t), p3(12.0, 6.0, t), p3(12.0, 12.0, t), p3(-12.0, 12.0, t)]));
    facets.extend(quad([p3(-12.0, -6.0, t), p3(-6.0, -6.0, t), p3(-6.0, 6.0, t), p3(-12.0, 6.0, t)]));
    facets.extend(quad([p3(6.0, -6.0, t), p3(12.0, -6.0, t), p3(12.0, 6.0, t), p3(6.0, 6.0, t)]));
    // Pocket walls, z 5..10.
    facets.extend(quad([p3(6.0, -6.0, f), p3(-6.0, -6.0, f), p3(-6.0, -6.0, t), p3(6.0, -6.0, t)]));
    facets.extend(quad([p3(6.0, 6.0, f), p3(6.0, -6.0, f), p3(6.0, -6.0, t), p3(6.0, 6.0, t)]));
    facets.extend(quad([p3(-6.0, 6.0, f), p3(6.0, 6.0, f), p3(6.0, 6.0, t), p3(-6.0, 6.0, t)]));
    facets.extend(quad([p3(-6.0, -6.0, f), p3(-6.0, 6.0, f), p3(-6.0, 6.0, t), p3(-6.0, -6.0, t)]));
    // Pocket floor.
    facets.extend(quad([p3(-6.0, -6.0, f), p3(6.0, -6.0, f), p3(6.0, 6.0, f), p3(-6.0, 6.0, f)]));
    // Outer walls, z 0..10.
    facets.extend(quad([p3(-12.0, -12.0, 0.0), p3(12.0, -12.0, 0.0), p3(12.0, -12.0, t), p3(-12.0, -12.0, t)]));
    facets.extend(quad([p3(12.0, -12.0, 0.0), p3(12.0, 12.0, 0.0), p3(12.0, 12.0, t), p3(12.0, -12.0, t)]));
    facets.extend(quad([p3(12.0, 12.0, 0.0), p3(-12.0, 12.0, 0.0), p3(-12.0, 12.0, t), p3(12.0, 12.0, t)]));
    facets.extend(quad([p3(-12.0, 12.0, 0.0), p3(-12.0, -12.0, 0.0), p3(-12.0, -12.0, t), p3(-12.0, 12.0, t)]));
    facets
}

/// A 30 x 30 shelf at z = 10 hiding a 12 x 12 floor at z = 0 wholly
/// beneath it: no path can reach that floor from above, and the
/// finisher must say so instead of pretending.
fn hidden_floor() -> Vec<Triangle> {
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let mut facets = Vec::new();
    facets.extend(quad([
        p3(-15.0, -15.0, 10.0),
        p3(15.0, -15.0, 10.0),
        p3(15.0, 15.0, 10.0),
        p3(-15.0, 15.0, 10.0),
    ]));
    facets.extend(quad([
        p3(-6.0, -6.0, 0.0),
        p3(6.0, -6.0, 0.0),
        p3(6.0, 6.0, 0.0),
        p3(-6.0, 6.0, 0.0),
    ]));
    facets
}

/// The discontinuity law, made visible: no LATERAL Feed move may
/// change Z faster than a 45-degree climb plus the tolerance - a
/// straddled surface is a broken line, never one move. (Vertical moves,
/// plunges and retracts, carry no XY, so this law says nothing of
/// them; the envelope is their judge.)
fn assert_no_feed_spans_a_step(t: &Toolpath) {
    for m in &t.moves {
        if m.kind != MoveKind::Feed {
            continue;
        }
        let run = ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2)).sqrt();
        if run < 1e-9 {
            continue; // a pure retract or in-place hop: vertical, not this law's subject
        }
        let rise = (m.to.z - m.from.z).abs();
        assert!(
            rise <= run + TOL + 1e-9,
            "Feed move {:?}->{:?} spans a step: rise {:.4} over run {:.4}",
            m.from,
            m.to,
            rise,
            run
        );
    }
}

/// The y-values where the path plunges into a line that then runs a
/// substantial straight cut: the raster's line register.
fn line_starts(t: &Toolpath) -> Vec<f64> {
    let mut ys = Vec::new();
    let m = &t.moves;
    for i in 0..m.len() {
        if m[i].kind != MoveKind::Plunge {
            continue;
        }
        let straight = m[i + 1..]
            .iter()
            .take_while(|n| n.kind == MoveKind::Feed)
            .any(|n| n.arc.is_none() && (n.to.x - n.from.x).abs() > 0.3);
        if straight {
            ys.push(m[i].to.y);
        }
    }
    ys.sort_by(|a, b| a.total_cmp(b));
    ys.dedup();
    ys
}

#[test]
fn scallop_stepover_law_is_exact_and_total() {
    assert!((scallop_stepover(3.0, 0.1) - 2.4f64.sqrt()).abs() <= 1e-12);
    for (r, tol) in [(0.0, 0.1), (3.0, 0.0), (-1.0, 0.1), (3.0, -0.1)] {
        assert_eq!(scallop_stepover(r, tol), 0.0);
    }
    assert_eq!(scallop_stepover(f64::NAN, 0.1), 0.0);
    assert_eq!(scallop_stepover(3.0, f64::INFINITY), 0.0);
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn plateau_parallel_cuts_the_shelf_and_the_rings_take_the_walls() {
    // The auto-split's headline: one path, two strategies, and the
    // shelf - waterline's known blind spot - comes off clean.
    let f = plateau();
    let t = parallel_all(&f, &ppattern(10.0, 0.0, FinishPattern::Parallel)).expect("plateau parallel");
    // The shelf really was ridden: a lateral Feed sits on z = 10 well
    // inside the footprint.
    let on_shelf = t.moves.iter().any(|m| {
        m.kind == MoveKind::Feed
            && (m.to.z - 10.0).abs() < 1e-6
            && m.to.x.abs() < 6.0
            && m.to.y.abs() < 6.0
    });
    assert!(on_shelf, "the raster never rode the shelf");
    assert_no_feed_spans_a_step(&t);
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "gouged {} mm at {:?}", rep.gouge_mm, rep.at);
    assert!(rep.rest_mm <= gate(rep.cell), "left {} mm of rest at {:?}", rep.rest_mm, rep.at);
    assert_eq!(verify_finish(&f, &t, 10.0), Vec::new(), "the verifier agrees");
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn tilted_shelf_lines_are_the_stepover_times_the_cosine() {
    // The cos-law, measured: the raster's line spacing on a 30-degree
    // slope is sqrt(8 r tol) * cos(alpha) exactly, and the cusp the
    // wider-spaced lines would leave stays inside the budget.
    let alpha = PI / 6.0;
    let want = scallop_stepover(R, TOL) * alpha.cos();
    let f = tilted(alpha, 8.0);
    let t = parallel_all(&f, &ppattern(14.0, 4.0, FinishPattern::Parallel)).expect("tilted parallel");
    // The ring ladder may contribute its own contour lines at the
    // stepdown spacing; the raster's progression must be findable in
    // the line register regardless - a chain of at least three lines
    // at exactly the law's spacing.
    let ys = line_starts(&t);
    let longest_chain = |want: f64| -> usize {
        let mut best = 0;
        for &y0 in &ys {
            let mut k = 0;
            while k < 64
                && ys.iter().any(|y| (y - (y0 + (k + 1) as f64 * want)).abs() < 1e-6)
            {
                k += 1;
            }
            best = best.max(k);
        }
        best
    };
    assert!(
        longest_chain(want) >= 2,
        "no cos-law progression in the line register: {ys:?}, want {want}"
    );
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "gouged {} mm at {:?}", rep.gouge_mm, rep.at);
    assert!(
        rep.rest_mm <= gate(rep.cell),
        "the tilted cusp escaped the budget: {} mm at {:?}",
        rep.rest_mm,
        rep.at
    );
    assert_no_feed_spans_a_step(&t);
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn blind_pocket_floor_is_cut_and_nothing_sinks_below_it() {
    // Two laws at once: the topmost-support law gives the floor
    // (the pocket's columns answer to their own floor at 5, not to
    // the surrounding shelf at 10), and the floor-outranks law keeps
    // every cutting tip at or above bottom_z.
    let f = step_pocket();
    let t = parallel_all(&f, &ppattern(10.0, 5.0, FinishPattern::Parallel)).expect("pocket parallel");
    let on_floor = t.moves.iter().any(|m| {
        m.kind == MoveKind::Feed
            && (m.to.z - 5.0).abs() < 1e-6
            && m.to.x.abs() < 5.0
            && m.to.y.abs() < 5.0
    });
    assert!(on_floor, "the pocket floor was never ridden");
    let lowest = t
        .moves
        .iter()
        .filter(|m| matches!(m.kind, MoveKind::Feed | MoveKind::Plunge))
        .map(|m| m.to.z)
        .fold(f64::INFINITY, f64::min);
    assert!(lowest >= 5.0 - 1e-9, "the path sank below the floor: {lowest}");
    assert_no_feed_spans_a_step(&t);
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "gouged {} mm at {:?}", rep.gouge_mm, rep.at);
    assert!(rep.rest_mm <= gate(rep.cell), "left {} mm of rest at {:?}", rep.rest_mm, rep.at);
    assert_eq!(verify_finish(&f, &t, 10.0), Vec::new(), "the verifier agrees");
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn multidirection_rasters_cross() {
    // The second pass runs perpendicular: substantial straight feeds
    // along BOTH axes, where the single raster runs only along x.
    let f = dome();
    let one = parallel(&f, &ppattern(18.0, RIM_Z, FinishPattern::Parallel)).expect("parallel dome");
    let two = parallel(&f, &ppattern(18.0, RIM_Z, FinishPattern::ParallelMulti)).expect("multi dome");
    let dirs = |t: &Toolpath| -> (usize, usize) {
        let (mut x, mut y) = (0, 0);
        for m in &t.moves {
            if m.kind != MoveKind::Feed || m.arc.is_some() {
                continue;
            }
            let (dx, dy) = ((m.to.x - m.from.x).abs(), (m.to.y - m.from.y).abs());
            if dy > 1e-9 && dy > 2.0 * dx {
                y += 1;
            } else if dx > 1e-9 && dx > 2.0 * dy {
                x += 1;
            }
        }
        (x, y)
    };
    let (x1, y1) = dirs(&one);
    let (x2, y2) = dirs(&two);
    assert!(x1 > 50 && y1 <= x1 / 10, "single raster not unidirectional: {x1} x, {y1} y");
    assert!(x2 > 50 && y2 > 50, "multi raster not crossing: {x2} x, {y2} y");
    for t in [&one, &two] {
        assert_no_feed_spans_a_step(t);
        let rep = envelope(&f, t);
        assert!(rep.gouge_mm <= gate(rep.cell), "gouged {} mm at {:?}", rep.gouge_mm, rep.at);
        assert!(rep.rest_mm <= gate(rep.cell), "left {} mm of rest at {:?}", rep.rest_mm, rep.at);
        scallop_ratchet("dome multidirection", &rep);
    }
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn hidden_floor_stays_rest_and_the_path_stays_above_it() {
    // The visible ceiling's jurisdiction, stated as far as today's
    // measurement can honestly go: the finisher machines the surface
    // it can reach from above and never gouges its way elsewhere.
    // The floor under the shelf is unreachable; the envelope kernel's
    // part model is a top-down height field, so that floor's rest
    // stands OUTSIDE what the kernel can see (a column reports its
    // highest facet). Proving the unreachable material is left as
    // rest - visible, not merely uncut - is M2.2d's job; what this
    // test pins is the safety half: nothing gouges, and the path
    // never plunges below its own bottom plane to chase what it
    // cannot see.
    let f = hidden_floor();
    let t = parallel_all(&f, &ppattern(10.0, 10.0, FinishPattern::Parallel)).expect("overhang parallel");
    let on_shelf = t
        .moves
        .iter()
        .any(|m| m.kind == MoveKind::Feed && (m.to.z - 10.0).abs() < 1e-6);
    assert!(on_shelf, "the visible ceiling went uncut");
    let lowest = t
        .moves
        .iter()
        .filter(|m| matches!(m.kind, MoveKind::Feed | MoveKind::Plunge))
        .map(|m| m.to.z)
        .fold(f64::INFINITY, f64::min);
    assert!(lowest >= 10.0 - 1e-6, "the path chased the hidden floor: {lowest}");
    assert_no_feed_spans_a_step(&t);
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "gouged {} mm at {:?}", rep.gouge_mm, rep.at);
    assert_eq!(verify_finish(&f, &t, 10.0), Vec::new(), "the verifier agrees");
}

#[test]
fn parallel_refusals_are_clean() {
    // Degenerate inputs error, never crash or hang.
    let p = ppattern(10.0, 0.0, FinishPattern::Parallel);
    assert!(parallel(&[], &p).is_err(), "an empty model must not pass");
    let bad_tol = Finish3DParams { tolerance: 0.0, ..p };
    assert!(parallel(&plateau(), &bad_tol).is_err(), "zero tolerance is nonsense");
    let bad_step = Finish3DParams { stepover: Some(0.0), ..p };
    assert!(parallel(&plateau(), &bad_step).is_err(), "a zero stepover is nonsense");
    let nan_z = Finish3DParams { top_z: f64::NAN, ..p };
    assert!(parallel(&plateau(), &nan_z).is_err(), "a NaN height is nonsense");
    // All-steep: no shallow facet at all - the rings alone carry the
    // wall, and the path generates.
    let wall = vec![
        Triangle::new(p3(-5.0, 0.0, 0.0), p3(5.0, 0.0, 5.0), p3(5.0, 0.0, 0.0)),
        Triangle::new(p3(-5.0, 0.0, 0.0), p3(-5.0, 0.0, 5.0), p3(5.0, 0.0, 5.0)),
    ];
    assert!(parallel(&wall, &ppattern(5.0, 0.0, FinishPattern::Parallel)).is_ok());
    // A scatter of degenerate facets, marched: nothing unwraps.
    let scatter: Vec<Triangle> = (-8..8)
        .flat_map(|i| (-8..8).map(move |j| (i, j)))
        .map(|(i, j)| {
            let (x, y) = (i as f64, j as f64);
            Triangle::new(p3(x, y, 1.0), p3(x, y, 1.0), p3(x, y, 1.0))
        })
        .collect();
    let _ = parallel(&scatter, &ppattern(1.0, 0.0, FinishPattern::ParallelMulti));
}

// ---------------------------------------------------------------------------
// M2.2c - scallop: the same raster family crossed along the diagonals.
// The constant-cusp promise is MEASURED, per the kernel's law: the
// crossed pair buys directional coverage (no linear terrain feature is
// parallel to both families), not an analytic discount on the flat.
// ---------------------------------------------------------------------------

/// Straight (non-arc) feeds bucketed by XY direction: runs along the
/// diagonals, either sense, and along the axes.
fn dir_buckets(t: &Toolpath) -> (usize, usize, usize) {
    let (mut d1, mut d2, mut ax) = (0, 0, 0);
    for m in &t.moves {
        if m.kind != MoveKind::Feed || m.arc.is_some() {
            continue;
        }
        let (dx, dy) = ((m.to.x - m.from.x).abs(), (m.to.y - m.from.y).abs());
        if dx < 1e-9 || dy < 1e-9 {
            continue; // vertical or degenerate: says nothing of direction
        }
        if (dx - dy).abs() <= 0.2 * (dx + dy) {
            if (m.to.x - m.from.x) * (m.to.y - m.from.y) > 0.0 {
                d1 += 1;
            } else {
                d2 += 1;
            }
        } else {
            ax += 1;
        }
    }
    (d1, d2, ax)
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn scallop_crosses_the_diagonals_and_measures_clean() {
    // The headline: both diagonal senses run real feeds, and the
    // envelope kernel signs off on the whole path - gouge and cusp
    // both inside the gate, measured on the grid.
    let f = dome();
    let t = parallel_all(&f, &ppattern(18.0, RIM_Z, FinishPattern::Scallop)).expect("scallop dome");
    let (d1, d2, ax) = dir_buckets(&t);
    assert!(d1 > 50 && d2 > 50, "scallop not crossing the diagonals: {d1} / {d2}");
    // The dome's steep flanks belong to the ring ladder (the M2.2b
    // jurisdiction law), and a circle's chords run near-axis at the
    // cardinal points - so a few axis-parallel feeds are the LADDER's
    // signature, not a family's. The law that IS pattern-shape: the
    // crossed pair carries the bulk of the cut on the shallow ground.
    assert!(ax * 2 < d1 + d2, "axis feeds {ax} outweigh the cross {}", d1 + d2);
    assert_no_feed_spans_a_step(&t);
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "gouged {} mm at {:?}", rep.gouge_mm, rep.at);
    assert!(rep.rest_mm <= gate(rep.cell), "left {} mm of rest at {:?}", rep.rest_mm, rep.at);
    scallop_ratchet("dome scallop", &rep);
    assert_eq!(sans_scallop(verify_finish(&f, &t, 18.0)), Vec::new(), "the verifier agrees");
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn scallop_plateau_is_constant_cusp_on_the_shelf() {
    // The prismatic case the diagonals exist for: a flat shelf ringed
    // by vertical walls. The crossed pair must hold the cusp
    // everywhere and never drag a feed across a wall.
    let f = plateau();
    let t = parallel_all(&f, &ppattern(10.0, 0.0, FinishPattern::Scallop)).expect("plateau scallop");
    let (d1, d2, _) = dir_buckets(&t);
    assert!(d1 > 10 && d2 > 10, "shelf not rastered diagonally: {d1} / {d2}");
    assert_no_feed_spans_a_step(&t);
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "gouged {} mm at {:?}", rep.gouge_mm, rep.at);
    assert!(rep.rest_mm <= gate(rep.cell), "left {} mm of rest at {:?}", rep.rest_mm, rep.at);
    assert_eq!(verify_finish(&f, &t, 10.0), Vec::new(), "the verifier agrees");
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn scallop_holds_the_cos_law_on_the_tilted_shelf() {
    // Same terrain the parallel cos-law test measures, crossed: the
    // lines run ±45 to the fall, so each family meets the slope at
    // 45 and the global min(m_z) law still under-promises (spends)
    // stepover, never over-spends it. Clean envelope is the promise.
    // half = 8, exactly the cos-law fixture: the plane's full extent
    // stays inside the op's [4, 14] band - a shelf tilted 30 deg out
    // of a 60 mm block falls below its own base and far above the
    // op's top, and a kernel verdict on a self-intersecting model
    // measures the fixture bug, not the pattern law.
    let f = tilted(PI / 6.0, 8.0);
    let t = parallel_all(&f, &ppattern(14.0, 4.0, FinishPattern::Scallop)).expect("tilted scallop");
    // Both diagonal senses ride the shelf. The third bucket (axis-
    // parallel feeds) is the wall ladder's - vertical walls ARE the
    // steep terrain the M2.2b jurisdiction law hands to the rings -
    // so no closed fixture is pure-cross and purity is not the claim.
    let (td1, td2, _) = dir_buckets(&t);
    assert!(td1 > 0 && td2 > 0, "tilted cross lost a sense: {td1} / {td2}");
    assert_no_feed_spans_a_step(&t);
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "gouged {} mm at {:?}", rep.gouge_mm, rep.at);
    assert!(rep.rest_mm <= gate(rep.cell), "left {} mm of rest at {:?}", rep.rest_mm, rep.at);
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn scallop_pocket_floor_gets_its_own_cross() {
    // The step_pocket: floor and shelf both shallow, walls steep.
    // Both regions must be ridden by diagonal lines and the ring
    // ladder, with the floor-outranks law keeping each region's
    // lines on its own ground.
    let f = step_pocket();
    let t = parallel_all(&f, &ppattern(10.0, 5.0, FinishPattern::Scallop)).expect("pocket scallop");
    let on_floor = t
        .moves
        .iter()
        .any(|m| m.kind == MoveKind::Feed && (m.to.z - 5.0).abs() < 1e-6);
    let on_shelf = t
        .moves
        .iter()
        .any(|m| m.kind == MoveKind::Feed && (m.to.z - 10.0).abs() < 1e-6);
    assert!(on_floor && on_shelf, "a shallow region went uncrossed");
    assert_no_feed_spans_a_step(&t);
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "gouged {} mm at {:?}", rep.gouge_mm, rep.at);
    assert!(rep.rest_mm <= gate(rep.cell), "left {} mm of rest at {:?}", rep.rest_mm, rep.at);
}

#[test]
fn scallop_shares_the_raster_refusals() {
    // Same generator, so the refusal set is the parallel's - and the
    // all-steep wall still generates on rings alone.
    let p = ppattern(10.0, 0.0, FinishPattern::Scallop);
    assert!(parallel(&[], &p).is_err(), "an empty model must not pass");
    assert!(
        parallel(&plateau(), &Finish3DParams { stepover: Some(0.0), ..p }).is_err(),
        "a zero stepover is nonsense"
    );
    let wall = vec![
        Triangle::new(p3(-5.0, 0.0, 0.0), p3(5.0, 0.0, 5.0), p3(5.0, 0.0, 0.0)),
        Triangle::new(p3(-5.0, 0.0, 0.0), p3(-5.0, 0.0, 5.0), p3(5.0, 0.0, 5.0)),
    ];
    assert!(parallel(&wall, &ppattern(5.0, 0.0, FinishPattern::Scallop)).is_ok());
}


// ---- M2.2d stage B: the polar placements ride the same march laws ----

/// A small hill - the dome's kin at a fraction of the facet count, so
/// the envelope kernel can judge the polar patterns in milliseconds
/// where the big dome costs minutes (the design's fast-fixture law).
fn hill() -> Vec<Triangle> {
    sphere_cap(8.0, PI / 4.0, 5, 24)
}
const HILL_TOP: f64 = 8.0;
const HILL_RIM: f64 = 8.0 * std::f64::consts::FRAC_1_SQRT_2;

#[test]
fn radial_chords_and_the_spiral_cover_the_center() {
    // Structural promise of the two placements: every ground line
    // passes THROUGH the footprint center (chords) or winds in to a
    // half-step of it (spiral) - so the center of a hill is cut, the
    // way the raster's middle line cuts it. And the march's chord
    // budget, told apart from the rings' honest long strokes the way
    // the two motions themselves differ: a ring rides one tip plane
    // (iso-z, exact-straight collapse and all), a march sample
    // climbs - so every Feed that changes Z is a march move, and
    // none of them may span more XY than the stepdown budget.
    let f = hill();
    let delta0 = scallop_stepdown(R, TOL);
    for (pat, name) in [(FinishPattern::Radial, "radial"), (FinishPattern::Spiral, "spiral")] {
        let t = parallel_all(&f, &ppattern(HILL_TOP, HILL_RIM, pat)).expect(name);
        assert!(!t.moves.is_empty());
        assert_eq!(
            t.moves,
            parallel_all(&f, &ppattern(HILL_TOP, HILL_RIM, pat)).expect(name).moves,
            "{name}: two calls, two paths - a stamp would churn forever"
        );
        let near_center = t.moves.iter().filter(|m| m.kind == MoveKind::Feed).any(|m| {
            m.to.x.hypot(m.to.y) <= 1.5 * delta0
                && m.from.x.hypot(m.from.y) <= 3.0 * (delta0 + scallop_stepover(R, TOL))
        });
        assert!(near_center, "{name} never crossed the middle");
        let worst = t
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed && (m.to.z - m.from.z).abs() > 1e-9)
            .map(|m| (m.to.x - m.from.x).hypot(m.to.y - m.from.y))
            .fold(0.0f64, f64::max);
        assert!(
            worst <= 1.5 * delta0,
            "{name}: a climbing Feed spans {worst} mm of XY, over the chord budget {delta0}"
        );
    }
}

/// The operation-layer dispatch is the only route to radial/spiral
/// from the enum; the tests call the generators through the same
/// match shape (kept honest by refusing anything not shallow-ground).
fn parallel_all(f: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    match p.pattern {
        FinishPattern::Radial => radial(f, p),
        FinishPattern::Spiral => spiral(f, p),
        FinishPattern::Contour => contour(f, p),
        FinishPattern::Project => project(f, p),
        _ => parallel(f, p),
    }
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn the_hill_under_radial_and_spiral_is_envelope_clean() {
    // The measured promise, both families: the same cos-law spacing
    // that keeps the raster's cusp inside `tol` on a tilt `a` keeps
    // the chords' and the spiral's - the march laws are shared, so
    // the verdict must be too.
    let f = hill();
    for (pat, name) in [(FinishPattern::Radial, "radial"), (FinishPattern::Spiral, "spiral")] {
        let t = parallel_all(&f, &ppattern(HILL_TOP, HILL_RIM, pat)).expect(name);
        let rep = envelope(&f, &t);
        assert!(rep.gouge_mm <= gate(rep.cell), "{name}: gouged {} at {:?}", rep.gouge_mm, rep.at);
        assert!(rep.rest_mm <= gate(rep.cell), "{name}: left {} of rest at {:?}", rep.rest_mm, rep.at);
        scallop_ratchet(&format!("hill {name}"), &rep);
        assert_eq!(sans_scallop(verify_finish(&f, &t, HILL_TOP)), Vec::new(), "{name}: the verifier agrees");
    }
}

#[test]
fn polar_placements_refuse_without_a_number_or_past_the_cap() {
    // The raster's step_positive law, spoken in polar words, and the
    // line-count caps: refusals, never silent skips, never hours.
    let f = box_facets();
    for name in ["radial", "spiral"] {
        for (pat, bad_step) in [(FinishPattern::Radial, 0.0), (FinishPattern::Radial, f64::NAN), (FinishPattern::Spiral, 0.0), (FinishPattern::Spiral, f64::NAN)] {
            let pp = Finish3DParams { stepover: Some(bad_step), ..ppattern(10.0, 0.5, pat) };
            assert!(matches!(parallel_all(&f, &pp), Err(GenError::Invalid(_))), "{name} stepover {bad_step} must refuse");
        }
        let wide = Finish3DParams { stepover: Some(0.001), ..ppattern(10.0, 0.5, if name == "radial" { FinishPattern::Radial } else { FinishPattern::Spiral }) };
        assert!(matches!(parallel_all(&f, &wide), Err(GenError::Invalid(_))), "{name} at 0.001 spacing must hit the cap");
    }
}

// ---------- M2.2e: the around-the-model families ----------

#[test]
fn contour_closes_its_rings_and_project_walks_the_rim() {
    // The two laws the shadow families add over the polar ones: a
    // contour ring on unbroken ground comes back CLOSED (the clamp
    // sample at arc length L lands on the entry point - the march's
    // own duplicate law keeps it as the closing move), and project's
    // silhouette keeps the model's XY exactly (a flat top's shadow
    // shift is r·m_xy = 0 - the rim is the footprint boundary, lifted).
    let f = plateau();
    let t = contour(&f, &ppattern(10.0, 0.0, FinishPattern::Contour)).expect("contour plateau");
    assert_eq!(
        t.moves,
        contour(&f, &ppattern(10.0, 0.0, FinishPattern::Contour))
            .expect("contour again")
            .moves,
        "two calls, two paths - a stamp would churn forever"
    );
    // A plunge-run whose feeds return to the entry point: a closed
    // ring on the plateau's unbroken ground.
    let mut closed = false;
    for (i, m) in t.moves.iter().enumerate() {
        if m.kind != MoveKind::Plunge {
            continue;
        }
        let (ex, ey) = (m.to.x, m.to.y);
        closed |= t.moves[i + 1..]
            .iter()
            .take_while(|n| n.kind == MoveKind::Feed)
            .any(|n| (n.to.x - ex).abs() < 1e-9 && (n.to.y - ey).abs() < 1e-9);
    }
    assert!(closed, "a plateau contour never returned to its entry - the ring did not close");
    let tp = project(&f, &ppattern(10.0, 0.0, FinishPattern::Project)).expect("project plateau");
    let on_rim = tp
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed)
        .any(|m| (m.to.x.abs() - 10.0).abs() < 1e-9 && (m.to.z - 10.0).abs() < 1e-9);
    assert!(on_rim, "project never walked the model's own rim");
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn the_hill_under_contour_and_project_is_envelope_clean() {
    // The measured promise, both families: the shadow's spacing is
    // the same cos-law stepover the raster pays, and the lift is the
    // same tangent law the march rides - so the kernel's verdict on
    // the hill must be the same clean one.
    let f = hill();
    for (pat, name) in [
        (FinishPattern::Contour, "contour"),
        (FinishPattern::Project, "project"),
    ] {
        let t = parallel_all(&f, &ppattern(HILL_TOP, HILL_RIM, pat)).expect(name);
        let rep = envelope(&f, &t);
        assert!(rep.gouge_mm <= gate(rep.cell), "{name}: gouged {} at {:?}", rep.gouge_mm, rep.at);
        assert!(rep.rest_mm <= gate(rep.cell), "{name}: left {} of rest at {:?}", rep.rest_mm, rep.at);
        scallop_ratchet(&format!("hill {name}"), &rep);
        assert_eq!(sans_scallop(verify_finish(&f, &t, HILL_TOP)), Vec::new(), "{name}: the verifier agrees");
    }
}

#[test]
fn contour_answers_for_its_spacing_and_project_pays_only_the_gate() {
    // Two doors, and the families walk them differently. The EARLY
    // gate (`validate`) refuses a spacing without a number from
    // EVERY pattern, Waterline's "ignores the raster fields" law
    // included - a nonsense parameter is nonsense unused. The
    // spacing's COST (the ring-cap guard) only speaks where spacing
    // marches: contour at a hairline stepover is hours of rings and
    // must refuse; project's single rim pays nothing for it.
    let f = plateau();
    for bad in [0.0, f64::NAN] {
        for pat in [FinishPattern::Contour, FinishPattern::Project] {
            let p = Finish3DParams { stepover: Some(bad), ..ppattern(10.0, 0.0, pat) };
            assert!(
                matches!(parallel_all(&f, &p), Err(GenError::Invalid(_))),
                "{pat:?} stepover {bad} must refuse at the gate"
            );
        }
    }
    let hair = Finish3DParams { stepover: Some(0.001), ..ppattern(10.0, 0.0, FinishPattern::Contour) };
    assert!(
        matches!(contour(&f, &hair), Err(GenError::Invalid(_))),
        "contour at 0.001 spacing must hit the ring cap, not hours"
    );
    let hair_p = Finish3DParams { stepover: Some(0.001), ..ppattern(10.0, 0.0, FinishPattern::Project) };
    assert!(project(&f, &hair_p).is_ok(), "project's one rim costs no spacing budget");
}

#[test]
fn all_steep_ground_leaves_the_shadow_families_to_the_rings() {
    // The raster's all-steep answer, in shadow form: an empty center-
    // shadow is not an error - the rings carry the wall, and both
    // families generate.
    let wall = vec![
        Triangle::new(p3(-5.0, 0.0, 0.0), p3(5.0, 0.0, 5.0), p3(5.0, 0.0, 0.0)),
        Triangle::new(p3(-5.0, 0.0, 0.0), p3(-5.0, 0.0, 5.0), p3(5.0, 0.0, 5.0)),
    ];
    assert!(contour(&wall, &ppattern(5.0, 0.0, FinishPattern::Contour)).is_ok());
    assert!(project(&wall, &ppattern(5.0, 0.0, FinishPattern::Project)).is_ok());
    // Degenerate scatter: zero-area facets cast no shadow, and the
    // families must neither panic nor hang.
    let scatter: Vec<Triangle> = (-8..8)
        .flat_map(|i| (-8..8).map(move |j| (i, j)))
        .map(|(i, j)| {
            let (x, y) = (i as f64, j as f64);
            Triangle::new(p3(x, y, 1.0), p3(x, y, 1.0), p3(x, y, 1.0))
        })
        .collect();
    let _ = contour(&scatter, &ppattern(1.0, 0.0, FinishPattern::Contour));
    let _ = project(&scatter, &ppattern(1.0, 0.0, FinishPattern::Project));
}

// ---- M2.2e stage D: ramp entries, the diagonal chain ----

/// The hop's fingerprint. A ramp may only ever take counts DOWN -
/// rapids and plunges it leaves are the refusals it earned.
fn n_kind(t: &Toolpath, k: MoveKind) -> usize {
    t.moves.iter().filter(|m| m.kind == k).count()
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn ramp_chains_the_plateau_raster() {
    // Flat ground is the chain's best case: every line-to-line
    // diagonal rides above its own support, so the hop's
    // up-rapid-plunge collapses into one Feed at cutting height. The
    // first contact stays a plunge - a ramp chains BETWEEN cuts,
    // never INTO the first one.
    let f = plateau();
    let hop = Finish3DParams { ramp: false, ..ppattern(10.0, 0.0, FinishPattern::Parallel) };
    let rng = Finish3DParams { ramp: true, ..ppattern(10.0, 0.0, FinishPattern::Parallel) };
    let h = parallel(&f, &hop).expect("hop");
    let r = parallel(&f, &rng).expect("ramp");
    assert_eq!(
        r.moves,
        parallel(&f, &rng).expect("ramp again").moves,
        "two calls, two paths - a stamp would churn forever"
    );
    let first_cut = r
        .moves
        .iter()
        .find(|m| m.kind != MoveKind::Rapid)
        .expect("a path with no cut at all");
    assert_eq!(
        first_cut.kind,
        MoveKind::Plunge,
        "the first contact arrived as a chained Feed - the law says plunge"
    );
    assert!(
        n_kind(&r, MoveKind::Rapid) < n_kind(&h, MoveKind::Rapid),
        "the chain replaced nothing on flat ground"
    );
    assert!(n_kind(&r, MoveKind::Plunge) < n_kind(&h, MoveKind::Plunge));
    // The chained link, structurally: the serpentine raster's
    // line-to-line connector is a lateral one-stepover move, and the
    // ramp feeds it at cutting height instead of lifting for it -
    // one constant-Z Feed with a zero X-delta, where the hop path
    // has its up-rapid-plunge triple.
    let step = scallop_stepover(R, TOL);
    let chained = r.moves.iter().any(|m| {
        m.kind == MoveKind::Feed
            && (m.to.z - m.from.z).abs() < 1e-12
            && (m.to.x - m.from.x).abs() < 1e-9
            && ((m.to.y - m.from.y).abs() - step).abs() <= 1e-6
    });
    assert!(chained, "no lateral chain Feed at cutting height in the ramp path");
    let lifted = h.moves.iter().any(|m| {
        m.kind == MoveKind::Rapid
            && (m.to.x - m.from.x).abs() < 1e-9
            && ((m.to.y - m.from.y).abs() - step).abs() <= 1e-6
            && m.to.z > 20.0
    });
    assert!(lifted, "the hop path never lifted over a stepover - wrong fixture");
    assert_no_feed_spans_a_step(&r);
    let rep = envelope(&f, &r);
    assert!(rep.gouge_mm <= gate(rep.cell), "ramp gouged {} at {:?}", rep.gouge_mm, rep.at);
    assert!(rep.rest_mm <= gate(rep.cell), "ramp left {} at {:?}", rep.rest_mm, rep.at);
    assert_eq!(verify_finish(&f, &r, 10.0), Vec::new(), "the verifier agrees");
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn ramp_never_gouges_the_ground_it_certs() {
    // The dome is where a straight chord sinks below the arc's
    // support - whether each hill diagonal lands or refuses is
    // `support_at`'s answer, not the design's guess. What IS the law,
    // measured: fewer hops at worst, no gouge at best, and the same
    // clean verdict from the kernel either way.
    let f = hill();
    let hop = Finish3DParams { ramp: false, ..ppattern(HILL_TOP, HILL_RIM, FinishPattern::Parallel) };
    let rng = Finish3DParams { ramp: true, ..ppattern(HILL_TOP, HILL_RIM, FinishPattern::Parallel) };
    let h = parallel_all(&f, &hop).expect("hop hill");
    let r = parallel_all(&f, &rng).expect("ramp hill");
    assert!(n_kind(&r, MoveKind::Rapid) <= n_kind(&h, MoveKind::Rapid), "a ramp ADDED a hop");
    let rep = envelope(&f, &r);
    assert!(
        rep.gouge_mm <= gate(rep.cell),
        "a certified diagonal gouged: {} at {:?}",
        rep.gouge_mm,
        rep.at
    );
    assert!(rep.rest_mm <= gate(rep.cell), "ramp hill left {} of rest", rep.rest_mm);
    scallop_ratchet("ramp hill", &rep);
    assert_eq!(sans_scallop(verify_finish(&f, &r, HILL_TOP)), Vec::new(), "the verifier agrees");
}

#[test]
#[ignore = "heavy lane: waterline generation over the 12x96 dome (structural expectations ride this fixture's counts); ~10-50s in debug, seconds in release; run at milestone close with -- --ignored"]
fn a_chain_too_long_to_certify_declines_itself() {
    // The probe cap in test form: an explicit spacing of 0.5 mm under
    // a tolerance of 1e-4 puts the serpentine's lateral connector at
    // 0.5 / (2.5e-5) = 20000 probes - past the cap - so certification
    // declines every one and the path comes back byte-identical to
    // the hop. A claim too wide to check is refused, not
    // approximated: the budget-stated-zero law. (The flat ground
    // keeps the dump small: the march's exact-straight collapse puts
    // few samples on each line.)
    let f = plateau();
    let mk = |ramp| Finish3DParams {
        tolerance: 1e-4,
        stepover: Some(0.5),
        ramp,
        ..ppattern(10.0, 0.0, FinishPattern::Parallel)
    };
    let h = parallel(&f, &mk(false)).expect("hop");
    let r = parallel(&f, &mk(true)).expect("ramp");
    assert_eq!(h.moves, r.moves, "an uncertifiable diagonal was attempted anyway");
}

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn the_ring_families_ignore_the_chain() {
    // Ramp lives in the runs emitter's between-lines branch alone;
    // rings are their own emitter and never route through it. On
    // all-steep ground a contour is rings only, so the flag must be
    // byte-inert there - and every ground-line family still
    // generates, deterministically and envelope-clean, with it on.
    let wall = vec![
        Triangle::new(p3(-5.0, 0.0, 0.0), p3(5.0, 0.0, 5.0), p3(5.0, 0.0, 0.0)),
        Triangle::new(p3(-5.0, 0.0, 0.0), p3(-5.0, 0.0, 5.0), p3(5.0, 0.0, 5.0)),
    ];
    let h = contour(
        &wall,
        &Finish3DParams { ramp: false, ..ppattern(5.0, 0.0, FinishPattern::Contour) },
    )
    .expect("hop wall");
    let r = contour(
        &wall,
        &Finish3DParams { ramp: true, ..ppattern(5.0, 0.0, FinishPattern::Contour) },
    )
    .expect("ramp wall");
    assert_eq!(h.moves, r.moves, "a ramp reached the rings");
    let f = hill();
    for pat in [
        FinishPattern::Radial,
        FinishPattern::Spiral,
        FinishPattern::Contour,
        FinishPattern::Project,
    ] {
        let p = Finish3DParams { ramp: true, ..ppattern(HILL_TOP, HILL_RIM, pat) };
        let t = parallel_all(&f, &p).expect("family");
        assert_eq!(
            t.moves,
            parallel_all(&f, &p).expect("again").moves,
            "{pat:?} with ramp: two calls, two paths"
        );
        let rep = envelope(&f, &t);
        assert!(
            rep.gouge_mm <= gate(rep.cell) && rep.rest_mm <= gate(rep.cell),
            "{pat:?} with ramp: gouge {} rest {}",
            rep.gouge_mm,
            rep.rest_mm
        );
        scallop_ratchet(&format!("hill {} with ramp", format!("{pat:?}").to_lowercase()), &rep);
    }
    // Degenerate scatter under ramp: no panic, no hang, no promise.
    let scatter: Vec<Triangle> = (-8..8)
        .flat_map(|i| (-8..8).map(move |j| (i, j)))
        .map(|(i, j)| {
            let (x, y) = (i as f64, j as f64);
            Triangle::new(p3(x, y, 1.0), p3(x, y, 1.0), p3(x, y, 1.0))
        })
        .collect();
    let _ = contour(&scatter, &Finish3DParams { ramp: true, ..ppattern(1.0, 0.0, FinishPattern::Contour) });
    let _ = project(&scatter, &Finish3DParams { ramp: true, ..ppattern(1.0, 0.0, FinishPattern::Project) });
}

// ---------------------------------------------------------------------------
// M2.2e stage E - the surface-finish parameters: minimum step and ramp angle.
// Tolerance and stepover were spoken long ago; these two close the bullet.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "verdict lane: envelope scan over the 12x96 dome - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn the_ramp_angle_default_certifies_what_v17_certified() {
    // Stage D's certified diagonal chain, same ground as its cos-law
    // test: the tilted wedge, where every serpentine connector hops
    // one stepover down a 1-in-2 slope - a 26.57-degree diagonal,
    // short enough to certify and inside the 45 the law already
    // granted in v17. (A gable was the wrong witness for this law:
    // its connectors go the long way around the fold gap, every one
    // refused at every angle, and the flag looked inert for reasons
    // that had nothing to do with the number under test.)
    let f = tilted((0.5f64).atan(), 8.0);
    let hop = parallel(
        &f,
        &Finish3DParams { ramp: false, ..ppattern(14.0, 4.0, FinishPattern::Parallel) },
    )
    .expect("hop");
    let ramp = parallel(
        &f,
        &Finish3DParams { ramp: true, ..ppattern(14.0, 4.0, FinishPattern::Parallel) },
    )
    .expect("ramp at the default angle");
    assert_ne!(hop.moves, ramp.moves, "the default angle refused a chain v17 certified");
    assert!(
        n_kind(&ramp, MoveKind::Rapid) < n_kind(&hop, MoveKind::Rapid),
        "the chained path kept the hop's rapids"
    );
    // And the chains it lays really do descend - the slope this
    // fixture is built to carry: dz = dy / 2, exactly.
    let sloped = ramp.moves.iter().any(|m| {
        let dy = (m.to.y - m.from.y).abs();
        m.kind == MoveKind::Feed
            && (m.to.x - m.from.x).abs() < 1e-9
            && dy > 1e-6
            && (m.to.z - m.from.z).abs() - dy / 2.0 <= 1e-6 * dy
    });
    assert!(sloped, "no descending lateral chain in the ramp path");
    let rep = envelope(&f, &ramp);
    assert!(
        rep.gouge_mm <= gate(rep.cell) && rep.rest_mm <= gate(rep.cell),
        "tilted ramp envelope {} {}",
        rep.gouge_mm,
        rep.rest_mm
    );
    scallop_ratchet("tilted ramp", &rep);
    assert_eq!(sans_scallop(verify_finish(&f, &ramp, 14.0)), Vec::new(), "the verifier agrees");
}

#[test]
fn a_shallower_angle_gives_the_chain_back_to_the_hop() {
    // The gate is honest at both ends: 45 chains what the ground
    // offers (1:2 < 1:1), 20 refuses all of it (1:2 > 1:tan 20 =
    // 1:2.75). A link either descends within the promise or the hop
    // stands, move for move, byte for byte. The list stops short of
    // the fixture's own boundary, atan(1/2) = 26.57 degrees - a
    // number there sits on the float-noise cliff, not on ground the
    // law claims to hold.
    let f = tilted((0.5f64).atan(), 8.0);
    let hop = parallel(
        &f,
        &Finish3DParams { ramp: false, ..ppattern(14.0, 4.0, FinishPattern::Parallel) },
    )
    .expect("hop");
    for angle in [1.0, 20.0] {
        let t = parallel(
            &f,
            &Finish3DParams {
                ramp: true,
                ramp_angle: angle,
                ..ppattern(14.0, 4.0, FinishPattern::Parallel)
            },
        )
        .expect("steep-angled ramp");
        assert_eq!(t.moves, hop.moves, "angle {angle}: the chain slipped past its cap");
    }
    // The angle is inert while the ramp is off: a plunge-coated
    // number changes nothing on a hopped path.
    let inert = parallel(
        &f,
        &Finish3DParams {
            ramp: false,
            ramp_angle: 1.0,
            ..ppattern(14.0, 4.0, FinishPattern::Parallel)
        },
    )
    .expect("inert angle");
    assert_eq!(inert.moves, hop.moves, "an angle moved a path with no ramp");
}


/// The corrugated belt: five closed-width facets at 22.5 degrees,
/// alternately rising and falling across the x-raster. The fold
/// peaks are validity gaps - a ball center within 3 sin(22.5) of a
/// crease has its contact on the far facet, so the march never
/// chords a crease; the folds bound the runs, not the refinement.
/// What does answer to a refinement floor here is the staircase
/// the march walks INTO each gap edge: the None path keeps halving
/// toward its own 1/4096 right, and every floor from 0.3 to the
/// cruise step truncates that descent at a different rung. The
/// flanks ride a closed solid - walls down to z = 9 and a floor -
/// because the verifier's silhouette needs one and rings need not
/// to march phantom air off the sheet's free edges.
fn folds() -> Vec<Triangle> {
    let (a, run, hw, zb, zf) = (0.6628f64, 3.2f64, 8.0, 10.0, 9.0); // 22.5-deg flanks
    let z = |k: i32| zb + a * if k % 2 == 0 { -1.0 } else { 1.0 };
    let x = |k: i32| -8.0 + run * k as f64;
    let q = |pp: [(f64, f64, f64); 4]| {
        vec![
            Triangle::new(p3(pp[0].0, pp[0].1, pp[0].2), p3(pp[1].0, pp[1].1, pp[1].2), p3(pp[2].0, pp[2].1, pp[2].2)),
            Triangle::new(p3(pp[0].0, pp[0].1, pp[0].2), p3(pp[2].0, pp[2].1, pp[2].2), p3(pp[3].0, pp[3].1, pp[3].2)),
        ]
    };
    let mut f = Vec::new();
    for k in 0..5 {
        let (x0, x1, z0, z1) = (x(k), x(k + 1), z(k), z(k + 1));
        // Top band, and the walls under its edges. Windings are
        // the ones the tilted wedge already proves honest:
        // outward-up on the band, -y and +y on the walls.
        f.extend(q([(x0, -hw, z0), (x1, -hw, z1), (x1, hw, z1), (x0, hw, z0)]));
        f.extend(q([(x0, -hw, zf), (x1, -hw, zf), (x1, -hw, z1), (x0, -hw, z0)]));
        f.extend(q([(x0, hw, z0), (x1, hw, z1), (x1, hw, zf), (x0, hw, zf)]));
    }
    // The x-end walls and the floor close the box.
    f.extend(q([(-8.0, -hw, z(0)), (-8.0, hw, z(0)), (-8.0, hw, zf), (-8.0, -hw, zf)]));
    f.extend(q([(8.0, hw, z(5)), (8.0, -hw, z(5)), (8.0, -hw, zf), (8.0, hw, zf)]));
    f.extend(q([(-8.0, hw, zf), (8.0, hw, zf), (8.0, -hw, zf), (-8.0, -hw, zf)]));
    f
}

#[test]
#[ignore = "verdict lane: envelope scans over the corrugated belt on four ladders - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn minimum_step_floors_the_refinement_not_the_edges() {
    // The corrugated belt: five 22.5-degree facets across the raster.
    // Its fold PEAKS are validity gaps - a ball center within
    // 3 sin(22.5) of a crease has its contact on the far facet - so
    // no chord ever crosses a crease (the creases bound the runs,
    // floored by nobody); what a floor truncates is the staircase
    // the march walks into each gap edge. The raster band is the
    // gap between the flanks' heights: every floor lives inside one
    // envelope-clean promise, and the parameter's whole effect is
    // in how the march spends its chords.
    let f = folds();
    let at = |ms: Option<f64>| {
        parallel(
            &f,
            &Finish3DParams { min_step: ms, ..ppattern(11.0, 8.0, FinishPattern::Parallel) },
        )
        .expect("belt raster")
    };
    let n = |ms: Option<f64>| n_kind(&at(ms), MoveKind::Feed);
    // Strictly coarser rung by rung...
    assert!(n(None) > n(Some(0.3)), "0.3 did not truncate the staircase");
    assert!(n(Some(0.3)) > n(Some(0.7)), "0.7 did not truncate the staircase");
    assert!(n(Some(0.7)) > n(Some(1.2)), "1.2 did not truncate the staircase");
    // ...until it reaches cruise: a floor at or past delta0 can
    // make the march skip its own test, but never march coarser
    // than one step per chord.
    assert_eq!(n(Some(1.2)), n(Some(4.0)), "a floor past cruise kept cutting");
    // Chords below 0.3: the None path chooses them by the score
    // (its staircase runs to 0.068); every floor takes some away,
    // and from a floor at the rung right below cruise (0.7) up,
    // the short chords left are exactly the piece tails and edge
    // bisections - material's own boundaries, which no spacing
    // parameter owns. A finer floor can even ADD sub-floor chords
    // by landing more grids near an edge (the closed solid's
    // counts: 136 none, 70 at 0.3, 48 at 0.7 and 1.2 alike);
    // that is the floor trading chord length for piece count,
    // not refining. The edges stand untouched at every rung.
    let below = |t: &Toolpath, q: f64| -> usize {
        t.moves
            .iter()
            .filter(|m| {
                m.kind == MoveKind::Feed
                    && (m.to.x - m.from.x).hypot(m.to.y - m.from.y) < q * (1.0 - 1e-9)
            })
            .count()
    };
    let (t_none, t3, t7, t12) = (at(None), at(Some(0.3)), at(Some(0.7)), at(Some(1.2)));
    assert!(below(&t_none, 0.3) > below(&t3, 0.3), "the floor left the whole descent in");
    assert!(below(&t3, 0.3) >= below(&t7, 0.3), "the 0.7 floor refined where 0.3 had stopped");
    assert_eq!(below(&t7, 0.3), below(&t12, 0.3), "0.7 and 1.2 disagreed about the edges");
    // The piece edges are the same millimeters on every path.
    // "Same millimeters" to the float tail, that is: bisection
    // brackets are seeded from the sample grid, so a floor can
    // perturb an edge in its last digits (2e-8 mm) while the
    // material boundary it converges on never moves. The
    // tolerance is the honest reading of the law - one hundred-
    // thousandth of a millimeter, four orders below anything the
    // machine could hear, and an assert_eq on raw floats here
    // would be testing IEEE, not CAM.
    let ends = |t: &Toolpath| -> Vec<(f64, f64, f64)> {
        t.moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge)
            .map(|m| (m.to.x, m.to.y, m.to.z))
            .collect()
    };
    let same_ends = |a: &[(f64, f64, f64)], b: &[(f64, f64, f64)]| -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(p, q)| (p.0 - q.0).abs() <= 1e-4 && (p.1 - q.1).abs() <= 1e-4 && (p.2 - q.2).abs() <= 1e-4)
    };
    let (e_none, e3, e12) = (ends(&t_none), ends(&t3), ends(&t12));
    assert!(same_ends(&e_none, &e3), "the floor moved a run's entry");
    assert!(same_ends(&e3, &e12), "the floor moved a run's entry (2)");
    // A floor under the march's own right is not a second floor:
    // 1e-9 changes nothing, byte for byte.
    assert_eq!(at(Some(1e-9)).moves, t_none.moves, "a floor below the march's own raised it");
    // Coarser by the machinist's order, still inside the promise,
    // on every rung - and the verifier never reads the floor as
    // permission; it keeps its own score on each path.
    for (ms, t) in [(None, &t_none), (Some(0.3), &t3), (Some(4.0), &at(Some(4.0)))] {
        let rep = envelope(&f, t);
        assert!(
            rep.gouge_mm <= gate(rep.cell) && rep.rest_mm <= gate(rep.cell),
            "floor {ms:?} envelope {} {}",
            rep.gouge_mm,
            rep.rest_mm
        );
        assert_eq!(verify_finish(&f, t, 11.0), Vec::new(), "verifier spoke at {ms:?}");
    }
    // And the rings - their own emitter, never routed through the
    // chain or the march - answer to neither stage-E number:
    // exactly as if they had never arrived.
    let w1 = waterline(&f, &ppattern(11.0, 8.0, FinishPattern::Waterline)).expect("w1");
    let w2 = waterline(
        &f,
        &Finish3DParams {
            ramp: true,
            ramp_angle: 1.0,
            min_step: Some(4.0),
            ..ppattern(11.0, 8.0, FinishPattern::Waterline)
        },
    )
    .expect("w2");
    assert_eq!(w1.moves, w2.moves, "stage-E numbers spoke to the rings");
}

// ---- M2.5 A: the auto-split as a border, and the floors-only pass ----
//
// Two strategies, no new kinds: SteepShallow rides `parallel()`'s
// raster unchanged and asks the ring ladder one question the Waterline
// never had to - does THIS point of the loop touch a steep facet? A
// 45-degree line stops being a meeting place and becomes a border:
// pure-shallow tangency belongs to the march, walls to the rings, and
// where a loop straddles the crease both sides get their own path.
// Flat answers to nothing but the floors - facets shallow by the
// family's own flag AND flat enough that their tangent-sheet rise
// hides inside the tolerance - and refuses a part that has none.

#[test]
fn the_split_changes_nothing_where_there_is_no_shallow_to_drop() {
    // The twin-kernel law in one assertion: on all-steep ground - the
    // 60-degree cone's flank, every tangent steep by the flag - the
    // partition filter admits every run, so SteepShallow's path is
    // Parallel's byte for byte. The same kernel, the same numbers,
    // one question answered "all of it".
    let f = steep_cone(48);
    let par =
        parallel(&f, &ppattern(30.0, 0.0, FinishPattern::Parallel)).expect("cone parallel");
    let ss = parallel(&f, &ppattern(30.0, 0.0, FinishPattern::SteepShallow)).expect("cone split");
    assert_eq!(ss.moves, par.moves, "the split spoke on ground with no shallow to drop");
    assert!(n_kind(&ss, MoveKind::Feed) > 0, "and went silent elsewhere");
}

#[test]
fn the_border_gives_the_shallow_side_to_the_march() {
    // The tilted wedge: a 30-degree sheet (shallow by the flag) with
    // vertical walls (steep). Parallel lets the rings draw its loops
    // straight ACROSS the sheet, duplicating what the march already
    // rides; the split sends those chords home - strictly fewer feeds -
    // while the wall side and the march itself stand untouched.
    let f = tilted(std::f64::consts::PI / 6.0, 6.0);
    let par = parallel(&f, &ppattern(13.4, 4.0, FinishPattern::Parallel)).expect("wedge parallel");
    let ss =
        parallel(&f, &ppattern(13.4, 4.0, FinishPattern::SteepShallow)).expect("wedge split");
    assert!(
        n_kind(&ss, MoveKind::Feed) < n_kind(&par, MoveKind::Feed),
        "the rings still drew sheet chords under the split: {} vs {}",
        n_kind(&ss, MoveKind::Feed),
        n_kind(&par, MoveKind::Feed)
    );
    // The march took the other side of the border: sheet-height
    // feeds ride on both paths. The row is the machine's own: center
    // XY, tip Z - so a tangent center's tip sits R(1-cos)/cos above
    // the plane read at the CENTER's y, the ball's tilt tax, neither
    // the contact's z nor the axis'.
    let a = std::f64::consts::PI / 6.0;
    let on_sheet = |p: &Toolpath| {
        p.moves.iter().any(|m| {
            m.kind == MoveKind::Feed
                && m.to.x.abs() < 5.9
                && m.to.y.abs() < 5.9
                && (m.to.z
                    - (10.0 - m.to.y * a.tan() + R * (1.0 - a.cos()) / a.cos()))
                    .abs()
                    <= TOL + 1e-6
        })
    };
    assert!(on_sheet(&par) && on_sheet(&ss), "the sheet lost its march");
}

#[test]
fn the_authored_ladder_spaces_the_rings_and_says_nothing_elsewhere() {
    // A stated flank spacing authors the ladder under SteepShallow -
    // coarser by the number, strictly, rung by rung - and is inert
    // under every other pattern, as if the field had never arrived.
    let f = box_facets();
    let at = |s: Option<f64>| {
        parallel(
            &f,
            &Finish3DParams {
                pattern: FinishPattern::SteepShallow,
                steep_stepover: s,
                ..params(10.0, 0.0)
            },
        )
        .expect("box split")
    };
    let (n_none, n_two, n_five) = (at(None), at(Some(2.0)), at(Some(5.0)));
    let n_p = |t: &Toolpath| n_kind(t, MoveKind::Plunge);
    // (5, not 8: a flank spacing at or past the 6 mm ball diameter is
    // refused by the scallop ceiling, #37.)
    assert!(n_p(&n_none) > n_p(&n_two), "Some(2.0) did not coarsen the ladder");
    assert!(n_p(&n_two) > n_p(&n_five), "Some(5.0) did not coarsen the ladder");
    assert!(n_p(&n_five) > 0, "the ladder vanished with a coarse authoring");
    // None rides the scallop law - which is Parallel's ladder number:
    // the twin of test one, counted.
    let par = parallel(&f, &ppattern(10.0, 0.0, FinishPattern::Parallel)).expect("box parallel");
    assert_eq!(n_p(&n_none), n_p(&par), "None drifted off the scallop law");
    // And the word is mute outside its pattern: authored Parallel,
    // byte for byte the unauthored path.
    let authored = parallel(
        &f,
        &Finish3DParams {
            steep_stepover: Some(2.0),
            ..ppattern(10.0, 0.0, FinishPattern::Parallel)
        },
    )
    .expect("authored parallel");
    assert_eq!(authored.moves, par.moves, "a ladder number outside its pattern spoke");
}

#[test]
fn flat_rides_only_the_floors_and_never_a_flank() {
    // The step pocket: a z=10 shelf and a z=5 pocket floor, walls and
    // slopes between. Flat answers to exactly two heights on the
    // material; every Feed BELOW clearance sits on one of them, and
    // both get ridden. (Hop retracts to clearance are Feeds by the
    // family's own choreography - a vertical lift, not a flank.)
    let f = step_pocket();
    let clr = 25.0;
    let t = flat(&f, &ppattern(10.0, 0.0, FinishPattern::Flat)).expect("floors");
    let on_floor = |z: f64| {
        t.moves
            .iter()
            .any(|m| m.kind == MoveKind::Feed && (m.to.z - z).abs() < 1e-6)
    };
    assert!(on_floor(10.0) && on_floor(5.0), "a floor went uncut: {:?}", &t.moves[..2]);
    let stray = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.to.z < clr - 1e-6)
        .find(|m| (m.to.z - 10.0).abs() >= 1e-6 && (m.to.z - 5.0).abs() >= 1e-6);
    assert!(stray.is_none(), "flat rode a flank at z {:?}", stray.map(|m| m.to.z));
}

#[test]
fn flat_refuses_a_part_with_no_floor_and_ignores_the_ladder_number() {
    // The 60-degree cone stripped to its flank: everything faces the
    // sky steeply, nothing lies flat - and Flat names that refusal
    // rather than cutting slopes it was never promised.
    let walls: Vec<Triangle> = steep_cone(48)
        .into_iter()
        .filter(|t| !(t.a.z == 0.0 && t.b.z == 0.0 && t.c.z == 0.0))
        .collect();
    let e = flat(&walls, &ppattern(30.0, 0.0, FinishPattern::Flat)).expect_err("no floor here");
    assert!(matches!(e, GenError::Empty(_)), "{e:?}");
    assert!(e.to_string().contains("no floor"), "{e}");
    // And the ladder's own number is mute here too, byte for byte.
    let b = box_facets();
    let p = |s: Option<f64>| {
        flat(
            &b,
            &Finish3DParams {
                pattern: FinishPattern::Flat,
                steep_stepover: s,
                ..params(10.0, 0.0)
            },
        )
        .expect("box floors")
    };
    assert_eq!(p(Some(2.0)).moves, p(None).moves, "the ladder number spoke to the floors");
}

#[test]
fn the_ladder_number_validates_like_the_others() {
    // Nonsense authoring refuses at the gate on every door, whatever
    // the pattern reads it - validate early, name the parameter, and
    // a None is legal everywhere.
    for s in [0.0, -2.0, f64::NAN, f64::INFINITY] {
        let p = Finish3DParams {
            pattern: FinishPattern::SteepShallow,
            steep_stepover: Some(s),
            ..params(10.0, 0.0)
        };
        assert!(
            matches!(parallel(&box_facets(), &p), Err(GenError::Invalid(_))),
            "steep_stepover {s} passed the gate"
        );
        // Even where the field is inert, a nonsense word still refuses.
        assert!(
            matches!(waterline(&box_facets(), &p), Err(GenError::Invalid(_))),
            "steep_stepover {s} passed the waterline gate"
        );
        assert!(
            matches!(flat(&box_facets(), &p), Err(GenError::Invalid(_))),
            "steep_stepover {s} passed the flat gate"
        );
    }
}


#[test]
fn the_closed_dome_rides_the_split_and_refuses_the_floors() {
    // The dome the operation layer actually programs: the 12x96 cap
    // closed with a flat disc at the rim plane. The split must
    // answer - the march takes the bands, the rings stand off the
    // shallow tangency. And the floors must REFUSE BY NAME: the
    // disc is real floor but unreachable (a ball resting on it
    // buries 0.14 mm into the cap - the cavity is 2.86 tall, the
    // ball 6), and the pole's bands are convex - a tangent ring
    // there is a point the march cannot ride. Flat's honest answer
    // is the Empty refusal, not a path that lies.
    let mut f = dome();
    let rr = 20.0 * (PI / 4.0).sin();
    let center = p3(0.0, 0.0, RIM_Z);
    for j in 0..96usize {
        let th = TAU * j as f64 / 96.0;
        let th2 = TAU * (j + 1) as f64 / 96.0;
        let a = p3(rr * th.cos(), rr * th.sin(), RIM_Z);
        let b = p3(rr * th2.cos(), rr * th2.sin(), RIM_Z);
        f.push(Triangle::new(center, a, b));
    }
    let ss = parallel(&f, &ppattern(18.0, RIM_Z, FinishPattern::SteepShallow))
        .expect("closed-dome split");
    assert!(!ss.moves.is_empty(), "the split said nothing on the dome");
    // The disc floor REFUSES - unreachable, every rim center buries
    // 0.14 mm into the cap. The pole's fan IS floor by `is_floor`'s
    // own law (its tangent-sheet rise, 0.04 mm across a wedge, fits
    // the tolerance), and the plane-tangent ground is strict: each
    // wedge's footprint translated by `r·m_xy` is a sliver a few
    // tenths long, the fold gaps between them the march's own breaks.
    // Until M13.9 the skip phase's blind 4·delta0 probe never landed
    // on those slivers and Flat answered Empty - a refusal that was
    // the march's miss, not the floor's absence. Now the star is
    // ridden where a ball can rest on it: every cut stands at the
    // apex, tangent-high, and nothing touches the disc plane.
    let fl = flat(&f, &ppattern(18.0, RIM_Z, FinishPattern::Flat)).expect("the apex star is floor");
    let cuts: Vec<&camcraft_motion::cl::Move> = fl.moves.iter().filter(|m| m.kind == MoveKind::Feed && (m.to.z - m.from.z).abs() < 1e-6).collect();
    assert!(!cuts.is_empty(), "the apex star went unridden");
    for m in &cuts {
        assert!(m.to.x.hypot(m.to.y) < 1.5 && m.to.z > 19.9, "a flat cut left the apex star: {m:?}");
    }
    assert!(
        fl.moves.iter().all(|m| m.kind == MoveKind::Rapid || (m.to.z - RIM_Z).abs() > 1e-6),
        "the unreachable disc floor was ridden"
    );
    // The app's own heights, straight through both twins.
    let fp = parallel(&f, &ppattern(20.0, RIM_Z, FinishPattern::Parallel))
        .expect("closed-dome parallel");
    let fs = parallel(&f, &ppattern(20.0, RIM_Z, FinishPattern::SteepShallow))
        .expect("closed-dome split, app heights");
    assert!(!fp.moves.is_empty() && !fs.moves.is_empty());
    assert_ne!(fs.moves, fp.moves);
}

#[test]
#[ignore = "verdict lane: full-precision envelope scans over the M2.5 A fixtures - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn the_split_leaves_nothing_uncut_on_either_side_of_the_border() {
    // The Prime Directive reading of the partition: a border, not a
    // gap. Every path below stays inside the cusp budget on its own
    // fixture - and the wedge, where the drop happens, keeps BOTH
    // sides covered: rings on the walls, the march on the sheet.
    let cone = steep_cone(48);
    let f = |facets: &[Triangle], pattern, top: f64, bottom: f64| {
        parallel(
            facets,
            &ppattern(top, bottom, pattern),
        )
        .expect("split path")
    };
    let t = f(&cone, FinishPattern::SteepShallow, 30.0, 0.0);
    let rep = envelope(&cone, &t);
    // The cone's base disc lies UNDER its own flank - a roof. From
    // clearance 25 no vertical entry reaches it: the ball crosses the
    // flank mid-column (the old path's plunges gouged exactly there,
    // root-caused at cz 9 on rho 14.8). So the march now refuses that
    // ground by note, the rings stand off the flank clean, and the
    // floor stands as the rest no plunge at this clearance can
    // promise - reported honestly, never cut by a lie.
    assert!(
        rep.gouge_mm <= gate(rep.cell),
        "cone split gouges {} mm",
        rep.gouge_mm
    );
    assert!(
        t.notes.iter().any(|n| n.contains("plunge column")),
        "the trimmed floor went quiet: {:?}",
        t.notes
    );
    let wedge = tilted(std::f64::consts::PI / 6.0, 6.0);
    let t = f(&wedge, FinishPattern::SteepShallow, 13.4, 4.0);
    let rep = envelope(&wedge, &t);
    assert!(
        rep.gouge_mm <= gate(rep.cell) && rep.rest_mm <= gate(rep.cell),
        "wedge split gouged {} left {}",
        rep.gouge_mm,
        rep.rest_mm
    );
    assert_eq!(verify_finish(&wedge, &t, 13.4), Vec::new(), "the verifier agrees");
    // The pocket's floors: Flat gouges nothing on the flanks it
    // declines - rest there is the strategy's own honesty, and the
    // floors themselves come out inside the budget.
    let pocket = step_pocket();
    let t = flat(&pocket, &ppattern(10.0, 0.0, FinishPattern::Flat)).expect("floors");
    let rep = envelope(&pocket, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "flat gouged {} mm", rep.gouge_mm);
}

// ---------- M2.5 B: the crease pair ----------

/// A prism with a V-groove across its top: the valley floor line is
/// x=0, z=5; the flanks rise at `alpha_deg` to crests at x=+-8. The
/// valley's dihedral is 180 - 2 alpha, so alpha=12.5 gives theta=155
/// (a 0.665 band on the R3 ball) and alpha=1 gives theta=178 (a
/// 0.052 band: flat to the corner's eye, a pen's work).
fn v_groove(alpha_deg: f64) -> Vec<Triangle> {
    let t = (alpha_deg * PI / 180.0).tan();
    let ze = 5.0 + 8.0 * t; // the crest height
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let mut f = Vec::new();
    // Flanks, CCW from the groove side, and the box that closes them.
    f.extend(quad([p3(0.0, -8.0, 5.0), p3(0.0, 8.0, 5.0), p3(-8.0, 8.0, ze), p3(-8.0, -8.0, ze)]));
    f.extend(quad([p3(8.0, -8.0, ze), p3(8.0, 8.0, ze), p3(0.0, 8.0, 5.0), p3(0.0, -8.0, 5.0)]));
    f.extend(quad([p3(-8.0, -8.0, 0.0), p3(-8.0, 8.0, 0.0), p3(8.0, 8.0, 0.0), p3(8.0, -8.0, 0.0)]));
    f.extend(quad([p3(-8.0, -8.0, 0.0), p3(-8.0, -8.0, ze), p3(-8.0, 8.0, ze), p3(-8.0, 8.0, 0.0)]));
    f.extend(quad([p3(8.0, -8.0, 0.0), p3(8.0, 8.0, 0.0), p3(8.0, 8.0, ze), p3(8.0, -8.0, ze)]));
    let fan = |p: [Point3; 5]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
            Triangle::new(p[0], p[3], p[4]),
        ]
    };
    f.extend(fan([p3(-8.0, -8.0, 0.0), p3(-8.0, -8.0, ze), p3(0.0, -8.0, 5.0), p3(8.0, -8.0, ze), p3(8.0, -8.0, 0.0)]));
    f.extend(fan([p3(-8.0, 8.0, 0.0), p3(8.0, 8.0, 0.0), p3(8.0, 8.0, ze), p3(0.0, 8.0, 5.0), p3(-8.0, 8.0, ze)]));
    f
}

/// Every ball-center below the clearance, plunge heads included.
/// Moves speak tips (the convention of the whole module), so this
/// lifts what it reads by `r` - the crease kernels hold their law
/// in centers and emit them lowered.
fn centers(t: &Toolpath) -> Vec<Point3> {
    t.moves
        .iter()
        .filter(|m| matches!(m.kind, MoveKind::Feed | MoveKind::Plunge) && m.to.z < 25.0 - 1e-6)
        .map(|m| p3(m.to.x, m.to.y, m.to.z + R))
        .collect()
}

#[test]
fn corner_rings_the_pocket_and_stands_in_the_mouths() {
    // The step pocket's concave seams: four floor-wall corners (90
    // deg, band r = 3 > tol) and four verticals at the pocket's
    // upright corners. The floor-wall loci stand z = floor + r, clip
    // where the neighbouring wall's plane would let the ball cross
    // it, and weld at the four (3,3,8)-type points into one closed
    // ring; the verticals ride the mouth, clipped at the bottom by
    // the tip's window and kept exact at the top edge.
    let f = step_pocket();
    let t = corner(&f, &ppattern(10.0, 5.0, FinishPattern::Corner)).expect("pocket corners");
    assert_eq!(n_kind(&t, MoveKind::Plunge), 5, "one run per corner, ring + 4 mouths");
    // The ring: a closed run whose last point sits back on its first.
    let ring: Vec<Point3> = centers(&t)
        .into_iter()
        .filter(|p| (p.z - 8.0).abs() < 1e-9 && (p.x.abs() - 3.0).abs() < 1e-9 && (p.y.abs() - 3.0).abs() < 1e-9)
        .collect();
    assert_eq!(ring.len(), 5, "a four-point closed ring: {ring:?}");
    assert_eq!(ring[0], ring[4], "the weld did not close bit-for-bit");
    // The mouths: each stands on a corner diagonal, exact at the top.
    let mouths: Vec<Point3> = centers(&t)
        .into_iter()
        .filter(|p| (p.z - 8.0).abs() > 1e-9)
        .collect();
    assert_eq!(mouths.len(), 8, "four two-point mouth runs: {mouths:?}");
    for m in &mouths {
        assert!((m.x.abs() - 3.0).abs() < 1e-9 && (m.y.abs() - m.x.abs()).abs() < 1e-9, "mouth off the diagonal: {m:?}");
        assert!(m.z > 8.0 && m.z <= 10.0 + 1e-9, "mouth past the corner line: {m:?}");
    }
    assert!(mouths.iter().any(|m| (m.z - 10.0).abs() < 1e-9), "the mouth lost its exact top");
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "the crease pass gouged {} mm", rep.gouge_mm);
}

#[test]
fn the_ladder_hops_between_different_features() {
    // Corner runs are features, not levels: every run is announced
    // by a rapid over its start and a plunge straight down its
    // tangent column, and every travel between runs rides clearance.
    let f = step_pocket();
    let t = corner(&f, &ppattern(10.0, 5.0, FinishPattern::Corner)).expect("pocket corners");
    let mut prev: Option<Point3> = None;
    for m in &t.moves {
        match m.kind {
            MoveKind::Rapid => {
                assert!((m.to.z - 25.0).abs() < 1e-9, "a travel below clearance: {m:?}");
            }
            MoveKind::Plunge => {
                let head = prev.expect("a plunge without an announcing rapid");
                assert!((head.x - m.to.x).abs() < 1e-9 && (head.y - m.to.y).abs() < 1e-9, "a plunge that drifted: {head:?} -> {m:?}");
            }
            _ => {}
        }
        if matches!(m.kind, MoveKind::Rapid | MoveKind::Plunge | MoveKind::Feed) {
            prev = Some(m.to);
        }
    }
}

#[test]
fn the_corner_is_its_own_reference() {
    // Corner IS pencil with the tool's own ball: the band question
    // "what could this ball not clean" answers itself identically -
    // byte for byte on the pocket, and the authored reference is
    // mute for Corner as if it had never arrived.
    let f = step_pocket();
    let plain = corner(&f, &ppattern(10.0, 5.0, FinishPattern::Corner)).expect("corner");
    let named = corner(
        &f,
        &Finish3DParams { pattern: FinishPattern::Corner, pencil_reference: Some(99.0), ..params(10.0, 5.0) },
    )
    .expect("corner with a named reference");
    assert_eq!(plain.moves, named.moves, "Corner obeyed a reference ball");
    let pen = pencil(&f, &ppattern(10.0, 5.0, FinishPattern::Pencil)).expect("pencil, no reference");
    assert_eq!(plain.moves, pen.moves, "an unnamed pencil cut other corners than the ball's own band");
}

#[test]
fn a_convex_part_has_no_corner_to_clean() {
    // Box and plateau bend only the convex way: the faces' own
    // thirds say so, and the pass refuses rather than ride ridges.
    for (f, name) in [(box_facets(), "box"), (plateau(), "plateau")] {
        let e = corner(&f, &ppattern(10.0, 0.0, FinishPattern::Corner)).expect_err(name);
        assert!(matches!(e, GenError::Empty(_)), "{name}: {e:?}");
        assert!(e.to_string().contains("no concave corner"), "{name}: {e}");
        let e = pencil(&f, &ppattern(10.0, 0.0, FinishPattern::Pencil)).expect_err(name);
        assert!(matches!(e, GenError::Empty(_)), "{name} pencil: {e:?}");
    }
}

#[test]
fn the_valley_rides_its_bisector_column() {
    // The 155-deg groove: locus line stands 3 / cos(alpha) above the
    // valley floor, tangent to both flanks at exactly r - the
    // deepest center that touches both faces and cuts neither.
    let a = 12.5f64;
    let f = v_groove(a);
    let t = corner(&f, &ppattern(7.0, 0.0, FinishPattern::Corner)).expect("the groove has a valley");
    let c = centers(&t);
    let z = 5.0 + R / (a * PI / 180.0).cos();
    assert_eq!(c.len(), 2, "one run: plunge head + locus end: {c:?}");
    for p in &c {
        assert!(p.x.abs() < 1e-9, "off the bisector plane: {p:?}");
        assert!((p.z - z).abs() < 1e-9, "not the tangent center: {p:?} vs z={z}");
    }
    assert!(c.iter().any(|p| (p.y + 8.0).abs() < 1e-9) && c.iter().any(|p| (p.y - 8.0).abs() < 1e-9), "the run lost an exact end");
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "the valley pass gouged {} mm", rep.gouge_mm);
}

#[test]
fn the_band_decides_who_cuts() {
    // 178 degrees: the corner's own band is 0.052 - below the
    // tolerance, so Corner says there is nothing to clean. A pen of
    // diameter 20 reads (10 - 3) / 57.29 = 0.122: Pencil cuts.
    let f = v_groove(1.0);
    let e = corner(&f, &ppattern(7.0, 0.0, FinishPattern::Corner)).expect_err("flat to the corner's eye");
    assert!(matches!(e, GenError::Empty(_)), "{e:?}");
    let t = pencil(
        &f,
        &Finish3DParams { pattern: FinishPattern::Pencil, pencil_reference: Some(20.0), ..params(7.0, 0.0) },
    )
    .expect("the pen's leftover band");
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "pencil gouged {} mm", rep.gouge_mm);
    // 155 degrees: pen 8 clears a 0.222 band - cuts; pen 6.5 hides a
    // 0.055 one - refuses by name; and a pen that is not fatter than
    // the ball is not a reference at all.
    let g = v_groove(12.5);
    let cut = |d: f64, f: &[Triangle]| {
        pencil(f, &Finish3DParams { pattern: FinishPattern::Pencil, pencil_reference: Some(d), ..params(7.0, 0.0) })
    };
    assert!(cut(8.0, &g).is_ok(), "a 0.222 band hid from the pen");
    for d in [6.5, 6.0] {
        let e = cut(d, &g).expect_err("a hiding pen");
        assert!(
            (d > 6.0 && matches!(e, GenError::Empty(_)) && e.to_string().contains("pencil"))
                || (d == 6.0 && matches!(e, GenError::Invalid(_)) && e.to_string().contains("does not exceed")),
            "pen {d}: {e:?}"
        );
    }
}

#[test]
fn a_flipped_winding_is_skipped_by_note() {
    // Both faces must run the shared edge opposite ways. Flip one
    // pocket wall's windings and its seam's corner goes to the
    // note, never to a guess; the other seams keep their runs.
    let mut f = step_pocket();
    let mut flipped = false;
    for t in &mut f {
        let on = |p: &Point3| (p.x - 6.0).abs() < 1e-9;
        // The x=6 wall's floor-side triangle, wound along the seam.
        if on(&t.a) && on(&t.b) && on(&t.c) && (t.a.z - 5.0).abs() < 1e-9 && (t.b.z - 5.0).abs() < 1e-9 && (t.c.z - 10.0).abs() < 1e-9 && !flipped {
            std::mem::swap(&mut t.b, &mut t.c);
            flipped = true;
        }
    }
    assert!(flipped, "the fixture lost its wall x=6");
    let t = corner(&f, &ppattern(10.0, 5.0, FinishPattern::Corner)).expect("the other seams stand");
    assert!(
        t.notes.iter().any(|n| n.contains("winding")),
        "the flip went unannounced: {:?}",
        t.notes
    );
}

#[test]
fn the_crease_pair_is_deterministic() {
    // Segment, candidate and run order are all fixed by sorting:
    // the same facets twice write the same path twice.
    let f = step_pocket();
    let g = v_groove(12.5);
    let p1 = ppattern(10.0, 5.0, FinishPattern::Corner);
    let p2 = ppattern(10.0, 5.0, FinishPattern::Pencil);
    let p3p = ppattern(7.0, 0.0, FinishPattern::Corner);
    assert_eq!(corner(&f, &p1).expect("a").moves, corner(&f, &p1).expect("b").moves, "corner drifted");
    assert_eq!(pencil(&f, &p2).expect("a").moves, pencil(&f, &p2).expect("b").moves, "pencil drifted");
    assert_eq!(corner(&g, &p3p).expect("a").moves, corner(&g, &p3p).expect("b").moves, "valley drifted");
}

#[test]
fn the_reference_number_validates_like_the_others() {
    // Nonsense authoring refuses at the gate on every door, whether
    // or not the pattern reads the field - the ladder law again.
    for s in [0.0, -2.0, f64::NAN, f64::INFINITY] {
        for door in [FinishPattern::Pencil, FinishPattern::Corner, FinishPattern::Waterline] {
            let q = Finish3DParams { pattern: door, pencil_reference: Some(s), ..params(10.0, 0.0) };
            let r = if door == FinishPattern::Corner {
                corner(&box_facets(), &q)
            } else if door == FinishPattern::Pencil {
                pencil(&box_facets(), &q)
            } else {
                waterline(&box_facets(), &q)
            };
            assert!(matches!(r, Err(GenError::Invalid(_))), "{door:?} took pencil_reference {s}");
        }
    }
}

// ------------------------------------------------------------------ M2.5 C
// The sloped twin, the locally-spaced spiral, and the boundary-distance
// rings. Fixtures first: two OPEN shells, because the geodesic's whole
// datum is an open edge a closed model does not have.

/// Open dome: the sphere_cap's band and apex fan with the closing disc
/// left off. The equatorial rim is a real open edge - the geodesic
/// field's zero - and the normals tilt the whole 0..90 degrees, so the
/// field's own levels run from rim to crown.
fn open_dome() -> Vec<Triangle> {
    let (radius, n_lat, n_lon) = (20.0, 12, 48);
    let polar = PI / 2.0;
    let mut facets = Vec::new();
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon)
            .map(|j| {
                let th = TAU * j as f64 / n_lon as f64;
                p3(rr * th.cos(), rr * th.sin(), z)
            })
            .collect()
    };
    let apex = p3(0.0, 0.0, radius);
    let first = ring(1);
    for j in 0..n_lon {
        facets.push(Triangle::new(apex, first[j], first[(j + 1) % n_lon]));
    }
    for k in 1..n_lat {
        let (ra, rb) = (ring(k), ring(k + 1));
        for j in 0..n_lon {
            let j2 = (j + 1) % n_lon;
            facets.push(Triangle::new(ra[j], rb[j], rb[j2]));
            facets.push(Triangle::new(ra[j], rb[j2], ra[j2]));
        }
    }
    facets
}

/// One subdivided quad face of an axis-aligned grid, wound so its
/// normal points at `want`. The pocket's faces read as grids, not as
/// two huge triangles: a geodesic contour is only as honest as the
/// edges it can cross.
fn face_grid(
    o: (f64, f64, f64),
    e1: (f64, f64, f64),
    e2: (f64, f64, f64),
    want: (f64, f64, f64),
    n1: usize,
    n2: usize,
) -> Vec<Triangle> {
    let pt = |i: usize, j: usize| {
        p3(
            o.0 + e1.0 * i as f64 / n1 as f64 + e2.0 * j as f64 / n2 as f64,
            o.1 + e1.1 * i as f64 / n1 as f64 + e2.1 * j as f64 / n2 as f64,
            o.2 + e1.2 * i as f64 / n1 as f64 + e2.2 * j as f64 / n2 as f64,
        )
    };
    let w = p3(want.0, want.1, want.2);
    let mut f = Vec::new();
    for i in 0..n1 {
        for j in 0..n2 {
            let (q0, q1, q2, q3) = (pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1));
            if (q1 - q0).cross(q3 - q0).dot(w) >= 0.0 {
                f.push(Triangle::new(q0, q1, q2));
                f.push(Triangle::new(q0, q2, q3));
            } else {
                f.push(Triangle::new(q0, q2, q1));
                f.push(Triangle::new(q0, q3, q2));
            }
        }
    }
    f
}

/// A 20 x 20 pocket, z 0..10, with the top left off: floor at z = 0,
/// four walls, normals into the cavity. Every rim edge is open (the
/// datum runs the whole way around); the wall/wall and wall/floor
/// creases are the 90-degree corners the lifted tangent cannot carry
/// a ball around - the geodesic's honest-opening fixture.
fn pocket_open() -> Vec<Triangle> {
    let mut facets = face_grid((-10.0, -10.0, 0.0), (20.0, 0.0, 0.0), (0.0, 20.0, 0.0), (0.0, 0.0, 1.0), 10, 10);
    facets.extend(face_grid((10.0, -10.0, 0.0), (0.0, 20.0, 0.0), (0.0, 0.0, 10.0), (-1.0, 0.0, 0.0), 10, 4));
    facets.extend(face_grid((-10.0, -10.0, 0.0), (0.0, 20.0, 0.0), (0.0, 0.0, 10.0), (1.0, 0.0, 0.0), 10, 4));
    facets.extend(face_grid((-10.0, 10.0, 0.0), (20.0, 0.0, 0.0), (0.0, 0.0, 10.0), (0.0, -1.0, 0.0), 10, 4));
    facets.extend(face_grid((-10.0, -10.0, 0.0), (20.0, 0.0, 0.0), (0.0, 0.0, 10.0), (0.0, 1.0, 0.0), 10, 4));
    facets
}

/// A bottomless box, z 0..10, walls as grids (the plateau's coarse
/// triangles answer every contour endpoint at a corner, where the
/// bisector-lift bites - an honest refusal that loses the whole ring
/// on that mesh). Grid walls give the rings somewhere to stand, and
/// only the corner bands open.
fn box_open() -> Vec<Triangle> {
    let mut facets = face_grid((-10.0, -10.0, 10.0), (20.0, 0.0, 0.0), (0.0, 20.0, 0.0), (0.0, 0.0, 1.0), 8, 8);
    facets.extend(face_grid((10.0, -10.0, 0.0), (0.0, 20.0, 0.0), (0.0, 0.0, 10.0), (1.0, 0.0, 0.0), 8, 4));
    facets.extend(face_grid((-10.0, -10.0, 0.0), (0.0, 20.0, 0.0), (0.0, 0.0, 10.0), (-1.0, 0.0, 0.0), 8, 4));
    facets.extend(face_grid((-10.0, 10.0, 0.0), (20.0, 0.0, 0.0), (0.0, 0.0, 10.0), (0.0, 1.0, 0.0), 8, 4));
    facets.extend(face_grid((-10.0, -10.0, 0.0), (20.0, 0.0, 0.0), (0.0, 0.0, 10.0), (0.0, -1.0, 0.0), 8, 4));
    facets
}

/// A flat square sheet hung from its rim corners by a spoke fan: four
/// open rim edges, one interior vertex. With an unreachable closed
/// island parked beside it, this is the stranded-vertices fixture.
fn hung_sheet() -> Vec<Triangle> {
    let c = p3(0.0, 0.0, 0.0);
    let v = [p3(-6.0, -6.0, 0.0), p3(6.0, -6.0, 0.0), p3(6.0, 6.0, 0.0), p3(-6.0, 6.0, 0.0)];
    let mut f = Vec::new();
    for j in 0..4 {
        f.push(Triangle::new(c, v[j], v[(j + 1) % 4]));
    }
    f
}

/// A closed tetrahedron, far from everything: the island the boundary
/// cannot reach.
fn island() -> Vec<Triangle> {
    let (a, b, c, d) = (p3(90.0, 0.0, 0.0), p3(91.0, 0.0, 0.0), p3(90.5, 1.0, 0.0), p3(90.5, 0.5, 2.0));
    vec![
        Triangle::new(a, b, c),
        Triangle::new(a, b, d),
        Triangle::new(b, c, d),
        Triangle::new(c, a, d),
    ]
}

/// The path's runs as the machine sees them: a Plunge opens a run,
/// following Feeds extend it, anything else closes it.
fn runs_of(t: &Toolpath) -> Vec<Vec<Point3>> {
    let mut out = Vec::new();
    let mut cur: Vec<Point3> = Vec::new();
    for m in &t.moves {
        match m.kind {
            MoveKind::Plunge => {
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
                cur = vec![m.to];
            }
            MoveKind::Feed => cur.push(m.to),
            MoveKind::Rapid => {
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
            }
        }
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    out
}

/// The march, whole: the curve's own pieces, in travel order. The
/// radial advance per full turn is read off this stitch: unwrap
/// theta along the march (dense thousands of chord-budgeted
/// samples), then take r(t_k) - r(t_{k+1}) between turn boundaries
/// PHASED to the ray that matters - on ground with a fall direction,
/// the steepest ray is where the cusp law binds. This is the morphed
/// spiral's whole promise, measured where the curve says it. A
/// fragment of the spiral is a run whose radius ADVANCES with its
/// angle - the curve law itself, read backwards as an admission
/// test; a level ring (circle, arc, or corner-split piece) keeps
/// its radius while it winds and is refused. Tessellated domes and
/// wall-pierced sheets cut the march into pieces at every ring
/// crossing and shelf edge - the discontinuity law doing its job -
/// so per-turn laws are measured on the stitch, not on one piece.
fn march_stitched(t: &Toolpath, step: f64) -> Vec<Point3> {
    let (lo, hi) = (0.3 * step / TAU, 1.8 * step / TAU);
    let mut out = Vec::new();
    for run in runs_of(t) {
        let mut acc = 0.0f64;
        let mut prev: Option<f64> = None;
        for q in &run {
            let th = q.y.atan2(q.x);
            if let Some(pv) = prev {
                let mut d = th - pv;
                while d <= -PI {
                    d += TAU;
                }
                while d > PI {
                    d -= TAU;
                }
                acc += d;
            }
            prev = Some(th);
        }
        if acc.abs() < 0.08 {
            continue; // no opinion from a stub
        }
        let (r0, r1) = (
            run[0].x.hypot(run[0].y),
            run[run.len() - 1].x.hypot(run[run.len() - 1].y),
        );
        let rate = (r0 - r1).abs() / acc.abs();
        if rate > lo && rate < hi {
            out.extend(run);
        }
    }
    out
}

/// The pitch read PER FRAGMENT, for skins the march breaks at every
/// fold (the strict ground's own law - the dome test below is its
/// statement). A stitched full turn never exists there; the fragments
/// do, and they sit ON the curve: their XY positions are the spiral's
/// own `r(theta)` sampled, so a fragment's radius advance over its
/// angle advance is the spacing law that built it, read wherever it
/// still speaks. The same admission law as `march_stitched` (a rate
/// inside 0.3..1.8 of the spiral's, and enough angle to have an
/// opinion) filters rings and stubs; results keep travel order, each
/// paired with its own mid-radius (bands group by it; the morph's
/// within-band azimuthal wobble averages out across them).
fn fragment_pitch(t: &Toolpath, step: f64) -> Vec<(f64, f64)> {
    // A piece is read in windows of at most a quarter turn: since the
    // convex folds' edge contact (#36) the march no longer breaks at
    // the crests, so one piece can wind several turns - and a
    // multi-turn average would blur the per-band law under test.
    let (lo, hi) = (0.3 * step / TAU, 1.8 * step / TAU);
    let mut out = Vec::new();
    let mut emit = |a: Point3, b: Point3, acc: f64| {
        if acc.abs() < 0.08 {
            return; // no opinion from a stub
        }
        let (r0, r1) = (a.x.hypot(a.y), b.x.hypot(b.y));
        let rate = (r0 - r1).abs() / acc.abs();
        if rate > lo && rate < hi {
            out.push(((r0 + r1) * 0.5, rate * TAU));
        }
    };
    for run in runs_of(t) {
        let Some(&first) = run.first() else { continue };
        let (mut start, mut acc) = (first, 0.0f64);
        let mut prev: Option<(Point3, f64)> = None;
        for q in &run {
            let th = q.y.atan2(q.x);
            if let Some((_, pv)) = prev {
                let mut d = th - pv;
                while d <= -PI {
                    d += TAU;
                }
                while d > PI {
                    d -= TAU;
                }
                acc += d;
            }
            prev = Some((*q, th));
            if acc.abs() >= std::f64::consts::FRAC_PI_2 {
                emit(start, *q, acc);
                start = *q;
                acc = 0.0;
            }
        }
        if let Some((last, _)) = prev {
            emit(start, last, acc);
        }
    }
    out
}

fn turn_pitch(piece: &[Point3], phase: f64) -> Vec<f64> {
    // Unwrap the turn angle along the march; a stitch interval
    // whose XY chord is a skip-phase air stretch (more than 2.5
    // samples' worth) carries no curve law, so a turn boundary
    // falling inside it reads nothing - the pitch there is dropped,
    // not interpolated.
    let mut pts: Vec<(f64, f64, bool)> = Vec::new();
    let mut prev: Option<Point3> = None;
    let mut acc = 0.0f64;
    for q in piece {
        let th0 = q.y.atan2(q.x);
        if let Some(pv) = prev {
            let mut dth = th0 - pv.y.atan2(pv.x);
            while dth <= -PI {
                dth += TAU;
            }
            while dth > PI {
                dth -= TAU;
            }
            acc += dth;
        }
        let gap = prev.is_some_and(|pv| {
            (q.x - pv.x).hypot(q.y - pv.y) > 2.5 * scallop_stepdown(R, TOL)
        });
        pts.push((acc, q.x.hypot(q.y), gap));
        prev = Some(*q);
    }
    // The curve winds whichever way its parameterization says (the
    // polar family's sin_cos swap makes it clockwise); the turn
    // boundaries must ascend in the unwrapped angle either way.
    if pts.len() >= 2 && pts[pts.len() - 1].0 < pts[0].0 {
        for q in pts.iter_mut() {
            q.0 = -q.0;
        }
    }
    let r_at = |t: f64| -> Option<f64> {
        if pts.is_empty() || t < pts[0].0 || t > pts[pts.len() - 1].0 {
            return None;
        }
        pts.windows(2).find_map(|w| {
            if w[0].0 <= t && t <= w[1].0 && w[1].0 > w[0].0 {
                if t > w[0].0 && w[1].2 {
                    return None; // strictly inside an air stretch
                }
                let f = (t - w[0].0) / (w[1].0 - w[0].0);
                Some(w[0].1 + (w[1].1 - w[0].1) * f)
            } else {
                None
            }
        })
    };
    // A turn boundary whose interval contains an inter-run jump
    // chord (a hole the march refused: ground that answered nothing
    // anywhere on it) straddles territory the path never cut - its
    // radius delta belongs to the skipped annulus, not to the
    // spacing law. The reading is dropped, not interpolated.
    let hole = |ta: f64, tb: f64| {
        pts.windows(2).any(|w| {
            w[1].2 && w[0].0 < tb && w[1].0 > ta && w[0].0 < w[1].0
        })
    };
    let mut out = Vec::new();
    if pts.is_empty() {
        return out;
    }
    let start = pts[0].0 + (phase - pts[0].0).rem_euclid(TAU);
    let mut k = 0.0f64;
    while start + (k + 1.0) * TAU <= pts[pts.len() - 1].0 {
        if let (Some(a), Some(b)) = (r_at(start + k * TAU), r_at(start + (k + 1.0) * TAU))
            && !hole(start + k * TAU, start + (k + 1.0) * TAU)
        {
            out.push(a - b);
        }
        k += 1.0;
    }
    out
}

/// A lone vertical wall: no facet anywhere under it rests shallower
/// than 45 degrees, so the polar family has no footprint to spiral -
/// the all-steep fall-through's home fixture.
fn steep_wall() -> Vec<Triangle> {
    face_grid((5.0, -10.0, -6.0), (0.0, 20.0, 0.0), (0.0, 0.0, 12.0), (-1.0, 0.0, 0.0), 8, 8)
}

#[test]
fn the_geodesic_datum_is_a_real_boundary() {
    // A CLOSED shell has nowhere for the rings to start, and the
    // refusal says so by name. A bottomless box is the same law read
    // the other way: its open rims ARE a boundary, so its walls
    // march, and the corner meridian that bites says so by note.
    {
        let f = cone(32);
        let e = geodesic(&f, &ppattern(18.0, 0.0, FinishPattern::Geodesic));
        assert!(
            matches!(&e, Err(GenError::Invalid(m)) if m.contains("closed shell")),
            "a closed shell slipped through: {e:?}"
        );
    }
    let t = geodesic(&box_open(), &ppattern(18.0, -1.0, FinishPattern::Geodesic))
        .expect("a bottomless box has a real rim");
    assert!(n_kind(&t, MoveKind::Feed) > 0, "the rims seeded nothing");
    assert!(
        t.notes.iter().any(|n| n.contains("refused")),
        "the wall-corner meridian went unannounced: {:?}",
        t.notes
    );
}

#[test]
fn geodesic_ring_rides_the_open_dome_rim_to_crown() {
    // The continuity law on a dome: every level of the boundary field
    // closes into ONE loop, the loops climb to the crown (a waterline
    // ladder would stop at its own Z levels), and no feed breaks Z
    // along a run.
    let f = open_dome();
    let t = geodesic(&f, &ppattern(24.0, -5.0, FinishPattern::Geodesic)).expect("open dome marches");
    let runs = runs_of(&t);
    assert!(runs.len() >= 15, "the field marched {} rings", runs.len());
    // A level of the dome's field is a latitude loop: the chain
    // welds it closed by running the whole azimuth around. That is
    // the honest closure test - the chain stops AT the start weld,
    // it does not repeat the point.
    let around = runs
        .iter()
        .filter(|r| {
            let mut prev: Option<f64> = None;
            let mut acc = 0.0f64;
            for q in r.iter() {
                let th0 = q.y.atan2(q.x);
                if let Some(pv) = prev {
                    let mut d = th0 - pv;
                    while d <= -PI {
                        d += TAU;
                    }
                    while d > PI {
                        d -= TAU;
                    }
                    acc += d;
                }
                prev = Some(th0);
            }
            acc.abs() >= TAU * 0.9
        })
        .count();
    assert!(
        around >= 15,
        "{around} of {} runs made it round the dome - the crease chain dropped seams",
        runs.len()
    );
    assert!(
        t.moves.iter().filter(|m| m.kind == MoveKind::Feed).all(|m| (m.to.z - m.from.z).abs() <= 0.05),
        "a geodesic ring broke Z - the loop opened without saying so"
    );
    assert!(
        t.moves.iter().any(|m| m.kind == MoveKind::Feed && m.to.z > 19.5),
        "the field never climbed the crown"
    );
    assert!(n_kind(&t, MoveKind::Plunge) == runs.len(), "a run entered without its own certified plunge");
}

#[test]
fn geodesic_opens_its_loops_at_the_pocket_corner_and_reports() {
    // The pocket: wall levels that a ball can hold stand as four runs
    // each (the vertical corner opens every loop - the lifted tangent
    // bites the bisector there), floor levels far enough out close
    // again, and the corner band is REFUSED POINTS counted in a note,
    // not a smooth-looking lie.
    let f = pocket_open();
    let t = geodesic(&f, &ppattern(10.5, -1.0, FinishPattern::Geodesic)).expect("open pocket marches");
    let runs = runs_of(&t);
    assert!(runs.len() >= 16, "the pocket ran only {} pieces", runs.len());
    let wall = t.moves.iter().filter(|m| {
        m.kind == MoveKind::Feed && (m.to.x.abs() - 7.0).abs() < 0.11 && m.to.z > 0.5 && m.to.z < 7.0
    }).count();
    assert!(wall > 20, "the walls drew no runs: {wall}");
    let floor = t.moves.iter().filter(|m| {
        m.kind == MoveKind::Feed && m.to.z.abs() <= 3.0 * TOL && m.to.x.abs() < 7.0 && m.to.y.abs() < 7.0
    }).count();
    assert!(floor > 40, "the floor never got its rings: {floor}");
    assert!(
        t.notes.iter().any(|n| n.contains("refused")),
        "the corner band went unannounced: {:?}",
        t.notes
    );
}

#[test]
fn geodesic_counts_the_island_it_cannot_reach() {
    // A closed component riding along with the open mesh is refused
    // its contours and REPORTED - never invented a seam for.
    let mut f = hung_sheet();
    f.extend(island());
    let t = geodesic(&f, &ppattern(1.0, -2.0, FinishPattern::Geodesic)).expect("the sheet still marches");
    assert!(n_kind(&t, MoveKind::Feed) > 0, "the reachable rings vanished");
    assert!(
        t.notes.iter().any(|n| n.contains("cannot reach")),
        "the island rode uncounted: {:?}",
        t.notes
    );
}

#[test]
fn geodesic_refuses_nonsense_and_degeneracy_without_losing_its_head() {
    // No open edge, no window, no size: every answer is a named
    // refusal, never a panic and never a silent nothing.
    let sheet = vec![Triangle::new(p3(0.0, 0.0, 0.0), p3(5.0, 0.0, 0.0), p3(0.0, 5.0, 0.0))];
    let e = geodesic(&sheet, &ppattern(1.0, -1.0, FinishPattern::Geodesic));
    assert!(matches!(e, Err(GenError::Empty(_))), "a zero-extent field answered {e:?}");
    let dust = vec![Triangle::new(p3(1.0, 1.0, 1.0), p3(1.0, 1.0, 1.0), p3(1.0, 1.0, 1.0))];
    assert!(geodesic(&dust, &ppattern(1.0, -1.0, FinishPattern::Geodesic)).is_err(), "dust cut a path");
    let nan = vec![Triangle::new(p3(f64::NAN, 0.0, 0.0), p3(1.0, 0.0, 0.0), p3(0.0, 1.0, 0.0))];
    assert!(geodesic(&nan, &ppattern(1.0, -1.0, FinishPattern::Geodesic)).is_err(), "a NaN facet made geometry");
    for s in [0.0, -2.0, f64::NAN, f64::INFINITY] {
        let q = Finish3DParams {
            stepover: Some(s),
            ..ppattern(1.0, -1.0, FinishPattern::Geodesic)
        };
        assert!(
            matches!(geodesic(&pocket_open(), &q), Err(GenError::Invalid(_))),
            "a door took stepover {s}"
        );
    }
}

#[test]
fn the_ramp_variant_rides_parallels_raster_and_the_flags_are_the_story() {
    // At the kernel, Ramp is Parallel - the variant carries no new
    // geometry. The pattern's whole story is the two sloped laws, and
    // those are the twin the app FORCES (operations.rs pins that).
    let f = plateau();
    let plain = parallel(&f, &ppattern(10.0, 0.0, FinishPattern::Parallel)).expect("parallel");
    let variant = parallel(&f, &ppattern(10.0, 0.0, FinishPattern::Ramp)).expect("ramp variant");
    assert_eq!(variant.moves, plain.moves, "the Ramp variant invented geometry of its own");
    let forced = parallel(
        &f,
        &Finish3DParams {
            ramp: true,
            entry_ramp: true,
            ..ppattern(10.0, 0.0, FinishPattern::Parallel)
        },
    )
    .expect("forced twin");
    assert_ne!(forced.moves, plain.moves, "the sloped laws are inert - the twin says nothing");
}

#[test]
fn the_morphed_spiral_is_the_spiral_over_flat_ground() {
    // g = 0 everywhere: the local cos-law collapses to the authored
    // number, and the morph spaces its turns exactly where the
    // spiral does. Both march, both measure the scallop step.
    let f = tilted(0.0, 8.0);
    let step = scallop_stepover(R, TOL);
    let m = morphed_spiral(&f, &ppattern(10.0, 4.0, FinishPattern::MorphSpiral)).expect("flat morph");
    let s = spiral(&f, &ppattern(10.0, 4.0, FinishPattern::Spiral)).expect("flat spiral");
    for (t, who) in [(&m, "morph"), (&s, "spiral")] {
        let pitch = turn_pitch(&march_stitched(t, step), 0.0);
        assert!(pitch.len() >= 3, "{who} turned too few times to measure: {pitch:?}");
        assert!(
            pitch.iter().all(|d| (d - step).abs() <= 0.05),
            "{who}'s flat-ground turn pitch left the scallop step: {pitch:?}"
        );
    }
}

#[test]
fn the_morphed_spiral_spaces_turns_by_the_local_ground() {
    // Two readings of one slope. The turn-to-turn gap of a spiral is
    // CONSERVED across rays - it is the local law integrated around
    // a turn - so on a uniform 30-degree sheet the morph advances by
    // the MEAN of 1/cos over the turn, while the spiral takes the
    // WORST-tilt discount once up front. Locality is gentler than
    // the global law on the fall side and never tighter; both pay.
    // Author a stepover and they split again: the spiral obeys the
    // number verbatim, the morph still pays the slope.
    let f = tilted(PI / 6.0, 8.0);
    let step = scallop_stepover(R, TOL);
    let t30 = (PI / 6.0).tan();
    let mean_loc = (0..4096)
        .map(|i| {
            let th = TAU * i as f64 / 4096.0;
            let g = t30 * th.sin();
            1.0 / (1.0 + g * g).sqrt()
        })
        .sum::<f64>()
        / 4096.0;
    let m = morphed_spiral(&f, &ppattern(10.0, 4.0, FinishPattern::MorphSpiral)).expect("sheet morph");
    let s = spiral(&f, &ppattern(10.0, 4.0, FinishPattern::Spiral)).expect("sheet spiral");
    let (mp, sp) = (turn_pitch(&march_stitched(&m, step), 0.0), turn_pitch(&march_stitched(&s, step), 0.0));
    assert!(mp.len() >= 2 && sp.len() >= 2, "the sheet turned too little to read: {mp:?} / {sp:?}");
    // The morph's first turn crosses the sheet's rim (the march pays
    // nothing for air - the trim law, not the spacing law), so its
    // pitch is read from the second boundary in. The spiral starts
    // INSIDE the rim and needs no such trimming.
    let mp: Vec<f64> = mp.into_iter().skip(1).collect();
    let want_m = step * mean_loc;
    let want_s = step * (PI / 6.0).cos();
    assert!(
        mp.iter().all(|d| (d - want_m).abs() <= 0.06),
        "morph turn advance left the mean of its local law: {mp:?} (want {want_m:.4})"
    );
    assert!(
        sp.iter().all(|d| (d - want_s).abs() <= 0.06),
        "spiral turn advance left the worst-tilt law: {sp:?} (want {want_s:.4})"
    );
    assert!(want_m > want_s + 0.02, "law setup lost: mean {want_m} vs worst {want_s}");
    let mut qm = ppattern(10.0, 4.0, FinishPattern::MorphSpiral);
    qm.stepover = Some(2.4);
    let mut qs = qm;
    qs.pattern = FinishPattern::Spiral;
    let m2 = morphed_spiral(&f, &qm).expect("authored morph");
    let s2 = spiral(&f, &qs).expect("authored spiral");
    let mp2: Vec<f64> = turn_pitch(&march_stitched(&m2, 2.4), 0.0).into_iter().skip(1).collect();
    let sp2 = turn_pitch(&march_stitched(&s2, 2.4), 0.0);
    assert!(
        mp2.iter().all(|d| (d - 2.4 * mean_loc).abs() <= 0.07),
        "an authored stepover stopped paying the slope locally: {mp2:?}"
    );
    assert!(
        sp2.iter().all(|d| (d - 2.4).abs() <= 0.07),
        "the authored number stopped being the authored number: {sp2:?}"
    );
}


#[test]
fn the_morphed_spiral_widens_turns_where_the_cap_relaxes() {
    // The local law measured on a gentle cap: tilt runs 0 at the
    // apex to 30 degrees at the rim, and every facet stays inside
    // the raster's jurisdiction. The march breaks at the tessellation's
    // crests like anywhere else a plane law meets a fold (the dome
    // test states that law; chord sag here is ~0.02, a fifth of the
    // pattern's tol) - what survives between the breaks is the curve,
    // and the curve carries the spacing law verbatim.
    // The spiral pays the footprint's WORST tilt once (constant
    // step*cos(30)); the morph pays it WHERE IT STANDS - so its
    // turns run at the spiral's number at the rim, widen turn by
    // turn as the cap relaxes, and reach the flat-ground step by
    // the apex. And no turn ever narrows past the 45-degree line's
    // spacing - the clamp's floor, which the plane-tangent law
    // honors by construction (g <= 1 wherever it answers).
    let f = sphere_cap(20.0, PI / 6.0, 12, 96);
    let step = scallop_stepover(R, TOL);
    let m = morphed_spiral(&f, &ppattern(24.0, -5.0, FinishPattern::MorphSpiral)).expect("cap morph");
    let s = spiral(&f, &ppattern(24.0, -5.0, FinishPattern::Spiral)).expect("cap spiral");
    // Read PER FRAGMENT, not per stitched turn: the cap's ring crests
    // break the march at every fold (the strict ground's own law - the
    // dome test below is its statement), so no single piece winds a
    // whole turn. The fragments sit ON the curve, and that is exactly
    // where the spacing law lives: each piece's radius advance over
    // its angle advance IS the `r(theta)` that built it.
    let mut mf: Vec<(f64, f64)> = fragment_pitch(&m, step).into_iter().skip(1).collect();
    let sp: Vec<f64> = fragment_pitch(&s, step).into_iter().map(|(_, d)| d).collect();
    assert!(mf.len() >= 24 && sp.len() >= 24, "too few fragments read: {} / {}", mf.len(), sp.len());
    let want_s = step * (PI / 6.0).cos();
    assert!(
        sp.windows(2).all(|w| (w[1] - w[0]).abs() <= 0.05),
        "the spiral's global worst-tilt step drifted turn to turn: {sp:?}"
    );
    // The morph pays the tilt WHERE IT STANDS - and where it stands
    // is read from the radius a piece was cut at, not its place in the
    // file: the table's sixteen nodes per turn quantize the skin at
    // the band lines, and a piece straddling one carries whichever
    // facet owns its ground. The promise is per band: the mean of a
    // band's pitches is the flat step narrowed by that band's own
    // tilt, and no band's mean sits under the band outside it.
    mf.sort_by(|a, b| b.0.total_cmp(&a.0)); // rim first
    let mut means: Vec<f64> = Vec::new();
    let (mut band, mut sum, mut n) = (-1i64, 0.0f64, 0usize);
    for (r, d) in &mf {
        let b = (r / 20.0).min(1.0).asin() * (72.0 / PI);
        let b = b.floor().clamp(0.0, 11.0) as i64;
        if b != band {
            if n > 0 {
                means.push(sum / n as f64);
            }
            band = b;
            sum = 0.0;
            n = 0;
        }
        sum += d;
        n += 1;
    }
    if n > 0 {
        means.push(sum / n as f64);
    }
    assert!(means.len() >= 8, "the cap's bands barely spoke: {mf:?}");
    assert!(
        means.windows(2).all(|w| w[1] >= w[0] - 0.01),
        "the morph tightened where the cap relaxes: {means:?}"
    );
    assert!(
        (means[0] - want_s).abs() <= 0.05,
        "at the shared 30-degree rim the local law left the global one: morph {means:?} spiral {want_s:.4}"
    );
    assert!(
        (*means.last().unwrap() - step).abs() <= 0.06 * step,
        "near the apex the morph still paid a slope it outgrew: {means:?} (flat {step:.4})"
    );
    assert!(
        mf.iter().all(|(_, d)| *d >= step / std::f64::consts::SQRT_2 - 0.03),
        "a fragment narrowed past the 45-degree jurisdiction floor: {mf:?}"
    );
}

#[test]
fn the_morphed_spiral_marches_a_dome_and_reports_where_it_cannot_rest() {
    // A tessellated dome's ring crests straddle the tangent law:
    // where the contact circle crosses a fold, no facet plane
    // owns a rest and the march breaks. The path that stands is
    // honest (every leg certified) and the uncut bands are
    // REPORTED - a note counts them, never silence. (The spacing
    // law itself speaks on contiguous ground; the cone test above
    // is its jurisdiction proof. Full offset-surface support - a
    // rest on the fold itself - is M7.1's brick-SDF territory.)
    let f = dome();
    let step = scallop_stepover(R, TOL);
    let m = morphed_spiral(&f, &ppattern(24.0, -5.0, FinishPattern::MorphSpiral)).expect("dome morph");
    assert!(m.all_finite(), "the dome morph left the finite horizon");
    let feed = m.moves.iter().filter(|mv| matches!(mv.kind, MoveKind::Feed)).count();
    assert!(feed > 500, "the dome morph barely marched: {feed} feed moves");
    let hole = m.notes.iter().any(|n| n.contains("no facet plane rests a ball"));
    assert!(hole, "a dome march with holes went unreported: {:?}", m.notes);
    // What the march DID read still narrows: turn pitches inside
    // the contiguous arcs sit between the clamp and the flat law.
    let mp = turn_pitch(&march_stitched(&m, step), 0.0);
    assert!(
        mp.iter().all(|d| (step / std::f64::consts::SQRT_2 - 0.05..=step + 0.05).contains(d)),
        "a readable dome turn left the band the clamp and flat ground allow: {mp:?}"
    );
}

#[test]
fn the_morphed_spiral_falls_through_to_the_rings_on_all_steep_ground() {
    // No facet shallower than the 45-degree line, no footprint, no
    // spiral: the morph's answer is the rings' answer, byte for byte
    // - the same fall-through the spiral already made, shared
    // through rings_and_tail. (A 60-degree cone wall does NOT
    // qualify - its normal stands only 30 from vertical, inside the
    // equality line. Only a true wall has no footprint at all.)
    let f = steep_wall();
    let m = morphed_spiral(&f, &ppattern(30.0, 0.0, FinishPattern::MorphSpiral)).expect("cone morph");
    let s = spiral(&f, &ppattern(30.0, 0.0, FinishPattern::Spiral)).expect("cone spiral");
    assert_eq!(m.moves, s.moves, "the fall-through grew a geometry of its own");
    assert!(n_kind(&m, MoveKind::Feed) > 0, "and the rings went silent");
}

#[test]
fn the_stage_c_trio_is_deterministic() {
    // The heap, the buckets and the welded chain all fix their order
    // by keys, not by HashMap iteration: the same facets twice write
    // the same path twice.
    let dome = open_dome();
    let pocket = pocket_open();
    let sheet = tilted(PI / 6.0, 8.0);
    assert_eq!(
        geodesic(&dome, &ppattern(24.0, -5.0, FinishPattern::Geodesic)).expect("a").moves,
        geodesic(&dome, &ppattern(24.0, -5.0, FinishPattern::Geodesic)).expect("b").moves,
        "the dome field churned"
    );
    assert_eq!(
        geodesic(&pocket, &ppattern(10.5, -1.0, FinishPattern::Geodesic)).expect("a").moves,
        geodesic(&pocket, &ppattern(10.5, -1.0, FinishPattern::Geodesic)).expect("b").moves,
        "the pocket field churned"
    );
    assert_eq!(
        morphed_spiral(&sheet, &ppattern(10.0, 4.0, FinishPattern::MorphSpiral)).expect("a").moves,
        morphed_spiral(&sheet, &ppattern(10.0, 4.0, FinishPattern::MorphSpiral)).expect("b").moves,
        "the morph table re-rolled"
    );
}

#[test]
#[ignore = "verdict lane: envelope scans over the M2.5 C fixtures - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn geodesic_dome_is_envelope_clean() {
    // The continuity story's price tag: a field-marched ring set keeps
    // BOTH promises over the whole skin - nothing bitten, nothing
    // left standing beyond the cusp budget.
    let f = open_dome();
    let t = geodesic(&f, &ppattern(24.0, -5.0, FinishPattern::Geodesic)).expect("dome rings");
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "gouged {} mm at {:?}", rep.gouge_mm, rep.at);
    assert!(rep.rest_mm <= gate(rep.cell), "left {} mm of rest at {:?}", rep.rest_mm, rep.at);
}

#[test]
#[ignore = "verdict lane: envelope scan over the open pocket - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn geodesic_pocket_bites_nothing_and_owns_the_corner() {
    // The corner band is the crease pair's jurisdiction: the geodesic
    // may leave it standing (rest is REPORTED, the note counts it) -
    // what it may never do is bite the corner from the inside.
    let f = pocket_open();
    let t = geodesic(&f, &ppattern(10.5, -1.0, FinishPattern::Geodesic)).expect("pocket rings");
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "bit the corner: {} mm at {:?}", rep.gouge_mm, rep.at);
    assert!(rep.rest_mm > 0.0, "and the note lied - there was nothing to report");
}

#[test]
#[ignore = "verdict lane: envelope scan over the dome morph - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn morphed_spiral_dome_bites_nothing() {
    let f = open_dome();
    let t = morphed_spiral(&f, &ppattern(24.0, -5.0, FinishPattern::MorphSpiral)).expect("dome morph");
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "the widened turns bit: {} mm at {:?}", rep.gouge_mm, rep.at);
}

// M2.5 D. Blend: the transition band ridden from BOTH sides. The
// raster is Parallel's, untouched; the ring ladder is asked one new
// question - does a shallow tangency ALSO reach past thirty degrees?
// In the band between the two lines the ring's own cusp promise is
// at most 4/3 of the ladder's budget (BLEND_MZ's note), so cutting
// the band twice is a shave, not a waste: the staircase the 45-degree
// border leaves where the march and the ladder fail to meet goes
// away. Where the ground has no transition, the band says nothing -
// and says it byte for byte, the twin law of every M2.5 kernel.

#[test]
fn the_blend_rides_the_band_the_split_drops() {
    // The 38-degree sheet: shallow by the flag (cos 38 = .788 >
    // cos 45) and past the band line (.788 < cos 30 = .866). The
    // split sends loops that graze only the sheet to the march;
    // the Blend rides them. Parallel filters nothing, so the three
    // feed counts stand in the law's own order: par >= blend > ss.
    let f = tilted(38f64.to_radians(), 6.0);
    let par = parallel(&f, &ppattern(14.7, 4.0, FinishPattern::Parallel)).expect("band parallel");
    let ss = parallel(&f, &ppattern(14.7, 4.0, FinishPattern::SteepShallow)).expect("band split");
    let bl = blend(&f, &ppattern(14.7, 4.0, FinishPattern::Blend)).expect("band blend");
    let n = |t: &Toolpath| n_kind(t, MoveKind::Feed);
    assert!(n(&bl) > n(&ss), "the band rode nothing: {} vs {}", n(&bl), n(&ss));
    assert!(
        n(&bl) <= n(&par),
        "the band invented geometry the unfiltered ladder never drew: {} vs {}",
        n(&bl),
        n(&par)
    );
    // The plunge ledger keeps the order too: every extra run is
    // entered the way every other run is.
}

#[test]
fn the_band_says_nothing_where_there_is_no_transition() {
    // Steep-and-flat ground: the box. Flats are not band (their
    // normals stand at m.z 1, past cos 30), walls always rode - so
    // the Blend's path IS the split's, move for move. And on
    // all-steep ground the filter admits every loop whole, which is
    // the unfiltered geometry itself: the cone's Blend is its
    // Parallel, byte for byte.
    let b = box_facets();
    let ss =
        parallel(&b, &ppattern(10.0, 0.0, FinishPattern::SteepShallow)).expect("box split");
    let bl = blend(&b, &ppattern(10.0, 0.0, FinishPattern::Blend)).expect("box blend");
    assert_eq!(bl.moves, ss.moves, "the band spoke on ground with no transition");
    let c = steep_cone(48);
    let par = parallel(&c, &ppattern(30.0, 0.0, FinishPattern::Parallel)).expect("cone parallel");
    let bl = blend(&c, &ppattern(30.0, 0.0, FinishPattern::Blend)).expect("cone blend");
    assert_eq!(bl.moves, par.moves, "a filter with nothing to drop repainted the path");
}

#[test]
fn the_blend_is_deterministic_and_refuses_nonsense_by_the_shared_door() {
    // The band's flag is a predicate, not a scheduler: the same
    // facets twice write the same path twice. And the degeneracy
    // answers ride `validate` and the support build, the same door
    // every pattern walks: a NaN facet refuses by name, dust answers
    // Ok-or-Err (the tangent circle about a point is honest
    // geometry - `degenerate_meshes_never_panic`'s law), never a
    // panic.
    let f = tilted(38f64.to_radians(), 6.0);
    assert_eq!(
        blend(&f, &ppattern(14.7, 4.0, FinishPattern::Blend)).expect("a").moves,
        blend(&f, &ppattern(14.7, 4.0, FinishPattern::Blend)).expect("b").moves,
        "the band's run order wavered"
    );
    let dust = vec![Triangle::new(p3(1.0, 1.0, 1.0), p3(1.0, 1.0, 1.0), p3(1.0, 1.0, 1.0))];
    let r = blend(&dust, &ppattern(1.0, -1.0, FinishPattern::Blend));
    assert!(r.is_ok() || r.is_err(), "dust answered with a panic");
    let nan = vec![Triangle::new(p3(f64::NAN, 0.0, 0.0), p3(1.0, 0.0, 0.0), p3(0.0, 1.0, 0.0))];
    assert!(
        blend(&nan, &ppattern(1.0, -1.0, FinishPattern::Blend)).is_err(),
        "a NaN facet made geometry"
    );
}

#[test]
#[ignore = "verdict lane: envelope scan over the 38-degree band sheet - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn blend_band_sheet_is_envelope_clean() {
    // The double cut's promise: riding the band from both sides
    // bites nothing - the rings stay tangent law, the raster stays
    // the march law - and leaves nothing standing past the cusp
    // budget anywhere on the sheet, walls or floors.
    let f = tilted(38f64.to_radians(), 8.0);
    let t = blend(&f, &ppattern(14.7, 4.0, FinishPattern::Blend)).expect("band blend");
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "gouged {} mm at {:?}", rep.gouge_mm, rep.at);
    assert!(rep.rest_mm <= gate(rep.cell), "left {} mm of rest at {:?}", rep.rest_mm, rep.at);
}

// M2.5 D. Morph: the ladder's levels joined by ruled lofts. The rings
// ride exactly as the waterline places them - Morph is neither steep
// nor band in `emit_rings`, so the single-level ladder is the
// waterline path byte for byte, the twin law again - and between each
// pair of consecutive levels whose loops pair (same area rank, the
// centroids no further apart than a tool diameter) one interpolated
// guide is marched, its XY the loop-to-loop ruling's midpoint and its
// Z the column's own answer, never the interpolation's. Halving the
// wall's spacing quarters the cusp the ladder's promise bought.

#[test]
fn the_single_level_morph_is_the_waterline() {
    // One level, no gap, nothing to loft: the morph IS the ladder,
    // move for move, on a box and on a cone. The identity is exact
    // because the ring part is the same `emit_rings` walk (Morph
    // trips neither the steep nor the band flag) and the retract that
    // closes it is the waterline's own.
    // One level (top == bottom), inside each fixture's own reach: no
    // gap, nothing to loft.
    for (f, z) in [
        (box_facets(), 5.0),
        (steep_cone(48), 15.0),
        (tilted(30f64.to_radians(), 6.0), 10.0),
    ] {
        let wl = waterline(&f, &ppattern(z, z, FinishPattern::Waterline)).expect("one-level waterline");
        let mo = morph(&f, &ppattern(z, z, FinishPattern::Morph)).expect("one-level morph");
        assert_eq!(
            mo.moves, wl.moves,
            "with no gap to loft the morph repainted the ladder"
        );
    }
}

#[test]
fn the_morph_lays_a_loft_in_every_paired_gap() {
    // A real ladder on the 30-degree sheet: many level pairs, each a
    // wall strip the ruling can join (centroids drift by
    // dz·cot(alpha) per level, well inside the 2r gate). The morph's
    // feed count stands STRICTLY above the waterline's - the lofts
    // are extra cut lines, never replacements.
    let f = tilted(30f64.to_radians(), 6.0);
    let wl = waterline(&f, &ppattern(14.0, 4.0, FinishPattern::Waterline)).expect("wl");
    let mo = morph(&f, &ppattern(14.0, 4.0, FinishPattern::Morph)).expect("morph");
    let n = |t: &Toolpath| n_kind(t, MoveKind::Feed);
    assert!(
        n(&mo) > n(&wl),
        "the lofts cut nothing: morph {} feeds vs waterline {}",
        n(&mo),
        n(&wl)
    );
}

#[test]
fn the_morph_is_deterministic_and_refuses_nonsense_by_the_shared_door() {
    // The pairing is a predicate on facts (area rank, centroid), not
    // a scheduler: the same facets twice write the same path twice.
    // And degeneracy answers at `validate`'s door like every pattern:
    // a NaN facet refuses by name, dust answers Ok-or-Err (the
    // tangent circle about a point is honest geometry), never a panic.
    let f = tilted(30f64.to_radians(), 6.0);
    assert_eq!(
        morph(&f, &ppattern(14.0, 4.0, FinishPattern::Morph)).expect("a").moves,
        morph(&f, &ppattern(14.0, 4.0, FinishPattern::Morph)).expect("b").moves,
        "the loft pairing wavered between runs"
    );
    let dust = vec![Triangle::new(p3(1.0, 1.0, 1.0), p3(1.0, 1.0, 1.0), p3(1.0, 1.0, 1.0))];
    let r = morph(&dust, &ppattern(1.0, -1.0, FinishPattern::Morph));
    assert!(r.is_ok() || r.is_err(), "dust answered with a panic");
    let nan = vec![Triangle::new(p3(f64::NAN, 0.0, 0.0), p3(1.0, 0.0, 0.0), p3(0.0, 1.0, 0.0))];
    assert!(
        morph(&nan, &ppattern(1.0, -1.0, FinishPattern::Morph)).is_err(),
        "a NaN facet made geometry"
    );
}

#[test]
#[ignore = "verdict lane: envelope scan over the 30-degree loft sheet - minutes in debug, seconds in release; run at milestone close with -- --ignored"]
fn morph_loft_sheet_is_envelope_clean() {
    // The loft's promise, measured: grounding every interpolated
    // sample on the column (never the XY ruling's implied Z) means the
    // extra guides bite nothing and the quartered cusp leaves nothing
    // standing past the budget on the wall strips they join.
    let f = tilted(30f64.to_radians(), 8.0);
    let t = morph(&f, &ppattern(14.0, 4.0, FinishPattern::Morph)).expect("loft morph");
    let rep = envelope(&f, &t);
    assert!(rep.gouge_mm <= gate(rep.cell), "gouged {} mm at {:?}", rep.gouge_mm, rep.at);
    assert!(rep.rest_mm <= gate(rep.cell), "left {} mm of rest at {:?}", rep.rest_mm, rep.at);
}

// ---------- M2.5 D3: the drape - Project walks the part's OWN edges ----------

/// A tilted open sheet with a square through-hole: the plane
/// z = 10 − (x + y)·tan20°/√2 over [−10,10]² with [−3,3]² cut out, as
/// eight coplanar quads (24 triangles, welded at exact shared
/// corners). The tilt runs along the DIAGONAL, so the shadow's shift
/// r·m_xy is nonzero in both components: the silhouette rim and the
/// part's own rim fall on different ground, and which line the path
/// keeps is the drape's signature. The sheet's open edges - twelve
/// outer runs, four about the hole - chain into exactly two CLOSED
/// loops; nothing here stands open.
fn holed_sheet() -> Vec<Triangle> {
    let k = 20f64.to_radians().tan() * std::f64::consts::FRAC_1_SQRT_2;
    let surf = |x: f64, y: f64| 10.0 - (x + y) * k;
    let g = [-10.0, -3.0, 3.0, 10.0];
    let mut facets = Vec::new();
    for (ix, iy) in [
        (0, 0), (1, 0), (2, 0), (0, 1), (2, 1), (0, 2), (1, 2), (2, 2),
    ] {
        let (x0, x1) = (g[ix], g[ix + 1]);
        let (y0, y1) = (g[iy], g[iy + 1]);
        let q = |x: f64, y: f64| p3(x, y, surf(x, y));
        facets.push(Triangle::new(q(x0, y0), q(x1, y0), q(x1, y1)));
        facets.push(Triangle::new(q(x0, y0), q(x1, y1), q(x0, y1)));
    }
    facets
}

#[test]
fn the_drape_walks_the_parts_own_rims_not_their_shadows() {
    // D3's whole claim, in two counts and one exact z: feeds stand on
    // the model's own edge LINES - the outer rim's down-slope edges
    // and the hole's x = -3 edge - where a chord between two rim
    // vertices is exact (the march refines along the line, never off
    // it). The silhouette's counterparts sit at y = 10 + r·m_y,
    // strictly aside. Up-slope edges are the law's honest refusals: a
    // ball resting there would stand its CONTACT in air, so the march
    // opens the loop there - a break, counted by geometry, never a
    // chord dragged through air. And the rim sample's height is the
    // COLUMN'S answer: the tangent center over the shifted contact
    // q = rim − r·m, minus the radius - not a shadow value, not an
    // average, just where a ball resting on the sheet has its tip.
    let f = holed_sheet();
    let t = project(&f, &ppattern(14.0, 4.0, FinishPattern::Project)).expect("project the holed sheet");
    let feeds = t.moves.iter().filter(|m| m.kind == MoveKind::Feed);
    let on_outer = feeds.clone().filter(|m| (m.to.y - 10.0).abs() < 1e-9).count();
    assert!(on_outer >= 4, "the outer rim was never draped: {on_outer} samples on its own line");
    let on_hole = feeds
        .clone()
        .filter(|m| (m.to.x + 3.0).abs() < 1e-9 && m.to.y.abs() < 3.0)
        .count();
    assert!(on_hole >= 2, "the hole's rim was never draped: {on_hole} samples on its own line");
    // The grounded z of a rim sample: tip = plane(q) + r·m_z − r.
    let k = 20f64.to_radians().tan() * std::f64::consts::FRAC_1_SQRT_2;
    let inv = 1.0 / (1.0 + 2.0 * k * k).sqrt();
    let s = R * k * inv;
    let sample = t
        .moves
        .iter()
        .find(|m| m.kind == MoveKind::Feed && (m.to.y - 10.0).abs() < 1e-9)
        .expect("an outer-rim drape sample");
    let grounded = 10.0 - (sample.to.x - s + (10.0 - s)) * k + R * inv - R;
    assert!(
        (sample.to.z - grounded).abs() < 1e-6,
        "the rim stood on the shadow's height, not its own column: {} vs {grounded}",
        sample.to.z
    );
    assert!(
        t.notes.iter().all(|n| !n.contains("stand open")),
        "a cleanly closed chain reported a crack: {:?}",
        t.notes
    );
    let again = project(&f, &ppattern(14.0, 4.0, FinishPattern::Project)).expect("again");
    assert_eq!(
        t.moves, again.moves,
        "two drape passes, two paths - a stamp would churn forever"
    );
}

/// box_facets' six quads with the bottom split along its true
/// diagonal - a manifold with no open edge at all, which is what the
/// drape must fall silent on. (The fixture box below this family
/// names is NOT one: its bottom is a bowtie quad, and the drape is
/// the first code in this repo to say so.)
fn closed_box() -> Vec<Triangle> {
    let q = |sx: f64, sy: f64, z: f64| p3(sx * 10.0, sy * 10.0, z);
    let (a, b, c, d) = (q(-1.0, -1.0, 0.0), q(1.0, -1.0, 0.0), q(1.0, 1.0, 0.0), q(-1.0, 1.0, 0.0));
    let (e, f, g, h) = (q(-1.0, -1.0, 10.0), q(1.0, -1.0, 10.0), q(1.0, 1.0, 10.0), q(-1.0, 1.0, 10.0));
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let mut facets = Vec::new();
    facets.extend(quad([e, f, g, h])); // top
    facets.extend(quad([a, d, c, b])); // bottom, wound opposite the top, split true
    facets.extend(quad([a, b, f, e])); // sides
    facets.extend(quad([b, c, g, f]));
    facets.extend(quad([c, d, h, g]));
    facets.extend(quad([d, a, e, h]));
    facets
}

#[test]
fn a_closed_solid_hears_nothing_from_the_drape() {
    // The generalization's safety law: a closed manifold has no open
    // edge, so the boundary chain finds no loop, the drape answers
    // before a single sample, and Project's notes stay silent on both
    // drape branches - which is why every golden of a solid model
    // rides this change byte for byte (the gate's porcelain check is
    // the proof; this is the promise).
    let fixtures: Vec<(&str, Vec<Triangle>)> = vec![
        ("closed box", closed_box()),
        ("wedge", tilted(PI / 6.0, 8.0)),
        ("domed cap", sphere_cap(12.0, PI / 4.0, 12, 24)), // closed with its disc
    ];
    for (name, f) in fixtures {
        let t = project(&f, &ppattern(14.0, -2.0, FinishPattern::Project));
        let Ok(t) = t else { continue }; // a refusal is an answer too
        assert!(
            t.notes.iter().all(|n| !n.contains("stand open") && !n.contains("drape")),
            "{name} heard something from the drape: {:?}",
            t.notes
        );
    }
}

#[test]
fn a_crack_is_named_and_never_draped_as_if_closed() {
    // The refusal-by-count branch runs on REAL garbage: this repo's
    // own box fixture hides a bowtie bottom - three facet edges stand
    // open, and their chain dangles a run no ball can follow to a
    // closing sample. The drape says so once, drapes nothing of it,
    // and the silhouette and rings carry on untouched - a crack
    // reported, never cut as if it were a rim.
    let f = box_facets();
    let t = project(&f, &ppattern(14.0, -2.0, FinishPattern::Project)).expect("project the cracked-bottom box");
    assert!(
        t.notes
            .iter()
            .any(|n| n.contains("stand open") && n.contains("1 boundary run")),
        "the bowtie bottom's open chain went unreported: {:?}",
        t.notes
    );
    // And nothing stands on the open diagonals: had the run been
    // draped, its samples would answer through the flat top's tangent
    // - tip z = 10 at points strictly inside the footprint, on the
    // lines x = y and x = -y. A silhouette rim or ring point always
    // sits on |x| = 10, |y| = 10 or a |x + y| level line, never here.
    assert!(
        t.moves.iter().all(|m| {
            m.kind != MoveKind::Feed
                || (m.to.x - m.to.y).abs() > 1e-9
                || m.to.x.abs() >= 9.9
                || (m.to.z - 10.0).abs() > 1e-6
        }),
        "an open run was draped as if closed: {:?}",
        t.moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed
                && (m.to.x - m.to.y).abs() < 1e-9
                && m.to.x.abs() < 9.9
                && (m.to.z - 10.0).abs() < 1e-6)
            .map(|m| (m.to.x, m.to.y, m.to.z))
            .take(4)
            .collect::<Vec<_>>()
    );
}

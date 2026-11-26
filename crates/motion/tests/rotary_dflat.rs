//! Issue #54: the rotary wrap on a bar that is not a pure revolve - a
//! D-flat (a planar flat ground part of the way round the bar). Specced
//! from the wrap/development construction in `multiaxis::rotary_offset`
//! (a closed solid develops to a sheet whose only free edges are the
//! seam's two copies) and plain solid-modelling topology (a closed
//! 2-manifold has no free edge); no vendor material consulted.
//!
//! Read here: a D-flat bar wraps in every placement (flat through the
//! ends, a closed flat) and its Offset-rings rough keeps every cut tip out
//! of the part's true cross-section. A flat across the DEFAULT seam, on or
//! off its centre line, used to be refused by name (PR #210's review:
//! wrapped there, it gouged to the axis); since #226 the wrap turns its
//! seam to an azimuth where the bar is turned, and refuses by name only a
//! feature all the way round. A bar with a facet missing is refused by
//! name.
//!
//! #228: a plain bar tessellated as two rings wraps in every
//! orientation (its axis fit is exact, not tilted by rounding), and a
//! seam refusal names a tessellation cause as well as a feature.
//!
//! Gate map (M4.4): covers camcraft_motion::multiaxis (rotary_offset,
//! rotary_seam, rotary_seam_angle, rotary_sheet, axis, axis_refine), on
//! this suite's own prism fixtures.
//! Lane shape (M4.4): every wrap here is Offset rings or a refusal;
//! the suite runs in about a minute in debug.

use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::cl::MoveKind;
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams};
use camcraft_motion::multiaxis::{axis, rotary_offset_developed, rotary_offset_in, AxisControl};

const TAU: f64 = std::f64::consts::TAU;
const RAD: f64 = 6.0;

/// One cross-section ring of `n` points at station `z`: the circle of
/// radius RAD, or (with `flat`) the D its flat leaves - every point
/// past the flat's plane (distance `d` along the direction `phi`)
/// pushed back onto it.
fn ring(n: usize, z: f64, flat: Option<(f64, f64)>) -> Vec<Point3> {
    (0..n)
        .map(|j| {
            let th = TAU * j as f64 / n as f64;
            let (mut x, mut y) = (RAD * th.cos(), RAD * th.sin());
            if let Some((phi, d)) = flat {
                let (nx, ny) = (phi.cos(), phi.sin());
                let over = x * nx + y * ny - d;
                if over > 0.0 {
                    x -= over * nx;
                    y -= over * ny;
                }
            }
            Point3::new(x, y, z)
        })
        .collect()
}

fn same(a: Point3, b: Point3) -> bool {
    (a - b).dot(a - b) < 1e-24
}

fn tri(f: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3) {
    if !(same(a, b) || same(b, c) || same(c, a)) {
        f.push(Triangle::new(a, b, c));
    }
}

/// A closed prism through the rings in order (consecutive rings joined
/// by quads - a zero-length rung collapses its quad to the triangle it
/// is), capped by fans from the axis at both ends. Outward winding.
fn prism(rings: &[Vec<Point3>]) -> Vec<Triangle> {
    let mut f = Vec::new();
    for w in rings.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let n = a.len();
        for j in 0..n {
            let k = (j + 1) % n;
            tri(&mut f, a[j], a[k], b[k]);
            tri(&mut f, a[j], b[k], b[j]);
        }
    }
    let (Some(lo), Some(hi)) = (rings.first(), rings.last()) else { return f };
    let n = lo.len();
    let (c0, c1) = (Point3::new(0.0, 0.0, lo[0].z), Point3::new(0.0, 0.0, hi[0].z));
    for j in 0..n {
        let k = (j + 1) % n;
        tri(&mut f, c0, lo[k], lo[j]);
        tri(&mut f, c1, hi[j], hi[k]);
    }
    f
}

/// A bar of radius RAD over stations `z0..z1`, `n` round, with a D-flat
/// at depth `d` along `phi`: `None` for flat through both ends, else the
/// flat ground over the stations `lo..hi` only, round either side.
fn dflat_bar(n: usize, z0: f64, z1: f64, phi: f64, d: f64, span: Option<(f64, f64)>) -> Vec<Triangle> {
    let fl = Some((phi, d));
    match span {
        None => prism(&[ring(n, z0, fl), ring(n, z1, fl)]),
        Some((lo, hi)) => prism(&[
            ring(n, z0, None),
            ring(n, lo, None),
            ring(n, lo, fl),
            ring(n, hi, fl),
            ring(n, hi, None),
            ring(n, z1, None),
        ]),
    }
}

/// A coaxial round bar stock of radius `rho` over `z0..z1`.
fn stock(rho: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    prism(&[ring(64, z0, None), ring(64, z1, None)])
        .into_iter()
        .map(|t| {
            let s = |p: Point3| Point3::new(p.x * rho / RAD, p.y * rho / RAD, p.z);
            Triangle::new(s(t.a), s(t.b), s(t.c))
        })
        .collect()
}

/// The wrap kind's pocket numbers; `top_z`/`bottom_z` window the
/// stations (the axis stands on world Z).
fn params(strategy: PocketStrategy, d: f64, window: f64) -> Pocket3DParams {
    Pocket3DParams {
        pass_start: Default::default(),
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: d,
        stepover: 0.6 * d,
        stock_to_leave: 0.1,
        clearance_z: 50.0,
        top_z: window,
        bottom_z: -window,
        stepdown: 0.5,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        retract_z: None,
        entry: Entry3D::Helix,
        helix_radius: 0.25 * d,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

fn control() -> AxisControl {
    AxisControl { lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: None }
}

/// Flat placements: +x, the two y half-axes (one of them lies on the
/// wrap's +-pi branch, so its flat crosses the seam), and an odd one.
const PHIS: [f64; 4] = [0.0, std::f64::consts::FRAC_PI_2, -std::f64::consts::FRAC_PI_2, 2.3];

/// The flat across the default seam (the branch the wrap's basis puts at
/// -y for this fixture's fitted axis): the placement refused before #226.
const SEAM_PHI: f64 = -std::f64::consts::FRAC_PI_2;

fn feeds(t: &camcraft_motion::cl::Toolpath) -> impl Iterator<Item = &camcraft_motion::cl::Move> {
    t.moves.iter().filter(|m| m.kind == MoveKind::Feed)
}

/// The name the wrap refuses a feature across its seam by.
const SEAM_REFUSAL: &str = "crosses the rotary wrap's seam";

/// Is `q` inside the part by more than `margin`: inside the TRUE
/// cross-section polygon of its station (the D over the flat's
/// stations, the round elsewhere) of a bar over `z0..z1`?
fn in_part(q: Point3, n: usize, phi: f64, d: f64, span: Option<(f64, f64)>, z0: f64, z1: f64) -> bool {
    let flat_here = span.is_none_or(|(lo, hi)| q.z >= lo && q.z <= hi);
    let sect = ring(n, 0.0, if flat_here { Some((phi, d)) } else { None });
    q.z > z0 && q.z < z1 && deep_inside(&sect, q, 0.05)
}

/// Every placement of a D-flat wraps: a flat ground through both ends
/// (r5.8) and one closed over the middle stations (r5), at four azimuths,
/// the default seam's own included (#226: refused by name before, the
/// wrap now turns its seam clear of the flat). The closed one's channel
/// is roughed under the round's skin AT THE FLAT'S AZIMUTH (the turned
/// seam moves where the development splits, never where metal is cut),
/// and no cut tip stands inside the part.
#[test]
fn a_d_flat_bar_wraps_in_every_placement() {
    let st = stock(6.5, -20.0, 20.0);
    let p = params(PocketStrategy::OffsetRings, 2.0, 12.0);
    for (name, d, span) in [("through", 5.8, None), ("closed", 5.0, Some((-8.0, 8.0)))] {
        for phi in PHIS {
            let f = dflat_bar(64, -20.0, 20.0, phi, d, span);
            let t = rotary_offset_in(&f, Some(&st), &p, &control()).unwrap_or_else(|e| panic!("{name} flat at phi {phi}: {e}"));
            assert!(t.all_finite(), "{name} flat at phi {phi}: a non-finite move");
            let cuts = feeds(&t).count();
            assert!(cuts > 50, "{name} flat at phi {phi}: only {cuts} cut moves");
            for m in feeds(&t) {
                assert!(
                    !in_part(m.to, 64, phi, d, span, -20.0, 20.0),
                    "{name} flat at phi {phi}: a cut tip at {:?} stands inside the part",
                    m.to
                );
            }
            let low = feeds(&t).map(|m| m.to.x.hypot(m.to.y)).fold(f64::INFINITY, f64::min);
            if span.is_some() {
                assert!(low < 5.6, "{name} flat at phi {phi}: the flat's channel is not roughed (lowest tip r{low:.3})");
                // Every tip down in the channel stands over the flat: its
                // posted azimuth is the flat's true one, not a seam turn off.
                let half = (d / RAD).acos();
                for m in feeds(&t).filter(|m| m.to.x.hypot(m.to.y) < 5.6) {
                    let off = (m.to.y.atan2(m.to.x) - phi).rem_euclid(TAU);
                    let off = off.min(TAU - off);
                    assert!(off < half, "{name} flat at phi {phi}: a channel tip at {:?} stands {off:.3} rad off the flat", m.to);
                }
            }
        }
    }
}

/// A closed mesh wraps; one with a facet missing from its skin is
/// refused by name before the wrap - even where no level of the ladder
/// would ever slice through the hole.
#[test]
fn a_bar_with_a_hole_in_its_skin_is_still_refused_by_name() {
    let mut f = dflat_bar(64, -20.0, 20.0, 0.0, 5.0, Some((-8.0, 8.0)));
    let Some(i) = f.iter().position(|t| {
        let (cx, cz) = ((t.a.x + t.b.x + t.c.x) / 3.0, (t.a.z + t.b.z + t.c.z) / 3.0);
        cx < -5.0 && cz > 10.0 && cz < 19.0
    }) else {
        panic!("no facet to drop")
    };
    f.remove(i);
    let p = params(PocketStrategy::OffsetRings, 2.0, 12.0);
    match rotary_offset_in(&f, Some(&stock(6.5, -20.0, 20.0)), &p, &control()) {
        Ok(_) => panic!("an open mesh wrapped"),
        Err(e) => {
            let msg = e.to_string();
            assert!(msg.contains("the mesh is open"), "refused, but not by name: {msg}");
        }
    }
}

/// The seam case #9 round 2 wanted end to end (a flat's channel across
/// the default seam, under Adaptive and Offset rings) was refused by name
/// before the wrap; since #226 the seam turns clear of the flat and both
/// strategies develop it, the channel cut below the round's skin. #9's
/// seam law itself stays pinned by its unit test
/// (`machining3d::hsm::tests::the_seam_cuts_an_area_at_a_cover_end_short_of_a_lap`).
#[test]
fn a_flat_channel_across_the_default_seam_develops_under_adaptive_and_rings() {
    let f = dflat_bar(64, -40.0, 40.0, SEAM_PHI, 4.0, Some((-8.0, 8.0)));
    for strategy in [PocketStrategy::Adaptive, PocketStrategy::OffsetRings] {
        let p = Pocket3DParams { stepdown: 1.0, ..params(strategy, 4.0, 5.0) };
        let t = rotary_offset_developed(&f, None, &p).unwrap_or_else(|e| panic!("{strategy:?}: {e}"));
        let low = feeds(&t).map(|m| m.to.z).fold(f64::INFINITY, f64::min);
        assert!(low < 5.5, "{strategy:?}: the channel is not developed (lowest level r{low:.3})");
    }
}

/// #226: a bar with a flat at every azimuth - four short flats a quarter
/// turn apart, each over its own band of stations, their spans
/// overlapping (short, so the bar still fits its axis) - is
/// a feature all the way round: no seam angle stands on a turned column
/// pair, and the wrap refuses it by name.
#[test]
fn a_feature_all_the_way_round_is_still_refused_by_name() {
    let n = 64;
    let mut rings = vec![ring(n, -40.0, None)];
    for k in 0..4 {
        let phi = 0.1 + std::f64::consts::FRAC_PI_2 * k as f64;
        let (z0, z1) = (-30.0 + 18.0 * k as f64, -26.0 + 18.0 * k as f64);
        let fl = Some((phi, 4.0));
        rings.extend([ring(n, z0, None), ring(n, z0, fl), ring(n, z1, fl), ring(n, z1, None)]);
    }
    rings.push(ring(n, 40.0, None));
    let f = prism(&rings);
    let p = params(PocketStrategy::OffsetRings, 2.0, 12.0);
    match rotary_offset_in(&f, Some(&stock(6.5, -40.0, 40.0)), &p, &control()) {
        Ok(_) => panic!("a bar flatted all the way round wrapped"),
        Err(e) => {
            let msg = e.to_string();
            assert!(msg.contains(SEAM_REFUSAL) && msg.contains("all the way round"), "refused, but not by name: {msg}");
        }
    }
}

/// Is `q` inside the convex counter-clockwise polygon `ring` by more
/// than `margin` (its distance to every edge's line)?
fn deep_inside(ring: &[Point3], q: Point3, margin: f64) -> bool {
    let n = ring.len();
    (0..n).all(|j| {
        let (a, b) = (ring[j], ring[(j + 1) % n]);
        let (ex, ey) = (b.x - a.x, b.y - a.y);
        let l = ex.hypot(ey);
        l <= 1e-12 || (ex * (q.y - a.y) - ey * (q.x - a.x)) / l > margin
    })
}

/// PR #210's review probe (issue #54): a 3.5 deep flat across the default
/// seam, on and off its centre line (and midway between two vertices).
/// The first round wrapped them on that seam and the cover's ends read
/// other laps' metal as air: 12 to 1606 cut tips inside the part, down to
/// the axis (and a whole-lap band at the flat's end stations, even
/// centred). #210 refused every such placement by name; since #226 the
/// wrap turns its seam clear of the flat, and EVERY placement roughs with
/// no tip inside the TRUE cross-section of its station.
#[test]
fn an_off_centre_flat_across_the_seam_never_puts_a_tip_in_the_part() {
    let st = stock(6.5, -40.0, 40.0);
    let half = std::f64::consts::FRAC_PI_2;
    let mut wrapped = 0usize;
    for n in [16usize, 64] {
        for tool in [2.0, 0.8] {
            let p = params(PocketStrategy::OffsetRings, tool, 12.0);
            for phi in [-half - 0.4, -half + 0.4, -half - 0.7, -half + 0.7, -half, -half + std::f64::consts::PI / n as f64, 0.0, half, 2.3] {
                let f = dflat_bar(n, -40.0, 40.0, phi, 3.5, Some((-8.0, 8.0)));
                let t = rotary_offset_in(&f, Some(&st), &p, &control()).unwrap_or_else(|e| panic!("n {n} D{tool} phi {phi:.3}: {e}"));
                let inside = feeds(&t).filter(|m| in_part(m.to, n, phi, 3.5, Some((-8.0, 8.0)), -40.0, 40.0)).count();
                println!("n {n} D{tool} phi {phi:.3}: {} cut moves, {inside} tips inside", feeds(&t).count());
                assert_eq!(inside, 0, "n {n} D{tool} phi {phi:.3}: cut tips inside the part");
                wrapped += 1;
            }
        }
    }
    assert_eq!(wrapped, 36, "every placement, either tool, either column count, roughs");
}

/// [`ring`] turned `rot` radians about the axis.
fn turned(ring: Vec<Point3>, rot: f64) -> Vec<Point3> {
    let (c, s) = (rot.cos(), rot.sin());
    ring.into_iter().map(|q| Point3::new(q.x * c - q.y * s, q.x * s + q.y * c, q.z)).collect()
}

/// The ring counts and turns #228's review refused a plain bar at: at
/// turn 0 one ring vertex stands on the wrap's +-pi branch.
const PLAIN_CASES: [(usize, f64); 4] = [(16, 0.0), (16, 0.3), (64, 0.0), (64, 0.3)];

/// #228: a plain bar meshed as two rings (long side facets, axis-fan
/// caps, the way CAD exports a cylinder) fits its true axis exactly.
/// Every side facet anchors its normal line at the bar's mid-station,
/// so no line sees a tilt about it, and the refinement used to pick one
/// by rounding (26 microradians at n 16).
#[test]
fn a_two_ring_bar_fits_its_true_axis() {
    for (n, rot) in PLAIN_CASES {
        let f = prism(&[turned(ring(n, -40.0, None), rot), turned(ring(n, 40.0, None), rot)]);
        let ax = axis::fit(&f).expect("a revolve");
        let tilt = ax.dir.x.hypot(ax.dir.y);
        assert!(tilt < 1e-12, "n {n} turn {rot}: the axis leans {tilt:e} rad: {ax:?}");
    }
}

/// #228: the same plain bar wraps in every one of those orientations,
/// with no cut tip inside the part (main refused all but one, naming a
/// feature the bar does not have).
#[test]
fn a_plain_two_ring_bar_wraps_in_every_orientation() {
    let st = stock(6.5, -40.0, 40.0);
    let p = params(PocketStrategy::OffsetRings, 2.0, 12.0);
    for (n, rot) in PLAIN_CASES {
        let f = prism(&[turned(ring(n, -40.0, None), rot), turned(ring(n, 40.0, None), rot)]);
        let t = rotary_offset_in(&f, Some(&st), &p, &control()).unwrap_or_else(|e| panic!("n {n} turn {rot}: {e}"));
        assert!(t.all_finite(), "n {n} turn {rot}: a non-finite move");
        let sect = turned(ring(n, 0.0, None), rot);
        let inside = feeds(&t).filter(|m| m.to.z.abs() < 40.0 && deep_inside(&sect, m.to, 0.05)).count();
        assert!(feeds(&t).count() > 50, "n {n} turn {rot}: too few cuts");
        assert_eq!(inside, 0, "n {n} turn {rot}: cut tips inside the part");
    }
}

/// #228's other case: a stepped shaft whose two diameters were meshed
/// with different column counts (16 on the neck, 64 on the body) puts
/// more than two columns on the seam. It is refused, and the refusal
/// names the tessellation, not only a feature the shaft does not have
/// (wrapping it would also answer the issue, but nothing here pins what
/// such a wrap cuts, so the refusal is what is pinned).
#[test]
fn a_shaft_meshed_with_mixed_column_counts_is_refused_naming_the_tessellation() {
    let neck = |z: f64| ring(16, z, None).into_iter().map(|q| Point3::new(q.x * 4.0 / RAD, q.y * 4.0 / RAD, z)).collect::<Vec<_>>();
    let (n0, n1) = (neck(-40.0), neck(0.0));
    let (b0, b1) = (ring(64, 0.0, None), ring(64, 40.0, None));
    let mut f = Vec::new();
    for (a, b) in [(&n0, &n1), (&b0, &b1)] {
        let n = a.len();
        for j in 0..n {
            let k = (j + 1) % n;
            tri(&mut f, a[j], a[k], b[k]);
            tri(&mut f, a[j], b[k], b[j]);
        }
    }
    // The shoulder at Z 0: four body columns to each neck column, zipped
    // as a fan from each neck vertex over its quarter of the body ring
    // and one triangle across to the next neck vertex.
    for j in 0..16 {
        let k = (j + 1) % 16;
        for m in 0..4 {
            let (b, c) = (4 * j + m, (4 * j + m + 1) % 64);
            tri(&mut f, n1[j], b0[c], b0[b]);
        }
        tri(&mut f, n1[j], n1[k], b0[(4 * j + 4) % 64]);
    }
    let (c0, c1) = (Point3::new(0.0, 0.0, -40.0), Point3::new(0.0, 0.0, 40.0));
    for j in 0..16 {
        tri(&mut f, c0, n0[(j + 1) % 16], n0[j]);
    }
    for j in 0..64 {
        tri(&mut f, c1, b1[j], b1[(j + 1) % 64]);
    }
    let p = params(PocketStrategy::OffsetRings, 2.0, 12.0);
    match rotary_offset_in(&f, Some(&stock(6.5, -40.0, 40.0)), &p, &control()) {
        Ok(_) => panic!("the mixed-count shaft wrapped: pin what it cuts before accepting that"),
        Err(e) => assert!(e.to_string().contains("different column counts"), "refused, but not naming the tessellation: {e}"),
    }
}

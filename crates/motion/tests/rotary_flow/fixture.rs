//! The Rotary Flow suite's fixtures and its independent pins: the
//! helical groove (a rotor's flute), the oblique ruled band, an open
//! N-sided prism, the ball tool, and the two clearance pins - the
//! station pin and the swept-ball pin, both read with the sim's own
//! exact distance law, never the kernel's oracle.

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::multiaxis::axis::Axis;
use camcraft_motion::multiaxis::RotaryFlowParams;
use camcraft_motion::tools::{Tool, ToolKind};
use camcraft_sim::stock_sim::dist_seg_tri;

pub const TAU: f64 = std::f64::consts::TAU;
/// The groove's rim radius.
pub const R_OUT: f64 = 10.0;
/// The groove's floor radius.
pub const R_ROOT: f64 = 8.0;
/// The advance of one full turn.
pub const PITCH: f64 = 12.0;
/// The turns the default groove winds.
pub const TURNS: usize = 3;
/// Floor vertices per turn of the default groove.
pub const N_CIRC: usize = 72;
/// The groove's width, in axial stations (the developed width).
pub const W_V: f64 = 4.0;

pub fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The groove's own law: the near edge rides `v = PITCH * th / TAU`.
pub fn law(th: f64) -> f64 {
    PITCH * th / TAU
}

/// The rotor's stated axis: world Z through the origin - a
/// parameter, never fitted from the mesh.
pub fn stated_axis() -> Axis {
    Axis {
        origin: Point3::ZERO,
        dir: p3(0.0, 0.0, 1.0),
    }
}

/// A point at radius `r`, azimuth `th`, axial station `v`.
pub fn gp(r: f64, th: f64, v: f64) -> Point3 {
    p3(r * th.cos(), r * th.sin(), v)
}

/// The helical groove: a floor strip on the `R_ROOT` cylinder
/// between the two root edge helices (`W_V` of axial width,
/// subdivided across it), and a radial wall on each v-edge rising
/// to the rim - a helicoid, the steep flank the issue's gouge law
/// exists for. An open channel by design: the kernel reads facets
/// as the gouge oracle and the chain ground, never as a wrap.
pub fn groove(n_circ: usize, turns: usize, phase: f64) -> Vec<Triangle> {
    const FLOOR_COLS: usize = 8;
    let step = TAU / n_circ as f64;
    let mut t = Vec::new();
    for j in 0..n_circ * turns {
        let (th, thn) = (phase + step * j as f64, phase + step * (j + 1) as f64);
        for jc in 0..FLOOR_COLS {
            let (v0, v1) = (
                law(th) + W_V * jc as f64 / FLOOR_COLS as f64,
                law(th) + W_V * (jc + 1) as f64 / FLOOR_COLS as f64,
            );
            let (v0n, v1n) = (law(thn) + v0 - law(th), law(thn) + v1 - law(th));
            let (a0, b0) = (gp(R_ROOT, th, v0), gp(R_ROOT, th, v1));
            let (a1, b1) = (gp(R_ROOT, thn, v0n), gp(R_ROOT, thn, v1n));
            t.push(Triangle::new(a0, b0, b1));
            t.push(Triangle::new(a0, b1, a1));
        }
        for base in [0.0, W_V] {
            let (v, vn) = (law(th) + base, law(thn) + base);
            let (r0, r1) = (gp(R_ROOT, th, v), gp(R_OUT, th, v));
            let (r0n, r1n) = (gp(R_ROOT, thn, vn), gp(R_OUT, thn, vn));
            t.push(Triangle::new(r0, r1, r1n));
            t.push(Triangle::new(r0, r1n, r0n));
        }
    }
    t
}

/// The groove's two root edge helices - the picked guide chains,
/// exact mesh vertices (points ON the model by construction).
pub fn root_pair(n_circ: usize, turns: usize, phase: f64) -> Vec<Vec<Point3>> {
    let step = TAU / n_circ as f64;
    let mut a = Vec::with_capacity(n_circ * turns + 1);
    let mut b = Vec::with_capacity(n_circ * turns + 1);
    for j in 0..=n_circ * turns {
        let th = phase + step * j as f64;
        a.push(gp(R_ROOT, th, law(th)));
        b.push(gp(R_ROOT, th, law(th) + W_V));
    }
    vec![a, b]
}

/// An oblique band for the alignment law: two helices at the same
/// radius, separated `wb` in azimuth and `PITCH * wb / TAU + W_V`
/// in station, spanned by one ruled strip.
pub fn ruled_band(wb: f64, n: usize, phase: f64) -> Vec<Triangle> {
    let step = TAU / n as f64;
    let a = |th: f64| gp(R_ROOT, th, law(th));
    let b = |th: f64| gp(R_ROOT, th + wb, law(th + wb) + W_V);
    let mut t = Vec::new();
    for j in 0..n {
        let (th, thn) = (phase + step * j as f64, phase + step * (j + 1) as f64);
        t.push(Triangle::new(a(th), b(th), b(thn)));
        t.push(Triangle::new(a(th), b(thn), a(thn)));
    }
    t
}

/// The ruled band's two edge chains.
pub fn band_pair(wb: f64, n: usize, phase: f64) -> Vec<Vec<Point3>> {
    let step = TAU / n as f64;
    let mut a = Vec::with_capacity(n + 1);
    let mut b = Vec::with_capacity(n + 1);
    for j in 0..=n {
        let th = phase + step * j as f64;
        a.push(gp(R_ROOT, th, law(th)));
        b.push(gp(R_ROOT, th + wb, law(th + wb) + W_V));
    }
    vec![a, b]
}

/// An open `n`-sided prism of corner radius `r` about world Z, from
/// `z0` to `z1`: one quad per side, its corners at azimuths `k * TAU
/// / n` (so a point on a corner line is ON the model).
pub fn prism(n: usize, r: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    let step = TAU / n as f64;
    let mut t = Vec::with_capacity(2 * n);
    for k in 0..n {
        let (th, thn) = (step * k as f64, step * (k + 1) as f64);
        let (a0, a1) = (gp(r, th, z0), gp(r, th, z1));
        let (b0, b1) = (gp(r, thn, z0), gp(r, thn, z1));
        t.push(Triangle::new(a0, b0, b1));
        t.push(Triangle::new(a0, b1, a1));
    }
    t
}

pub fn ball(d: f64) -> Tool {
    Tool {
        kind: ToolKind::BallNose,
        diameter: d,
        corner_r: d * 0.5,
        tip_angle: 0.0,
        pitch: 0.0,
        pitch_min: 0.0,
        thread_hand: Default::default(),
        nose_radius: 0.0,
        width: 0.0,
        flute_len: 30.0,
        shank_d: d,
        shank_len: 30.0,
        holder_d: 20.0,
        holder_len: 20.0,
        material_group: "aluminum".into(),
        center_cutting: true,
        rpm: 10_000.0,
        feed: 400.0,
        tool_material: None,
        flutes: None,
        orientation: None,
        catalog: camcraft_tools::catalog_fields::ToolCatalog::default(),
        cutting: Vec::new(),
        outline: Vec::new(),
        turning: None,
        tip_form: Default::default(),
        tip_diameter: 0.0,
        form: None,
    }
}

pub fn params<'a>(tool: &'a Tool, stepover: Option<f64>) -> RotaryFlowParams<'a> {
    RotaryFlowParams {
        tool,
        stepover,
        tolerance: 0.05,
        clearance: 14.0,
        retract: None,
        feed: 400.0,
        plunge_feed: 100.0,
        climb: true,
    }
}

/// The ball center a move's tip and pose state (the pivot law: the
/// tip is the center backed off `r` along its own pose).
pub fn center_of(m: &Move, r: f64) -> Option<Point3> {
    let a = m.axis?;
    Some(p3(m.to.x + a[0] * r, m.to.y + a[1] * r, m.to.z + a[2] * r))
}

/// The (azimuth, radius) of a world point about the stated axis.
pub fn cyl(q: Point3) -> (f64, f64) {
    (q.y.atan2(q.x), (q.x * q.x + q.y * q.y).sqrt())
}

/// The smallest distance from the segment `a -> b` to the mesh - the
/// sim's own exact segment-to-triangle law.
pub fn seg_dist(a: Point3, b: Point3, facets: &[Triangle]) -> f64 {
    facets
        .iter()
        .map(|f| dist_seg_tri(a, b, f))
        .fold(f64::INFINITY, f64::min)
}

/// The station pin: every cut station's ball keeps its radius off the
/// part, and the path is chained and finite.
pub fn no_gouge(path: &Toolpath, facets: &[Triangle], r: f64) {
    assert!(path.is_chained());
    assert!(path.all_finite());
    for m in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        let Some(c) = center_of(m, r) else {
            panic!("a cut move states no pose: {m:?}");
        };
        let d = seg_dist(c, c, facets);
        assert!(d >= r - 1e-6, "a station's ball at {c:?} stands {:e} inside the part", r - d);
    }
}

/// The swept pin: EVERY move - cut, plunge, retract, link, exit -
/// carries the ball center along a segment that keeps the ball radius
/// off the part, and the controller's blend of the two poses (the tip
/// and the normalized axis interpolated together) does too.
pub fn no_swept_gouge(path: &Toolpath, facets: &[Triangle], r: f64) {
    let mut prev: Option<[f64; 3]> = None;
    for (i, m) in path.moves.iter().enumerate() {
        let a1 = m.axis.or(prev).unwrap_or([0.0, 0.0, 1.0]);
        let a0 = prev.unwrap_or(a1);
        let at = |t: f64| {
            let tip = p3(
                m.from.x + (m.to.x - m.from.x) * t,
                m.from.y + (m.to.y - m.from.y) * t,
                m.from.z + (m.to.z - m.from.z) * t,
            );
            let ax = [
                a0[0] + (a1[0] - a0[0]) * t,
                a0[1] + (a1[1] - a0[1]) * t,
                a0[2] + (a1[2] - a0[2]) * t,
            ];
            let l = (ax[0] * ax[0] + ax[1] * ax[1] + ax[2] * ax[2]).sqrt();
            p3(tip.x + ax[0] / l * r, tip.y + ax[1] / l * r, tip.z + ax[2] / l * r)
        };
        let (c0, c1) = (at(0.0), at(1.0));
        // The screen: a facet whose box stands more than two radii off
        // the move's box cannot come within `r` of any blended center
        // (the blend drifts less than `r` off the segment).
        let reach = 2.0 * r + 1e-3;
        let lo = p3(c0.x.min(c1.x) - reach, c0.y.min(c1.y) - reach, c0.z.min(c1.z) - reach);
        let hi = p3(c0.x.max(c1.x) + reach, c0.y.max(c1.y) + reach, c0.z.max(c1.z) + reach);
        let near: Vec<Triangle> = facets
            .iter()
            .filter(|f| {
                let (a, b, c) = (f.a, f.b, f.c);
                a.x.max(b.x).max(c.x) >= lo.x
                    && a.x.min(b.x).min(c.x) <= hi.x
                    && a.y.max(b.y).max(c.y) >= lo.y
                    && a.y.min(b.y).min(c.y) <= hi.y
                    && a.z.max(b.z).max(c.z) >= lo.z
                    && a.z.min(b.z).min(c.z) <= hi.z
            })
            .copied()
            .collect();
        let facets = &near[..];
        let d = seg_dist(c0, c1, facets);
        assert!(
            d >= r - 1e-6,
            "move {} ({:?}) sweeps the ball center {c0:?} -> {c1:?} only {d:.9} mm off the part",
            i + 1,
            m.kind
        );
        for k in 1..8 {
            let c = at(k as f64 / 8.0);
            let d = seg_dist(c, c, facets);
            assert!(
                d >= r - 1e-6,
                "move {} ({:?}) blends its pose through {c:?}, only {d:.9} mm off the part",
                i + 1,
                m.kind
            );
        }
        prev = Some(a1);
    }
}

/// The first pass's ball centers: the plunge's landing station and
/// every feed station after it, up to the retract onto the `clear`
/// cylinder.
pub fn first_pass_centers(path: &Toolpath, clear: f64, r: f64) -> Vec<Point3> {
    let mut out = Vec::new();
    for m in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        let Some(c) = center_of(m, r) else { continue };
        if cyl(c).1 > clear - 2.0 {
            break; // the retract onto the link cylinder
        }
        out.push(c);
    }
    out
}

/// The azimuth a run of centers winds, unwound step by step.
pub fn wound(centers: &[Point3]) -> f64 {
    let mut span = 0.0f64;
    for w in centers.windows(2) {
        let d = cyl(w[1]).0 - cyl(w[0]).0;
        span += if d.abs() > std::f64::consts::PI {
            d - d.signum() * TAU
        } else {
            d
        };
    }
    span
}

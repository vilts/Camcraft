//! The f32 cell cut (#2626, GPU stock sim S1): the CPU twin of the
//! per-cell removal kernel that a compute shader runs, in single
//! precision, on tile-relative coordinates.
//!
//! Provenance: the swept-solid distance laws of `field_cut` /
//! `field_sweep` (capsule closed form, upright flat cylinder in the
//! (rho, z) half-plane, golden section on a convex 1-D minimum),
//! re-stated in IEEE-754 single precision; the order-preserving
//! float-to-unsigned key is the standard radix-sort transform (flip
//! all bits of a negative, the sign bit of a positive). Nothing from
//! vendor binaries.
//!
//! ## The laws here
//!
//! - **Tile-relative.** Every coordinate the kernel sees is a world
//!   value minus the origin of its brick (`BRICK` cells a side),
//!   formed in f64 and rounded once to f32. A cell center is
//!   `(local + 0.5) * cell` with `local` in `0..BRICK`, so the f32
//!   values stay near the tile and the rounding scales with the move's
//!   reach from it, not with the world position.
//! - **Never more removal than the f64 law.** The kernel adds a
//!   margin to every distance before it claims: `TWIN_ULPS` f32
//!   epsilons of the tile's coordinate scale, plus the f64 golden
//!   section's own bracket on the path that uses it. The f32 cut is
//!   therefore never above the f64 cut; it sits below it by at most
//!   about twice that margin (`stock_cut_twin` pins both sides).
//! - **The max law on ordered keys.** A claim is `min(-phi, cap)`,
//!   written as `ordered_bits` and folded with an unsigned max - the
//!   one atomic a GPU has for this. The key keeps the order of every
//!   non-NaN f32, so the max of keys is the key of the max, and the
//!   fold commutes: any move order answers bit for bit the same.
//! - **No NaN is claimed.** A non-finite distance claims nothing.

use super::field::BRICK;

/// Rounding slack of the f32 distance, in f32 epsilons of the tile
/// scale: the endpoints round once (half an ulp each), and the
/// longest path (the flat's hypot of positive parts after a golden
/// section) takes about a dozen roundings. 32 covers both with room.
pub const TWIN_ULPS: f32 = 32.0;

/// The f64 golden section's final bracket, as a fraction of the path
/// length (`field_sweep::GS_RESIDUAL`): the f64 distance on that path
/// may read this much above the truth, so the twin's margin holds it.
const GS_RESIDUAL: f64 = 1e-6;

/// Golden-section iterations of the f32 search (the f64 count).
const GS_ITERS: u32 = 32;

/// (sqrt(5) - 1) / 2 in f32.
const PHI: f32 = 0.618_034;

/// The tool of one twin chord. Only the two the kernel has: other
/// tools refuse by name in `twin_work`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TwinTool {
    /// Ball nose of radius `r`; the chord is the ball's CENTER path.
    Ball { r: f64 },
    /// Upright flat end mill: disc of `r`, flank `flank` up +Z; the
    /// chord is the TIP path.
    Flat { r: f64, flank: f64 },
}

/// One linear chord in world coordinates (f64), as the CPU uploads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TwinChord {
    pub a: [f64; 3],
    pub b: [f64; 3],
    pub tool: TwinTool,
}

/// One chord as the kernel reads it inside one tile: f32, relative to
/// the tile origin, with its claim margin.
#[derive(Clone, Copy, Debug)]
pub struct TileChord {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub r: f32,
    /// Flank length; `0` marks the ball.
    pub flank: f32,
    pub ball: bool,
    pub margin: f32,
}

impl TileChord {
    /// The chord seen from the tile whose world origin is `origin`.
    #[must_use]
    pub fn of(ch: &TwinChord, origin: [f64; 3], cell: f64) -> TileChord {
        let rel = |p: [f64; 3]| [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]];
        let (ra, rb) = (rel(ch.a), rel(ch.b));
        let (r, flank, ball) = match ch.tool {
            TwinTool::Ball { r } => (r, 0.0, true),
            TwinTool::Flat { r, flank } => (r, flank, false),
        };
        let reach = ra.iter().chain(rb.iter()).fold(0.0f64, |m, v| m.max(v.abs()));
        let scale = reach + r + flank + f64::from(BRICK) * cell;
        let mut margin = f64::from(TWIN_ULPS) * f64::from(f32::EPSILON) * scale;
        if !ball {
            let d = [rb[0] - ra[0], rb[1] - ra[1], rb[2] - ra[2]];
            margin += (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() * GS_RESIDUAL;
        }
        let f = |p: [f64; 3]| [p[0] as f32, p[1] as f32, p[2] as f32];
        TileChord {
            a: f(ra),
            b: f(rb),
            r: r as f32,
            flank: flank as f32,
            ball,
            // rounded UP so the margin never shrinks in the cast
            margin: (margin as f32) * (1.0 + f32::EPSILON),
        }
    }
}

/// The cell cut at tile-relative center `q`: `Some(min(-phi, cap))`
/// when the biased distance `phi` is under `cap`, `None` otherwise
/// (the f64 claim's two rejects) and for any non-finite value.
#[must_use]
pub fn cell_cut(tc: &TileChord, q: [f32; 3], cap: f32) -> Option<f32> {
    let d = if tc.ball { ball_dist(tc, q) } else { flat_dist(tc, q) };
    let phi = d + tc.margin;
    if !phi.is_finite() || phi >= cap {
        return None;
    }
    Some((-phi).min(cap))
}

/// The capsule: distance to the center segment, less `r`.
fn ball_dist(tc: &TileChord, q: [f32; 3]) -> f32 {
    let d = sub(tc.b, tc.a);
    let v = sub(q, tc.a);
    let dd = dot(d, d);
    let t = if dd > 0.0 { (dot(v, d) / dd).clamp(0.0, 1.0) } else { 0.0 };
    let w = [v[0] - t * d[0], v[1] - t * d[1], v[2] - t * d[2]];
    dot(w, w).sqrt() - tc.r
}

/// The upright flat cylinder swept along the tip chord: closed form on
/// a level move (foot at the XY projection) and a plunge (foot at
/// mid-flank), the golden section on every other move - the three
/// paths of `field_sweep::Sweep::dist`, in f32.
fn flat_dist(tc: &TileChord, q: [f32; 3]) -> f32 {
    let d = sub(tc.b, tc.a);
    let at = |t: f32| pose(tc, [tc.a[0] + t * d[0], tc.a[1] + t * d[1], tc.a[2] + t * d[2]], q);
    if d[2] == 0.0 {
        let dd = d[0] * d[0] + d[1] * d[1];
        let t = ((q[0] - tc.a[0]) * d[0] + (q[1] - tc.a[1]) * d[1]) / dd;
        return at(if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 });
    }
    if d[0] == 0.0 && d[1] == 0.0 {
        let t = (q[2] - tc.a[2] - 0.5 * tc.flank) / d[2];
        return at(if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 });
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    let mut c = hi - PHI * (hi - lo);
    let mut e = lo + PHI * (hi - lo);
    let mut fc = at(c);
    let mut fe = at(e);
    for _ in 0..GS_ITERS {
        if fc < fe {
            hi = e;
            e = c;
            fe = fc;
            c = hi - PHI * (hi - lo);
            fc = at(c);
        } else {
            lo = c;
            c = e;
            fc = fe;
            e = lo + PHI * (hi - lo);
            fe = at(e);
        }
    }
    fc.min(fe)
}

/// One upright flat pose with its tip at `t`: the box distance in the
/// (rho, z) half-plane (`field_solid::pose_dist`'s flat arm).
fn pose(tc: &TileChord, t: [f32; 3], q: [f32; 3]) -> f32 {
    let (x, y, zl) = (q[0] - t[0], q[1] - t[1], q[2] - t[2]);
    let rho = (x * x + y * y).sqrt();
    let (e1, e2, e3) = (rho - tc.r, -zl, zl - tc.flank);
    let m = e1.max(e2).max(e3);
    if m > 0.0 {
        let (p1, p2, p3) = (e1.max(0.0), e2.max(0.0), e3.max(0.0));
        (p1 * p1 + p2 * p2 + p3 * p3).sqrt()
    } else {
        m
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The order-preserving key of an f32: for any two non-NaN values,
/// `x < y` gives `ordered_bits(x) < ordered_bits(y)` (`-0.0` sorts
/// just under `+0.0`). A negative flips every bit; a positive sets
/// the sign bit.
#[must_use]
pub fn ordered_bits(v: f32) -> u32 {
    let b = v.to_bits();
    if b & 0x8000_0000 != 0 {
        !b
    } else {
        b | 0x8000_0000
    }
}

/// The inverse of [`ordered_bits`].
#[must_use]
pub fn from_ordered_bits(u: u32) -> f32 {
    if u & 0x8000_0000 != 0 {
        f32::from_bits(u & 0x7fff_ffff)
    } else {
        f32::from_bits(!u)
    }
}

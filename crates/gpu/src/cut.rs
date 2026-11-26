//! The plain-value input of the stock removal kernel (#2627, GPU stock
//! sim S2) and its CPU law: the per-cell cut of a ball or an upright
//! flat end mill on one linear chord, in f32, folded with the max law
//! on order-preserving keys into a dense z-major lattice.
//!
//! Provenance: a copy, operation for operation, of the f32 cut twin of
//! camcraft-sim (`stock_sim::cut_twin`, #2626): the capsule closed form,
//! the upright flat cylinder in the (rho, z) half-plane and the golden
//! section on a convex 1-D minimum (J. Kiefer, "Sequential minimax
//! search for a maximum", Proc. AMS 4, 1953); the key is the standard
//! radix-sort float transform. Our own design notes; nothing from vendor
//! binaries.
//!
//! ## The laws here
//!
//! - **Plain values in.** The caller (which sees camcraft-sim) gives
//!   each work item as numbers: the chord relative to its brick origin,
//!   already rounded to f32 with its claim margin, and the brick's
//!   cells. This crate imports no camcraft crate (decision 2620).
//! - **Validate before any work.** A non-finite or out-of-range value,
//!   an item outside the lattice or a lattice over the texel budget is
//!   an `Err` that names it. The CPU law and the kernel never see it.
//! - **The CPU law is the twin, bit for bit.** `cut_cpu` gives the same
//!   bits as camcraft-sim's `run_twin` on the same items
//!   (`tests/cut_parity.rs` reads the recorded twin output). It is the
//!   fallback of the GPU path and the oracle of its spot check.

/// The brick edge in cells (camcraft-sim's `field::BRICK`).
pub const CUT_BRICK: u32 = 8;

/// The dense lattice budget in texels (camcraft-sim's
/// `preview::pack::MAX_TEXELS`, the same value).
pub const MAX_CUT_TEXELS: usize = 1 << 23;

/// The largest coordinate magnitude that an item may carry, in mm. A
/// tile-relative value past it is no stock cut; it also keeps every
/// square inside the f32 range, so no distance overflows.
pub const MAX_CUT_COORD: f32 = 1.0e6;

/// Golden-section iterations (the twin's and the f64 law's count).
const GS_ITERS: u32 = 32;

/// (sqrt(5) - 1) / 2 in f32 (the twin's value).
const PHI: f32 = 0.618_034;

/// The dense lattice: `dims` cells a side, edge `cell`, claim cap `cap`.
/// Texel `(x, y, z)` is slot `(z * dims[1] + y) * dims[0] + x`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CutGrid {
    pub dims: [u32; 3],
    pub cell: f32,
    pub cap: f32,
}

/// One unit of kernel work: one chord against one brick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CutItem {
    /// Chord start, relative to the brick origin (ball: the center
    /// path; flat: the tip path).
    pub a: [f32; 3],
    /// Chord end, relative to the brick origin.
    pub b: [f32; 3],
    /// Tool radius.
    pub r: f32,
    /// Flank length up +Z (flat only; `0` for the ball).
    pub flank: f32,
    /// The claim margin that keeps the f32 cut under the f64 cut.
    pub margin: f32,
    pub ball: bool,
    /// The brick origin's lattice offset in cells (may be negative
    /// when the brick starts before the lattice).
    pub base: [i32; 3],
    /// First local cell of the brick to cut, in `0..CUT_BRICK`.
    pub lo: [u32; 3],
    /// Last local cell to cut (inclusive), in `lo..CUT_BRICK`.
    pub hi: [u32; 3],
}

/// One batch for the kernel.
#[derive(Clone, Debug, PartialEq)]
pub struct CutInput {
    pub grid: CutGrid,
    pub items: Vec<CutItem>,
}

fn bad(msg: String) -> String {
    format!("stock cut kernel: {msg}")
}

impl CutGrid {
    /// The number of texels, or why the lattice is refused.
    pub fn texels(&self) -> Result<usize, String> {
        if !(self.cell.is_finite() && self.cell > 0.0 && self.cap.is_finite() && self.cap > 0.0) {
            return Err(bad(format!("the cell {} and cap {} must be positive finite numbers", self.cell, self.cap)));
        }
        let [x, y, z] = self.dims.map(|d| d as usize);
        match x.checked_mul(y).and_then(|n| n.checked_mul(z)) {
            Some(n) if n > 0 && n <= MAX_CUT_TEXELS => Ok(n),
            _ => Err(bad(format!("the lattice dims {:?} are empty or over the {MAX_CUT_TEXELS} texel budget", self.dims))),
        }
    }

    /// The key of the "no cut" floor, `-4 cap` (the `pack_cut` floor).
    #[must_use]
    pub fn floor_key(&self) -> u32 {
        ordered_bits(-4.0 * self.cap)
    }
}

impl CutItem {
    fn check(&self, n: usize, dims: [u32; 3]) -> Result<(), String> {
        let coords = self.a.iter().chain(self.b.iter());
        if !coords.clone().all(|v| v.is_finite() && v.abs() <= MAX_CUT_COORD) {
            return Err(bad(format!("item {n} has a chord endpoint that is not finite or past {MAX_CUT_COORD} mm")));
        }
        let size_ok = |v: f32| v.is_finite() && v <= MAX_CUT_COORD;
        if !(size_ok(self.r) && self.r > 0.0 && size_ok(self.flank) && self.flank >= 0.0) {
            return Err(bad(format!("item {n} has the radius {} and flank {}: not a tool size", self.r, self.flank)));
        }
        if !(size_ok(self.margin) && self.margin >= 0.0) {
            return Err(bad(format!("item {n} has the margin {}: not a finite non-negative value", self.margin)));
        }
        if !self.ball && self.flank <= 0.0 {
            return Err(bad(format!("item {n} is a flat end mill with no flank")));
        }
        for (a, dim) in dims.into_iter().enumerate() {
            let (lo, hi) = (self.lo[a], self.hi[a]);
            if lo > hi || hi >= CUT_BRICK {
                return Err(bad(format!("item {n} has the cells {lo}..={hi} on axis {a}: not inside one brick")));
            }
            let first = i64::from(self.base[a]) + i64::from(lo);
            let last = i64::from(self.base[a]) + i64::from(hi);
            if first < 0 || last >= i64::from(dim) {
                return Err(bad(format!("item {n} reaches the cells {first}..={last} on axis {a}, outside the lattice")));
            }
        }
        Ok(())
    }
}

impl CutInput {
    /// Validate the whole batch; the texel count when it is good.
    pub fn check(&self) -> Result<usize, String> {
        let n = self.grid.texels()?;
        if u32::try_from(self.items.len()).is_err() {
            return Err(bad(format!("{} items do not fit one batch", self.items.len())));
        }
        for (i, it) in self.items.iter().enumerate() {
            it.check(i, self.grid.dims)?;
        }
        Ok(n)
    }
}

/// The lattice slot of local cell `l` of `it`. The item is checked, so
/// the sum is inside the lattice.
pub(crate) fn slot(it: &CutItem, l: [u32; 3], dims: [u32; 3]) -> Option<usize> {
    let mut at = [0usize; 3];
    for a in 0..3 {
        let v = i64::from(it.base[a]) + i64::from(l[a]);
        if v < 0 || v >= i64::from(dims[a]) {
            return None;
        }
        at[a] = usize::try_from(v).ok()?;
    }
    let [dx, dy, _] = dims.map(|d| d as usize);
    Some((at[2] * dy + at[1]) * dx + at[0])
}

/// The cell cut of `it` at local cell `l`: `Some(min(-phi, cap))` when
/// the biased distance `phi` is under `cap` (the twin's `cell_cut`).
#[must_use]
pub fn cell_cut(it: &CutItem, l: [u32; 3], grid: &CutGrid) -> Option<f32> {
    let q = l.map(|v| (v as f32 + 0.5) * grid.cell);
    let d = if it.ball { ball_dist(it, q) } else { flat_dist(it, q) };
    let phi = d + it.margin;
    if !phi.is_finite() || phi >= grid.cap {
        return None;
    }
    Some((-phi).min(grid.cap))
}

/// The CPU run of the kernel: the cut layer in the dense layout, with
/// the `-4 cap` floor where no item claims a cell.
pub fn cut_cpu(input: &CutInput) -> Result<Vec<f32>, String> {
    let n = input.check()?;
    let mut keys = Vec::new();
    keys.try_reserve_exact(n).map_err(|e| bad(format!("no memory for {n} texels: {e}")))?;
    keys.resize(n, input.grid.floor_key());
    fold(input, keys)
}

/// The incremental CPU run (#2629, GPU stock sim S3a): cut `input` onto
/// an earlier layer `base` of the same lattice (a playback keyframe)
/// instead of the empty floor. The max law makes it exact: cutting A,
/// then B onto it, gives the bits of cutting A and B in one batch.
pub fn cut_cpu_onto(input: &CutInput, base: &[f32]) -> Result<Vec<f32>, String> {
    let n = input.check()?;
    check_base(base, n)?;
    let mut keys = Vec::new();
    keys.try_reserve_exact(n).map_err(|e| bad(format!("no memory for {n} texels: {e}")))?;
    keys.extend(base.iter().map(|v| ordered_bits(*v)));
    fold(input, keys)
}

/// A base layer must match the lattice and hold finite values only (the
/// key order of a NaN is not a cut depth).
pub(crate) fn check_base(base: &[f32], n: usize) -> Result<(), String> {
    if base.len() != n {
        return Err(bad(format!("the base layer has {} texels, the lattice {n}", base.len())));
    }
    match base.iter().position(|v| !v.is_finite()) {
        Some(at) => Err(bad(format!("the base layer texel {at} is not finite"))),
        None => Ok(()),
    }
}

/// Fold every item's claims into `keys` (one key for each texel) with the
/// max law, and decode the keys into the layer.
fn fold(input: &CutInput, mut keys: Vec<u32>) -> Result<Vec<f32>, String> {
    for it in &input.items {
        for z in it.lo[2]..=it.hi[2] {
            for y in it.lo[1]..=it.hi[1] {
                for x in it.lo[0]..=it.hi[0] {
                    let Some(v) = cell_cut(it, [x, y, z], &input.grid) else {
                        continue;
                    };
                    let dst = slot(it, [x, y, z], input.grid.dims).and_then(|s| keys.get_mut(s));
                    let Some(dst) = dst else {
                        return Err(bad("a checked item left the lattice".into()));
                    };
                    *dst = (*dst).max(ordered_bits(v));
                }
            }
        }
    }
    Ok(keys.into_iter().map(from_ordered_bits).collect())
}

fn ball_dist(it: &CutItem, q: [f32; 3]) -> f32 {
    let d = sub(it.b, it.a);
    let v = sub(q, it.a);
    let dd = dot(d, d);
    let t = if dd > 0.0 { (dot(v, d) / dd).clamp(0.0, 1.0) } else { 0.0 };
    let w = [v[0] - t * d[0], v[1] - t * d[1], v[2] - t * d[2]];
    dot(w, w).sqrt() - it.r
}

fn flat_dist(it: &CutItem, q: [f32; 3]) -> f32 {
    let d = sub(it.b, it.a);
    let at = |t: f32| pose(it, [it.a[0] + t * d[0], it.a[1] + t * d[1], it.a[2] + t * d[2]], q);
    if d[2] == 0.0 {
        let dd = d[0] * d[0] + d[1] * d[1];
        let t = ((q[0] - it.a[0]) * d[0] + (q[1] - it.a[1]) * d[1]) / dd;
        return at(if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 });
    }
    if d[0] == 0.0 && d[1] == 0.0 {
        let t = (q[2] - it.a[2] - 0.5 * it.flank) / d[2];
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

fn pose(it: &CutItem, t: [f32; 3], q: [f32; 3]) -> f32 {
    let (x, y, zl) = (q[0] - t[0], q[1] - t[1], q[2] - t[2]);
    let rho = (x * x + y * y).sqrt();
    let (e1, e2, e3) = (rho - it.r, -zl, zl - it.flank);
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

/// The order-preserving key of an f32 (a negative flips every bit, a
/// positive sets the sign bit): the twin's `ordered_bits`.
#[must_use]
pub fn ordered_bits(v: f32) -> u32 {
    let b = v.to_bits();
    if b & 0x8000_0000 != 0 { !b } else { b | 0x8000_0000 }
}

/// The inverse of [`ordered_bits`].
#[must_use]
pub fn from_ordered_bits(u: u32) -> f32 {
    if u & 0x8000_0000 != 0 { f32::from_bits(u & 0x7fff_ffff) } else { f32::from_bits(!u) }
}

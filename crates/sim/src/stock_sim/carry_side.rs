//! The machined body carried through a side-on quarter turn (#874): a
//! setup that stands the part on its edge takes the column body the
//! setup before it left, read again along the new tool axis.
//!
//! Provenance: our own design after #308's research note and the
//! maintainer's rule on #874 ("a re-sample that never leaves the carried
//! stock smaller than the real one"); the column model is the classic
//! Z-map with a floor per column (`carry`), and reading a dexel body
//! along another axis is the tri-dexel idea (Benouamer and Michelucci
//! 1997, "Bridging the gap between CSG and Brep via a triple ray
//! representation"). No vendor material consulted.
//!
//! ## The laws here
//!
//! - **Only the sixteen side-on quarter turns.** A rotation that is a
//!   signed permutation of the axes and puts the old X or Y on the new
//!   tool axis. A lattice turn (Z kept on +-Z) is `LatticeTurn`'s; every
//!   other rotation is refused by name ([`CARRY_TURN_REFUSAL`]).
//! - **The axis that stays level is re-indexed.** The old horizontal
//!   axis that stays horizontal keeps its cells: centre onto centre.
//! - **The old heights become one level axis, in slabs of one cell.**
//!   The old Z range is cut into whole slabs, centred on it, so the
//!   slab at each end has its centre inside the range. An old column
//!   feeds every slab its span (`floor..top`) reaches into: a face
//!   inside a slab counts the slab whole, and a span thinner than a
//!   cell still feeds one. No material is dropped.
//! - **The old level axis becomes the height, never read short.** Along
//!   its slab a new column knows the material only at the old column
//!   centres. Its top and its floor are the hull of those centres,
//!   each end moved out to the next old centre that holds no material
//!   there (the real face lies between the two), or to the lattice
//!   edge when there is no next centre. A cavity between the ends is
//!   filled. The carried body is therefore never smaller than
//!   the body the columns hold, and at most one cell larger on a turned
//!   face.
//! - **The floor is not judged after the turn.** The new floor is a
//!   face the earlier setup held level; the caller judges the body
//!   before the turn and keeps those gouges (`stock_sim_carry`), because
//!   the hull can hide them.
//! - **The field engine does not carry side-on yet**
//!   ([`FIELD_SIDE_CARRY_REFUSAL`]): its column read assumes the cuts
//!   came along the tool axis.

use super::carried::CarriedStock;
use super::carry::{unit, Body, LatticeTurn, CARRY_TURN_REFUSAL};
use super::dexel::Dexel;
use super::kernel::MAX_CELLS_PER_AXIS;
use crate::geometry::Point3;

/// The field arm's refusal of a side-on turn (the module law).
pub const FIELD_SIDE_CARRY_REFUSAL: &str =
    "the field engine does not carry machined stock through a side-on turn yet - the column engine carries it";

/// A rigid transform whose rotation is a signed permutation that puts
/// the old X or Y on the new tool axis: `p' = q * p + t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SideTurn {
    q: [[i8; 3]; 3],
    t: [f64; 3],
}

impl SideTurn {
    /// Read a rotation matrix (row-major, `p' = r * p + t`) as a side-on
    /// quarter turn, or refuse by name.
    ///
    /// # Errors
    /// A non-finite transform, a rotation that is not a signed
    /// permutation, one that mirrors, or one that keeps Z on +-Z.
    pub fn from_matrix(r: [[f64; 3]; 3], t: [f64; 3]) -> Result<Self, String> {
        if r.iter().flatten().chain(t.iter()).any(|v| !v.is_finite()) {
            return Err("the transform between the setups is not finite".into());
        }
        let mut q = [[0i8; 3]; 3];
        for (i, row) in r.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                q[i][j] = unit(*v).ok_or(CARRY_TURN_REFUSAL)?;
            }
        }
        let one = |a: [i8; 3]| a.iter().filter(|v| **v != 0).count() == 1;
        let cols = [0, 1, 2].map(|j| [q[0][j], q[1][j], q[2][j]]);
        if !(q.iter().all(|r| one(*r)) && cols.iter().all(|c| one(*c))) {
            return Err(CARRY_TURN_REFUSAL.into());
        }
        let m = |a: usize, b: usize| i32::from(q[a][b]);
        let det = m(0, 0) * (m(1, 1) * m(2, 2) - m(1, 2) * m(2, 1)) - m(0, 1) * (m(1, 0) * m(2, 2) - m(1, 2) * m(2, 0))
            + m(0, 2) * (m(1, 0) * m(2, 1) - m(1, 1) * m(2, 0));
        if det != 1 {
            return Err("the transform between the setups mirrors the part - no rigid re-clamp does that".into());
        }
        if q[2][2] != 0 {
            return Err("the turn keeps the tool axis vertical - it is not a side-on turn".into());
        }
        Ok(SideTurn { q, t })
    }

    /// Per new axis: the old axis it reads, and that axis's sign.
    fn axes(&self) -> ([usize; 3], [f64; 3]) {
        let (mut src, mut sgn) = ([0usize; 3], [1.0f64; 3]);
        for (a, row) in self.q.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                if *v != 0 {
                    src[a] = j;
                    sgn[a] = f64::from(*v);
                }
            }
        }
        (src, sgn)
    }

    /// Map a point.
    #[must_use]
    pub fn apply(&self, p: Point3) -> Point3 {
        let (src, sgn) = self.axes();
        let c = [p.x, p.y, p.z];
        Point3::new(sgn[0] * c[src[0]] + self.t[0], sgn[1] * c[src[1]] + self.t[1], sgn[2] * c[src[2]] + self.t[2])
    }
}

/// The turn a body is carried through between two setups: a lattice
/// turn (re-indexed), or a side-on quarter turn (the module law).
#[derive(Clone, Copy, PartialEq)]
pub enum CarryTurn {
    Lattice(LatticeTurn),
    Side(SideTurn),
}

/// The inner turn's own text: a lattice turn prints as it did before
/// #874, so a digest written from it does not move.
impl std::fmt::Debug for CarryTurn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CarryTurn::Lattice(t) => t.fmt(f),
            CarryTurn::Side(t) => t.fmt(f),
        }
    }
}

impl CarryTurn {
    /// Read a rotation matrix (row-major, `p' = r * p + t`) as a turn a
    /// body can be carried through, or refuse by name.
    ///
    /// # Errors
    /// As [`LatticeTurn::from_matrix`]; a turn off the quarter turns is
    /// [`CARRY_TURN_REFUSAL`].
    pub fn from_matrix(r: [[f64; 3]; 3], t: [f64; 3]) -> Result<Self, String> {
        match LatticeTurn::from_matrix(r, t) {
            Ok(turn) => Ok(CarryTurn::Lattice(turn)),
            Err(e) if e == CARRY_TURN_REFUSAL => SideTurn::from_matrix(r, t).map(CarryTurn::Side),
            Err(e) => Err(e),
        }
    }

    /// Does this turn stand the part on its side?
    #[must_use]
    pub fn side_on(&self) -> bool {
        matches!(self, CarryTurn::Side(_))
    }

    /// Map a point.
    #[must_use]
    pub fn apply(&self, p: Point3) -> Point3 {
        match self {
            CarryTurn::Lattice(t) => t.apply(p),
            CarryTurn::Side(t) => t.apply(p),
        }
    }
}

impl CarriedStock {
    /// The same stock seen through `turn`, on either kind of turn.
    ///
    /// # Errors
    /// `Body::carry`'s and `Body::carry_side`'s; a side-on turn on the
    /// field engine ([`FIELD_SIDE_CARRY_REFUSAL`]).
    pub fn carry_through(self, turn: &CarryTurn) -> Result<CarriedStock, String> {
        match (turn, self) {
            (CarryTurn::Lattice(t), stock) => stock.carry(t),
            (CarryTurn::Side(t), CarriedStock::Column(b)) => b.carry_side(t).map(CarriedStock::Column),
            (CarryTurn::Side(_), CarriedStock::Field(_)) => Err(FIELD_SIDE_CARRY_REFUSAL.into()),
        }
    }
}

/// The slabs (first, last) an old span `f..t` reaches into, of `n` slabs
/// of pitch `c` from `a`: a slab the span only touches at a face is not
/// counted, a span with no length feeds the slab it lies in.
fn slabs(f: f64, t: f64, a: f64, c: f64, n: usize) -> (usize, usize) {
    let last = n.saturating_sub(1);
    let at = |v: f64| if v > 0.0 { (v as usize).min(last) } else { 0 };
    let lo = at(((f - a) / c).floor());
    (lo, at(((t - a) / c).ceil() - 1.0).max(lo))
}

impl Body {
    /// The same body seen through a side-on quarter turn (the module
    /// law): never smaller than the body the columns hold, at most one
    /// cell larger on a turned face. The floor is not judged after it.
    ///
    /// # Errors
    /// A body too tall for the turned lattice, by name.
    pub fn carry_side(&self, turn: &SideTurn) -> Result<Body, String> {
        let d = &self.dex;
        let c = d.cell;
        let (src, sgn) = turn.axes();
        // `v`: the old level axis that becomes the height; `u`: the one
        // that stays level; new axis `h` reads the old Z, `k` reads `u`.
        let v = src[2].min(1);
        let u = 1 - v;
        let h = usize::from(src[0] != 2);
        let k = 1 - h;
        let (count, origin) = ([d.nx, d.ny], [d.x0, d.y0]);
        let (nu, nv) = (count[u], count[v]);
        let old = |iu: usize, iv: usize| if v == 1 { iu + iv * d.nx } else { iv + iu * d.nx };
        let (mut z0, mut z1) = (f64::INFINITY, f64::NEG_INFINITY);
        for i in (0..nu * nv).filter(|&i| !self.empty_at(i)) {
            z0 = z0.min(self.floor_at(i));
            z1 = z1.max(d.top_at(i));
        }
        let any = z0.is_finite() && z1.is_finite();
        let (z0, z1) = if any { (z0, z1) } else { (0.0, 0.0) };
        let slabs_f = ((z1 - z0) / c).ceil().max(1.0);
        if !(slabs_f.is_finite() && slabs_f <= MAX_CELLS_PER_AXIS as f64) {
            return Err(format!(
                "the machined stock is {slabs_f} cells tall - turned side-on its lattice would pass {MAX_CELLS_PER_AXIS} cells on one axis"
            ));
        }
        let nh = slabs_f as usize;
        // The slabs are centred on the range: each end slab's centre is
        // inside it, and the slabs cover it whole.
        let z0 = z0 - 0.5 * (slabs_f * c - (z1 - z0));
        // Per new column (iu, slab): the first and last old `v` cell whose
        // span reaches into the slab.
        let mut ends: Vec<Option<(usize, usize)>> = vec![None; nu * nh];
        for iu in 0..nu {
            for iv in 0..nv {
                let i = old(iu, iv);
                if self.empty_at(i) {
                    continue;
                }
                let (m0, m1) = slabs(self.floor_at(i), d.top_at(i), z0, c, nh);
                for e in ends.iter_mut().skip(iu * nh + m0).take(m1.saturating_sub(m0) + 1) {
                    *e = Some(e.map_or((iv, iv), |(a, b)| (a.min(iv), b.max(iv))));
                }
            }
        }
        // The new lattice: axis `k` re-indexes `u`, axis `h` holds the slabs.
        let low = |a: usize, from: f64, n: usize| (sgn[a] * from).min(sgn[a] * (from + n as f64 * c)) + turn.t[a];
        let (lo_k, lo_h) = (low(k, origin[u], nu), low(h, z0, nh));
        let (nx, ny, x0, y0) = if k == 0 { (nu, nh, lo_k, lo_h) } else { (nh, nu, lo_h, lo_k) };
        let mut top = vec![f64::NEG_INFINITY; nx * ny];
        let mut floor = vec![f64::INFINITY; nx * ny];
        for iu in 0..nu {
            for m in 0..nh {
                let Some((a, b)) = ends[iu * nh + m] else { continue };
                // Out to the next old centre that holds nothing here, or
                // to the lattice edge.
                let v_lo = origin[v] + if a == 0 { 0.0 } else { (a as f64 - 0.5) * c };
                let v_hi = origin[v] + if b + 1 >= nv { nv as f64 * c } else { (b as f64 + 1.5) * c };
                let (t, f) = if sgn[2] > 0.0 { (v_hi, v_lo) } else { (-v_lo, -v_hi) };
                let jk = if sgn[k] > 0.0 { iu } else { nu - 1 - iu };
                let jh = if sgn[h] > 0.0 { m } else { nh - 1 - m };
                let j = if k == 0 { jk + jh * nx } else { jh + jk * nx };
                top[j] = t + turn.t[2];
                floor[j] = f + turn.t[2];
            }
        }
        let stock_z = top.iter().copied().filter(|z| z.is_finite()).fold(f64::NEG_INFINITY, f64::max);
        let stock_z = if stock_z.is_finite() { stock_z } else { 0.0 };
        Body::from_parts(Dexel::from_grid(x0, y0, c, nx, ny, stock_z, top), floor, false)
    }
}

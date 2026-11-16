//! fill.rs - the land model (#1196): closed openings narrower than a
//! stated width are lidded at their rim on the DRIVE surface only, so a
//! 3D finish flies over bolt holes and small pockets instead of diving
//! down their walls.
//!
//! Provenance: grey-scale morphological closing with a flat disk
//! structuring element (Serra, *Image Analysis and Mathematical
//! Morphology*, 1982; Soille, *Morphological Image Analysis*, 2003) on
//! the model's top-down height field, and connected-component labelling
//! of the raised cells. The lid emission and the enclosure rule are our
//! own, written from geometry, not from any vendor's code or output.
//!
//! ## The laws here
//!
//! - **The fill rule.** On the height field `H(x, y)` (the topmost
//!   facet over each grid cell; no facet reads as the model's lowest Z),
//!   the closing `C = erode(dilate(H, disk), disk)` with a flat disk of
//!   the stated diameter `w` raises exactly the depressions the disk
//!   cannot enter, to the level where it can. A raised cell (more than
//!   `tol` above `H`) belongs to an opening; an opening is filled only
//!   when it is CLOSED - none of its cells on the grid border and every
//!   cell bordering it standing on material at the lid's own level
//!   (within one cell's 45-degree rise plus `tol`): a lid that would
//!   spill over a lower rim is not held, the feature is open. A slot or a valley that
//!   runs out to the part's edge is open and is never filled.
//! - **Only ever added.** The drive facets are the model's facets,
//!   untouched and in order, plus horizontal lids at `C` (never below
//!   `H`, which the closing guarantees and the code re-asserts). Every
//!   finishing law keeps the ball off EVERY facet it is given, so a
//!   path planned on a superset of the model can never bite the model
//!   deeper than one planned on the model itself - the fill can raise
//!   the path, never lower it. The verifier still judges the real
//!   model (the caller hands it the model, not these facets).
//! - **One plane per opening.** A closed opening's whole lid - its
//!   cells and the ring around them - is one horizontal plane at the
//!   highest of the lid cells' closed levels. The raster closing gives
//!   those cells near-equal levels; keeping each cell its own would
//!   tile the lid as steps, and a stepped lid catches the ball at
//!   every step edge: across the edge is an annulus where the ball can
//!   seat on neither plane, support goes None, and each step turns
//!   into a retract-and-plunge.
//! - **One cell of reach.** Lids cover the opening's cells plus the
//!   ring of cells around it, so no crack along the rim is wider than
//!   the grid's sampling error.

use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

/// The grid's cell count ceiling: the closing costs cells x disk
/// offsets, so the grid is coarsened up to this, and a width too fine
/// for the model's size at that resolution is refused by name.
const MAX_CELLS: usize = 4_000_000;

/// Cells per disk radius at the preferred resolution.
const CELLS_PER_RADIUS: f64 = 6.0;

/// The drive surface a land finish plans on, and what it says.
#[derive(Clone, Debug)]
pub struct Filled {
    /// The model's facets, unchanged and first, then the lids.
    pub facets: Vec<Triangle>,
    /// How many closed openings were lidded.
    pub openings: usize,
}

impl Filled {
    /// The path note: what stands as land, on purpose.
    #[must_use]
    pub fn note(&self, width: f64) -> String {
        if self.openings == 0 {
            return format!(
                "land: no closed opening narrower than {width} mm found - the path follows the model"
            );
        }
        format!(
            "land: {} closed opening(s) narrower than {width} mm filled at their rim and flown over, not cut - their walls stand as land on purpose",
            self.openings
        )
    }
}

/// Lid every closed opening of `facets` that a flat disk of diameter
/// `width` cannot enter (the fill rule above). `tol` is the finish's
/// own tolerance: a cell raised by no more than it is not an opening.
pub fn fill_openings(facets: &[Triangle], width: f64, tol: f64) -> Result<Filled, GenError> {
    if !(width.is_finite() && width > 0.0) {
        return Err(GenError::Invalid(format!(
            "3D finish fill-openings width must be a finite positive diameter, got {width}"
        )));
    }
    if !(tol.is_finite() && tol > 0.0) {
        return Err(GenError::Invalid("3D finish fill needs a finite positive tolerance".into()));
    }
    let grid = Grid::sample(facets, width)?;
    let mut closed = grid.closing();
    let raised: Vec<bool> = (0..grid.h.len()).map(|i| closed[i] > grid.floor_of(i) + tol).collect();
    let mut lid = vec![false; grid.h.len()];
    let mut seen = vec![false; grid.h.len()];
    let mut openings = 0;
    for start in 0..grid.h.len() {
        if !raised[start] || seen[start] {
            continue;
        }
        let comp = grid.component(start, &raised, &mut seen);
        if grid.enclosed(&comp, &raised, &closed, tol) {
            openings += 1;
            let mut cells = comp.clone();
            for &i in &comp {
                for j in grid.ring8(i) {
                    lid[j] = true;
                    cells.push(j);
                }
            }
            grid.plateau(&mut closed, &cells);
        }
    }
    let mut out = facets.to_vec();
    grid.emit(&lid, &closed, &mut out);
    Ok(Filled { facets: out, openings })
}

struct Grid {
    nx: usize,
    ny: usize,
    x0: f64,
    y0: f64,
    cell: f64,
    /// Disk radius in cells.
    rc: f64,
    z_floor: f64,
    /// Topmost facet Z over each cell centre, `None` over no facet.
    h: Vec<Option<f64>>,
}

impl Grid {
    fn sample(facets: &[Triangle], width: f64) -> Result<Self, GenError> {
        if facets.is_empty() {
            return Err(GenError::Empty("model has no facets".into()));
        }
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for f in facets {
            for q in [f.a, f.b, f.c] {
                if !q.is_finite() {
                    return Err(GenError::Invalid("model has a non-finite facet".into()));
                }
                for (k, v) in [q.x, q.y, q.z].into_iter().enumerate() {
                    lo[k] = lo[k].min(v);
                    hi[k] = hi[k].max(v);
                }
            }
        }
        let radius = 0.5 * width;
        let (w, l) = (hi[0] - lo[0] + 2.0 * width, hi[1] - lo[1] + 2.0 * width);
        let mut cell = radius / CELLS_PER_RADIUS;
        let fine = (w / cell).ceil() * (l / cell).ceil();
        if fine > MAX_CELLS as f64 {
            cell = (w * l / MAX_CELLS as f64).sqrt();
        }
        if cell > 0.25 * radius {
            return Err(GenError::Invalid(format!(
                "3D finish fill-openings width {width} mm is too fine for a {:.1} x {:.1} mm model: its land grid would pass {MAX_CELLS} cells - state a wider opening",
                hi[0] - lo[0],
                hi[1] - lo[1]
            )));
        }
        let nx = ((w / cell).ceil() as usize).max(1);
        let ny = ((l / cell).ceil() as usize).max(1);
        let mut g = Grid {
            nx,
            ny,
            x0: lo[0] - width,
            y0: lo[1] - width,
            cell,
            rc: radius / cell,
            z_floor: lo[2],
            h: vec![None; nx.saturating_mul(ny)],
        };
        for f in facets {
            g.raster(f);
        }
        Ok(g)
    }

    /// The facet's height over every cell centre its XY shadow covers.
    fn raster(&mut self, f: &Triangle) {
        let (a, b, c) = (f.a, f.b, f.c);
        let det = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
        if det.abs() < 1e-12 {
            return; // a wall: no top-down area
        }
        let span = |v: f64, o: f64| ((v - o) / self.cell - 0.5).floor().max(0.0) as usize;
        let i0 = span(a.x.min(b.x).min(c.x), self.x0);
        let i1 = span(a.x.max(b.x).max(c.x), self.x0).saturating_add(1).min(self.nx - 1);
        let j0 = span(a.y.min(b.y).min(c.y), self.y0);
        let j1 = span(a.y.max(b.y).max(c.y), self.y0).saturating_add(1).min(self.ny - 1);
        let eps = 1e-9;
        for j in j0..=j1 {
            let y = self.y0 + (j as f64 + 0.5) * self.cell;
            for i in i0..=i1 {
                let x = self.x0 + (i as f64 + 0.5) * self.cell;
                let u = ((b.x - x) * (c.y - y) - (c.x - x) * (b.y - y)) / det;
                let v = ((c.x - x) * (a.y - y) - (a.x - x) * (c.y - y)) / det;
                let w = 1.0 - u - v;
                if u < -eps || v < -eps || w < -eps {
                    continue;
                }
                let z = u * a.z + v * b.z + w * c.z;
                let k = j * self.nx + i;
                self.h[k] = Some(self.h[k].map_or(z, |o| o.max(z)));
            }
        }
    }

    fn floor_of(&self, i: usize) -> f64 {
        self.h[i].unwrap_or(self.z_floor)
    }

    fn disk(&self) -> Vec<(isize, isize)> {
        let r = self.rc.floor() as isize;
        let mut d = Vec::new();
        for dy in -r..=r {
            for dx in -r..=r {
                if ((dx * dx + dy * dy) as f64) <= self.rc * self.rc {
                    d.push((dx, dy));
                }
            }
        }
        d
    }

    /// Flat-disk grey closing; off-grid reads as the floor. The result
    /// is re-asserted never below the field itself.
    fn closing(&self) -> Vec<f64> {
        let disk = self.disk();
        let base: Vec<f64> = (0..self.h.len()).map(|i| self.floor_of(i)).collect();
        let pass = |src: &[f64], up: bool| -> Vec<f64> {
            let mut out = vec![self.z_floor; src.len()];
            for j in 0..self.ny {
                for i in 0..self.nx {
                    let mut acc = if up { f64::NEG_INFINITY } else { f64::INFINITY };
                    for &(dx, dy) in &disk {
                        let (x, y) = (i as isize + dx, j as isize + dy);
                        let v = if x < 0 || y < 0 || x >= self.nx as isize || y >= self.ny as isize {
                            self.z_floor
                        } else {
                            src[y as usize * self.nx + x as usize]
                        };
                        acc = if up { acc.max(v) } else { acc.min(v) };
                    }
                    out[j * self.nx + i] = acc;
                }
            }
            out
        };
        let mut c = pass(&pass(&base, true), false);
        for (ci, bi) in c.iter_mut().zip(&base) {
            *ci = ci.max(*bi);
        }
        c
    }

    fn n4(&self, i: usize) -> impl Iterator<Item = Option<usize>> + '_ {
        let (x, y) = (i % self.nx, i / self.nx);
        [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)].into_iter().map(move |(dx, dy)| {
            let (a, b) = (x as isize + dx, y as isize + dy);
            (a >= 0 && b >= 0 && a < self.nx as isize && b < self.ny as isize)
                .then(|| b as usize * self.nx + a as usize)
        })
    }

    fn ring8(&self, i: usize) -> Vec<usize> {
        let (x, y) = ((i % self.nx) as isize, (i / self.nx) as isize);
        let mut v = Vec::with_capacity(9);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (a, b) = (x + dx, y + dy);
                if a >= 0 && b >= 0 && a < self.nx as isize && b < self.ny as isize {
                    v.push(b as usize * self.nx + a as usize);
                }
            }
        }
        v
    }

    fn component(&self, start: usize, raised: &[bool], seen: &mut [bool]) -> Vec<usize> {
        let mut comp = vec![start];
        seen[start] = true;
        let mut k = 0;
        while k < comp.len() {
            let i = comp[k];
            k += 1;
            for j in self.n4(i).flatten() {
                if raised[j] && !seen[j] {
                    seen[j] = true;
                    comp.push(j);
                }
            }
        }
        comp
    }

    /// One flat plane per opening: lift every cell of a closed opening's
    /// lid - the raised component and its one-cell ring - to the
    /// highest closed height over all of them, so the lid has no steps
    /// anywhere on it. The closing's per-cell levels differ only by
    /// raster slop, but a stepped lid is worse than flat: across each
    /// step edge there is a full ball-width annulus where the ball
    /// rests on neither plane - the higher lid's top blocks it from
    /// seating on the lower one, and a lid top's free edge carries no
    /// fold support - so the support field goes None there and every
    /// step becomes a retract and plunge. One plane per opening removes
    /// that annulus; the highest level keeps every lid cell at or above
    /// its own closed height, so the lid still never digs below the
    /// closing.
    fn plateau(&self, closed: &mut [f64], cells: &[usize]) {
        let top = cells.iter().fold(f64::NEG_INFINITY, |m, &i| m.max(closed[i]));
        for &i in cells {
            closed[i] = top;
        }
    }

    /// Closed: no cell on the border, and every cell bordering the
    /// opening is material standing at its lid's level (within one
    /// cell's rise, the 45-degree step, plus `tol`) - a lid that would
    /// spill over a lower rim, like a slot's mouth, is an open feature.
    fn enclosed(&self, comp: &[usize], raised: &[bool], closed: &[f64], tol: f64) -> bool {
        comp.iter().all(|&i| {
            self.n4(i).all(|n| match n {
                None => false,
                Some(j) => {
                    raised[j]
                        || self.h[j].is_some_and(|z| z + self.cell + tol >= closed[i])
                }
            })
        })
    }

    /// Horizontal lids, one rectangle per run of equal height in a row.
    fn emit(&self, lid: &[bool], closed: &[f64], out: &mut Vec<Triangle>) {
        for j in 0..self.ny {
            let mut i = 0;
            while i < self.nx {
                let k = j * self.nx + i;
                if !lid[k] {
                    i += 1;
                    continue;
                }
                let z = closed[k];
                let mut e = i + 1;
                while e < self.nx && lid[j * self.nx + e] && closed[j * self.nx + e] == z {
                    e += 1;
                }
                let (xa, xb) = (self.x0 + i as f64 * self.cell, self.x0 + e as f64 * self.cell);
                let (ya, yb) = (self.y0 + j as f64 * self.cell, self.y0 + (j + 1) as f64 * self.cell);
                let p = |x, y| Point3::new(x, y, z);
                out.push(Triangle::new(p(xa, ya), p(xb, ya), p(xb, yb)));
                out.push(Triangle::new(p(xa, ya), p(xb, yb), p(xa, yb)));
                i = e;
            }
        }
    }
}

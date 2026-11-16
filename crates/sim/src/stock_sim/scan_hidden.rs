//! The hidden-rest scan (M2.2d) and its budget, split from `scan.rs`
//! by the 400-line law (M13.1). Provenance: the parity-paired solid
//! truth of `solid_rest` (ray-crossing parity, the published
//! point-in-solid test) judged by the envelope's own distance law,
//! streamed in row bands (out-of-core scanline practice); nothing
//! from vendor binaries.
//!
//! ## The law here
//!
//! - **Over budget is "not judged", never zero** (M13.1, paying the
//!   M2.2 debt the ROADMAP names): past its budget the scan cannot
//!   answer, and it says so through the report's type
//!   (`HiddenRest::OverBudget`) instead of the old stated zero that
//!   read exactly like "no hidden rest". Every consumer shows the
//!   unjudged state by name.
//! - **Past the holding budget the scan streams** (M13.1, #24): the
//!   `cells` / `claims` numbers bound what one pass HOLDS, not what
//!   the verdict may read. A lattice whose claims do not fit at once
//!   is counted row by row, cut into bands that do, and judged band
//!   after band by the very same body - each column sees the same
//!   claims, the same pairing, the same distance law, so the banded
//!   answer is the whole-lattice answer, bit for bit (rows ascend,
//!   ties keep the first worst). Nothing is sampled coarser: a
//!   skipped column could hide a ledge, and a false clean is a
//!   gouge-class bug.
//! - **The walk is bounded too.** Every facet-column visit and every
//!   ball-center distance is counted against `work`; past it, or when
//!   one row's claims alone cannot be held, the scan still answers
//!   `OverBudget` - a stated refusal, never a hang and never a zero.

use super::dexel::fy0;
use super::scan_arc::{seg_dist, ScanArc};
use super::scan_near::SegGrid;
use super::scan_turned::TurnedBand;

use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::solid_rest::Claim;

/// What the hidden-rest scan may spend: lattice cells held at once
/// (the bin headers, ~30 MB at the shipped 1.2 M), plane claims held
/// at once within them (40 M, ~1.6 GB), and the whole walk's `work` -
/// facet-column visits plus ball-center distances, across every band
/// (1 G, about ten seconds at worst). A stated budget, so a caller can name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanBudget {
    pub cells: usize,
    pub claims: usize,
    pub work: usize,
}

impl ScanBudget {
    /// The budget every shipped verdict scans under.
    pub const SHIPPED: ScanBudget =
        ScanBudget { cells: 1_200_000, claims: 40_000_000, work: 1_000_000_000 };
}

/// Whether the hidden-rest half of an envelope verdict was judged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HiddenRest {
    /// The scan ran (or had nothing to scan): `rest_mm` carries its
    /// answer, zero included as a real zero - banded or not.
    #[default]
    Judged,
    /// The scan stopped at its budget and judged NOTHING: `rest_mm`
    /// speaks for the ridge law alone, and material hidden under an
    /// overhang may stand unreported. `cells` is the lattice, `claims`
    /// the plane claims counted when it stopped (one row's, when that
    /// row alone cannot be held).
    OverBudget { cells: usize, claims: usize },
}

/// Why one pass stopped: too many claims to hold, or the walk's work
/// (with the claims counted by then).
enum Stop {
    Claims,
    Work(usize),
}

/// A facet's column span `(i0, i1, j0, j1)`.
type Span = Option<(usize, usize, usize, usize)>;

/// The scan's geometry frame, threaded once.
struct Frame<'a> {
    facets: &'a [Triangle],
    segs: &'a [(Point3, Point3)],
    arcs: &'a [Option<ScanArc>],
    grid: Option<SegGrid<'a>>,
    part: &'a [f64],
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    r: f64,
    floor: f64,
    /// A turning path's spindle band (#1408, `scan_turned`); `None`
    /// for a mill path.
    band: Option<&'a TurnedBand<'a>>,
}

impl Frame<'_> {
    /// The nearest ball-center distance to `q`, exact (`scan_near`),
    /// or an early bound at or under `best` (the module law there).
    fn nearest(&self, q: Point3, best: f64, hint: &mut Option<u32>, work: &mut usize) -> f64 {
        match &self.grid {
            Some(g) => g.nearest(q, (self.r, self.floor, best), hint, work),
            None => {
                *work = work.saturating_add(self.segs.len());
                self.segs
                    .iter()
                    .enumerate()
                    .map(|(i, s)| seg_dist(q, s, self.arcs.get(i).and_then(Option::as_ref)))
                    .fold(f64::INFINITY, f64::min)
            }
        }
    }

    /// A facet's span on the lattice, `None` for a non-finite facet
    /// or one wholly off it.
    fn span(&self, t: &Triangle) -> Span {
        let (a, b, c) = (t.a, t.b, t.c);
        if !(a.is_finite() && b.is_finite() && c.is_finite()) {
            return None;
        }
        let fx = |v: f64| ((v - self.x0) / self.cell).floor();
        let i0 = fx(a.x.min(b.x).min(c.x)).max(0.0) as usize;
        let i1 = fx(a.x.max(b.x).max(c.x)).clamp(0.0, self.nx as f64 - 1.0) as usize;
        let j0 = fy0(a.y.min(b.y).min(c.y), self.y0, self.cell).max(0.0) as usize;
        let j1 = fy0(a.y.max(b.y).max(c.y), self.y0, self.cell).clamp(0.0, self.ny as f64 - 1.0)
            as usize;
        (i0 < self.nx && j0 < self.ny && i0 <= i1 && j0 <= j1).then_some((i0, i1, j0, j1))
    }

    fn center(&self, ix: usize, iy: usize) -> (f64, f64) {
        (self.x0 + (ix as f64 + 0.5) * self.cell, self.y0 + (iy as f64 + 0.5) * self.cell)
    }
}

/// The hidden-rest scan, paying M2.2b's open debt. Per column the
/// mesh's SOLID truth is paired (`solid_rest` law (a)); every
/// material top below the height field's own top minus the
/// quantization floor is occluded material - invisible to a
/// Z-following measure by construction. Each occluded top is then
/// judged by the ridge law's own distance truth: nearest ball
/// center minus the ball minus the floor, so the two reports are
/// one yardstick reading two depths, never two predicates. An odd
/// column is an open shell - unknown, and unknown never reports
/// material (law (b)). Work is binned facet-outer (the
/// `part_surface` shape): the whole lattice at once when it fits, in
/// row bands when it does not (the module law); past the work
/// budget the answer is `Err(HiddenRest::OverBudget)` - not judged,
/// said by name.
#[allow(clippy::too_many_arguments)] // the scan's whole geometry frame, threaded once from envelope_scan
pub(super) fn hidden_rest(
    facets: &[Triangle],
    segs: &[(Point3, Point3)],
    arcs: &[Option<ScanArc>],
    part: &[f64],
    (x0, y0, cell): (f64, f64, f64),
    nx: usize,
    ny: usize,
    r: f64,
    floor: f64,
    budget: ScanBudget,
    band: Option<&TurnedBand<'_>>,
) -> Result<(f64, Point2), HiddenRest> {
    if segs.is_empty() || facets.is_empty() || nx == 0 || ny == 0 {
        return Ok((0.0, Point2::new(0.0, 0.0))); // nothing swept, or nothing to hide: judged
    }
    let cells = nx.saturating_mul(ny);
    if part.len() < cells {
        return Err(HiddenRest::OverBudget { cells, claims: 0 }); // no ground to judge on
    }
    let grid = SegGrid::new(segs, arcs, r.max(cell * 4.0));
    let f = Frame { facets, segs, arcs, grid, part, x0, y0, cell, nx, ny, r, floor, band };
    let spans: Vec<Span> = facets.iter().map(|t| f.span(t)).collect();
    let mut work = 0usize;
    if cells <= budget.cells {
        let all: Vec<usize> = (0..facets.len()).filter(|&k| spans[k].is_some()).collect();
        match judge_rows(&f, &spans, &all, (0, ny), budget, &mut work) {
            Ok(v) => return Ok(v),
            Err(Stop::Work(claims)) => return Err(HiddenRest::OverBudget { cells, claims }),
            Err(Stop::Claims) => {} // too many to hold at once: stream the bands
        }
    }
    banded(&f, &spans, budget, &mut work)
}

/// The streamed scan (module law): count each row's claims, cut the
/// rows into bands that fit both holding budgets, judge them in order.
fn banded(
    f: &Frame,
    spans: &[Span],
    budget: ScanBudget,
    work: &mut usize,
) -> Result<(f64, Point2), HiddenRest> {
    let cells = f.nx.saturating_mul(f.ny);
    let over = |claims| HiddenRest::OverBudget { cells, claims };
    let rows_cap = budget.cells / f.nx;
    if rows_cap == 0 {
        return Err(over(0)); // one row of bin headers cannot be held
    }
    let mut per_row = vec![0usize; f.ny];
    let mut total = 0usize;
    for (t, sp) in f.facets.iter().zip(spans) {
        let Some((i0, i1, j0, j1)) = *sp else { continue };
        for (iy, row) in per_row.iter_mut().enumerate().take(j1 + 1).skip(j0) {
            for ix in i0..=i1 {
                *work = work.saturating_add(1);
                let (cx, cy) = f.center(ix, iy);
                if crate::solid_rest::plane_claim(&t.a, &t.b, &t.c, cx, cy).is_some() {
                    *row += 1;
                    total += 1;
                }
            }
            if *work > budget.work {
                return Err(over(total));
            }
        }
    }
    if let Some(&row) = per_row.iter().find(|&&n| n > budget.claims) {
        return Err(over(row)); // a single row cannot be held
    }
    // Facets by first row: a sweep keeps only those a band can touch.
    let mut order: Vec<usize> = (0..f.facets.len()).filter(|&k| spans[k].is_some()).collect();
    order.sort_by_key(|&k| spans[k].map_or(0, |s| s.2));
    let (mut next, mut active) = (0usize, Vec::<usize>::new());
    let (mut best, mut at) = (0.0f64, Point2::new(0.0, 0.0));
    let mut ja = 0usize;
    while ja < f.ny {
        let (mut jb, mut held) = (ja, 0usize);
        while jb < f.ny && jb - ja < rows_cap && held + per_row[jb] <= budget.claims {
            held += per_row[jb];
            jb += 1;
        }
        while let Some(&k) = order.get(next) {
            if !spans[k].is_some_and(|s| s.2 < jb) {
                break;
            }
            active.push(k);
            next += 1;
        }
        active.retain(|&k| spans[k].is_some_and(|s| s.3 >= ja));
        match judge_rows(f, spans, &active, (ja, jb), budget, work) {
            Ok((rest, p)) => {
                if rest > best {
                    best = rest;
                    at = p;
                }
            }
            Err(_) => return Err(over(total)),
        }
        ja = jb;
    }
    Ok((best, at))
}

/// Bin and judge the rows `ja..jb` from the facets `active` - the one
/// body the whole lattice and every band run (module law).
fn judge_rows(
    f: &Frame,
    spans: &[Span],
    active: &[usize],
    (ja, jb): (usize, usize),
    budget: ScanBudget,
    work: &mut usize,
) -> Result<(f64, Point2), Stop> {
    let nx = f.nx;
    let mut bins: Vec<Vec<Claim>> = (0..nx * (jb - ja)).map(|_| Vec::new()).collect();
    let mut claims = 0usize;
    for &k in active {
        let (Some(t), Some(Some((i0, i1, j0, j1)))) = (f.facets.get(k), spans.get(k)) else {
            continue;
        };
        for iy in (*j0).max(ja)..=(*j1).min(jb.saturating_sub(1)) {
            for ix in *i0..=*i1 {
                *work = work.saturating_add(1);
                let (cx, cy) = f.center(ix, iy);
                if let Some(cl) = crate::solid_rest::plane_claim(&t.a, &t.b, &t.c, cx, cy) {
                    bins[ix + (iy - ja) * nx].push(cl);
                    claims += 1;
                    if claims > budget.claims {
                        return Err(Stop::Claims);
                    }
                }
            }
            if *work > budget.work {
                return Err(Stop::Work(claims));
            }
        }
    }
    let mut best = 0.0;
    let mut at = Point2::new(0.0, 0.0);
    let mut hint = None;
    // The band's reach (#1408): the ball, the floor, and the most an
    // arc piece stands off the chord whose ends set the band.
    let reach = f.r + f.floor + super::scan_arc::max_bulge(f.arcs);
    for iy in ja..jb {
        for ix in 0..nx {
            let bin = &bins[ix + (iy - ja) * nx];
            if bin.is_empty() {
                continue;
            }
            let t = f.part[ix + iy * nx];
            if !t.is_finite() {
                continue; // the field sees nothing here; nor has this scan's ground
            }
            let crate::solid_rest::Crossings::Paired(z) =
                crate::solid_rest::crossings_from_claims(bin)
            else {
                continue; // open shell: unknown never reports material
            };
            // Interval tops, high to low: the highest is the visible
            // surface; every lower one hides under a top above it.
            let chunks = z.as_chunks::<2>().0;
            for (k, c) in chunks.iter().enumerate() {
                let hi = c[1];
                // The part's next surface over this top: its recess's
                // roof (none known: infinite, and never an excuse).
                let roof = chunks.get(k + 1).map_or(f64::INFINITY, |n| n[0]);
                if hi >= t - f.floor {
                    continue; // not occluded: the ridge law already owns this top
                }
                let (cx, cy) = f.center(ix, iy);
                let p = Point3::new(cx, cy, hi);
                // #1408: a top out of every turning nose's reach, in a
                // recess the pass never stood level with, is another
                // op's floor (`scan_turned`); one the pass went into
                // and stopped short of is its rest (PR #1429 review).
                if f.band.is_some_and(|b| b.excuses(p, roof, reach)) {
                    continue;
                }
                // The nearest ball center, exact (`scan_near`); a top
                // that cannot raise the worst is let go at its first
                // proof, which can never change the answer. A turning
                // path reads the top on its orbit too (#1408).
                let mut dmin = f.nearest(p, best, &mut hint, work);
                for q in f.band.map(|b| b.turned(p)).unwrap_or_default() {
                    dmin = dmin.min(f.nearest(q, best, &mut hint, work));
                }
                // A turning op's leave is its promise (#1408, `scan_turned`).
                let leave = f.band.map_or(0.0, TurnedBand::leave);
                let rest = (dmin - f.r - f.floor - leave).max(0.0);
                // #1408: a crease narrower than a turning nose is no
                // pass's of this insert (`scan_turned`), asked only
                // where it could raise the worst.
                if rest > best && f.band.is_some_and(|b| !b.touchable(p)) {
                    continue;
                }
                if rest > best {
                    best = rest;
                    at = Point2::new(cx, cy);
                }
            }
            if *work > budget.work {
                return Err(Stop::Work(claims));
            }
        }
    }
    Ok((best, at))
}

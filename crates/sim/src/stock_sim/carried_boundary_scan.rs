//! Sub-pitch carried-stock workholding checks over continuous history (#1580).
//! Provenance: adaptive spatial subdivision (k-d bisection) pairing continuous stock
//! classification against box AABBs and closed fixture meshes.

use super::carry::Body;
use super::carry_boundary::RegionStock;
use crate::fixture::{FIXTURE_HIT_TOL, FixtureBox};
use crate::fixture_mesh::{BoxSteel, FixtureMesh};

const BOX_LEAF_EPS: f64 = 1e-3;
const MESH_LEAF_EPS: f64 = 2e-3;
const LOCAL_TRI_CAP: usize = 256;
const HISTORY_BUDGET: u64 = 50_000_000;

fn overlap(spans: &[(f64, f64)], lo: f64, hi: f64) -> f64 {
    spans.iter().map(|&(a, b)| (b.min(hi) - a.max(lo)).max(0.0)).sum()
}

fn spans_overlap(stock: &[(f64, f64)], steel: &[(f64, f64)], z: (f64, f64)) -> f64 {
    stock
        .iter()
        .map(|&(lo, hi)| {
            let (a, b) = (lo.max(z.0), hi.min(z.1));
            if b > a { overlap(steel, a, b) } else { 0.0 }
        })
        .sum()
}

fn take_max(worst: &mut Option<f64>, d: f64) {
    if d.is_finite() {
        *worst = Some(worst.map_or(d, |w| w.max(d)));
    }
}

fn ordered_box(b: &FixtureBox) -> Option<[(f64, f64); 3]> {
    let x = (b.min.x.min(b.max.x), b.min.x.max(b.max.x));
    let y = (b.min.y.min(b.max.y), b.min.y.max(b.max.y));
    let z = (b.min.z.min(b.max.z), b.min.z.max(b.max.z));
    [x.0, x.1, y.0, y.1, z.0, z.1].iter().all(|v| v.is_finite()).then_some([x, y, z])
}

fn sample_box_tile(
    body: &Body,
    x: (f64, f64),
    y: (f64, f64),
    z: (f64, f64),
    tol: f64,
    conservative_err: bool,
    worst: &mut Option<f64>,
) {
    let (cx, cy) = ((x.0 + x.1) * 0.5, (y.0 + y.1) * 0.5);
    for (px, py) in [(cx, cy), (x.0, y.0), (x.1, y.0), (x.0, y.1), (x.1, y.1)] {
        match body.eroded_spans_at(px, py, tol) {
            Ok(spans) => {
                let d = overlap(&spans, z.0, z.1);
                if d > tol {
                    take_max(worst, d);
                }
            }
            Err(_) if conservative_err => {
                take_max(worst, z.1 - z.0);
                return;
            }
            Err(_) => {}
        }
    }
}

fn choose_split_x(x: (f64, f64), y: (f64, f64), eps: f64, prefer_x: bool) -> bool {
    let (dx, dy) = (x.1 - x.0, y.1 - y.0);
    if prefer_x && dx > eps {
        true
    } else if !prefer_x && dy > eps {
        false
    } else {
        dx >= dy
    }
}

fn resolve_box_tile(
    body: &Body,
    bounds: [(f64, f64); 3],
    tol: f64,
    prefer_x: bool,
    budget: &mut u64,
    worst: &mut Option<f64>,
) {
    let [x, y, z] = bounds;
    let span_z = z.1 - z.0;
    if !(span_z.is_finite() && span_z > tol) || worst.is_some_and(|w| span_z <= w) {
        return;
    }
    match body.classify_region(x, y, z, tol, tol, budget) {
        RegionStock::Empty => {}
        RegionStock::Unread(reach) => {
            if reach > tol {
                take_max(worst, reach);
            }
        }
        RegionStock::Full(spans) => {
            let d = overlap(&spans, z.0, z.1);
            if d > tol {
                take_max(worst, d);
            }
        }
        RegionStock::Mixed(max_reach, z_tight) => {
            if !(max_reach.is_finite() && max_reach > tol) || worst.is_some_and(|w| max_reach <= w) {
                return;
            }
            let z = (z.0.max(z_tight.0), z.1.min(z_tight.1));
            let at_leaf = x.1 - x.0 <= BOX_LEAF_EPS && y.1 - y.0 <= BOX_LEAF_EPS;
            sample_box_tile(body, x, y, z, tol, at_leaf, worst);
            if at_leaf || worst.is_some_and(|w| w >= max_reach - 1e-12) {
                return;
            }
            if choose_split_x(x, y, BOX_LEAF_EPS, prefer_x) {
                let mid = (x.0 + x.1) * 0.5;
                resolve_box_tile(body, [(x.0, mid), y, z], tol, false, budget, worst);
                resolve_box_tile(body, [(mid, x.1), y, z], tol, false, budget, worst);
            } else {
                let mid = (y.0 + y.1) * 0.5;
                resolve_box_tile(body, [x, (y.0, mid), z], tol, true, budget, worst);
                resolve_box_tile(body, [x, (mid, y.1), z], tol, true, budget, worst);
            }
        }
    }
}

/// Refine a box-vs-carried-stock check over sub-pitch boundaries when
/// continuous history is retained.
#[must_use]
pub fn boundary_box_penetration(body: &Body, b: &FixtureBox, mut worst: Option<f64>) -> Option<f64> {
    let Some([bx, by, bz]) = ordered_box(b) else { return worst };
    let Some((hlo, hhi)) = body.continuous_bounds() else { return worst };
    let tol = FIXTURE_HIT_TOL;
    let z_keep = (hlo.z.max(bz.0), hhi.z.min(bz.1));
    let max_z = z_keep.1 - z_keep.0;
    if !(max_z.is_finite() && max_z > tol) || worst.is_some_and(|w| max_z <= w) {
        return worst;
    }
    let fx = if bx.1 - bx.0 > 2.0 * tol { (bx.0 + tol, bx.1 - tol) } else { ((bx.0 + bx.1) * 0.5, (bx.0 + bx.1) * 0.5) };
    let fy = if by.1 - by.0 > 2.0 * tol { (by.0 + tol, by.1 - tol) } else { ((by.0 + by.1) * 0.5, (by.0 + by.1) * 0.5) };
    let (qx0, qx1) = (fx.0.max(hlo.x + tol), fx.1.min(hhi.x - tol));
    let (qy0, qy1) = (fy.0.max(hlo.y + tol), fy.1.min(hhi.y - tol));
    if qx0 > qx1 || qy0 > qy1 {
        return worst;
    }
    let mut budget = HISTORY_BUDGET;
    resolve_box_tile(body, [(qx0, qx1), (qy0, qy1), z_keep], tol, qx1 - qx0 >= qy1 - qy0, &mut budget, &mut worst);
    worst
}

struct MeshQuery<'a> {
    body: &'a Body,
    mesh: &'a FixtureMesh,
    tris: Option<&'a [usize]>,
    tol: f64,
}

impl<'a> MeshQuery<'a> {
    fn judge_box(&self, bounds: [(f64, f64); 3], reach: f64, budget: &mut u64, worst: &mut Option<f64>) {
        match self.mesh.steel_in_axis_box(self.tris, bounds, 2, self.tol, budget) {
            BoxSteel::Clear => {}
            BoxSteel::Boundary(d) | BoxSteel::WhollyInside(d) => {
                take_max(worst, d.min(reach).max(f64::MIN_POSITIVE));
            }
            BoxSteel::Unread => take_max(worst, reach),
        }
    }

    fn check_full(&self, bounds: [(f64, f64); 3], spans: &[(f64, f64)], inside: bool, budget: &mut u64, worst: &mut Option<f64>) {
        let [x, y, z] = bounds;
        for &(lo, hi) in spans {
            let (zlo, zhi) = (lo.max(z.0), hi.min(z.1));
            let reach = zhi - zlo;
            if !(reach.is_finite() && reach > self.tol) || worst.is_some_and(|w| reach <= w) {
                continue;
            }
            if inside {
                take_max(worst, reach);
            } else {
                self.judge_box([x, y, (zlo, zhi)], reach, budget, worst);
            }
        }
    }

    fn leaf(&self, bounds: [(f64, f64); 3], inside: bool, max_reach: f64, budget: &mut u64, worst: &mut Option<f64>) {
        let [x, y, z] = bounds;
        let (cx, cy) = ((x.0 + x.1) * 0.5, (y.0 + y.1) * 0.5);
        for (px, py) in [(cx, cy), (x.0, y.0), (x.1, y.0), (x.0, y.1), (x.1, y.1)] {
            let Ok(stock) = self.body.eroded_spans_at(px, py, self.tol) else {
                take_max(worst, max_reach);
                return;
            };
            if stock.is_empty() {
                continue;
            }
            let d = if inside {
                overlap(&stock, z.0, z.1)
            } else {
                let steel = match self.tris {
                    Some(list) => self.mesh.steel_on_line_local(list, px, py, budget),
                    None => self.mesh.steel_on_line(px, py, budget),
                };
                let Some(steel) = steel else {
                    take_max(worst, max_reach);
                    return;
                };
                spans_overlap(&stock, &steel, z)
            };
            if d > self.tol {
                take_max(worst, d);
            }
        }
    }

    fn resolve(&self, bounds: [(f64, f64); 3], inside: bool, prefer_x: bool, budget: &mut u64, worst: &mut Option<f64>) {
        let [x, y, z] = bounds;
        let span_z = z.1 - z.0;
        if !(span_z.is_finite() && span_z > self.tol) || worst.is_some_and(|w| span_z <= w) {
            return;
        }
        let mut cls_budget = HISTORY_BUDGET;
        match self.body.classify_region(x, y, z, self.tol, self.tol, &mut cls_budget) {
            RegionStock::Empty => {}
            RegionStock::Unread(reach) => {
                if reach > self.tol && !worst.is_some_and(|w| reach <= w) {
                    if inside {
                        take_max(worst, reach);
                    } else {
                        self.judge_box([x, y, z], reach, budget, worst);
                    }
                }
            }
            RegionStock::Full(spans) => self.check_full([x, y, z], &spans, inside, budget, worst),
            RegionStock::Mixed(max_reach, z_tight) => {
                if !(max_reach.is_finite() && max_reach > self.tol) || worst.is_some_and(|w| max_reach <= w) {
                    return;
                }
                let z = (z.0.max(z_tight.0), z.1.min(z_tight.1));
                let (inside_next, guided) = if inside {
                    (true, None)
                } else {
                    let (steel, [hx, hy]) = self.mesh.steel_in_axis_box_uv(self.tris, [x, y, z], 2, self.tol, budget);
                    match steel {
                        BoxSteel::Clear => return,
                        BoxSteel::Unread => {
                            take_max(worst, max_reach);
                            return;
                        }
                        BoxSteel::Boundary(d) => {
                            if d <= self.tol || worst.is_some_and(|w| d <= w) {
                                return;
                            }
                            let plane = |span: (f64, f64), hit: (f64, f64)| -> Option<f64> {
                                const GUARD: f64 = 1e-4;
                                const STEP: f64 = 1e-6;
                                if hit.0 - span.0 > GUARD && span.1 - hit.0 > GUARD {
                                    Some(hit.0 - STEP)
                                } else if hit.1 - span.0 > GUARD && span.1 - hit.1 > GUARD {
                                    Some(hit.1 + STEP)
                                } else {
                                    None
                                }
                            };
                            let split = plane(x, hx).map(|m| (true, m)).or_else(|| plane(y, hy).map(|m| (false, m)));
                            (false, split)
                        }
                        BoxSteel::WhollyInside(d) => {
                            if d <= self.tol || worst.is_some_and(|w| d <= w) {
                                return;
                            }
                            (true, None)
                        }
                    }
                };
                if x.1 - x.0 <= MESH_LEAF_EPS && y.1 - y.0 <= MESH_LEAF_EPS {
                    self.leaf([x, y, z], inside_next, max_reach, budget, worst);
                    return;
                }
                if !inside_next && self.tris.is_none() && x.1 - x.0 <= 2.0 && y.1 - y.0 <= 2.0 && *budget > LOCAL_TRI_CAP as u64 * 4 {
                    let Some(tris_opt) = self.mesh.collect_tris_in_xy(x, y, 1e-3, LOCAL_TRI_CAP, budget) else {
                        take_max(worst, max_reach);
                        return;
                    };
                    if let Some(list) = tris_opt {
                        if list.is_empty() {
                            return;
                        }
                        MeshQuery { tris: Some(&list), ..*self }.resolve([x, y, z], inside_next, prefer_x, budget, worst);
                        return;
                    }
                }
                let (split_x, mid) = match guided {
                    Some((sx, m)) => (sx, m),
                    None => {
                        let sx = choose_split_x(x, y, MESH_LEAF_EPS, prefer_x);
                        let m = if sx { (x.0 + x.1) * 0.5 } else { (y.0 + y.1) * 0.5 };
                        (sx, m)
                    }
                };
                if split_x {
                    self.resolve([(x.0, mid), y, z], inside_next, false, budget, worst);
                    if !worst.is_some_and(|w| w >= max_reach - 1e-12) {
                        self.resolve([(mid, x.1), y, z], inside_next, false, budget, worst);
                    }
                } else {
                    self.resolve([x, (y.0, mid), z], inside_next, true, budget, worst);
                    if !worst.is_some_and(|w| w >= max_reach - 1e-12) {
                        self.resolve([x, (mid, y.1), z], inside_next, true, budget, worst);
                    }
                }
            }
        }
    }
}

/// Resolve sub-pitch side-face and step contact between `mesh` (with world
/// AABB `b`) and `body` when continuous history is retained.
#[must_use]
pub fn boundary_mesh_penetration(
    body: &Body,
    b: &FixtureBox,
    mesh: &FixtureMesh,
    tol: f64,
    budget: &mut u64,
    mut worst: Option<f64>,
) -> Option<f64> {
    let Some([bx, by, bz]) = ordered_box(b) else { return worst };
    let Some((hlo, hhi)) = body.continuous_bounds() else { return worst };
    let z_keep = (hlo.z.max(bz.0), hhi.z.min(bz.1));
    let max_z = z_keep.1 - z_keep.0;
    if !(max_z.is_finite() && max_z > tol) || worst.is_some_and(|w| max_z <= w) {
        return worst;
    }
    let (qx0, qx1) = (bx.0.max(hlo.x + tol), bx.1.min(hhi.x - tol));
    let (qy0, qy1) = (by.0.max(hlo.y + tol), by.1.min(hhi.y - tol));
    if qx0 >= qx1 || qy0 >= qy1 {
        return worst;
    }
    MeshQuery { body, mesh, tris: None, tol }.resolve([(qx0, qx1), (qy0, qy1), z_keep], false, qx1 - qx0 >= qy1 - qy0, budget, &mut worst);
    worst
}

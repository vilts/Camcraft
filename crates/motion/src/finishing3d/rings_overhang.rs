//! Provenance: overhang column clearance and ring-segment trimming for finishing3d::rings (#1840).

use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::stock_sim::dist_seg_tri;

use super::super::params::SPLIT_MZ;
use super::super::support::{Sup, dist2_pt_tri};
use super::split_true_runs;

/// Parameters for vertical column clearance along one ring level.
pub(super) struct ColumnLaw<'a> {
    pub(super) cz: f64,
    pub(super) top: f64,
    pub(super) wall_lim: f64,
    pub(super) strict_lim: f64,
    pub(super) r: f64,
    pub(super) sups: &'a [Sup],
}

/// Column clearance that enforces `wall_lim = r - tol` along the existing
/// level wall and `strict_lim = r - min(tol, 0.005)` wherever overhead or
/// upper-tier facets come strictly closer above `cz` than at `cz` itself.
fn overhang_column_clear(q: Point2, law: &ColumnLaw<'_>) -> bool {
    let (x, y, cz, top_cz) = (q.x, q.y, law.cz, law.top);
    let (hi, lo) = (top_cz.max(cz), top_cz.min(cz));
    let (a, b, c) = (Point3::new(x, y, hi), Point3::new(x, y, lo), Point3::new(x, y, cz));
    let mut min_col = f64::INFINITY;
    let mut upper_tier_hit = false;
    for s in law.sups {
        if x < s.xmin || x > s.xmax || y < s.ymin || y > s.ymax {
            continue;
        }
        let d = dist_seg_tri(a, b, &Triangle::new(s.tri[0], s.tri[1], s.tri[2]));
        if d < law.wall_lim {
            return false;
        }
        min_col = min_col.min(d);
        if d < law.strict_lim && !upper_tier_hit {
            let tri_zmin = s.tri[0].z.min(s.tri[1].z).min(s.tri[2].z);
            if s.m.z <= SPLIT_MZ && tri_zmin > cz + 1e-4 && d < dist2_pt_tri(c, &s.tri).sqrt() - 1e-4
            {
                upper_tier_hit = true;
            }
        }
    }
    if upper_tier_hit {
        return false;
    }
    if min_col >= law.strict_lim {
        return true;
    }
    let mut min_cz = f64::INFINITY;
    for s in law.sups {
        if x < s.xmin || x > s.xmax || y < s.ymin || y > s.ymax {
            continue;
        }
        min_cz = min_cz.min(dist2_pt_tri(c, &s.tri).sqrt());
    }
    min_col >= min_cz - 1e-4
}

/// Binary-search the overhang clearance boundary on an edge crossing.
fn bisect_column_edge(
    mut ok: Point2,
    mut bad: Point2,
    is_clear: &impl Fn(Point2) -> bool,
) -> Point2 {
    for _ in 0..10 {
        let mid = Point2::new(0.5 * (ok.x + bad.x), 0.5 * (ok.y + bad.y));
        if is_clear(mid) {
            ok = mid;
        } else {
            bad = mid;
        }
    }
    ok
}

/// Trim a ring or run against overhead solid material using `overhang_column_clear`.
/// Subdivides long edges and bisects boundary crossings so no segment passes
/// beneath an overhang; returns surviving clear runs and whether any part was trimmed.
pub(super) fn trim_blocked_columns(
    pts: &[Point2],
    closed: bool,
    law: &ColumnLaw<'_>,
) -> (Vec<(Vec<Point2>, bool)>, bool) {
    if pts.len() < 2 {
        return (Vec::new(), false);
    }
    let is_clear = |q: Point2| overhang_column_clear(q, law);
    let step = (law.r * 0.1).clamp(0.05, 0.25);
    let n = pts.len();
    let edge_count = if closed { n } else { n - 1 };
    let mut dense_pts: Vec<Point2> = Vec::with_capacity(n);
    let mut dense_flags: Vec<bool> = Vec::with_capacity(n);
    let mut any_blocked = false;
    let mut ca = is_clear(pts[0]);
    for i in 0..edge_count {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        dense_pts.push(a);
        dense_flags.push(ca);
        if !ca {
            any_blocked = true;
        }
        let len = (b.x - a.x).hypot(b.y - a.y);
        let m = (len / step).ceil().max(1.0) as usize;
        let (mut prev_q, mut prev_c) = (a, ca);
        let mut cb = ca;
        for k in 1..=m {
            let u = k as f64 / m as f64;
            let next_q = if k == m {
                b
            } else {
                Point2::new(a.x + (b.x - a.x) * u, a.y + (b.y - a.y) * u)
            };
            let next_c = is_clear(next_q);
            if k == m {
                cb = next_c;
            }
            if next_c != prev_c {
                any_blocked = true;
                let edge_clear = if prev_c {
                    bisect_column_edge(prev_q, next_q, &is_clear)
                } else {
                    bisect_column_edge(next_q, prev_q, &is_clear)
                };
                dense_pts.push(edge_clear);
                dense_flags.push(true);
            }
            if k < m {
                if !next_c {
                    any_blocked = true;
                }
                dense_pts.push(next_q);
                dense_flags.push(next_c);
            }
            prev_q = next_q;
            prev_c = next_c;
        }
        ca = cb;
    }
    if !closed {
        dense_pts.push(pts[n - 1]);
        dense_flags.push(ca);
        if !ca {
            any_blocked = true;
        }
    }
    if !any_blocked {
        return (vec![(pts.to_vec(), closed)], false);
    }
    (split_true_runs(&dense_pts, &dense_flags, closed), true)
}

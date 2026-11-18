//! Conservative bounds and pruning predicates for continuous carried-stock regions (#1580).
//! Provenance: interval arithmetic, 3-D convex cut pruning, and stencil tests over history nodes.

use std::f64::consts::{PI, TAU};

use super::carry_exact::Frame;
use super::carry_history::{Clip, Cut, History, Shape};
use super::dexel_arc::ArcSweep;
use crate::geometry::Point3;

pub(super) fn box_through_frame_back(frame: Frame, x: (f64, f64), y: (f64, f64), z: (f64, f64)) -> (Point3, Point3) {
    let mut lo = Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut hi = Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &px in &[x.0, x.1] {
        for &py in &[y.0, y.1] {
            for &pz in &[z.0, z.1] {
                let q = frame.back(Point3::new(px, py, pz));
                lo = Point3::new(lo.x.min(q.x), lo.y.min(q.y), lo.z.min(q.z));
                hi = Point3::new(hi.x.max(q.x), hi.y.max(q.y), hi.z.max(q.z));
            }
        }
    }
    (lo, hi)
}

fn dist2d_seg(qx: f64, qy: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let (dx, dy) = (bx - ax, by - ay);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((qx - ax) * dx + (qy - ay) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (qx - (ax + t * dx)).hypot(qy - (ay + t * dy))
}

fn eroded_z_window(dir_z: f64, z_keep: (f64, f64), z_tol: f64, flat_z: bool) -> Result<Option<(f64, f64, bool)>, ()> {
    let (z0, z1, strict) = if dir_z > 0.0 {
        (z_keep.0 + z_tol, z_keep.1, false)
    } else if dir_z < 0.0 {
        (z_keep.0, z_keep.1 - z_tol, false)
    } else if flat_z {
        (z_keep.0, z_keep.1, true)
    } else {
        return Err(());
    };
    Ok((z0 <= z1).then_some((z0, z1, strict)))
}

pub(super) fn cut_current_bounds(cut: &Cut) -> (Point3, Point3) {
    let m = &cut.profile.motion;
    let (ox0, oy0, ox1, oy1, z0) = match &cut.shape {
        Shape::Sweep(s) => {
            let (p0, p1) = s.path();
            let lat = s.reach().0;
            let lift = if cut.profile.ball { cut.profile.radius } else { 0.0 };
            (p0.x.min(p1.x) - lat, p0.y.min(p1.y) - lat, p0.x.max(p1.x) + lat, p0.y.max(p1.y) + lat, p0.z.min(p1.z) - lift)
        }
        Shape::Cone(_) => {
            let r = cut.profile.radius;
            if let Some(sw) = m.arc.filter(|a| a.plane.is_xy()).and_then(|a| ArcSweep::of(m, a)) {
                (sw.ox - sw.rad - r, sw.oy - sw.rad - r, sw.ox + sw.rad + r, sw.oy + sw.rad + r, m.from.z.min(m.to.z))
            } else if m.arc.is_some() {
                let (mut x0, mut y0, mut z0) = (m.from.x, m.from.y, m.from.z);
                let (mut x1, mut y1) = (m.from.x, m.from.y);
                for (a, b) in m.segments3(cut.profile.chord_tol) {
                    for p in [a, b] {
                        x0 = x0.min(p.x);
                        y0 = y0.min(p.y);
                        z0 = z0.min(p.z);
                        x1 = x1.max(p.x);
                        y1 = y1.max(p.y);
                    }
                }
                (x0 - r, y0 - r, x1 + r, y1 + r, z0)
            } else {
                (m.from.x.min(m.to.x) - r, m.from.y.min(m.to.y) - r, m.from.x.max(m.to.x) + r, m.from.y.max(m.to.y) + r, m.from.z.min(m.to.z))
            }
        }
    };
    let (mut cx0, mut cx1) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut cy0, mut cy1) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut cz0, mut cz1) = (f64::INFINITY, f64::NEG_INFINITY);
    for &ox in &[ox0, ox1] {
        for &oy in &[oy0, oy1] {
            let p = cut.frame.apply(Point3::new(ox, oy, z0));
            cx0 = cx0.min(p.x);
            cx1 = cx1.max(p.x);
            cy0 = cy0.min(p.y);
            cy1 = cy1.max(p.y);
            cz0 = cz0.min(p.z);
            cz1 = cz1.max(p.z);
        }
    }
    let dir = [cut.frame.q[0][2], cut.frame.q[1][2], cut.frame.q[2][2]];
    if dir[0] > 0.0 { cx1 = f64::INFINITY; } else if dir[0] < 0.0 { cx0 = f64::NEG_INFINITY; }
    if dir[1] > 0.0 { cy1 = f64::INFINITY; } else if dir[1] < 0.0 { cy0 = f64::NEG_INFINITY; }
    if dir[2] > 0.0 { cz1 = f64::INFINITY; } else if dir[2] < 0.0 { cz0 = f64::NEG_INFINITY; }
    (Point3::new(cx0, cy0, cz0), Point3::new(cx1, cy1, cz1))
}

#[derive(Clone, Copy, Debug)]
pub(super) enum CutRegion {
    RemovesAll,
    UniformRemove(f64, f64),
    Straddles,
}

fn box_in_arc_sweep(sw: &ArcSweep, r: f64, rx: (f64, f64), ry: (f64, f64)) -> bool {
    let corners = [(rx.0, ry.0), (rx.1, ry.0), (rx.0, ry.1), (rx.1, ry.1)];
    if corners.iter().all(|&(px, py)| (px - sw.from.x).hypot(py - sw.from.y) <= r)
        || corners.iter().all(|&(px, py)| (px - sw.to.x).hypot(py - sw.to.y) <= r)
    {
        return true;
    }
    let nx = if sw.ox < rx.0 { rx.0 - sw.ox } else if sw.ox > rx.1 { sw.ox - rx.1 } else { 0.0 };
    let ny = if sw.oy < ry.0 { ry.0 - sw.oy } else if sw.oy > ry.1 { sw.oy - ry.1 } else { 0.0 };
    let d_min = nx.hypot(ny);
    let d_max = (sw.ox - rx.0).abs().max((sw.ox - rx.1).abs()).hypot((sw.oy - ry.0).abs().max((sw.oy - ry.1).abs()));
    if d_min < (sw.rad - r).max(0.0) || d_max > sw.rad + r {
        return false;
    }
    if sw.span >= TAU - 1e-12 {
        return true;
    }
    if d_min <= 0.0 {
        return false;
    }
    let (mut u_min, mut u_max) = (f64::INFINITY, f64::NEG_INFINITY);
    for &(px, py) in &corners {
        let u = (sw.dir * ((py - sw.oy).atan2(px - sw.ox) - sw.a0)).rem_euclid(TAU);
        if u > sw.span {
            return false;
        }
        u_min = u_min.min(u);
        u_max = u_max.max(u);
    }
    u_max - u_min < PI - 1e-12
}

pub(super) fn cut_effect(
    cut: &Cut,
    x: (f64, f64),
    y: (f64, f64),
    z_keep: (f64, f64),
    xy_tol: f64,
    z_tol: f64,
    offsets: &[(f64, f64)],
) -> CutRegion {
    if !cut.profile.flat_endmill() {
        return CutRegion::Straddles;
    }
    let m = &cut.profile.motion;
    let r = cut.profile.radius;
    let flat_z = m.from.z == m.to.z;
    let z_cut = m.from.z.max(m.to.z);
    let qz = cut.frame.q[2][2];
    if flat_z && cut.frame.q[2][0] == 0.0 && cut.frame.q[2][1] == 0.0 && qz != 0.0 {
        let arc = m.arc.filter(|a| a.plane.is_xy()).and_then(|a| ArcSweep::of(m, a));
        let rect_in = |ox0: f64, ox1: f64, oy0: f64, oy1: f64| match &arc {
            None if m.arc.is_none() => [(ox0, oy0), (ox1, oy0), (ox0, oy1), (ox1, oy1)].into_iter().all(|(px, py)| {
                let q = cut.frame.back(Point3::new(px, py, 0.0));
                dist2d_seg(q.x, q.y, m.from.x, m.from.y, m.to.x, m.to.y) <= r
            }),
            Some(sw) => {
                let (mut rx0, mut rx1, mut ry0, mut ry1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
                for (px, py) in [(ox0, oy0), (ox1, oy0), (ox0, oy1), (ox1, oy1)] {
                    let q = cut.frame.back(Point3::new(px, py, 0.0));
                    rx0 = rx0.min(q.x);
                    rx1 = rx1.max(q.x);
                    ry0 = ry0.min(q.y);
                    ry1 = ry1.max(q.y);
                }
                box_in_arc_sweep(sw, r, (rx0, rx1), (ry0, ry1))
            }
            _ => false,
        };
        if rect_in(x.0 - xy_tol, x.1 + xy_tol, y.0 - xy_tol, y.1 + xy_tol)
            || offsets.iter().any(|&(dx, dy)| rect_in(x.0 + dx, x.1 + dx, y.0 + dy, y.1 + dy))
        {
            let wz = cut.frame.apply(Point3::new(0.0, 0.0, z_cut)).z;
            return if qz > 0.0 {
                CutRegion::UniformRemove(wz, f64::INFINITY)
            } else {
                CutRegion::UniformRemove(f64::NEG_INFINITY, wz)
            };
        }
        return CutRegion::Straddles;
    }
    if m.arc.is_some() {
        return CutRegion::Straddles;
    }
    if qz == 0.0 && flat_z && let Some((u0, u1)) = side_uniform_remove(cut, x, y, xy_tol, offsets) {
        return CutRegion::UniformRemove(u0, u1);
    }
    let Ok(win) = eroded_z_window(qz, z_keep, z_tol, flat_z) else {
        return CutRegion::Straddles;
    };
    let Some((z0, z1, strict)) = win else {
        return CutRegion::RemovesAll;
    };
    let removes = offsets.iter().any(|&(dx, dy)| {
        for &px in &[x.0 + dx, x.1 + dx] {
            for &py in &[y.0 + dy, y.1 + dy] {
                for &pz in &[z0, z1] {
                    let q = cut.frame.back(Point3::new(px, py, pz));
                    let z_ok = if strict { q.z > z_cut } else { q.z >= z_cut };
                    if !z_ok || dist2d_seg(q.x, q.y, m.from.x, m.from.y, m.to.x, m.to.y) > r {
                        return false;
                    }
                }
            }
        }
        true
    });
    if removes { CutRegion::RemovesAll } else { CutRegion::Straddles }
}

fn side_uniform_remove(
    cut: &Cut,
    x: (f64, f64),
    y: (f64, f64),
    xy_tol: f64,
    offsets: &[(f64, f64)],
) -> Option<(f64, f64)> {
    let w_ax = if cut.frame.q[2][0].abs() == 1.0 && cut.frame.q[2][1] == 0.0 {
        0
    } else if cut.frame.q[2][1].abs() == 1.0 && cut.frame.q[2][0] == 0.0 {
        1
    } else {
        return None;
    };
    let m = &cut.profile.motion;
    let r = cut.profile.radius;
    let (w_from, w_to, h_from, h_to) = if w_ax == 0 {
        (m.from.x, m.to.x, m.from.y, m.to.y)
    } else {
        (m.from.y, m.to.y, m.from.x, m.to.x)
    };
    if w_from != w_to {
        return None;
    }
    let h_coord = |px: f64, py: f64| {
        let q = cut.frame.back(Point3::new(px, py, 0.0));
        (if w_ax == 0 { q.y } else { q.x }, q.z)
    };
    let (mut qh_lo, mut qh_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (px, py) in [(x.0 - xy_tol, y.0 - xy_tol), (x.1 + xy_tol, y.0 - xy_tol), (x.0 - xy_tol, y.1 + xy_tol), (x.1 + xy_tol, y.1 + xy_tol)] {
        let (qh, _) = h_coord(px, py);
        qh_lo = qh_lo.min(qh);
        qh_hi = qh_hi.max(qh);
    }
    if qh_lo < h_from.min(h_to) || qh_hi > h_from.max(h_to) {
        return None;
    }
    let z_cut = m.from.z;
    let deep_in = offsets.iter().any(|&(dx, dy)| {
        [(x.0 + dx, y.0 + dy), (x.1 + dx, y.0 + dy), (x.0 + dx, y.1 + dy), (x.1 + dx, y.1 + dy)]
            .into_iter()
            .all(|(px, py)| h_coord(px, py).1 > z_cut)
    });
    if !deep_in {
        return None;
    }
    let pt = |w: f64| if w_ax == 0 { Point3::new(w, 0.0, 0.0) } else { Point3::new(0.0, w, 0.0) };
    let (za, zb) = (cut.frame.apply(pt(w_from - r)).z, cut.frame.apply(pt(w_from + r)).z);
    Some((za.min(zb), za.max(zb)))
}

pub(super) fn clip_effect(
    clip: &Clip,
    x: (f64, f64),
    y: (f64, f64),
    z_keep: (f64, f64),
    xy_tol: f64,
    z_tol: f64,
    offsets: &[(f64, f64)],
) -> (bool, bool) {
    let (glo, ghi) = box_through_frame_back(clip.frame, (x.0 - xy_tol, x.1 + xy_tol), (y.0 - xy_tol, y.1 + xy_tol), z_keep);
    let g = &clip.grid;
    let cell_span = |a: f64, b: f64, o: f64, n: usize| {
        (((a - o) / g.cell).floor().clamp(0.0, n as f64) as usize, ((b - o) / g.cell).ceil().clamp(0.0, n as f64) as usize)
    };
    let (ix0, ix1) = cell_span(glo.x, ghi.x, g.x0, g.nx);
    let (iy0, iy1) = cell_span(glo.y, ghi.y, g.y0, g.ny);
    if ix0 >= ix1 || iy0 >= iy1 {
        return (false, false);
    }
    let touches = (iy0..iy1).any(|iy| (ix0..ix1).any(|ix| {
        let top = g.top_at(ix + iy * g.nx);
        top.is_finite() && top < ghi.z
    }));
    if !touches {
        return (false, false);
    }
    let Some((z0, z1, strict)) = eroded_z_window(clip.frame.q[2][2], z_keep, z_tol, true).unwrap_or(None) else {
        return (true, true);
    };
    let removes_all = offsets.iter().any(|&(dx, dy)| {
        let (lo, hi) = box_through_frame_back(clip.frame, (x.0 + dx, x.1 + dx), (y.0 + dy, y.1 + dy), (z0, z1));
        if lo.x < g.x0 || hi.x > g.x0 + g.nx as f64 * g.cell || lo.y < g.y0 || hi.y > g.y0 + g.ny as f64 * g.cell {
            return false;
        }
        let (cx0, cx1) = cell_span(lo.x, hi.x, g.x0, g.nx);
        let (cy0, cy1) = cell_span(lo.y, hi.y, g.y0, g.ny);
        cx0 < cx1 && cy0 < cy1 && (cy0..cy1).all(|iy| (cx0..cx1).all(|ix| {
            let top = g.top_at(ix + iy * g.nx);
            top.is_finite() && if strict { top < lo.z } else { top <= lo.z }
        }))
    });
    (removes_all, true)
}

pub(super) fn cylinder_check(h: &History, cyl: (f64, f64, f64), bounds: [(f64, f64); 3], xy_tol: f64) -> (bool, bool) {
    let (cx, cy, r) = cyl;
    let [x, y, z_keep] = bounds;
    let c = h.frame.apply(Point3::new(cx, cy, 0.0));
    let dir = [h.frame.q[0][2], h.frame.q[1][2], h.frame.q[2][2]];
    let axis_dist = |lo: f64, hi: f64, v: f64| -> (f64, f64) {
        let near = if v < lo { lo - v } else if v > hi { v - hi } else { 0.0 };
        (near, (v - lo).abs().max((v - hi).abs()))
    };
    if dir[2] != 0.0 {
        let er = r - xy_tol;
        if er <= 0.0 {
            return (false, false);
        }
        let ((nx, fx), (ny, fy)) = (axis_dist(x.0, x.1, c.x), axis_dist(y.0, y.1, c.y));
        return (nx * nx + ny * ny <= er * er, fx * fx + fy * fy <= er * er);
    }
    let (perp, pc, zc) = if dir[0] != 0.0 { (y, c.y, c.z) } else { (x, c.x, c.z) };
    let ((np, fp), (nz, fz)) = (axis_dist(perp.0, perp.1, pc), axis_dist(z_keep.0, z_keep.1, zc));
    if np >= r - xy_tol || nz >= r || (np + xy_tol).hypot(nz) > r {
        return (false, false);
    }
    (true, (fp + xy_tol).hypot(fz) <= r)
}

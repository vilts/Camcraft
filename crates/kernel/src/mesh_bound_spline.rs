//! Certified continuous spline and revolved surface Hessian bounds
//! (M12.1, #1029).
//! Provenance: our own implementation of Peano/Taylor C^{1,1} linear
//! interpolation remainder `1/8 ||D^2 S||_inf` using de Boor blossom span
//! restriction ([`crate::mesh_bound_blossom`]) and rational Bernstein/Bezier
//! hodograph bounds (Piegl & Tiller, *The NURBS Book*, eqs. 3.3-3.8 and
//! section 4.5 quotient rule).

use solidcore::eval::plan::{
    EvalCurve, EvalSurface,
    uv::{Domain, closed_curve, unwrap},
};

use crate::P3;
use crate::mesh_bound::{dot, norm, norm2, scale, sub};
use crate::mesh_bound_blossom::{
    SplineDir, W, blossom_restrict_1d, cart, collect_indices, h_diff, h_shift, jump_bound,
    safe_sub_interval, span_box_stats, spans_and_kinks,
};

fn bezier_hodograph(
    p: usize,
    width: f64,
    p0: P3,
    axis_opt: Option<P3>,
    at: impl Fn(usize) -> [f64; 4],
) -> [f64; 5] {
    let s1 = p as f64 / width;
    let s2 = (p.saturating_sub(1)) as f64 / width;
    let (mut ma1, mut ma1p, mut mw1, mut ma2, mut mw2) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    for i in 0..p {
        let d1 = h_diff(h_shift(at(i + 1), p0), h_shift(at(i), p0), s1);
        let v1 = [d1[0], d1[1], d1[2]];
        ma1 = ma1.max(norm(v1));
        if let Some(ax) = axis_opt {
            ma1p = ma1p.max(norm(perp_comp(v1, ax)));
        }
        mw1 = mw1.max(d1[3].abs());
        if i + 1 < p {
            let d1b = h_diff(h_shift(at(i + 2), p0), h_shift(at(i + 1), p0), s1);
            let d2 = h_diff(d1b, d1, s2);
            ma2 = ma2.max(norm([d2[0], d2[1], d2[2]]));
            mw2 = mw2.max(d2[3].abs());
        }
    }
    [ma1, ma1p, mw1, ma2, mw2]
}

fn nurbs_eps(s: &EvalSurface, q: [[f64; 2]; 3]) -> Option<f64> {
    let EvalSurface::Nurbs { degree: [pu, pv], knots, count: [nu, nv], ctrl, closed } = s else {
        return None;
    };
    if nu.checked_mul(*nv)? != ctrl.len() {
        return None;
    }
    let (u_lo, u_hi) = (q[0][0].min(q[1][0]).min(q[2][0]), q[0][0].max(q[1][0]).max(q[2][0]));
    let (v_lo, v_hi) = (q[0][1].min(q[1][1]).min(q[2][1]), q[0][1].max(q[1][1]).max(q[2][1]));
    let (du, dv) = (u_hi - u_lo, v_hi - v_lo);
    let (su_s, ku, wu, wua, wub) = spans_and_kinks(&knots[0], *pu, *nu, u_lo, u_hi, closed[0])?;
    let (sv_s, kv, wv, wva, wvb) = spans_and_kinks(&knots[1], *pv, *nv, v_lo, v_hi, closed[1])?;
    let (mut jump_u, mut jump_v) = (0.0_f64, 0.0_f64);
    if !ku.is_empty() || wu {
        let dir = SplineDir { ctrl, stride: 1, knots: &knots[0], p: *pu, n: *nu };
        let v_idx = collect_indices(sv_s.as_slice(), *pv);
        for &st in &ku {
            jump_u += 0.25 * jump_bound(&dir, Some(st), &v_idx, *nu)? * du;
        }
        if wu {
            jump_u += 0.25 * jump_bound(&dir, None, &v_idx, *nu)? * du;
        }
    }
    if !kv.is_empty() || wv {
        let dir = SplineDir { ctrl, stride: *nu, knots: &knots[1], p: *pv, n: *nv };
        let u_idx = collect_indices(su_s.as_slice(), *pu);
        for &st in &kv {
            jump_v += 0.25 * jump_bound(&dir, Some(st), &u_idx, 1)? * dv;
        }
        if wv {
            jump_v += 0.25 * jump_bound(&dir, None, &u_idx, 1)? * dv;
        }
    }
    let (mut muu, mut muv, mut mvv) = (0.0_f64, 0.0_f64, 0.0_f64);
    let (len_u, len_v) = (*pu + 1, *pv + 1);
    let total_tile = len_u * len_v;
    let mut tile = [[0.0_f64; 4]; W * W];
    let mut col = [[0.0_f64; 4]; W];
    for &su in su_s.as_slice() {
        let (au, bu) = safe_sub_interval(knots[0][su], knots[0][su + 1], wua, wub, knots[0][*pu], knots[0][*nu], wu);
        let du_b = bu - au;
        for &sv in sv_s.as_slice() {
            let (av, bv) = safe_sub_interval(knots[1][sv], knots[1][sv + 1], wva, wvb, knots[1][*pv], knots[1][*nv], wv);
            let dv_b = bv - av;
            let (u0, v0) = (su - *pu, sv - *pv);
            for j in 0..len_v {
                let row = &mut tile[j * len_u..j * len_u + len_u];
                for i in 0..len_u {
                    row[i] = ctrl[(v0 + j) * *nu + u0 + i];
                }
                blossom_restrict_1d(&knots[0], *pu, su, row, au, bu);
            }
            for i in 0..len_u {
                for j in 0..len_v {
                    col[j] = tile[j * len_u + i];
                }
                blossom_restrict_1d(&knots[1], *pv, sv, &mut col[0..len_v], av, bv);
                for j in 0..len_v {
                    tile[j * len_u + i] = col[j];
                }
            }
            let (p0, r_span, w_min, const_w) = span_box_stats(total_tile, |idx| tile[idx])?;
            let (mut mau, mut mwu, mut mauu, mut mwuu) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
            let (mut mav, mut mwv, mut mavv, mut mwvv) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
            let (mut mauv, mut mwuv) = (0.0_f64, 0.0_f64);
            for j in 0..len_v {
                let [a1, _, w1, a2, w2] = bezier_hodograph(*pu, du_b, p0, None, |i| tile[j * len_u + i]);
                (mau, mwu, mauu, mwuu) = (mau.max(a1), mwu.max(w1), mauu.max(a2), mwuu.max(w2));
            }
            for i in 0..len_u {
                let [a1, _, w1, a2, w2] = bezier_hodograph(*pv, dv_b, p0, None, |j| tile[j * len_u + i]);
                (mav, mwv, mavv, mwvv) = (mav.max(a1), mwv.max(w1), mavv.max(a2), mwvv.max(w2));
            }
            let (ku0, kv0) = (*pu as f64 / du_b, *pv as f64 / dv_b);
            for j in 0..*pv {
                for i in 0..*pu {
                    let at = |ii: usize, jj: usize| h_shift(tile[jj * len_u + ii], p0);
                    let d11 = h_diff(h_diff(at(i + 1, j + 1), at(i, j + 1), ku0), h_diff(at(i + 1, j), at(i, j), ku0), kv0);
                    mauv = mauv.max(norm([d11[0], d11[1], d11[2]]));
                    mwuv = mwuv.max(d11[3].abs());
                }
            }
            if const_w {
                (muu, mvv, muv) = (muu.max(mauu / w_min), mvv.max(mavv / w_min), muv.max(mauv / w_min));
            } else {
                let (mu, mv) = ((mau + mwu * r_span) / w_min, (mav + mwv * r_span) / w_min);
                muu = muu.max((mauu + 2.0 * mwu * mu + mwuu * r_span) / w_min);
                mvv = mvv.max((mavv + 2.0 * mwv * mv + mwvv * r_span) / w_min);
                muv = muv.max((mauv + mwv * mu + mwu * mv + mwuv * r_span) / w_min);
            }
        }
    }
    Some(0.125 * (muu * du * du + 2.0 * muv * du * dv + mvv * dv * dv) + jump_u + jump_v)
}

fn mat3_op_bound(m: &[[f64; 4]; 3]) -> f64 {
    let mut g = [[0.0_f64; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            g[r][c] = (0..3).map(|k| m[k][r] * m[k][c]).sum();
        }
    }
    g.iter().map(|row| row.iter().map(|v| v.abs()).sum::<f64>()).fold(0.0_f64, f64::max).sqrt()
}

fn perp_comp(v: P3, axis: P3) -> P3 {
    let ax_n = norm2(axis);
    if !ax_n.is_finite() || ax_n <= 0.0 { [0.0; 3] } else { sub(v, scale(axis, dot(v, axis) / ax_n)) }
}

fn revolved_eps(s: &EvalSurface, q: [[f64; 2]; 3]) -> Option<f64> {
    let EvalSurface::Revolved { profile: EvalCurve { degree: p, knots, ctrl }, origin, axis, transform, swap } = s else {
        return None;
    };
    let n = ctrl.len();
    let (u_lo, u_hi) = (q[0][0].min(q[1][0]).min(q[2][0]), q[0][0].max(q[1][0]).max(q[2][0]));
    let (v_lo, v_hi) = (q[0][1].min(q[1][1]).min(q[2][1]), q[0][1].max(q[1][1]).max(q[2][1]));
    let (du, dv) = (u_hi - u_lo, v_hi - v_lo);
    let (s_lo, s_hi, ds) = if *swap { (v_lo, v_hi, dv) } else { (u_lo, u_hi, du) };
    let curve = EvalCurve { degree: *p, knots: knots.clone(), ctrl: ctrl.clone() };
    let (spans, kinks, wraps, wsa, wsb) = spans_and_kinks(knots, *p, n, s_lo, s_hi, closed_curve(&curve))?;
    let mut jump_s = 0.0_f64;
    let dir = SplineDir { ctrl, stride: 1, knots, p: *p, n };
    for &st in &kinks {
        jump_s += 0.25 * jump_bound(&dir, Some(st), &[0], 0)? * ds;
    }
    if wraps {
        jump_s += 0.25 * jump_bound(&dir, None, &[0], 0)? * ds;
    }
    let (ax, org) = ([axis.x, axis.y, axis.z], [origin.x, origin.y, origin.z]);
    let (ax_len, l_norm) = (norm(ax), mat3_op_bound(transform));
    if !(ax_len.is_finite() && ax_len > 0.0) {
        return None;
    }
    let (mut mc2, mut mc1p, mut r_perp) = (0.0_f64, 0.0_f64, 0.0_f64);
    let mut seg = [[0.0_f64; 4]; W];
    for &sp in spans.as_slice() {
        let (a_s, b_s) = safe_sub_interval(knots[sp], knots[sp + 1], wsa, wsb, knots[*p], knots[n], wraps);
        let base = sp - *p;
        seg[..=*p].copy_from_slice(&ctrl[base..=base + *p]);
        blossom_restrict_1d(knots, *p, sp, &mut seg[0..=*p], a_s, b_s);
        let (p0, r_span, w_min, const_w) = span_box_stats(*p + 1, |idx| seg[idx])?;
        let mut r_span_perp = 0.0_f64;
        for &pt in seg.iter().take(*p + 1) {
            let (c, _) = cart(pt)?;
            r_span_perp = r_span_perp.max(norm(perp_comp(sub(c, p0), ax)));
            r_perp = r_perp.max(norm(perp_comp(sub(c, org), ax)));
        }
        let [ma1, ma1p, mw1, ma2, mw2] = bezier_hodograph(*p, b_s - a_s, p0, Some(ax), |i| seg[i]);
        if const_w {
            mc1p = mc1p.max(ma1p / w_min);
            mc2 = mc2.max(ma2 / w_min);
        } else {
            let c1 = (ma1 + mw1 * r_span) / w_min;
            mc1p = mc1p.max((ma1p + mw1 * r_span_perp) / w_min);
            mc2 = mc2.max((ma2 + 2.0 * mw1 * c1 + mw2 * r_span) / w_min);
        }
    }
    let (mss, msa, maa) = (l_norm * mc2, l_norm * ax_len * mc1p, l_norm * ax_len * ax_len * r_perp);
    let (muu, mvv) = if *swap { (maa, mss) } else { (mss, maa) };
    Some(0.125 * (muu * du * du + 2.0 * msa * du * dv + mvv * dv * dv) + l_norm * jump_s)
}

/// Certified `sup_b |S(q(b)) - sum b_k S(q_k)|` for parametric surface `s`
/// and sub-triangle corner parameters `q`.
#[must_use]
pub fn parametric_eps(s: &EvalSurface, dom: &Domain, q: [[f64; 2]; 3]) -> Option<f64> {
    let q0 = q[0];
    let q1 = [unwrap(q[1][0], q0[0], dom.period[0]), unwrap(q[1][1], q0[1], dom.period[1])];
    let q2 = [unwrap(q[2][0], q0[0], dom.period[0]), unwrap(q[2][1], q0[1], dom.period[1])];
    let out = match s {
        EvalSurface::Plane { .. } => Some(0.0),
        EvalSurface::Nurbs { .. } => nurbs_eps(s, [q0, q1, q2]),
        EvalSurface::Revolved { .. } => revolved_eps(s, [q0, q1, q2]),
    }?;
    out.is_finite().then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use solidcore::Vec3;

    #[test]
    fn parametric_eps_dominates_rational_nurbs_and_revolved_samples() {
        let nurbs = EvalSurface::nurbs(
            [2, 1],
            [vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0], vec![0.0, 0.0, 1.0, 1.0]],
            [3, 2],
            vec![
                [1.0, 0.0, 0.0, 1.0], [0.707, 0.707, 0.0, 0.707], [0.0, 1.0, 0.0, 1.0],
                [1.0, 0.0, 2.0, 1.0], [0.707, 0.707, 2.0, 0.707], [0.0, 1.0, 2.0, 1.0],
            ],
        );
        let kinked_nurbs = EvalSurface::nurbs(
            [2, 1],
            [vec![0.0, 0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0], vec![0.0, 0.0, 1.0, 1.0]],
            [5, 2],
            vec![
                [0.0, 0.0, 0.0, 1.0], [0.25, 0.2, 0.0, 1.5], [0.5, 0.0, 0.4, 2.0], [0.75, -0.3, 0.1, 0.8], [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0], [0.25, 1.1, 0.1, 0.9], [0.5, 1.0, -0.3, 1.2], [0.75, 0.8, 0.2, 1.4], [1.0, 1.0, 0.0, 1.0],
            ],
        );
        let rev = EvalSurface::Revolved {
            profile: EvalCurve {
                degree: 2,
                knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                ctrl: vec![[3.0, 0.0, 0.0, 1.0], [4.5, 0.0, 1.5, 0.75], [3.5, 0.0, 3.0, 1.0]],
            },
            origin: Vec3::ZERO,
            axis: Vec3::new(0.0, 0.0, 0.5),
            transform: [[0.0, -1.2, 0.0, 1.0], [1.2, 0.0, 0.0, -2.0], [0.0, 0.0, 1.2, 0.5]],
            swap: true,
        };
        let rev_noswap = EvalSurface::Revolved {
            profile: EvalCurve {
                degree: 2,
                knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                ctrl: vec![[3.0, 0.0, 0.0, 1.0], [4.5, 0.0, 1.5, 0.75], [3.5, 0.0, 3.0, 1.0]],
            },
            origin: Vec3::ZERO,
            axis: Vec3::new(0.0, 0.0, 2.0),
            transform: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]],
            swap: false,
        };
        for surf in [&nurbs, &kinked_nurbs, &rev, &rev_noswap] {
            let dom = solidcore::eval::plan::uv::domain(surf);
            for qs in [[[0.1, 0.1], [0.85, 0.2], [0.3, 0.85]], [[0.4, 0.4], [0.45, 0.41], [0.42, 0.46]]] {
                let eps = parametric_eps(surf, &dom, qs).unwrap();
                let c = qs.map(|uv| { let (p, _, _) = surf.derivs(uv[0], uv[1]).unwrap(); [p.x, p.y, p.z] });
                for i in 0..=32 {
                    for j in 0..=(32 - i) {
                        let (b0, b1, b2) = (i as f64 / 32.0, j as f64 / 32.0, (32 - i - j) as f64 / 32.0);
                        let uv = [0, 1].map(|k| b0 * qs[0][k] + b1 * qs[1][k] + b2 * qs[2][k]);
                        let lin = [0, 1, 2].map(|k| b0 * c[0][k] + b1 * c[1][k] + b2 * c[2][k]);
                        let (p, _, _) = surf.derivs(uv[0], uv[1]).unwrap();
                        assert!(norm(sub([p.x, p.y, p.z], lin)) <= eps + 1e-12);
                    }
                }
            }
        }
    }
}

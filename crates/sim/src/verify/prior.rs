//! Issue #96 review: law (d)'s vertical dives read the stock the setup's earlier operations left standing (the re-machining rest grid), not only the raw block - our own reading of the shop rule "never traverse through uncut stock" on the dexel model; no vendor material consulted.

use super::types::Warning;

use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::Point2;
use crate::stock_sim::Columns;

/// `warns` without the convictions of vertical rapids DOWN that the
/// program ahead of this operation already made safe. Law (d) judges a
/// vertical dive against `min(this op's own cuts before it, the stock
/// top)` at its column; the operations ahead of it in the same setup
/// have cut too, and their rest grid `prior` is the third term of that
/// min. So a conviction stands exactly while the dive also ends below
/// the prior surface - an acquittal by the prior is the same verdict
/// the column law would give with the prior in its min.
///
/// Read conservatively: the prior surface under a dive is the HIGHEST
/// grid column the tool's disc of radius `tool_r` covers (the whole
/// footprint must have been cut, not just the centre); cells off the
/// grid are outside the stock (air); a disc that covers no cell, or a
/// cell whose height is not a number, leaves the conviction standing -
/// an unknown prior is the raw stock. Lateral rapids keep every
/// conviction: only the vertical dive's verdict is re-read here.
pub fn acquit_by_prior(path: &Toolpath, prior: &Columns<'_>, tool_r: f64, z_tol: f64, warns: &mut Vec<Warning>) {
    warns.retain(|w| {
        let Warning::RapidInStock { at } = *w else { return true };
        let Some(m) = path.moves.get(at) else { return true };
        let (dx, dy) = (m.to.x - m.from.x, m.to.y - m.from.y);
        let vertical_down = m.kind == MoveKind::Rapid && dx * dx + dy * dy <= z_tol * z_tol && m.to.z < m.from.z;
        if !vertical_down {
            return true;
        }
        match prior_surface(prior, Point2::new(m.to.x, m.to.y), tool_r) {
            Some(top) => m.to.z < top - z_tol,
            None => true,
        }
    });
}

/// `warns` without the convictions of law (d3), the feed plunge depth
/// (#3679), that the program ahead of this operation already made
/// safe. Law (d3) judges a feed descent against this op's own rest
/// floor - the surface its cuts before the dive left, capped at the
/// raw stock top - and the operations ahead of it in the same setup
/// have cut too: their grid `prior` is a third term of that floor's
/// min, the same acquittal [`acquit_by_prior`] gives a dive. A
/// conviction stands exactly while the dive's own bite also reaches
/// deeper than one stepdown below the prior surface.
///
/// Read conservatively, the dive's own rules: the prior surface under
/// the disc is the HIGHEST grid column the tool's full radius covers
/// (the whole footprint must have been cut), and a dive whose tip
/// ends within one stepdown of that surface cannot convict at any
/// sample - the bite's floor is the lesser of the move's start, the
/// op's own floor and the prior's, and `lift_at` is never negative.
/// A prior that cannot be read (`None`) leaves the conviction
/// standing, and `!(.. <= ..)` reads a limit no number can read as no
/// acquittal - the law's own fail-closed form.
pub fn acquit_plunge_by_prior(path: &Toolpath, prior: &Columns<'_>, tool_r: f64, z_tol: f64, warns: &mut Vec<Warning>) {
    warns.retain(|w| {
        let Warning::PlungeTooDeep { at, limit_mm, .. } = *w else { return true };
        let Some(m) = path.moves.get(at) else { return true };
        let (dx, dy) = (m.to.x - m.from.x, m.to.y - m.from.y);
        let feed_descent = m.kind != MoveKind::Rapid
            && dx * dx + dy * dy <= z_tol * z_tol
            && m.to.z < m.from.z - z_tol;
        if !feed_descent {
            return true; // not the plunge law's shape: keep the word
        }
        // The `partial_cmp` form is the law's own fail-closed
        // `!(.. <= ..)`: a limit no number can read is no acquittal.
        match prior_surface(prior, Point2::new(m.to.x, m.to.y), tool_r) {
            Some(top) => (top - m.to.z).partial_cmp(&(limit_mm + z_tol)).is_none_or(|o| o.is_gt()),
            None => true,
        }
    });
}

/// The highest material the prior stock holds under the disc (q, r):
/// `None` when the disc covers no column or a height is unreadable.
/// Issue #89: drilling's reach, R-plane and holder laws read the same
/// conservative surface the rapid law does. Read through the column
/// seam (#585, M7.3): a rest grid or a field stock answers alike.
pub fn prior_surface(dex: &Columns<'_>, q: Point2, r: f64) -> Option<f64> {
    let cell = dex.cell();
    if !(r.is_finite() && r >= 0.0 && cell > 0.0 && q.x.is_finite() && q.y.is_finite()) {
        return None;
    }
    let o = dex.origin();
    // Every cell whose centre lies in the disc, plus q's own cell (a
    // disc narrower than a cell still stands on one).
    let rr = r + cell * 0.5;
    let lo = |v: f64, o: f64| ((v - rr - o) / cell).floor().max(0.0);
    let hi = |v: f64, o: f64, n: usize| ((v + rr - o) / cell).ceil().min(n as f64);
    let (ix0, ix1) = (lo(q.x, o.x), hi(q.x, o.x, dex.nx()));
    let (iy0, iy1) = (lo(q.y, o.y), hi(q.y, o.y, dex.ny()));
    let mut top: Option<f64> = dex.col(q).map(|(ix, iy)| dex.top_at(ix, iy));
    let r2 = r * r;
    let (mut iy, mut ix);
    iy = iy0;
    while iy < iy1 {
        ix = ix0;
        while ix < ix1 {
            let c = dex.center(ix as usize, iy as usize);
            let (ex, ey) = (c.x - q.x, c.y - q.y);
            if ex * ex + ey * ey <= r2
                && let Some((cx, cy)) = dex.col(c)
            {
                let z = dex.top_at(cx, cy);
                top = Some(top.map_or(z, |t| t.max(z)));
                if z.is_nan() {
                    return None;
                }
            }
            ix += 1.0;
        }
        iy += 1.0;
    }
    top.filter(|t| !t.is_nan())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::Move;
    use crate::geometry::Point3;
    use crate::stock_sim::Dexel;

    fn dive(z0: f64, z1: f64) -> Toolpath {
        let mv = |kind, a: f64, b: f64| Move {
            rotary: None,
            kind,
            from: Point3::new(0.0, 0.0, a),
            to: Point3::new(0.0, 0.0, b),
            feed: if kind == MoveKind::Rapid { None } else { Some(80.0) },
            arc: None,
            axis: None,
        };
        Toolpath {
            moves: vec![mv(MoveKind::Rapid, z0, z1), mv(MoveKind::Plunge, z1, z1 - 4.0)],
            cycles: Vec::new(),
            notes: Vec::new(),
            comps: Vec::new(),
            threads: Vec::new(),
            probes: Vec::new(),
            breaks: Vec::new(),
            plasma: None,
        }
    }

    /// A 40 x 40 block, top Z 25, with a 10 x 10 pocket at the origin
    /// cleared to Z 15 by an earlier operation.
    fn pocketed() -> Dexel {
        let mut d = match Dexel::block(-20.0, -20.0, 20.0, 20.0, 25.0, 0.25) {
            Ok(d) => d,
            Err(e) => panic!("block: {e:?}"),
        };
        for iy in 0..d.ny {
            for ix in 0..d.nx {
                let c = d.center(ix, iy);
                if c.x.abs() < 5.0
                    && c.y.abs() < 5.0
                    && let Some(k) = d.col(c)
                {
                    d.lower(k, 15.0);
                }
            }
        }
        d
    }

    #[test]
    fn a_dive_into_a_pocket_an_earlier_op_cleared_is_acquitted() {
        // A drill rapids 35 -> 16 over the pocket floor (Z 15): its own
        // law convicts it against the raw top Z 25; the prior says air.
        let mut w = vec![Warning::RapidInStock { at: 0 }];
        acquit_by_prior(&dive(35.0, 16.0), &Columns::of_grid(&pocketed()), 2.0, 1e-3, &mut w);
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn a_dive_past_the_prior_floor_still_stands() {
        let mut w = vec![Warning::RapidInStock { at: 0 }];
        acquit_by_prior(&dive(35.0, 14.0), &Columns::of_grid(&pocketed()), 2.0, 1e-3, &mut w);
        assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
    }

    #[test]
    fn a_tool_wider_than_the_cleared_pocket_still_stands() {
        // A 12 mm disc over a 10 mm pocket: its rim meets the raw top.
        let mut w = vec![Warning::RapidInStock { at: 0 }];
        acquit_by_prior(&dive(35.0, 16.0), &Columns::of_grid(&pocketed()), 6.0, 1e-3, &mut w);
        assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
    }

    /// A rapid to `z0` over the station, then a feed plunge `z0` -> `z1`.
    fn feed_dive(z0: f64, z1: f64) -> Toolpath {
        let mv = |kind, a: f64, b: f64| Move {
            rotary: None,
            kind,
            from: Point3::new(0.0, 0.0, a),
            to: Point3::new(0.0, 0.0, b),
            feed: if kind == MoveKind::Rapid { None } else { Some(80.0) },
            arc: None,
            axis: None,
        };
        Toolpath {
            moves: vec![mv(MoveKind::Rapid, 35.0, z0), mv(MoveKind::Plunge, z0, z1)],
            cycles: Vec::new(),
            notes: Vec::new(),
            comps: Vec::new(),
            threads: Vec::new(),
            probes: Vec::new(),
            breaks: Vec::new(),
            plasma: None,
        }
    }

    #[test]
    fn a_plunge_conviction_over_stock_the_prior_cleared_within_one_stepdown_is_acquitted() {
        // The earlier op left Z 15 under the disc; the feed to Z 11
        // takes 4 mm of what stands - one stepdown, no conviction.
        let mut w = vec![Warning::PlungeTooDeep { at: 1, mm: 9.0, limit_mm: 4.0 }];
        acquit_plunge_by_prior(&feed_dive(35.0, 11.0), &Columns::of_grid(&pocketed()), 2.0, 1e-3, &mut w);
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn a_plunge_below_the_prior_floor_still_stands() {
        // To Z 9 the feed takes 6 mm of what stands: over one stepdown.
        let mut w = vec![Warning::PlungeTooDeep { at: 1, mm: 9.0, limit_mm: 4.0 }];
        acquit_plunge_by_prior(&feed_dive(35.0, 9.0), &Columns::of_grid(&pocketed()), 2.0, 1e-3, &mut w);
        assert_eq!(w, vec![Warning::PlungeTooDeep { at: 1, mm: 9.0, limit_mm: 4.0 }]);
    }

    #[test]
    fn a_plunge_conviction_a_wider_tool_cannot_clear_stands() {
        // A 12 mm disc over the 10 mm pocket: its rim meets the raw top.
        let mut w = vec![Warning::PlungeTooDeep { at: 1, mm: 9.0, limit_mm: 4.0 }];
        acquit_plunge_by_prior(&feed_dive(35.0, 11.0), &Columns::of_grid(&pocketed()), 6.0, 1e-3, &mut w);
        assert_eq!(w, vec![Warning::PlungeTooDeep { at: 1, mm: 9.0, limit_mm: 4.0 }]);
    }

    #[test]
    fn a_lateral_feed_keeps_its_plunge_conviction() {
        // Only the feed descent's verdict is re-read here.
        let mut p = feed_dive(35.0, 11.0);
        if let Some(m) = p.moves.get_mut(1) {
            m.to.x = 3.0;
        }
        let mut w = vec![Warning::PlungeTooDeep { at: 1, mm: 9.0, limit_mm: 4.0 }];
        acquit_plunge_by_prior(&p, &Columns::of_grid(&pocketed()), 2.0, 1e-3, &mut w);
        assert_eq!(w, vec![Warning::PlungeTooDeep { at: 1, mm: 9.0, limit_mm: 4.0 }]);
    }

    #[test]
    fn a_limit_no_number_can_read_keeps_the_plunge_conviction() {
        // The law's own fail-closed form: an unreadable limit is no
        // acquittal, the prior's word included.
        let mut w = vec![Warning::PlungeTooDeep { at: 1, mm: 9.0, limit_mm: f64::NAN }];
        acquit_plunge_by_prior(&feed_dive(35.0, 11.0), &Columns::of_grid(&pocketed()), 2.0, 1e-3, &mut w);
        // A NaN limit cannot be compared: the conviction stands as is.
        assert_eq!(w.len(), 1, "an unreadable limit acquits nothing: {w:?}");
        assert!(matches!(w[0], Warning::PlungeTooDeep { at: 1, .. }), "{w:?}");
    }

    #[test]
    fn a_lateral_conviction_is_never_re_read() {
        let mut p = dive(35.0, 16.0);
        if let Some(m) = p.moves.get_mut(0) {
            m.to.x = 3.0;
        }
        let mut w = vec![Warning::RapidInStock { at: 0 }];
        acquit_by_prior(&p, &Columns::of_grid(&pocketed()), 2.0, 1e-3, &mut w);
        assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
    }
}

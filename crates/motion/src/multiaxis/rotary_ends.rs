//! Rotary Parallel's held ends (#2343). Where the setup stock goes on
//! past an end of the model along the fitted axis - the bar in the
//! chuck, or on a centre - the model's end face is inside solid bar,
//! and the raster's pass down it is a slot one tool wide from the skin
//! to the axis: it parts the bar. [`held`] reads the two ends off the
//! laid frame's stations (local x); [`clip`] keeps the ball's CENTER
//! on the model's side of a held end's plane: the turned surface is
//! finished to its end, the end face is left, and past the plane the
//! ball (half of it at most) goes no deeper than the surface it rides.
//! Stock that goes on by no more than one tool diameter is a facing
//! allowance: the end-face pass clears all of it, and that end stays
//! as it was.
//!
//! Provenance: the held-bar reading of 4-axis rotary work (chuck and
//! tailstock practice, machine-tool handbooks); clipping a segment
//! against a slab is elementary geometry. Our own design; nothing of
//! any vendor's consulted.

use crate::cl::{Move, MoveKind, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{GenError, ARC_FIT_TOL};

/// One held end: the station of the model's end plane, and how far
/// the stock goes on past it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HeldEnd {
    pub station: f64,
    pub beyond: f64,
}

/// The stations the ball CENTER may stand at, and the ends that
/// narrowed them. A free end leaves its side unbounded.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Window {
    pub lo: f64,
    pub hi: f64,
    pub ends: Vec<HeldEnd>,
}

impl Window {
    pub(super) fn holds(&self, x: f64) -> bool {
        x >= self.lo && x <= self.hi
    }
}

/// The finite span of `facets` along the laid frame's x.
fn span(facets: &[Triangle]) -> Option<(f64, f64)> {
    let (lo, hi) = facets
        .iter()
        .flat_map(|t| [t.a.x, t.b.x, t.c.x])
        .filter(|x| x.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), x| (l.min(x), h.max(x)));
    (lo <= hi).then_some((lo, hi))
}

/// The window the held ends leave, for a ball of radius `r`. `model`
/// and `stock` are in the laid frame. None: no separate stock, or it
/// goes on past neither end by more than the tool's diameter (plus the
/// plan tolerance `tol`) - the raster stands as planned. A model with
/// both ends held and no length between its end planes is refused by
/// name.
pub(super) fn held(model: &[Triangle], stock: Option<&[Triangle]>, r: f64, tol: f64) -> Result<Option<Window>, GenError> {
    let (Some((m_lo, m_hi)), Some((s_lo, s_hi))) = (span(model), stock.and_then(span)) else {
        return Ok(None);
    };
    let reach = 2.0 * r + tol.max(0.0);
    let mut w = Window { lo: f64::NEG_INFINITY, hi: f64::INFINITY, ends: Vec::new() };
    if m_lo - s_lo > reach {
        w.lo = m_lo;
        w.ends.push(HeldEnd { station: m_lo, beyond: m_lo - s_lo });
    }
    if s_hi - m_hi > reach {
        w.hi = m_hi;
        w.ends.push(HeldEnd { station: m_hi, beyond: s_hi - m_hi });
    }
    if w.ends.is_empty() {
        return Ok(None);
    }
    if w.hi - w.lo <= tol.max(0.0) {
        return Err(GenError::Invalid(format!(
            "Rotary Parallel: the stock goes on past both ends of the model, and the {:.3} mm between its end planes is within the {:.3} mm tolerance - no pass fits between the two end faces",
            m_hi - m_lo,
            tol.max(0.0)
        )));
    }
    Ok(Some(w))
}

/// `base` (the laid-frame raster, vertical-ball tips: the ball center
/// stands at the tip's own station) with every cutting move kept to
/// the window. Links (rapids, plunges) pass through: the radial relink
/// rebuilds them from the cut runs. Where the cut leaves the window
/// and comes back, a plunge marker stands between the two pieces, so
/// the relink reads two runs and links them over the retract cylinder,
/// never with a straight feed across the stretch that was left. An arc
/// the window cuts rides as its chords at `ARC_FIT_TOL`.
pub(super) fn clip(base: &Toolpath, w: &Window) -> Toolpath {
    let mut out: Vec<Move> = Vec::with_capacity(base.moves.len());
    // Where the last kept cut piece ended; None after a link.
    let mut cut_end: Option<Point3> = None;
    for m in &base.moves {
        if m.kind != MoveKind::Feed {
            out.push(*m);
            cut_end = None;
            continue;
        }
        let chords: Vec<(Point3, Point3)> = match m.arc {
            None => vec![(m.from, m.to)],
            Some(_) => m.segments3(ARC_FIT_TOL),
        };
        let whole = chords.iter().all(|(a, b)| w.holds(a.x) && w.holds(b.x));
        let pieces: Vec<(Point3, Point3)> = if whole {
            vec![(m.from, m.to)]
        } else {
            chords.into_iter().filter_map(|(a, b)| clip_seg(a, b, w)).collect()
        };
        for (a, b) in pieces {
            if let Some(e) = cut_end
                && e != a
            {
                out.push(Move::new(MoveKind::Plunge, e, a, m.feed));
            }
            out.push(if whole { *m } else { Move { from: a, to: b, arc: None, ..*m } });
            cut_end = Some(b);
        }
    }
    let mut path = Toolpath::new(out);
    path.notes = base.notes.clone();
    path
}

/// The part of the segment `a..b` whose stations the window holds;
/// None when none of it does, or only a point.
fn clip_seg(a: Point3, b: Point3, w: &Window) -> Option<(Point3, Point3)> {
    let dx = b.x - a.x;
    if dx == 0.0 {
        return w.holds(a.x).then_some((a, b));
    }
    let (ta, tb) = ((w.lo - a.x) / dx, (w.hi - a.x) / dx);
    let (t0, t1) = (ta.min(tb).max(0.0), ta.max(tb).min(1.0));
    // Not-a-number ends keep nothing: only a proved order passes.
    if t0.partial_cmp(&t1) != Some(std::cmp::Ordering::Less) {
        return None;
    }
    let at = |t: f64| {
        if t <= 0.0 {
            a
        } else if t >= 1.0 {
            b
        } else {
            // The cut end stands ON the plane: the station is the
            // window's own number, not a rounded product.
            let x = (a.x + dx * t).clamp(w.lo, w.hi);
            Point3::new(x, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t)
        }
    };
    Some((at(t0), at(t1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    fn slab(x0: f64, x1: f64) -> Vec<Triangle> {
        vec![Triangle::new(p(x0, -1.0, 0.0), p(x1, 1.0, 0.0), p(x0, 1.0, 1.0))]
    }

    #[test]
    fn stock_past_an_end_by_more_than_the_tool_holds_that_end() {
        let model = slab(0.0, 30.0);
        assert_eq!(held(&model, None, 1.0, 0.05).expect("no stock"), None);
        assert_eq!(held(&model, Some(&slab(0.0, 30.0)), 1.0, 0.05).expect("flush"), None);
        // 2 mm of facing allowance under a 2 mm tool: the end-face pass clears it.
        assert_eq!(held(&model, Some(&slab(-2.0, 32.0)), 1.0, 0.05).expect("allowance"), None);
        let w = held(&model, Some(&slab(-20.0, 30.0)), 1.0, 0.05).expect("chuck").expect("held");
        assert_eq!((w.lo, w.hi), (0.0, f64::INFINITY));
        assert_eq!(w.ends, [HeldEnd { station: 0.0, beyond: 20.0 }]);
        let w = held(&model, Some(&slab(-20.0, 40.0)), 1.0, 0.05).expect("centre").expect("held");
        assert_eq!((w.lo, w.hi), (0.0, 30.0));
    }

    #[test]
    fn a_model_with_no_length_between_two_held_ends_is_refused_by_name() {
        let e = held(&slab(0.0, 0.01), Some(&slab(-20.0, 20.0)), 1.0, 0.05).expect_err("no room");
        assert!(e.to_string().contains("Rotary Parallel: the stock goes on past both ends"), "{e}");
    }

    #[test]
    fn the_clip_keeps_the_cut_inside_and_breaks_it_where_it_left() {
        let feed = |a, b| Move::new(MoveKind::Feed, a, b, Some(100.0));
        // A loop that runs out past the low end, down the end face and back.
        let base = Toolpath::new(vec![
            Move::new(MoveKind::Rapid, p(5.0, 0.0, 9.0), p(5.0, 1.0, 9.0), None),
            feed(p(5.0, 1.0, 2.0), p(-1.0, 1.0, 2.0)),
            feed(p(-1.0, 1.0, 2.0), p(-1.0, -1.0, 0.0)),
            feed(p(-1.0, -1.0, 0.0), p(5.0, -1.0, 0.0)),
        ]);
        let w = Window { lo: 1.0, hi: f64::INFINITY, ends: Vec::new() };
        let got = clip(&base, &w);
        let kinds: Vec<MoveKind> = got.moves.iter().map(|m| m.kind).collect();
        assert_eq!(kinds, [MoveKind::Rapid, MoveKind::Feed, MoveKind::Plunge, MoveKind::Feed]);
        assert_eq!((got.moves[1].from, got.moves[1].to), (p(5.0, 1.0, 2.0), p(1.0, 1.0, 2.0)));
        assert_eq!((got.moves[2].from, got.moves[2].to), (p(1.0, 1.0, 2.0), p(1.0, -1.0, 0.0)));
        assert_eq!((got.moves[3].from, got.moves[3].to), (p(1.0, -1.0, 0.0), p(5.0, -1.0, 0.0)));
        for m in got.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
            assert!(w.holds(m.from.x) && w.holds(m.to.x), "{m:?}");
        }
        // Nothing to clip: the path is the planned one, move for move.
        let all = Window { lo: -5.0, hi: 9.0, ends: Vec::new() };
        assert_eq!(clip(&base, &all).moves, base.moves);
    }
}

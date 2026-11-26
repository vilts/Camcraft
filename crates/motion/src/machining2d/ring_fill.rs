//! The fill rings of the one ring set (#3788). Where the offsets of two
//! sides of a part merge (the waist of a figure eight, the neck of a dog
//! bone), a ring corner stands more than one stepover from the corner of
//! the next ring, and a ring there cuts the full tool width. A fill ring
//! is the same distance curve at half (and a quarter) of a step between
//! the two rings, cut only where they leave stock that a flank of either
//! one would meet: a level curve of the distance from the part, so it
//! crosses no ring, and Inside and Outside walk the same fills.
//!
//! Provenance: contour-parallel offset curves as level sets of the
//! distance to the boundary (Held, "On the Computational Geometry of
//! Pocket Machining", Springer LNCS 500, 1991), with intermediate levels
//! where the offsets merge; the zone test is ours (the flanks law of
//! `level_flanks`). No vendor material consulted.

use crate::geo2d::{Point2, Region, RingClip};

use super::face::GenError;
use super::level_order::Piece;
use super::ring::dist;

/// The sub-levels of one ring: ring `j` is level `RING * j`, its fills
/// the levels between `RING * (j - 1)` and `RING * j`.
pub(super) const RING: u32 = 4;

/// How far (in tool radii) a flank point stands from a walk.
const FLANK: f64 = 0.9;

/// A zone smaller than this (mm2, after a 10 um erosion) needs no fill.
const ZONE_MIN: f64 = 0.002;

/// Fills shorter than this (mm) are dropped.
const MIN_FILL: f64 = 0.5;

/// One band of the ring set: the ring `j` between the distance curves
/// `inner` (ring `j - 1`) and `outer` (ring `j`), with `hc` the part's
/// centre keep-out (distance 0) and `c` the cutter-centre area.
pub(super) struct Band<'a> {
    pub(super) c: &'a Region,
    pub(super) hc: &'a Region,
    pub(super) j: u32,
    pub(super) step: f64,
    pub(super) tool_r: f64,
    pub(super) climb: bool,
}

/// The fills of `band`, given the pieces of its two rings: none where
/// each flank of each ring meets the other ring's swath (the common
/// case, a stepover under 1.9 radii), a half ring where they leave stock
/// between them, and two quarter rings where the half leaves some.
pub(super) fn fills(band: &Band<'_>, inner: &[Vec<Point2>], outer: &[Vec<Point2>]) -> Result<Vec<Piece>, GenError> {
    let mut out = Vec::new();
    let Band { c, hc, j, step, tool_r, .. } = *band;
    if j == 0 || inner.is_empty() || outer.is_empty() {
        return Ok(out);
    }
    let d = |k: f64| (f64::from(j) - 1.0 + k) * step;
    let lo = if j == 1 { hc.clone() } else { hc.offset(d(0.0))? };
    let ring = hc.offset(d(1.0))?.difference(&lo)?.intersect(c)?;
    let zone = gap(&ring, inner, outer, tool_r)?;
    if zone.offset(-0.01)?.area().abs() <= ZONE_MIN {
        return Ok(out);
    }
    let half = level(band, d(0.5), &zone, RING * j - 2)?;
    let left = zone.difference(&swath(&half, tool_r)?)?;
    out.extend(half);
    if left.offset(-0.01)?.area().abs() > ZONE_MIN {
        out.extend(level(band, d(0.25), &left, RING * j - 3)?);
        out.extend(level(band, d(0.75), &left, RING * j - 1)?);
    }
    Ok(out)
}

/// The stock between two rings that a flank of one meets and the swath
/// of the other leaves: either order of the walk cuts the full width
/// there.
fn gap(ring: &Region, inner: &[Vec<Point2>], outer: &[Vec<Point2>], tool_r: f64) -> Result<Region, GenError> {
    let near = |w: &[Vec<Point2>], r: f64| Region::sweep(w, r);
    let (i_flank, i_cut) = (near(inner, FLANK * tool_r)?, near(inner, tool_r)?);
    let (o_flank, o_cut) = (near(outer, FLANK * tool_r)?, near(outer, tool_r)?);
    let a = ring.intersect(&i_flank)?.difference(&o_cut)?;
    let b = ring.intersect(&o_flank)?.difference(&i_cut)?;
    Ok(a.union(&b)?)
}

fn swath(ps: &[Piece], tool_r: f64) -> Result<Region, GenError> {
    let walks: Vec<Vec<Point2>> = ps.iter().map(closed_walk).collect();
    Ok(Region::sweep(&walks, tool_r)?)
}

/// A piece's walk, closed when the piece is a whole ring.
pub(super) fn closed_walk(p: &Piece) -> Vec<Point2> {
    let mut w = p.pts.clone();
    if p.closed && let Some(&f) = w.first() {
        w.push(f);
    }
    w
}

/// The distance curve at `d` from the part, walked as the rings are, and
/// clipped to the area round `zone` that a fill needs (the zone grown by
/// the tool radius, inside the cutter-centre area).
fn level(band: &Band<'_>, d: f64, zone: &Region, lev: u32) -> Result<Vec<Piece>, GenError> {
    let near = zone.offset(band.tool_r)?.intersect(band.c)?;
    let mut out = Vec::new();
    for l in band.hc.offset(d)?.loops() {
        // As `level_rings::curves` walks ring `j`: the distance curve has
        // the cut side on its left, reversed for climb.
        let walk: Vec<Point2> = if band.climb { l.points().to_vec() } else { l.points().iter().rev().copied().collect() };
        match near.clip_ring(&walk, true)? {
            RingClip::Whole if walk.len() >= 2 => out.push(Piece { level: lev, pts: walk, closed: true, core: false }),
            RingClip::Whole => {}
            RingClip::Pieces(ps) => {
                for pts in ps {
                    let len: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
                    if pts.len() >= 2 && len >= MIN_FILL {
                        out.push(Piece { level: lev, pts, closed: false, core: false });
                    }
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{Band, RING, fills};
    use crate::geo2d::{Loop, Point2, Region};

    fn circle(r: f64, from: f64, to: f64, n: u32) -> Vec<Point2> {
        (0..=n).map(|k| {
            let a = from + (to - from) * f64::from(k) / f64::from(n);
            Point2::new(r * a.cos(), r * a.sin())
        })
        .collect()
    }

    fn region(pts: Vec<Point2>) -> Region {
        Region::new(vec![Loop::new(pts).unwrap_or_else(|e| panic!("{e:?}"))]).unwrap_or_else(|e| panic!("{e:?}"))
    }

    /// #3788 pin: a band whose outer ring is whole needs no fill; a band
    /// whose outer ring stops half way leaves stock that the inner ring's
    /// flank meets, and gets a fill at half a step, between the two rings.
    #[test]
    fn a_band_with_a_ring_gap_gets_a_fill_and_a_whole_band_none() {
        let tau = std::f64::consts::TAU;
        let hc = region(circle(10.0, 0.0, tau, 255).into_iter().take(256).collect());
        let c = region(vec![Point2::new(-30.0, -30.0), Point2::new(30.0, -30.0), Point2::new(30.0, 30.0), Point2::new(-30.0, 30.0)]);
        let band = Band { c: &c, hc: &hc, j: 1, step: 2.0, tool_r: 4.0, climb: true };
        let inner = vec![circle(10.0, 0.0, tau, 256)];
        let whole = fills(&band, &inner, &[circle(12.0, 0.0, tau, 256)]).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(whole.is_empty(), "a whole band: {} fill(s)", whole.len());
        // The wide band: step 8 (2 radii), the outer ring on one half only.
        let wide = Band { step: 8.0, ..band };
        let half = fills(&wide, &inner, &[circle(18.0, 0.0, 0.5 * tau, 128)]).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(!half.is_empty(), "the gap gets a fill");
        for f in &half {
            assert!(f.level > 0 && f.level < RING, "a fill lies between ring 0 and ring 1: {}", f.level);
            let r = f.pts.iter().map(|q| q.x.hypot(q.y)).fold(0.0_f64, f64::max);
            assert!(r > 10.5 && r < 17.5, "the fill stands between the two rings: {r}");
        }
    }
}

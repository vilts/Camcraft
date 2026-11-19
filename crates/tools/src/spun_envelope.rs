//! Conservative stepped-band reduction from [`HolderBand`] envelopes to
//! [`SpunProfile`] (#743): merges fine bands strictly upward in radius so the
//! resulting spun profile fits [`MAX_POINTS`] and [`MIN_SEGMENT`] without ever
//! shrinking below any source band.
//! Provenance: original greedy minimum-volume band merge matching
//! `holder_profile::merge`; no external code consulted.

use crate::holder_profile::{HolderBand, MAX_BODY_RADIUS};
use crate::spun::{MAX_POINTS, MIN_SEGMENT, SpunError, SpunPoint, SpunProfile};

/// At most this many stepped cylindrical bands fit inside [`MAX_POINTS`]
/// (`2 * bands + 2 <= 64`, one riser/run pair per band plus axis closures).
const MAX_STEPPED_BANDS: usize = (MAX_POINTS - 2) / 2;

impl SpunProfile {
    /// Convert cylindrical envelope `bands` conservatively into a validated
    /// [`SpunProfile`]. Neighbours are merged greedily by minimum added
    /// swept volume (taking `max(r)`) until segments clear [`MIN_SEGMENT`]
    /// and the stepped outline fits [`MAX_POINTS`]. Never shrinks a band.
    pub fn conservative_from_bands(bands: &[HolderBand]) -> Result<Self, SpunError> {
        let mut work = normalize_bands(bands)?;
        loop {
            if merge_equal(&mut work) {
                continue;
            }
            if merge_short_step(&mut work) {
                continue;
            }
            if merge_short_band(&mut work) {
                continue;
            }
            if work.len() > MAX_STEPPED_BANDS {
                merge_cheapest(&mut work);
                continue;
            }
            break;
        }
        let first = work.first().ok_or(SpunError::TooFew)?;
        let last = work.last().ok_or(SpunError::TooFew)?;
        let mut pts = Vec::with_capacity(2 * work.len() + 2);
        pts.push(SpunPoint::line(first.above, 0.0));
        for b in &work {
            pts.push(SpunPoint::line(b.above, b.r));
            pts.push(SpunPoint::line(b.above + b.len, b.r));
        }
        pts.push(SpunPoint::line(last.above + last.len, 0.0));
        Self::new(pts)
    }
}

fn normalize_bands(bands: &[HolderBand]) -> Result<Vec<HolderBand>, SpunError> {
    if bands.is_empty() {
        return Err(SpunError::TooFew);
    }
    let mut out = Vec::with_capacity(bands.len());
    for (i, b) in bands.iter().enumerate() {
        if !(b.above.is_finite() && b.len.is_finite() && b.r.is_finite()) {
            return Err(SpunError::NonFinite(i + 1));
        }
        if b.r < 0.0 || b.len < 0.0 {
            return Err(SpunError::Negative(i + 1));
        }
        if b.r > MAX_BODY_RADIUS {
            return Err(SpunError::TooLarge);
        }
        if b.above < -1e-9 {
            return Err(SpunError::BelowTip(i + 1));
        }
        let above = b.above.max(0.0);
        let len = (b.above + b.len - above).max(0.0);
        // Clamp micro-radii up to MIN_SEGMENT so interior vertices never fail
        // `TouchesAxis` or `Short` on the closing leg; strictly conservative.
        let r = b.r.max(MIN_SEGMENT);
        out.push(HolderBand { above, len, r });
    }
    out.sort_by(|a, c| a.above.total_cmp(&c.above));
    Ok(out)
}

fn join(x: &HolderBand, y: &HolderBand) -> HolderBand {
    let top = (x.above + x.len).max(y.above + y.len);
    HolderBand {
        above: x.above,
        len: (top - x.above).max(0.0),
        r: x.r.max(y.r),
    }
}

fn cost(x: &HolderBand, y: &HolderBand) -> f64 {
    let j = join(x, y);
    (j.r * j.r * j.len - x.r * x.r * x.len - y.r * y.r * y.len).max(0.0)
}

fn merge_at(bands: &mut Vec<HolderBand>, idx: usize) {
    let merged = join(&bands[idx], &bands[idx + 1]);
    bands[idx] = merged;
    bands.remove(idx + 1);
}

fn merge_equal(bands: &mut Vec<HolderBand>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i + 1 < bands.len() {
        if (bands[i].r - bands[i + 1].r).abs() <= 1e-12
            && bands[i + 1].above <= bands[i].above + bands[i].len + 1e-9
        {
            merge_at(bands, i);
            changed = true;
        } else {
            i += 1;
        }
    }
    changed
}

fn merge_short_step(bands: &mut Vec<HolderBand>) -> bool {
    let mut best: Option<(f64, usize)> = None;
    for i in 0..bands.len().saturating_sub(1) {
        let gap = bands[i + 1].above - (bands[i].above + bands[i].len);
        let chord = gap.hypot(bands[i + 1].r - bands[i].r);
        if chord < MIN_SEGMENT {
            let c = cost(&bands[i], &bands[i + 1]);
            if best.is_none_or(|(bc, _)| c < bc) {
                best = Some((c, i));
            }
        }
    }
    if let Some((_, i)) = best {
        merge_at(bands, i);
        true
    } else {
        false
    }
}

fn merge_short_band(bands: &mut Vec<HolderBand>) -> bool {
    if bands.len() <= 1 {
        return false;
    }
    for i in 0..bands.len() {
        if bands[i].len < MIN_SEGMENT {
            let pick = if i == 0 {
                0
            } else if i + 1 == bands.len()
                || cost(&bands[i - 1], &bands[i]) <= cost(&bands[i], &bands[i + 1])
            {
                i - 1
            } else {
                i
            };
            merge_at(bands, pick);
            return true;
        }
    }
    false
}

fn merge_cheapest(bands: &mut Vec<HolderBand>) {
    let mut best = (f64::INFINITY, 0usize);
    for i in 0..bands.len() - 1 {
        let c = cost(&bands[i], &bands[i + 1]);
        if c < best.0 {
            best = (c, i);
        }
    }
    merge_at(bands, best.1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thirty_two_distinct_bands_reduce_conservatively_to_spun_profile() {
        let bands: Vec<HolderBand> = (0..32)
            .map(|k| HolderBand {
                above: 25.0 + k as f64 * 2.0,
                len: 2.0,
                r: 10.0 + k as f64 * 0.5,
            })
            .collect();
        assert!(SpunProfile::from_bands(&bands).is_none(), "naive from_bands hits MAX_POINTS");
        let spun = SpunProfile::conservative_from_bands(&bands).expect("conservative merge succeeds");
        assert!(spun.points().len() <= MAX_POINTS);
        let out_bands = spun.bands();
        for b in &bands {
            let mid = b.above + b.len * 0.5;
            let got = out_bands
                .iter()
                .filter(|ob| mid >= ob.above - 1e-9 && mid <= ob.above + ob.len + 1e-9)
                .map(|ob| ob.r)
                .fold(0.0, f64::max);
            assert!(got >= b.r - 1e-12, "at {mid}: shrunk {got} < {}", b.r);
        }
    }
}

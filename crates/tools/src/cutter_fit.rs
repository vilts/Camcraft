//! The drawn cutter edited by its parameters (#93 slice 3): carry a
//! hand-drawn cutter, neck and shank outline onto new tool numbers so
//! the two keep agreeing (`refit`), and the neck as a parameter of the
//! outline (`necked` / `neck_of`) - a milling tool states no neck
//! number, so a neck lives only in the drawn outline. The tip within the
//! flutes is the kind's own (regenerated from the numbers, the tip the
//! generators cut with); what was drawn above the flute top keeps its
//! shape, moved with the flute top and stretched to the new shank top.
//! Whatever comes out is still judged by `cutter_profile::check`.
//! Our own design over elementary plane geometry; no vendor source
//! consulted.

use crate::cutter_profile::{AGREE, parametric};
use crate::spun::{MIN_SEGMENT, SpunPoint};
use crate::tools::Tool;

/// Do `a` and `b` state the same cutter outline (every number
/// `Tool::profile` draws the cutter, neck and shank from)?
#[must_use]
pub fn same_shape(a: &Tool, b: &Tool) -> bool {
    let n = |t: &Tool| [t.diameter, t.corner_r, t.tip_angle, t.flute_len, t.shank_d, t.shank_len];
    a.kind == b.kind && n(a) == n(b)
}

/// A drawn cutter `points` (drawn to `old`'s numbers) carried onto
/// `new`'s: the tip within the flutes regenerated from `new`'s kind and
/// numbers; every point drawn above the old flute top moved by the
/// flute-length change, the shank top moved to the new one, the stated
/// shank radius following the new shank, and anything wider than the
/// new shank clamped to it. Unchanged when the shape numbers are; the
/// numbers' own outline when nothing above the flutes can be found.
#[must_use]
pub fn refit(old: &Tool, new: &Tool, points: &[SpunPoint]) -> Vec<SpunPoint> {
    if same_shape(old, new) {
        return points.to_vec();
    }
    let (Some(mut out), Some((above, blend))) = (tip_of(new), above_flutes(old, points)) else {
        return parametric(new);
    };
    if let Some(last) = out.last_mut() {
        last.arc = blend;
    }
    let (zc0, zc1) = (old.flute_len.max(0.0), new.flute_len.max(0.0));
    let (top0, top1) = (zc0 + old.shank_len.max(0.0), zc1 + new.shank_len.max(0.0));
    let (sr0, sr1) = (old.shank_d.max(0.0) * 0.5, new.shank_d.max(0.0) * 0.5);
    let r1 = new.diameter.max(0.0) * 0.5;
    for p in above {
        let z = if p.z >= top0 - AGREE { top1 } else { p.z + (zc1 - zc0) };
        let r = if p.r > AGREE && (p.r - sr0).abs() <= AGREE { sr1 } else { p.r };
        let limit = if z > zc1 + AGREE { sr1 } else { r1.max(sr1) };
        if r > limit + AGREE {
            // Clamped: the arc into and out of this point no longer
            // fits its chord, so both segments go straight.
            if let Some(prev) = out.last_mut() {
                prev.arc = 0.0;
            }
            out.push(SpunPoint::line(z, limit));
        } else {
            out.push(SpunPoint { z, r, arc: p.arc });
        }
    }
    out.dedup_by(|b, a| (a.z - b.z).hypot(a.r - b.r) < MIN_SEGMENT);
    out
}

/// A neck between the flute top and the shank: its diameter and its
/// length up from the flute top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Neck {
    pub d: f64,
    pub len: f64,
}

/// Why a neck is refused.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NeckError {
    /// A turning tool, or numbers that draw no cutter.
    NoCutter,
    /// Not between 0 and the shank diameter.
    Diameter { d: f64, shank: f64 },
    /// Not within the shank length (less the shortest segment).
    Length { len: f64, shank: f64 },
}

impl core::fmt::Display for NeckError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::NoCutter => write!(f, "the tool's numbers draw no milling cutter to neck"),
            Self::Diameter { d, shank } => {
                write!(f, "the neck is Ø {d:.3} mm - it must be above 0 and below the shank's Ø {shank:.3} mm")
            }
            Self::Length { len, shank } => write!(
                f,
                "the neck is {len:.3} mm long - it must be above {MIN_SEGMENT} mm and fit inside the {shank:.3} mm shank"
            ),
        }
    }
}

impl std::error::Error for NeckError {}

/// `tool`'s cutter with a neck: the kind's tip up to the flute top, the
/// neck `neck.len` up from there at `neck.d`, then the stated shank to
/// its top. Refused by name when the neck does not fit the shank.
pub fn necked(tool: &Tool, neck: Neck) -> Result<Vec<SpunPoint>, NeckError> {
    let shank = tool.shank_d.max(0.0);
    if !(neck.d.is_finite() && neck.d > 0.0 && neck.d < shank - AGREE) {
        return Err(NeckError::Diameter { d: neck.d, shank });
    }
    let room = tool.shank_len.max(0.0);
    if !(neck.len.is_finite() && neck.len >= MIN_SEGMENT && neck.len <= room - MIN_SEGMENT) {
        return Err(NeckError::Length { len: neck.len, shank: room });
    }
    let mut out = tip_of(tool).ok_or(NeckError::NoCutter)?;
    let zc = tool.flute_len.max(0.0);
    let (zn, top, nr, sr) = (zc + neck.len, zc + room, neck.d * 0.5, shank * 0.5);
    out.extend([SpunPoint::line(zc, nr), SpunPoint::line(zn, nr), SpunPoint::line(zn, sr), SpunPoint::line(top, sr), SpunPoint::line(top, 0.0)]);
    out.dedup_by(|b, a| (a.z - b.z).hypot(a.r - b.r) < MIN_SEGMENT);
    Ok(out)
}

/// The neck `points` draws on `tool`, when the outline is exactly the
/// kind's tip, one straight neck and the stated shank (what `necked`
/// draws) - a hand-drawn outline of any other shape has none.
#[must_use]
pub fn neck_of(tool: &Tool, points: &[SpunPoint]) -> Option<Neck> {
    let zc = tool.flute_len.max(0.0);
    let p = points.iter().find(|p| p.z > zc + AGREE)?;
    let neck = Neck { d: 2.0 * p.r, len: p.z - zc };
    let want = necked(tool, neck).ok()?;
    let same = |a: &SpunPoint, b: &SpunPoint| (a.z - b.z).abs() <= AGREE && (a.r - b.r).abs() <= AGREE && (a.arc - b.arc).abs() <= AGREE;
    (want.len() == points.len() && want.iter().zip(points).all(|(a, b)| same(a, b))).then_some(neck)
}

/// The kind's tip as `tool`'s numbers draw it: the parametric outline
/// from the tip up to the first point at the flute top on the stated
/// diameter. `None` when the numbers draw no milling cutter.
fn tip_of(tool: &Tool) -> Option<Vec<SpunPoint>> {
    let (zc, r) = (tool.flute_len.max(0.0), tool.diameter.max(0.0) * 0.5);
    let mut pts = parametric(tool);
    let end = pts.iter().position(|p| p.z >= zc - AGREE && (p.r - r).abs() <= AGREE)?;
    pts.truncate(end + 1);
    Some(pts)
}

/// What `points` (drawn to `old`) draws above the flute top: from the
/// last point at the flute top when it steps off the stated diameter
/// (a neck or a shank of another size), else from the point after it;
/// with the arc that left the flute-top point (the blend the regenerated
/// tip keeps).
fn above_flutes<'a>(old: &Tool, points: &'a [SpunPoint]) -> Option<(&'a [SpunPoint], f64)> {
    let (zc, r) = (old.flute_len.max(0.0), old.diameter.max(0.0) * 0.5);
    let k = points.iter().rposition(|p| p.z <= zc + AGREE)?;
    let p = points.get(k)?;
    if (p.r - r).abs() > AGREE { Some((points.get(k..)?, 0.0)) } else { Some((points.get(k + 1..)?, p.arc)) }
}

#[cfg(test)]
#[path = "cutter_fit_tests.rs"]
mod tests;

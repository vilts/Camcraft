//! A milling tool's own drawn shape (#93, slice 2): the cutter, neck and
//! shank as a spun half-outline (`spun::SpunProfile`), checked against
//! the numbers the generators and the gouge and stack laws read. The
//! numbers stay the truth: a drawn outline that states another cutting
//! diameter, flute length or stickout, draws a shank wider than the
//! laws judge, or folds back over itself is refused by name - so a drawn
//! cutter never changes what a toolpath computes, and a check reads it
//! only where it is narrower than the stated shank (#852, `neck`). Our
//! own design over elementary plane geometry; no vendor source consulted.

use crate::spun::{SpunError, SpunPoint, SpunProfile};
use crate::tools::{Tool, ToolKind};

/// Agreement slack (mm) between a drawn outline and the tool's numbers:
/// a typed value, not float dust - a 0.01 mm difference is a different
/// tool and is refused.
pub const AGREE: f64 = 1e-3;

/// Why a drawn cutter outline is refused.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CutterError {
    /// The outline itself is no spun profile.
    Shape(SpunError),
    /// A turning tool: not a spun profile (owner, #93).
    Lathe,
    /// The outline starts above the tool tip (first point z > 0).
    OffTip(f64),
    /// The widest point within the flutes is not the stated diameter.
    Diameter { drawn: f64, stated: f64 },
    /// The outline ends at another height than flute plus shank length.
    Length { drawn: f64, stated: f64 },
    /// At the flute top wider than cutter and shank, or above it wider
    /// than the shank the laws judge (point number, 1-based).
    TooWide { point: usize, limit: f64 },
    /// Two flat steps at one height run out and back over each other
    /// (point number, 1-based): the outline crosses itself.
    Folds(usize),
}

impl core::fmt::Display for CutterError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::Shape(e) => write!(f, "{e}"),
            Self::Lathe => write!(f, "a turning tool is not a spun profile - it keeps its own editor"),
            Self::OffTip(z) => write!(f, "the outline must start at the tool tip (z = 0), not z = {z:.3} mm"),
            Self::Diameter { drawn, stated } => write!(
                f,
                "the drawn cutter is Ø {drawn:.3} mm within the flutes, the tool states Ø {stated:.3} mm"
            ),
            Self::Length { drawn, stated } => write!(
                f,
                "the drawn outline ends {drawn:.3} mm above the tip, the tool states flute + shank = {stated:.3} mm"
            ),
            Self::TooWide { point, limit } => write!(
                f,
                "point {point} is wider than the shank the tool states (r ≤ {limit:.3} mm there) - the checks judge the stated shank"
            ),
            Self::Folds(i) => write!(f, "the outline folds back over itself at point {i}"),
        }
    }
}

impl std::error::Error for CutterError {}

/// Validate `points` as `tool`'s drawn cutter: a spun profile (every
/// `SpunProfile::new` refusal), on a milling tool, from the tip
/// (z = 0) to the shank top (flute + shank length), reaching exactly the
/// stated diameter within the flute length, never wider above it than
/// the stated shank, and not folding over itself.
pub fn check(tool: &Tool, points: Vec<SpunPoint>) -> Result<SpunProfile, CutterError> {
    if is_lathe(tool.kind) {
        return Err(CutterError::Lathe);
    }
    if let Some(i) = fold_at(&points) {
        return Err(CutterError::Folds(i));
    }
    let spun = SpunProfile::new(points).map_err(CutterError::Shape)?;
    agrees(tool, &spun)?;
    Ok(spun)
}

/// Does a validated outline still agree with `tool`'s numbers? The
/// stored cutter is re-judged by this after every number edit.
pub fn agrees(tool: &Tool, spun: &SpunProfile) -> Result<(), CutterError> {
    if is_lathe(tool.kind) {
        return Err(CutterError::Lathe);
    }
    let pts = spun.points();
    let foot = pts.first().map_or(0.0, |p| p.z);
    if foot > AGREE {
        return Err(CutterError::OffTip(foot));
    }
    let zc = tool.flute_len.max(0.0);
    let stated_len = zc + tool.shank_len.max(0.0);
    let top = pts.last().map_or(0.0, |p| p.z);
    if (top - stated_len).abs() > AGREE {
        return Err(CutterError::Length { drawn: top, stated: stated_len });
    }
    let out = spun.outline();
    let r = tool.diameter.max(0.0) * 0.5;
    // Below the flute top every radius counts; AT it, the step out to a
    // wider shank is the shank's (judged by the width law below), so
    // only radii up to the cutter's own count there.
    let in_flutes = out
        .iter()
        .filter(|q| q.0 < zc - AGREE || (q.0 <= zc + AGREE && q.1 <= r + AGREE))
        .fold(0.0f64, |m, q| m.max(q.1));
    if (in_flutes - r).abs() > AGREE {
        return Err(CutterError::Diameter { drawn: 2.0 * in_flutes, stated: tool.diameter });
    }
    // Above the flute top the laws judge the stated shank (`shank_d`), so
    // nothing drawn there may be wider; AT the flute top the step runs
    // between cutter and shank, so the larger of the two bounds it.
    let sr = tool.shank_d.max(0.0) * 0.5;
    let limit_at = |z: f64| if z > zc + AGREE { sr } else { r.max(sr) };
    let over = |z: f64, rr: f64| z >= zc - AGREE && rr > limit_at(z) + AGREE;
    // Checked on the drawn points (named in the refusal) and on the
    // flattened outline (a convex arc between two in-limit points).
    if let Some((i, p)) = pts.iter().enumerate().find(|(_, p)| over(p.z, p.r)) {
        return Err(CutterError::TooWide { point: i + 1, limit: limit_at(p.z) });
    }
    if let Some(q) = out.iter().find(|q| over(q.0, q.1)) {
        let i = pts.iter().rposition(|p| p.z <= q.0).map_or(1, |i| i + 1);
        return Err(CutterError::TooWide { point: i, limit: limit_at(q.0) });
    }
    Ok(())
}

/// The cutter as `tool`'s numbers draw it (`SpunProfile::cutter_of`,
/// tip to shank top) as editable points, near-duplicates (closer than
/// `spun::MIN_SEGMENT`, e.g. a chamfer cone meeting the diameter at the
/// flute top) merged. Empty for a turning tool.
#[must_use]
pub fn parametric(tool: &Tool) -> Vec<SpunPoint> {
    let mut pts = SpunProfile::cutter_of(tool).map(|s| s.points().to_vec()).unwrap_or_default();
    pts.dedup_by(|b, a| (a.z - b.z).hypot(a.r - b.r) < crate::spun::MIN_SEGMENT);
    pts
}

/// The first point (1-based) where two consecutive flat steps at one
/// height reverse direction - out and back over the same line.
fn fold_at(points: &[SpunPoint]) -> Option<usize> {
    points.windows(3).position(|w| {
        let flat = |a: &SpunPoint, b: &SpunPoint| a.arc == 0.0 && (a.z - b.z).abs() <= 1e-9;
        flat(&w[0], &w[1]) && flat(&w[1], &w[2]) && (w[1].r - w[0].r) * (w[2].r - w[1].r) < 0.0
    })
    .map(|i| i + 2)
}

fn is_lathe(k: ToolKind) -> bool {
    matches!(k, ToolKind::Turning | ToolKind::Groove | ToolKind::Thread | ToolKind::Form)
}

#[cfg(test)]
#[path = "cutter_profile_tests.rs"]
mod tests;

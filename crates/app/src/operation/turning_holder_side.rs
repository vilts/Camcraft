//! Issue #1102: which side of the spindle axis a lathe holder stands on for a turning path - a rigid holder never flips through the axis mid-path, so its +x (radial, away from the axis) is the side the path works on, read off the path's signed radii (the handbook turning convention: a front or a rear tool post, a facing pass running a little past centre); our own design, no vendor material consulted.
//!
//! The holder law reads a body point (x, y, z) at sqrt((X + x)^2 + y^2)
//! with X the tip's radius signed along the HOLDER's +x. The path's
//! signed meridian ([`SignedMeridian`]) takes its + side from the path's
//! first point off the axis, which need not be the holder's side: a rear
//! tool that starts a hair past centre puts every later tip at X < 0 of
//! that frame. Reading such a body along the frame's +x would mirror it
//! through the axis, so a holder that strikes could be judged clear.

/// How far past the spindle axis a tip may run while its holder stays
/// on its own side (mm): a facing pass cleaning the centre pip, which
/// overruns by up to a nose radius. Covers the common ISO noses up to
/// 2.4 mm - not every insert (a 3.2 mm corner or a round insert runs
/// farther): a path past centre farther than this is refused by name,
/// never read on a guessed side. The far side must also run beyond
/// twice this ([`side_of`]), so a near side that is the holder's own is
/// never mistaken for an overrun.
pub(super) const PAST_CENTRE: f64 = 2.5;
/// On-axis float dust: a tip this close to the axis stands on neither side.
const ON_AXIS: f64 = 1e-6;

/// A meridian piece, (station, signed radius) at both ends.
type Seg = ((f64, f64), (f64, f64));

/// The holder's side of a path's signed radial, from its moves' meridian
/// `pieces` (each flagged `true` when it is a round arc about the axis),
/// and the pieces set on that side: `1.0` when the holder stands on
/// the plus side, `-1.0` on the minus side ([`side_of`]). A round arc
/// about the axis (a thread's half-turn) has its ends on both sides of the axis
/// by construction while the tip rides one radius: it tells no side, is
/// left out of the reading, and is read at its radius on the holder's
/// side (review of PR #1167: the thread kernel's antipodal arc ends read
/// raw convicted every threading op as "both sides").
pub(super) fn sided(pieces: Vec<(Vec<Seg>, bool)>) -> Result<(f64, Vec<Vec<Seg>>), String> {
    let side = side_of(pieces.iter().filter(|(_, round)| !round).flat_map(|(ps, _)| ps.iter().flat_map(|&(p, q)| [p.1, q.1])))?;
    let on_side = |(s, r): (f64, f64)| (s, side * r.abs());
    let pieces = pieces.into_iter().map(|(ps, round)| if round { ps.into_iter().map(|(p, q)| (on_side(p), on_side(q))).collect() } else { ps }).collect();
    Ok((side, pieces))
}

/// The holder's side over the signed tip radii `xs`: the side the path
/// reaches farthest on; the other may hold only a past-centre overrun
/// ([`PAST_CENTRE`]) while the far side runs beyond twice it. Refuses by
/// name otherwise - the side a body is read on is never guessed.
pub(super) fn side_of(xs: impl IntoIterator<Item = f64>) -> Result<f64, String> {
    let (mut pos, mut neg) = (0.0_f64, 0.0_f64);
    for x in xs {
        if !x.is_finite() {
            return Err("a tip's radius is not finite".into());
        }
        pos = pos.max(x);
        neg = neg.max(-x);
    }
    let (far, near) = if neg > pos { (neg, pos) } else { (pos, neg) };
    if near > ON_AXIS && (near > PAST_CENTRE || far <= 2.0 * PAST_CENTRE) {
        return Err(format!(
            "the path stands on both sides of the spindle axis ({:.3} mm on one, {:.3} mm on the other) - which side holds the holder cannot be told; a tip may run at most {PAST_CENTRE} mm past centre",
            far, near
        ));
    }
    Ok(if neg > pos { -1.0 } else { 1.0 })
}

#[cfg(test)]
#[path = "turning_holder_side_tests.rs"]
mod tests;

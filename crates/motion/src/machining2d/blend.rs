//! Turning-point blends (M11.1 A): a feed reversal rides ONE tangent
//! circular arc instead of a feed-stop-feed corner. Provenance: RS-274 /
//! ISO 6983 G2/G3 circular interpolation (public standards) and the
//! path-filleting practice of general machining knowledge; clean-room.
//!
//! Scope: the facing serpentine's row-end reversals. Contour corners and
//! between-pass links (M11.1 B/C) round by the tangent-fillet law in
//! `fillet.rs`, which shares this module's resolution floor.
//!
//! # The legality law (Prime Directive: never gouge, never silent)
//!
//! A reversal between rows `y0 < y1` (stride `s = y1 - y0`) at the row
//! end `xe` - the footprint's bbox edge plus a one-radius overrun - is,
//! sharp: feed to `(xe, y0)`, feed to `(xe, y1)`. Blended, the rows are
//! untouched and the step becomes a semicircle of radius `rho = s/2`
//! about `(xe, (y0 + y1)/2)`, bulging AWAY from the footprint: same
//! endpoints, same feed, one G2/G3 block. The arc is legal iff:
//!
//! - **footprint (live geometry)**: every arc point has its center at
//!   least `xe`, i.e. one tool radius outside the footprint - exactly
//!   where the sharp step stood. The arc's disk trail therefore never
//!   enters the footprint, the rows are byte-identical, and the
//!   footprint's coverage is unchanged to the point. This is also the
//!   verifier's boundary law (`verify` law (b): every law point - an
//!   arc's chord joints - holds `r` off the region boundary), so the
//!   blended path verifies exactly as clean as the sharp one. (The
//!   inward variant - rows stopped `rho` short, apex on the old step -
//!   sweeps a strict subset of the sharp path but parks row ends within
//!   `r - rho` of the footprint edge, which that law reads as a gouge
//!   and the post gate refuses; rejected for that reason.)
//! - **tool radius**: `rho <= r`. The arc reaches `rho` past the sharp
//!   overrun, outside the footprint; bounding it by one tool radius
//!   keeps the extra reach within the same scale the generator's own
//!   overrun already claims. A wider stride is refused - sharp, named.
//!   Whatever stands out there (fixtures, travel limits) is judged by
//!   the existing verify/machine laws on the arc's chords, which block
//!   the post on a hit; nothing is assumed.
//! - **resolution**: `rho >= MIN_BLEND_RADIUS`. A stride narrower than
//!   that (the band's last snapped row can be arbitrarily close) is a
//!   circle no control can interpolate against a micron-lattice center:
//!   refused, sharp, named.
//! - **tolerance / rest**: no deviation reaches the footprint, so an op
//!   tolerance does not bind the facing radius (contour-corner blends,
//!   later, are where it will). The rounding's rest - the overrun corners
//!   between rows the sharp step swept and the arc does not - lies
//!   outside the footprint; where stock stands there, the stock sim
//!   measures and reports it like any rest. Nothing is hidden here.

/// The smallest blend radius emitted, mm: below it the arc is finer than
/// what a control's arc-endpoint check tolerates against a center on the
/// 1 um lattice ([`crate::cl::lattice`]), so the corner stays sharp.
pub const MIN_BLEND_RADIUS: f64 = 0.01;

/// Why a reversal kept its sharp corner - the named refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sharp {
    /// Stride wider than the tool diameter: the arc would reach more
    /// than one tool radius past the overrun.
    Wide,
    /// Stride below twice [`MIN_BLEND_RADIUS`].
    Tiny,
}

/// The legality law for one reversal: the blend radius, or the reason
/// the corner stands. Non-finite input is never an arc.
pub(super) fn reversal_radius(stride: f64, r: f64) -> Result<f64, Sharp> {
    let rho = stride * 0.5;
    if !(rho.is_finite() && r.is_finite()) || rho > r {
        return Err(Sharp::Wide);
    }
    if rho < MIN_BLEND_RADIUS {
        return Err(Sharp::Tiny);
    }
    Ok(rho)
}

/// How a blended reversal turns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Turn {
    /// One semicircle of half the stride on the step's own endpoints.
    Semicircle,
    /// Two quarter arcs of this radius joined by a straight step (#1759).
    Corners(f64),
}

/// The corner-arc law for one reversal under a user radius `rho`
/// (#1759, held to `rho <= r` by `corner_arc_radius`). A stride with
/// room for two quarter arcs (`stride > 2 rho`) turns on them: the step
/// stands `rho` past the sharp one, every center at least `xe` (the
/// footprint law of the module doc, unchanged), the reach `rho <= r`
/// (the tool-radius law) - so a stride wider than the tool rounds too.
/// A stride without that room turns on its semicircle, the largest
/// tangent arc it holds: the radius shrinks, never grows.
///
/// The radius never grows past `rho` (review of #2831): a stride whose
/// half exceeds `rho` by less than the straight-step floor turns on
/// corner arcs shrunk to keep that floor, not on its larger semicircle;
/// below [`MIN_BLEND_RADIUS`] it stays sharp (`Tiny`).
pub(super) fn corner_turn(stride: f64, r: f64, rho: f64) -> Result<Turn, Sharp> {
    let ok = rho.is_finite() && rho >= MIN_BLEND_RADIUS && rho <= r && stride.is_finite();
    let gap = stride * 0.5 - rho;
    if ok && gap > MIN_BLEND_RADIUS * 0.5 {
        return Ok(Turn::Corners(rho));
    }
    // Float noise only: a semicircle within this of `rho` is `rho`.
    const SAME: f64 = 1e-9;
    if ok && gap > SAME {
        let shrunk = stride * 0.5 - MIN_BLEND_RADIUS * 0.5;
        return if shrunk >= MIN_BLEND_RADIUS { Ok(Turn::Corners(shrunk)) } else { Err(Sharp::Tiny) };
    }
    reversal_radius(stride, r).map(|_| Turn::Semicircle)
}

/// The reversal plan for sorted row centers `ys`: entry `i` is the turn
/// from row `i` to row `i + 1` - `Some` blends it, `None` keeps it
/// sharp. With `enabled == false` every turn is sharp and nothing is
/// said: the legacy serpentine, byte-for-byte. `radius` is the user's
/// corner-arc radius (`None`: the semicircle). The notes name every
/// sharp corner the law refused (counted per reason), so a facing that
/// asked for blends and got fewer says so on the status line.
pub(super) fn plan_reversals(ys: &[f64], r: f64, enabled: bool, radius: Option<f64>) -> (Vec<Option<Turn>>, Vec<String>) {
    let turns = ys.len().saturating_sub(1);
    if !enabled {
        return (vec![None; turns], Vec::new());
    }
    let mut plan = Vec::with_capacity(turns);
    let (mut wide, mut tiny) = (0usize, 0usize);
    for w in ys.windows(2) {
        let turn = match radius {
            Some(rho) => corner_turn(w[1] - w[0], r, rho),
            None => reversal_radius(w[1] - w[0], r).map(|_| Turn::Semicircle),
        };
        match turn {
            Ok(t) => plan.push(Some(t)),
            Err(Sharp::Wide) => {
                wide += 1;
                plan.push(None);
            }
            Err(Sharp::Tiny) => {
                tiny += 1;
                plan.push(None);
            }
        }
    }
    let mut notes = Vec::new();
    if wide > 0 {
        notes.push(format!(
            "turn blends: {wide} of {turns} row reversal(s) kept sharp - row spacing wider than the tool diameter, the arc would reach past the overrun by more than a tool radius"
        ));
    }
    if tiny > 0 {
        notes.push(format!(
            "turn blends: {tiny} of {turns} row reversal(s) kept sharp - row spacing below the {:.3} mm arc resolution",
            2.0 * MIN_BLEND_RADIUS
        ));
    }
    (plan, notes)
}

#[cfg(test)]
mod law {
    use super::*;

    #[test]
    fn a_user_radius_turns_on_corner_arcs_where_the_stride_has_room() {
        assert_eq!(corner_turn(8.0, 5.0, 2.0), Ok(Turn::Corners(2.0)));
        // No room for two quarter arcs: the semicircle, never a larger arc.
        assert_eq!(corner_turn(8.0, 5.0, 4.0), Ok(Turn::Semicircle));
        assert_eq!(corner_turn(8.0, 5.0, 5.0), Ok(Turn::Semicircle));
        // A stride wider than the tool rounds at a radius, not without one.
        assert_eq!(corner_turn(12.0, 5.0, 3.0), Ok(Turn::Corners(3.0)));
        assert_eq!(corner_turn(12.0, 5.0, f64::NAN), Err(Sharp::Wide));
        assert_eq!(corner_turn(0.001, 5.0, 2.0), Err(Sharp::Tiny));
        let (plan, notes) = plan_reversals(&[0.0, 12.0, 20.0, 20.001], 5.0, true, Some(3.0));
        assert_eq!(plan, vec![Some(Turn::Corners(3.0)), Some(Turn::Corners(3.0)), None]);
        assert_eq!(notes.len(), 1, "{notes:?}");
    }

    #[test]
    fn a_short_step_never_turns_on_an_arc_larger_than_the_radius() {
        // Half the stride is 0.0025 mm past rho: no semicircle of 0.0125.
        assert_eq!(corner_turn(0.025, 5.0, 0.01), Err(Sharp::Tiny));
        // Room to shrink: corner arcs below rho, keeping the step floor.
        match corner_turn(8.004, 5.0, 4.0) {
            Ok(Turn::Corners(s)) => assert!(s < 4.0 && (s - 3.997).abs() < 1e-9, "{s}"),
            other => panic!("{other:?}"),
        }
        // Exactly half the stride: the semicircle, radius == rho.
        assert_eq!(corner_turn(8.0, 5.0, 4.0), Ok(Turn::Semicircle));
    }

    #[test]
    fn radius_is_half_the_stride_up_to_the_tool_radius() {
        assert_eq!(reversal_radius(8.0, 5.0), Ok(4.0));
        assert_eq!(reversal_radius(10.0, 5.0), Ok(5.0));
        assert_eq!(reversal_radius(10.5, 5.0), Err(Sharp::Wide));
        assert_eq!(reversal_radius(0.001, 5.0), Err(Sharp::Tiny));
        assert_eq!(reversal_radius(f64::NAN, 5.0), Err(Sharp::Wide));
        assert_eq!(reversal_radius(4.0, f64::INFINITY), Err(Sharp::Wide));
    }

    #[test]
    fn disabled_plans_nothing_and_says_nothing() {
        let (plan, notes) = plan_reversals(&[5.0, 13.0, 21.0], 5.0, false, Some(1.0));
        assert_eq!(plan, vec![None, None]);
        assert!(notes.is_empty());
        let (plan, notes) = plan_reversals(&[20.0], 5.0, true, None);
        assert!(plan.is_empty() && notes.is_empty(), "one row, no turn");
    }

    #[test]
    fn refusals_are_counted_and_named() {
        let (plan, notes) = plan_reversals(&[0.0, 12.0, 20.0, 20.001], 5.0, true, None);
        assert_eq!(plan, vec![None, Some(Turn::Semicircle), None]);
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(notes[0].contains("1 of 3") && notes[0].contains("wider"));
        assert!(notes[1].contains("1 of 3") && notes[1].contains("resolution"));
    }
}

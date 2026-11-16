//! Sinker EDM side burning (#545): the electrode fed down to its depth
//! as the burn to depth does (#541), then moved sideways at the floor
//! along one machine axis, so its side face burns the wall it faces out
//! by a stated amount - every move explicit CL, so the 3-axis stock sim
//! verifies exactly what posts (#290 D3, D7).
//!
//! Provenance: lateral (side) burning as described in public EDM
//! machining texts and machine-tool handbooks - an electrode made
//! undersize per side, sunk to depth, then translated toward one wall
//! so the side spark gap cuts that wall to size; the allowance law and
//! the refusals are our own design (#290 D1-D10). No vendor material.
//!
//! The allowance: the electrode is `undersize` smaller per side than
//! the finished cavity. Sunk at the burn point under its condition, its
//! side face leaves each wall one spark gap out, `undersize - gap` short
//! of the finished wall. That is the side's allowance: a lateral move
//! of `amount` moves the faced wall out by exactly `amount`, and an
//! amount past the allowance burns into the part - refused by name. A
//! gap larger than the undersize burns past the wall on the plunge
//! alone - refused by name too.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::Point3;
use crate::machining2d::GenError;
use crate::sinker::{check, SinkerParams, ON_FLOOR};
use crate::sinker_wear::{burn_compensated, deepest_tip, SinkerWear};

/// Slack under which an amount counts as "at" the allowance, mm.
const AT_ALLOWANCE: f64 = 1e-9;

/// The machine axis direction the electrode moves along to burn a wall.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SideDirection {
    /// Toward +X: burns the wall on the electrode's +X side.
    #[default]
    PlusX,
    MinusX,
    PlusY,
    MinusY,
}

impl SideDirection {
    /// The four directions, in the order a page lists them.
    pub const ALL: [SideDirection; 4] = [Self::PlusX, Self::MinusX, Self::PlusY, Self::MinusY];

    /// The direction's name, as a refusal, a note or the file states it.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::PlusX => "+X",
            Self::MinusX => "-X",
            Self::PlusY => "+Y",
            Self::MinusY => "-Y",
        }
    }

    /// The unit step in XY.
    #[must_use]
    pub fn unit(self) -> (f64, f64) {
        match self {
            Self::PlusX => (1.0, 0.0),
            Self::MinusX => (-1.0, 0.0),
            Self::PlusY => (0.0, 1.0),
            Self::MinusY => (0.0, -1.0),
        }
    }
}

/// The side burn's own numbers beside the burn's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SideBurn {
    pub direction: SideDirection,
    /// How far the faced wall moves out, mm: the lateral travel.
    pub amount: f64,
    /// The electrode's undersize per side against the finished cavity, mm.
    pub undersize: f64,
}

/// The side's structural refusals, with no condition read yet: an
/// amount or undersize that is not a number in range, an amount past
/// the undersize (no spark gap is negative, so it is past any
/// allowance). `None` when sound.
#[must_use]
pub fn side_refusal(s: &SideBurn) -> Option<String> {
    if !(s.amount.is_finite() && s.amount > 0.0) {
        return Some(format!(
            "sinker side burn: a lateral amount of {} moves no wall - state a positive, finite amount",
            s.amount
        ));
    }
    if !(s.undersize.is_finite() && s.undersize >= 0.0) {
        return Some(format!(
            "sinker side burn: an electrode undersize of {} per side is not a finite, non-negative number",
            s.undersize
        ));
    }
    if s.amount > s.undersize + AT_ALLOWANCE {
        return Some(format!(
            "sinker side burn: a lateral move of {:.3} toward {} burns into the part - it exceeds the electrode undersize {:.3} per side",
            s.amount,
            s.direction.word(),
            s.undersize
        ));
    }
    None
}

/// The side's allowance under a condition's spark gap: `undersize -
/// gap`. Refused by name: a gap the undersize cannot honour, or an
/// amount past the allowance (a lateral move into the part).
///
/// # Errors
/// Each refusal above, and [`side_refusal`]'s, in words.
pub fn allowance(s: &SideBurn, gap: f64) -> Result<f64, String> {
    if let Some(why) = side_refusal(s) {
        return Err(why);
    }
    if !(gap.is_finite() && gap >= 0.0) {
        return Err(format!("sinker side burn: the condition's spark gap {gap} is not finite and non-negative"));
    }
    if gap > s.undersize + AT_ALLOWANCE {
        return Err(format!(
            "sinker side burn: the condition's spark gap {gap:.3} exceeds the electrode undersize {:.3} per side - the plunge alone would burn past the finished wall; pick a condition whose gap the undersize can honour",
            s.undersize
        ));
    }
    let room = (s.undersize - gap).max(0.0);
    if s.amount > room + AT_ALLOWANCE {
        return Err(format!(
            "sinker side burn: a lateral move of {:.3} toward {} burns into the part - the side's allowance is {room:.3} (undersize {:.3} less spark gap {gap:.3})",
            s.amount,
            s.direction.word(),
            s.undersize
        ));
    }
    Ok(room)
}

/// The side burn: the burn's approach and plunge (its every refusal),
/// one lateral feed of `amount` along the direction at the floor, the
/// same feed back to the burn point, and the rapid retract up Z. The
/// moves are the electrode TIP's path (the face centre).
///
/// # Errors
/// The burn's refusals, an orbit (a side burn moves along one axis
/// only), and [`allowance`]'s.
pub fn side_burn(p: &SinkerParams, s: &SideBurn) -> Result<Toolpath, GenError> {
    if p.orbit.is_some() {
        return Err(GenError::Invalid(
            "sinker side burn: a side burn moves along one axis - it takes no orbit (orbit with a Sinker Burn)".into(),
        ));
    }
    let z = check(p)?;
    let room = allowance(s, p.gap).map_err(GenError::Invalid)?;
    // The plunge's tip as the burn door placed it (one spark gap; a
    // side burn's op never states #543's stand-off).
    let tip = z + crate::sinker::stand(p);
    let (ux, uy) = s.direction.unit();
    let side = Point3::new(p.x + ux * s.amount, p.y + uy * s.amount, tip);
    let mut b = PathBuilder::approached(Point3::new(p.x, p.y, p.approach_z));
    b.push(MoveKind::Feed, Point3::new(p.x, p.y, tip), Some(p.feed));
    b.push(MoveKind::Feed, side, Some(p.feed));
    b.push(MoveKind::Feed, Point3::new(p.x, p.y, tip), Some(p.feed));
    b.push(MoveKind::Rapid, Point3::new(p.x, p.y, p.approach_z), None);
    b.note(format!(
        "sinker side burn: {:.3} toward {} at Z {z:.3}, tip at Z {tip:.3} (undersize {:.3}, spark gap {:.3}, allowance {room:.3})",
        s.amount,
        s.direction.word(),
        s.undersize,
        p.gap
    ));
    Ok(b.finish())
}

/// The side burn with #546's electrode wear compensation and layering
/// (#1340): the plunge is `sinker_wear::burn_compensated`'s - every layer
/// tip deepened by the stated axial wear rate, under all of its refusals
/// (rate, layer step, electrode length, the deepest-tip bound) - and the
/// lateral move and its return run at the last layer's compensated tip,
/// re-judged against that bound, so the worn face rides the burn floor
/// one spark gap up, as the plain side burn's does. The side's own
/// allowance law is unchanged: only the axial (end) wear is compensated;
/// the electrode's side wear makes it smaller, which leaves the wall
/// short of the stated amount, never past it. With no rate and no layer
/// step the moves and notes are exactly [`side_burn`]'s.
///
/// # Errors
/// [`side_burn`]'s refusals and `sinker_wear::burn_compensated`'s.
pub fn side_burn_worn(p: &SinkerParams, s: &SideBurn, w: &SinkerWear) -> Result<Toolpath, GenError> {
    // Uncompensated, or an orbit (which `side_burn` refuses by name).
    if (w.comp.is_none() && w.layer.is_none()) || p.orbit.is_some() {
        return side_burn(p, s);
    }
    let z = check(p)?;
    let room = allowance(s, p.gap).map_err(GenError::Invalid)?;
    let plunge = burn_compensated(p, w)?;
    let rate = w.comp.as_ref().map_or(0.0, |c| c.rate);
    let deepest = deepest_tip(p, rate);
    let tip = plunge.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| m.to.z).next_back();
    let Some(tip) = tip.filter(|t| t.is_finite() && *t >= deepest - ON_FLOOR) else {
        return Err(GenError::Invalid(format!(
            "sinker side burn: the compensated plunge reaches no tip at or above the compensated floor Z {deepest:.3}"
        )));
    };
    let (ux, uy) = s.direction.unit();
    let mut b = PathBuilder::approached(Point3::new(p.x, p.y, p.approach_z));
    for m in plunge.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        b.push(MoveKind::Feed, m.to, m.feed);
    }
    b.push(MoveKind::Feed, Point3::new(p.x + ux * s.amount, p.y + uy * s.amount, tip), Some(p.feed));
    b.push(MoveKind::Feed, Point3::new(p.x, p.y, tip), Some(p.feed));
    b.push(MoveKind::Rapid, Point3::new(p.x, p.y, p.approach_z), None);
    for n in &plunge.notes {
        b.note(n.clone());
    }
    b.note(format!(
        "sinker side burn: {:.3} toward {} at Z {z:.3}, tip at Z {tip:.3} after wear compensation (undersize {:.3}, spark gap {:.3}, allowance {room:.3}; side wear not compensated)",
        s.amount,
        s.direction.word(),
        s.undersize,
        p.gap
    ));
    Ok(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinker::SinkerDatum;

    fn params() -> SinkerParams {
        SinkerParams {
            x: 0.0,
            y: 0.0,
            approach_z: 25.0,
            top_z: 20.0,
            floor_z: 10.0,
            model_floor_z: Some(0.0),
            datum: SinkerDatum::WorkTop,
            depth: 6.0,
            electrode_len: 30.0,
            feed: 40.0,
            gap: 0.05,
            stand: None,
            orbit: None,
        }
    }

    #[test]
    fn sinker_side_moves_along_its_axis_by_the_amount_and_back() {
        for d in SideDirection::ALL {
            let s = SideBurn { direction: d, amount: 0.15, undersize: 0.2 };
            let path = side_burn(&params(), &s).expect("burns");
            let feeds: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
            assert_eq!(feeds.len(), 3, "{d:?}: plunge, out, back");
            let (ux, uy) = d.unit();
            assert_eq!((feeds[1].to.x, feeds[1].to.y, feeds[1].to.z), (ux * 0.15, uy * 0.15, 14.05));
            assert_eq!((feeds[2].to.x, feeds[2].to.y), (0.0, 0.0));
            assert_eq!(path.moves.last().map(|m| (m.kind, m.to.z)), Some((MoveKind::Rapid, 25.0)));
        }
    }

    #[test]
    fn sinker_side_refuses_past_the_allowance_by_name() {
        let p = params();
        let s = SideBurn { direction: SideDirection::MinusY, amount: 0.16, undersize: 0.2 };
        let e = side_burn(&p, &s).expect_err("past the allowance");
        assert!(e.to_string().contains("toward -Y burns into the part - the side's allowance is 0.150"), "{e}");
        let at = SideBurn { amount: 0.15, ..s };
        assert!(side_burn(&p, &at).is_ok(), "exactly the allowance is admitted");
        let thin = SideBurn { undersize: 0.04, amount: 0.01, ..s };
        let e = side_burn(&p, &thin).expect_err("gap past the undersize");
        assert!(e.to_string().contains("exceeds the electrode undersize 0.040"), "{e}");
        for amount in [0.0, -0.1, f64::NAN, f64::INFINITY] {
            let e = side_burn(&p, &SideBurn { amount, ..s }).expect_err("no amount");
            assert!(e.to_string().contains("moves no wall"), "{amount}: {e}");
        }
        let orbit = crate::sinker_orbit::Orbit { kind: crate::sinker_orbit::OrbitKind::Circular, radius: 0.1, vectors: 4, turns: 1 };
        let e = side_burn(&SinkerParams { orbit: Some(orbit), ..p }, &at).expect_err("an orbit");
        assert!(e.to_string().contains("takes no orbit"), "{e}");
    }

    fn worn(rate: f64, layer: Option<f64>) -> SinkerWear {
        SinkerWear { comp: Some(crate::sinker_wear::WearComp { rate, from: "test".into() }), layer }
    }

    #[test]
    fn sinker_side_worn_off_is_the_plain_side_burn() {
        let s = SideBurn { direction: SideDirection::PlusY, amount: 0.15, undersize: 0.2 };
        let off = side_burn_worn(&params(), &s, &SinkerWear::default()).expect("burns");
        let plain = side_burn(&params(), &s).expect("burns");
        assert_eq!((off.moves, off.notes), (plain.moves, plain.notes));
    }

    #[test]
    fn sinker_side_worn_plunges_in_compensated_layers_and_moves_at_the_deepest_tip() {
        let p = params();
        let s = SideBurn { direction: SideDirection::PlusX, amount: 0.15, undersize: 0.2 };
        let path = side_burn_worn(&p, &s, &worn(0.02, Some(2.0))).expect("burns");
        let feeds: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
        // Layers at 2, 4, 6 below the top, each deepened by 0.02 per mm
        // burned; then out and back at the last layer's tip.
        let want = [18.05 - 0.04, 16.05 - 0.08, 14.05 - 0.12];
        assert_eq!(feeds.len(), 5, "three layers, out, back");
        for (m, z) in feeds.iter().zip(want) {
            assert!((m.to.z - z).abs() < 1e-9 && m.to.x == 0.0, "{:?}", m.to);
        }
        let deepest = deepest_tip(&p, 0.02);
        assert!((feeds[3].to.z - deepest).abs() < 1e-9 && (feeds[4].to.z - deepest).abs() < 1e-9);
        assert_eq!((feeds[3].to.x, feeds[4].to.x), (0.15, 0.0));
        // No move below the compensated floor; the worn face rides the
        // floor one gap up through the lateral move.
        assert!(path.moves.iter().all(|m| m.to.z >= deepest - 1e-9));
        assert!((crate::sinker_wear::worn_tip(feeds[3].to.z, 20.0, 0.05, 0.02) - 14.05).abs() < 1e-9);
        assert_eq!(path.moves.last().map(|m| (m.kind, m.to.z)), Some((MoveKind::Rapid, 25.0)));
        assert!(path.notes.iter().any(|n| n.contains("compensated in Z at 0.0200")), "{:?}", path.notes);
        // A layer step alone layers the plunge without compensation.
        let layered = side_burn_worn(&p, &s, &SinkerWear { comp: None, layer: Some(3.0) }).expect("burns");
        let z: Vec<_> = layered.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| m.to.z).collect();
        assert_eq!(z, [17.05, 14.05, 14.05, 14.05]);
    }

    #[test]
    fn sinker_side_worn_keeps_every_refusal_by_name() {
        let p = params();
        let s = SideBurn { direction: SideDirection::MinusX, amount: 0.16, undersize: 0.2 };
        let e = side_burn_worn(&p, &s, &worn(0.02, None)).expect_err("past the allowance");
        assert!(e.to_string().contains("burns into the part"), "{e}");
        let at = SideBurn { amount: 0.15, ..s };
        for r in [f64::NAN, -0.1, 1.0] {
            let e = side_burn_worn(&p, &at, &worn(r, None)).expect_err("rate");
            assert!(e.to_string().contains("not an axial wear rate"), "{r}: {e}");
        }
        let e = side_burn_worn(&p, &at, &SinkerWear { comp: None, layer: Some(0.0) }).expect_err("step");
        assert!(e.to_string().contains("layer step"), "{e}");
        let short = SinkerParams { electrode_len: 6.0, ..p };
        let e = side_burn_worn(&short, &at, &worn(0.05, None)).expect_err("holder");
        assert!(e.to_string().contains("with wear compensation"), "{e}");
        let orbit = crate::sinker_orbit::Orbit { kind: crate::sinker_orbit::OrbitKind::Circular, radius: 0.1, vectors: 4, turns: 1 };
        let e = side_burn_worn(&SinkerParams { orbit: Some(orbit), ..p }, &at, &worn(0.02, None)).expect_err("orbit");
        assert!(e.to_string().contains("takes no orbit"), "{e}");
    }
}

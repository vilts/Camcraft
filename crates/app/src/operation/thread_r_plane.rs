//! The thread mill's R plane (#1212): where each rapid down to a hole or a boss stops and the feed begins, judged at the door and against the stock the program ahead leaves.
//!
//! Provenance: the drill cycle's R plane as RS-274 / ISO 6983 canned
//! cycles and public machine-tool handbooks state it (rapid to R, feed
//! from R), and the shop rule "never rapid into uncut stock"; the
//! readings are our own design over the prior grid
//! ([`super::drill_prior`]). No vendor material consulted.
//!
//! The kernel ([`crate::machining2d::thread_mill`]) stops every rapid
//! at the operation's stated R plane (`top_z`, "R plane (feed starts
//! here)") and refuses one under a feature's top. Two readings stand in
//! front of it here: the door refuses a FIXED R plane under a listed
//! feature's top when the operation is made or edited (a referenced
//! height resolves only at generate, where the kernel asks), and
//! [`judge`] refuses a rapid whose end the stock left by the program
//! ahead still stands over - the plane would end the rapid inside it.

use crate::cl::{MoveKind, Toolpath};
use crate::drilling::Hole;
use crate::geo2d::Point2;
use crate::machining2d::GenError;
use crate::stock_sim::{Columns, Dexel};

use super::types::{Height, HeightReference};

/// Float slack on the grid's heights, mm: stock cut exactly to the R
/// plane is air there.
const Z_SLACK: f64 = 1e-6;

/// Why a fixed R plane `top_z` cannot serve `features`, by name, or
/// `None` (a referenced height is judged at generate, resolved).
#[must_use]
pub(super) fn door(top_z: Height, features: &[Hole], external: bool) -> Option<String> {
    if top_z.reference != HeightReference::Fixed {
        return None;
    }
    let r = top_z.offset;
    let what = if external { "boss" } else { "hole" };
    features.iter().find(|x| !r.is_finite() || r < x.entry_z - 1e-9).map(|x| {
        format!(
            "the R plane at z {r} is below the top of the {what} at ({}, {}) at z {} - the rapid down to R would run into it; raise the R plane (feed starts here) to z {} or above",
            x.xy.x, x.xy.y, x.entry_z, x.entry_z
        )
    })
}

/// Why a rapid of `path` would end inside the stock the program ahead
/// leaves standing, by name, or `Ok`. Every vertical rapid DOWN is a
/// dive onto the R plane (or the ride's own run-out, when higher); the
/// highest column the tool's disc of `tool_r` covers there must stand
/// at or below its end. An unknown prior (`None`) or an unreadable disc
/// is left to the way-in laws ([`super::thread_entry`]), which refuse
/// both by name, and to verify's rapid law.
pub(super) fn judge(path: &Toolpath, tool_r: f64, prior: Option<&Dexel>) -> Result<(), GenError> {
    let Some(dex) = prior else {
        return Ok(());
    };
    let cols = Columns::of_grid(dex);
    for m in path.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
        let (dx, dy) = (m.to.x - m.from.x, m.to.y - m.from.y);
        if dx.hypot(dy) > 1e-9 || m.to.z >= m.from.z {
            continue;
        }
        let Some(top) = crate::verify::prior_surface(&cols, Point2::new(m.to.x, m.to.y), tool_r) else {
            continue;
        };
        if top > m.to.z + Z_SLACK {
            return Err(GenError::Invalid(format!(
                "the rapid down to the R plane ends at z {:.3} at ({:.3}, {:.3}), inside the stock the program ahead leaves standing at z {top:.3} over the thread mill there - raise the R plane (feed starts here) to z {top:.3} or above, or machine that stock away before the thread",
                m.to.z, m.to.x, m.to.y
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::Move;
    use crate::geometry::Point3;

    fn hole(entry_z: f64) -> Hole {
        Hole { xy: Point2::new(0.0, 0.0), radius: 3.3, entry_z, floor_z: 8.0, through: false, stepped: false }
    }

    /// A rapid over the axis, the dive down to `r`, and the feed into the hole.
    fn path(r: f64) -> Toolpath {
        let mut t = Toolpath::default();
        t.moves.push(Move::new(MoveKind::Rapid, Point3::new(0.0, 0.0, 30.0), Point3::new(0.0, 0.0, r), None));
        t.moves.push(Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, r), Point3::new(0.0, 0.0, 8.0), Some(100.0)));
        t
    }

    /// A block topped at `z`, its centre cut down to 5 out to `cut_r`.
    fn block(z: f64, cut_r: f64) -> Dexel {
        let mut d = Dexel::block(-10.0, -10.0, 10.0, 10.0, z, 0.25).expect("block");
        d.cut_chord(Point3::new(0.0, 0.0, 5.0), Point3::new(0.0, 0.0, 5.0), cut_r);
        d
    }

    #[test]
    fn a_fixed_r_plane_under_a_features_top_is_refused_at_the_door() {
        let low = door(Height::fixed(19.0), &[hole(20.0)], false).unwrap_or_default();
        assert!(low.contains("the R plane at z 19 is below the top of the hole at (0, 0) at z 20"), "{low}");
        assert!(door(Height::fixed(19.0), &[hole(20.0)], true).is_some_and(|m| m.contains("of the boss")));
        assert_eq!(door(Height::fixed(20.0), &[hole(20.0)], false), None);
        assert!(door(Height::fixed(f64::NAN), &[hole(20.0)], false).is_some(), "a NaN plane is never above");
        // A referenced height resolves at generate, where the kernel asks.
        assert_eq!(door(Height::of(HeightReference::TopOfModel, -5.0), &[hole(20.0)], false), None);
    }

    #[test]
    fn a_rapid_ending_inside_the_stock_left_ahead_is_refused_by_name() {
        // The stock stands 2 mm over the hole's mouth and only a Ø5 column under the Ø6 mill is open.
        let msg = judge(&path(20.0), 3.0, Some(&block(22.0, 2.5))).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(msg.contains("ends at z 20.000") && msg.contains("standing at z 22.000"), "{msg}");
        // A raised R plane over that stock, or the column opened under the whole mill, is air.
        assert!(judge(&path(22.0), 3.0, Some(&block(22.0, 2.5))).is_ok());
        assert!(judge(&path(20.0), 3.0, Some(&block(22.0, 3.3))).is_ok());
        // Unknown is the way-in laws' to refuse.
        assert!(judge(&path(20.0), 3.0, None).is_ok());
    }
    /// The operation door's "pitch not stated yet" branch (a new Thread
    /// on a single-form mill) still asks the R-plane door: a fixed R
    /// plane under the hole's top is refused when the operation is made,
    /// not first at generate.
    #[test]
    fn a_low_r_plane_is_refused_at_the_door_before_the_pitch_is_stated() {
        let mill = crate::tools::catalog()
            .into_iter()
            .find(|(n, _)| *n == "TMS-0600-P050-150")
            .map(|(_, t)| t)
            .expect("the catalog's single-form thread mill");
        let op = |r: f64| crate::operation::OpParams::Thread {
            clearance_z: Height::fixed(30.0),
            retract_z: None,
            top_z: Height::fixed(r),
            bottom_z: None,
            infeed: 0.25,
            feed: 300.0,
            plunge_feed: 150.0,
            climb: true,
            holes: vec![hole(20.0)],
            pitch: None,
            hand: crate::cl::ThreadHand::Right,
            external: false,
            passes: crate::machining2d::ThreadPasses::default(),
        };
        let low = super::super::thread_op::door(&mill, &op(19.0)).unwrap_or_default();
        assert!(low.contains("the R plane at z 19 is below the top of the hole"), "{low}");
        assert_eq!(super::super::thread_op::door(&mill, &op(20.0)), None, "an unstated pitch alone is the generator's to name");
    }

    /// Only a vertical rapid DOWN is the dive onto the R plane: a
    /// sideways rapid (even one that descends), or one going up, that ends under the stock left
    /// ahead is the link and rapid laws' to judge, not this one.
    #[test]
    fn only_the_dive_is_judged_against_the_stock_left_ahead() {
        let stock = block(22.0, 3.3);
        let mut side = path(22.0);
        side.moves.insert(0, Move::new(MoveKind::Rapid, Point3::new(-8.0, 0.0, 25.0), Point3::new(8.0, 0.0, 21.0), None));
        assert!(judge(&side, 3.0, Some(&stock)).is_ok(), "a sideways rapid, even descending, is no dive");
        let mut up = path(22.0);
        up.moves.push(Move::new(MoveKind::Rapid, Point3::new(8.0, 0.0, 20.0), Point3::new(8.0, 0.0, 21.0), None));
        assert!(judge(&up, 3.0, Some(&stock)).is_ok(), "a rapid going up is no dive");
    }
}

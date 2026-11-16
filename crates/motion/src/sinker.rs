//! Sinker EDM burn to depth (#541): one electrode fed straight down Z
//! to a stated depth, measured from a stated datum - the plunge as CL
//! moves, so the 3-axis stock sim verifies exactly what posts (#290 D3,
//! D7).
//!
//! Provenance: general die-sink EDM practice from public machine-tool
//! handbooks - the electrode positioned over the cavity, fed down Z
//! under servo control to depth, retracted along the same axis; the
//! datum readings and the refusals are our own design (#290 D1-D10).
//! No vendor material.
//!
//! The path: a rapid to the approach plane over the burn point, a feed
//! plunge to the burn floor, a rapid retract back up the same axis.
//! Nothing here guesses: a depth that is not a positive finite number,
//! a burn floor below the stated floor datum or below the model's
//! floor, a burn deeper than the electrode is long, an approach plane
//! at or below the floor, or a spark gap that lifts the tip to the
//! approach plane or out of the work - each is refused by name.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::Point3;
use crate::machining2d::GenError;
use crate::sinker_orbit::{orbit_offsets, Orbit};

/// Where a burn's depth is measured from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SinkerDatum {
    /// Down from the work's top: the burn floor is `top - depth`.
    #[default]
    WorkTop,
    /// Up from the cavity floor: the burn floor is `floor + depth` -
    /// `depth` is what is left standing above the floor (0 burns
    /// exactly to it). A negative stand-off would be past the floor.
    CavityFloor,
    /// Down from the electrode tip as it stands at the approach plane:
    /// the burn floor is `approach - depth`.
    ElectrodeTip,
}

impl SinkerDatum {
    /// The datum in the words a refusal or a note names.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            SinkerDatum::WorkTop => "work top",
            SinkerDatum::CavityFloor => "cavity floor",
            SinkerDatum::ElectrodeTip => "electrode tip",
        }
    }
}

/// The burn's resolved numbers, world mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SinkerParams {
    /// The face centre the electrode burns at.
    pub x: f64,
    pub y: f64,
    /// The approach / retract plane the electrode tip travels on.
    pub approach_z: f64,
    /// The work's top.
    pub top_z: f64,
    /// The cavity floor datum - the burn never passes it.
    pub floor_z: f64,
    /// The model's lowest Z over all its facets (its global bottom, not
    /// the floor under the electrode's footprint), when one is known:
    /// the burn never passes it either. A coarse bound only - the stock
    /// sim's box atom is what judges the burn against the cavity.
    pub model_floor_z: Option<f64>,
    pub datum: SinkerDatum,
    pub depth: f64,
    /// The electrode's length below its holder: the burn never buries
    /// more than this below the work top.
    pub electrode_len: f64,
    /// The servo feed down, mm/min.
    pub feed: f64,
    /// The spark gap per side of the machine's condition (#531), mm:
    /// the cavity stands this far beyond the electrode's face, so the
    /// tip stops this far above the burn floor.
    pub gap: f64,
    /// How far above the finished floor the tip stops, when an
    /// undersize law places it (#543: the electrode's undersize per
    /// side, `sinker_undersize::judge`); `None` stops it one spark gap
    /// above (#541). Never less than the gap - that would burn past the
    /// floor.
    pub stand: Option<f64>,
    /// The orbit at the burn floor (#544), or none: a straight plunge.
    /// The machine-side laws (a listed pattern, the maximum radius) are
    /// the caller's; the generator checks the numbers.
    pub orbit: Option<Orbit>,
}

/// Slack under which a floor reading counts as "on" the floor, mm.
pub(crate) const ON_FLOOR: f64 = 1e-6;

/// The burn floor `depth` reaches from the stated datum: the cavity's
/// floor as burned (the electrode tip stops one spark gap above it).
#[must_use]
pub fn burn_floor(p: &SinkerParams) -> f64 {
    match p.datum {
        SinkerDatum::WorkTop => p.top_z - p.depth,
        SinkerDatum::CavityFloor => p.floor_z + p.depth,
        SinkerDatum::ElectrodeTip => p.approach_z - p.depth,
    }
}

/// Every refusal of the burn, by name, before a move is built; the
/// burn floor it reaches. The side burn (#545) and the wear-compensated
/// burn (#546) read the same door.
pub(crate) fn check(p: &SinkerParams) -> Result<f64, GenError> {
    let nums = [p.x, p.y, p.approach_z, p.top_z, p.floor_z, p.depth, p.electrode_len, p.feed, p.gap];
    if !nums.iter().all(|v| v.is_finite())
        || p.model_floor_z.is_some_and(|z| !z.is_finite())
        || p.stand.is_some_and(|s| !s.is_finite())
    {
        return Err(GenError::Invalid("sinker burn: a position, height or depth is not a finite number".into()));
    }
    if p.depth < 0.0 || (p.depth == 0.0 && p.datum != SinkerDatum::CavityFloor) {
        return Err(GenError::Invalid(format!(
            "sinker burn: a depth of {} from the {} burns nothing - state a positive depth",
            p.depth,
            p.datum.label()
        )));
    }
    if p.gap < 0.0 {
        return Err(GenError::Invalid(format!("sinker burn: the condition's spark gap {} is negative", p.gap)));
    }
    if let Some(s) = p.stand
        && s < p.gap - ON_FLOOR
    {
        return Err(GenError::Invalid(format!(
            "sinker burn: the tip would stand {s:.3} above the floor, inside its {:.3} spark gap - the floor would burn past size",
            p.gap
        )));
    }
    if p.feed <= 0.0 {
        return Err(GenError::Invalid(format!("sinker burn: the servo feed {} must be positive", p.feed)));
    }
    if p.floor_z > p.top_z {
        return Err(GenError::Invalid(format!(
            "sinker burn: the cavity floor datum ({:.3}) stands above the work top ({:.3})",
            p.floor_z, p.top_z
        )));
    }
    let z = burn_floor(p);
    if z < p.floor_z - ON_FLOOR {
        return Err(GenError::Invalid(format!(
            "sinker burn: a depth of {} from the {} reaches Z {z:.3}, past the cavity floor datum at Z {:.3}",
            p.depth,
            p.datum.label(),
            p.floor_z
        )));
    }
    if let Some(m) = p.model_floor_z
        && z < m - ON_FLOOR
    {
        return Err(GenError::Invalid(format!(
            "sinker burn: a depth of {} from the {} reaches Z {z:.3}, past the model's floor at Z {m:.3}",
            p.depth,
            p.datum.label()
        )));
    }
    if z >= p.top_z {
        return Err(GenError::Invalid(format!(
            "sinker burn: the burn floor Z {z:.3} does not enter the work (its top is Z {:.3})",
            p.top_z
        )));
    }
    if p.approach_z <= p.top_z {
        return Err(GenError::Invalid(format!(
            "sinker burn: the approach plane Z {:.3} must stand above the work top Z {:.3}",
            p.approach_z, p.top_z
        )));
    }
    // The tip stands [`stand`] above the burn floor: one spark gap
    // (#541), or #543's undersize stand-off (never less than the gap,
    // refused above). The two refusals below read that stand, so an
    // undersize cannot lift the tip past them where a gap would not.
    let lift = stand(p);
    let what = if p.stand.is_some() { "undersize stand-off" } else { "spark gap" };
    let tip = z + lift;
    // The plunge must go DOWN: a stand that lifts the tip to the approach
    // plane would post an upward "burn" feed (review of #1131).
    if tip >= p.approach_z {
        return Err(GenError::Invalid(format!(
            "sinker burn: the {what} {lift:.3} places the tip at or above the approach plane (tip Z {tip:.3}, approach Z {:.3})",
            p.approach_z
        )));
    }
    // A stand not smaller than the burn below the work top leaves the tip
    // at or above the top: the electrode never enters the work.
    let burn = p.top_z - z;
    if lift >= burn {
        return Err(GenError::Invalid(format!(
            "sinker burn: the {what} {lift:.3} is not smaller than the burn's {burn:.3} below the work top - the electrode would never enter the work"
        )));
    }
    let buried = p.top_z - tip;
    if buried > p.electrode_len + ON_FLOOR {
        return Err(GenError::Invalid(format!(
            "sinker burn: the burn reaches {buried:.3} below the work top, but the electrode is only {:.3} long - its holder would enter the work",
            p.electrode_len
        )));
    }
    Ok(z)
}

/// How far above the burn floor the tip stops: the stated stand-off
/// (#543's undersize), else one spark gap (#541).
#[must_use]
pub fn stand(p: &SinkerParams) -> f64 {
    p.stand.unwrap_or(p.gap)
}

/// Where the electrode tip stops: [`stand`] above the burn floor.
#[must_use]
pub fn tip_z(p: &SinkerParams) -> f64 {
    burn_floor(p) + stand(p)
}

/// The burn: approach over the point, feed the tip down to one spark
/// gap above the burn floor at the servo feed, orbit there when one is
/// stated (#544: every orbit move explicit, at the same feed, back to
/// the centre), retract up the same axis. The moves are the electrode
/// TIP's path (the face centre).
pub fn burn_to_depth(p: &SinkerParams) -> Result<Toolpath, GenError> {
    let z = check(p)?;
    let orbit = p.orbit.as_ref().map(orbit_offsets).transpose()?;
    let tip = z + stand(p);
    let mut b = PathBuilder::approached(Point3::new(p.x, p.y, p.approach_z));
    b.push(MoveKind::Feed, Point3::new(p.x, p.y, tip), Some(p.feed));
    if let (Some(o), Some(pts)) = (p.orbit.as_ref(), orbit.as_ref()) {
        for &(dx, dy) in pts {
            b.push(MoveKind::Feed, Point3::new(p.x + dx, p.y + dy, tip), Some(p.feed));
        }
        b.note(format!("sinker orbit: {} of radius {:.3} at the burn floor, {} moves", o.kind.word(), o.radius, pts.len()));
    }
    b.push(MoveKind::Rapid, Point3::new(p.x, p.y, p.approach_z), None);
    b.note(format!(
        "sinker burn: {} from the {} to Z {z:.3}, tip at Z {tip:.3} (spark gap {:.3}; cavity floor datum Z {:.3})",
        p.depth,
        p.datum.label(),
        p.gap,
        p.floor_z
    ));
    Ok(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

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
            gap: 0.0,
            stand: None,
            orbit: None,
        }
    }

    #[test]
    fn sinker_burn_reads_each_datum() {
        let mut p = params();
        assert_eq!(burn_floor(&p), 14.0);
        p.datum = SinkerDatum::CavityFloor;
        p.depth = 0.5;
        assert_eq!(burn_floor(&p), 10.5);
        p.datum = SinkerDatum::ElectrodeTip;
        p.depth = 12.0;
        assert_eq!(burn_floor(&p), 13.0);
        let path = burn_to_depth(&p).expect("burns");
        let feeds: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
        assert_eq!(feeds.len(), 1);
        assert_eq!(feeds[0].to.z, 13.0);
        p.gap = 0.25;
        let path = burn_to_depth(&p).expect("burns");
        assert!(path.moves.iter().any(|m| m.kind == MoveKind::Feed && m.to.z == 13.25));
        assert_eq!(path.moves.first().map(|m| m.kind), Some(MoveKind::Rapid));
    }

    #[test]
    fn sinker_burn_refuses_past_the_floor_by_name() {
        let mut p = params();
        p.depth = 10.5; // 20 - 10.5 = 9.5 < floor 10
        let e = burn_to_depth(&p).expect_err("past the floor");
        assert!(e.to_string().contains("past the cavity floor datum"), "{e}");
        p.floor_z = -5.0;
        p.depth = 21.0; // Z -1 < model floor 0
        let e = burn_to_depth(&p).expect_err("past the model");
        assert!(e.to_string().contains("past the model's floor"), "{e}");
        p.model_floor_z = None;
        p.electrode_len = 10.0;
        p.depth = 12.0;
        let e = burn_to_depth(&p).expect_err("buried holder");
        assert!(e.to_string().contains("only 10.000 long"), "{e}");
    }
}

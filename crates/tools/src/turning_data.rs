//! Turning cutting data (#891): the lathe's own numbers - a surface
//! speed Vc (m/min) and a feed per revolution f (mm/rev) per work
//! material and edge material, plus the tool's stated max-rpm clamp.
//! A lathe tool's speed is the WORK's diameter, not the tool's, so the
//! milling table's fz-at-10-mm column cannot speak for it.
//!
//! Provenance: the relation n = Vc*1000/(pi*D) at the working diameter
//! and the per-revolution feed are the textbook turning relations
//! (machine-tool handbook turning chapters). Every NUMBER below is our
//! own conservative starting value, picked at or under the middle of
//! the ranges public handbooks (Machinery's Handbook-style turning
//! speed/feed tables for HSS and uncoated carbide) print for the
//! group - no vendor insert catalog or tool-data file was copied.
//!
//! Pure module: no flecs, no I/O.

use serde::{Deserialize, Serialize};

use crate::cutting::{ToolMaterial, WorkMaterial};

/// The largest surface speed a tool may state, m/min - well above any
/// handbook turning row (aluminium on carbide tops out near 1000).
pub const MAX_VC: f64 = 2000.0;
/// The largest feed per revolution a tool may state, mm/rev - heavy
/// roughing inserts stay under 1.5.
pub const MAX_FEED_PER_REV: f64 = 3.0;
/// The largest max-rpm clamp a tool may state.
pub const MAX_RPM_CLAMP: f64 = 100_000.0;

/// What a lathe tool states for constant surface speed (#891). Stored
/// on the tool; the turning operations (threading aside) post it as
/// G96 S<vc> with the clamp, and per-rev feeds from `feed_per_rev`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurningData {
    /// Surface speed, m/min.
    pub vc: f64,
    /// Feed per revolution, mm/rev.
    pub feed_per_rev: f64,
    /// The tool's own spindle ceiling, rpm (an insert or a chuck rated
    /// lower than the machine). `None` = the machine's alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rpm: Option<f64>,
}

impl TurningData {
    /// Finite, positive and inside the bounds above.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        // #1186: the bounds live in `why_not_sane`, which names them.
        self.why_not_sane().is_none()
    }
}

/// A turning row's surface-speed and feed-per-rev ranges for one edge
/// material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurnWindow {
    pub vc: (f64, f64),
    pub frev: (f64, f64),
}

const fn tw(vc: (f64, f64), frev: (f64, f64)) -> TurnWindow {
    TurnWindow { vc, frev }
}

/// One row of the turning table: (HSS, carbide), in `WorkMaterial::ALL`
/// order (pinned by a test). Our own numbers inside public handbook
/// turning ranges (module provenance), held to the conservative side.
pub const TURN_TABLE: [(WorkMaterial, TurnWindow, TurnWindow); 6] = [
    (WorkMaterial::Aluminium6061, tw((90.0, 180.0), (0.10, 0.30)), tw((200.0, 450.0), (0.10, 0.35))),
    (WorkMaterial::MildSteel, tw((25.0, 40.0), (0.08, 0.25)), tw((120.0, 220.0), (0.10, 0.30))),
    (WorkMaterial::Stainless304, tw((12.0, 25.0), (0.05, 0.20)), tw((80.0, 150.0), (0.08, 0.25))),
    (WorkMaterial::GreyCastIron, tw((18.0, 30.0), (0.08, 0.25)), tw((80.0, 160.0), (0.10, 0.30))),
    (WorkMaterial::Brass, tw((60.0, 100.0), (0.08, 0.25)), tw((150.0, 300.0), (0.10, 0.30))),
    (WorkMaterial::Plastics, tw((80.0, 200.0), (0.08, 0.25)), tw((150.0, 350.0), (0.10, 0.30))),
];

/// The turning window of `m` for edge material `tm`.
#[must_use]
pub fn turn_window(m: WorkMaterial, tm: ToolMaterial) -> TurnWindow {
    let (_, hss, carbide) = TURN_TABLE[m as usize];
    match tm {
        ToolMaterial::Hss => hss,
        ToolMaterial::Carbide => carbide,
    }
}

/// The share of the row's feed per rev a blade takes: grooving and
/// parting blades (and form gauges, a full-width edge) run well under
/// a turning insert's feed in every handbook table. Our own factor.
pub const BLADE_FEED_SHARE: f64 = 0.4;
/// The share of the row's speed and feed a boring bar takes: the
/// overhang a bar cuts from chatters before an OD tool does. Our own
/// factor.
pub const BAR_SHARE: f64 = 0.8;

/// Spindle speed at working diameter `d` mm for surface speed `vc`
/// m/min: n = Vc*1000/(pi*D). `None` for a diameter that is not a
/// finite positive number (on centre the rpm is unbounded - the clamp's
/// job, never this formula's).
#[must_use]
pub fn rpm_at(vc: f64, d: f64) -> Option<f64> {
    (d.is_finite() && d > 0.0 && vc.is_finite() && vc > 0.0)
        .then(|| vc * 1000.0 / (std::f64::consts::PI * d))
        .filter(|n| n.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_indexed_in_all_order_and_windows_are_ordered() {
        for m in WorkMaterial::ALL {
            assert_eq!(TURN_TABLE[m as usize].0, m);
            for tm in ToolMaterial::ALL {
                let w = turn_window(m, tm);
                assert!(w.vc.0 > 0.0 && w.vc.0 < w.vc.1 && w.vc.1 <= MAX_VC);
                assert!(w.frev.0 > 0.0 && w.frev.0 < w.frev.1 && w.frev.1 <= MAX_FEED_PER_REV);
            }
            // Carbide outruns HSS on every row.
            assert!(turn_window(m, ToolMaterial::Carbide).vc.0 > turn_window(m, ToolMaterial::Hss).vc.0);
        }
    }

    #[test]
    fn rpm_at_matches_hand_calculation_and_refuses_the_centre() {
        // 170 m/min on a 50 mm bar: 170000 / (pi*50) = 1082.25 rpm.
        let n = rpm_at(170.0, 50.0).unwrap_or(0.0);
        assert!((n - 1_082.253_613_270_744).abs() < 1e-6, "{n}");
        assert!(rpm_at(170.0, 0.0).is_none());
        assert!(rpm_at(170.0, f64::NAN).is_none());
    }

    #[test]
    fn sanity_bounds() {
        let ok = TurningData { vc: 170.0, feed_per_rev: 0.2, max_rpm: Some(3000.0) };
        assert!(ok.is_sane());
        assert!(!TurningData { vc: 0.0, ..ok }.is_sane());
        assert!(!TurningData { feed_per_rev: f64::NAN, ..ok }.is_sane());
        assert!(!TurningData { max_rpm: Some(0.0), ..ok }.is_sane());
        assert!(!TurningData { vc: MAX_VC + 1.0, ..ok }.is_sane());
    }
}

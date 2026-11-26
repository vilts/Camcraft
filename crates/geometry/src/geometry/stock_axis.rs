//! The stock's axis (#2576): which world axis the stock's own Z (a
//! cylinder's axis, a box's height) runs along, as a unit quaternion for
//! the stock's `Location.rot`, and the way back from a stored rotation to
//! the choice the Stock page shows. Pure values; the document keeps only
//! the quaternion.
//!
//! Provenance: our own design over standard rigid-transform mathematics
//! (Tait-Bryan angles as in `quat_from_euler`); no vendor material.

use super::{IDENTITY_Q, normalize_quat, quat_from_axis_angle, quat_from_euler, quat_to_euler};

/// Which way the stock's own Z axis runs in the setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StockAxis {
    /// Upright: no turn (the default).
    #[default]
    Z,
    /// Laid along X: a quarter turn about Y carries the stock's Z onto +X.
    X,
    /// Laid along Y: a quarter turn about X carries the stock's Z onto +Y.
    Y,
    /// Any turn, given as yaw, pitch and roll in degrees.
    Custom,
}

impl StockAxis {
    /// The choices in the order the Stock page shows them.
    pub const ALL: [StockAxis; 4] = [StockAxis::Z, StockAxis::X, StockAxis::Y, StockAxis::Custom];

    /// The page's word for the choice.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            StockAxis::Z => "Along Z",
            StockAxis::X => "Along X",
            StockAxis::Y => "Along Y",
            StockAxis::Custom => "Custom",
        }
    }

    /// The fixed rotation of a preset; `None` for [`StockAxis::Custom`].
    #[must_use]
    pub fn preset(self) -> Option<[f64; 4]> {
        let quarter = std::f64::consts::FRAC_PI_2;
        match self {
            StockAxis::Z => Some(IDENTITY_Q),
            StockAxis::X => Some(quat_from_axis_angle(1, quarter)),
            StockAxis::Y => Some(quat_from_axis_angle(0, -quarter)),
            StockAxis::Custom => None,
        }
    }
}

/// The stock rotation for `axis`; `angles_deg` (`[yaw, pitch, roll]`) are
/// read only for [`StockAxis::Custom`]. `None` for a non-finite angle -
/// the caller refuses, it never keeps a guessed turn.
#[must_use]
pub fn stock_axis_rot(axis: StockAxis, angles_deg: [f64; 3]) -> Option<[f64; 4]> {
    if let Some(q) = axis.preset() {
        return Some(q);
    }
    if !angles_deg.iter().all(|a| a.is_finite()) {
        return None;
    }
    let [yaw, pitch, roll] = angles_deg.map(f64::to_radians);
    normalize_quat(quat_from_euler(roll, pitch, yaw))
}

/// Two quaternions within this (as `1 - |dot|`) are the same turn.
const SAME_TURN: f64 = 1e-9;

/// The page's choice for a stored rotation: a preset when `rot` is one
/// (either sign of the quaternion), else [`StockAxis::Custom`] with its
/// `[yaw, pitch, roll]` in degrees. A preset reads back with zero angles.
#[must_use]
pub fn stock_axis_of(rot: [f64; 4]) -> (StockAxis, [f64; 3]) {
    let Some(q) = normalize_quat(rot) else {
        return (StockAxis::Z, [0.0; 3]);
    };
    for axis in [StockAxis::Z, StockAxis::X, StockAxis::Y] {
        if let Some(p) = axis.preset() {
            let dot = p.iter().zip(q.iter()).map(|(a, b)| a * b).sum::<f64>();
            if 1.0 - dot.abs() <= SAME_TURN {
                return (axis, [0.0; 3]);
            }
        }
    }
    let (roll, pitch, yaw) = quat_to_euler(q);
    let clean = |r: f64| {
        let d = (r.to_degrees() * 1e9).round() / 1e9;
        if d == 0.0 { 0.0 } else { d }
    };
    (StockAxis::Custom, [clean(yaw), clean(pitch), clean(roll)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point3, quat_rotate_point};

    fn close(a: Point3, b: Point3) -> bool {
        (a.x - b.x).abs() < 1e-12 && (a.y - b.y).abs() < 1e-12 && (a.z - b.z).abs() < 1e-12
    }

    #[test]
    fn each_preset_carries_the_stock_axis_onto_its_world_axis() {
        let z = Point3::new(0.0, 0.0, 1.0);
        for (axis, want) in [
            (StockAxis::Z, z),
            (StockAxis::X, Point3::new(1.0, 0.0, 0.0)),
            (StockAxis::Y, Point3::new(0.0, 1.0, 0.0)),
        ] {
            let q = stock_axis_rot(axis, [0.0; 3]).expect("a preset");
            assert!(close(quat_rotate_point(q, z), want), "{axis:?}");
            assert_eq!(stock_axis_of(q), (axis, [0.0; 3]), "{axis:?} reads back");
            let neg = q.map(|c| -c);
            assert_eq!(stock_axis_of(neg).0, axis, "{axis:?}: the other sign is the same turn");
        }
    }

    #[test]
    fn custom_angles_read_back_and_a_bad_angle_is_refused() {
        let q = stock_axis_rot(StockAxis::Custom, [30.0, 20.0, 10.0]).expect("finite");
        let (axis, a) = stock_axis_of(q);
        assert_eq!(axis, StockAxis::Custom);
        assert!((a[0] - 30.0).abs() < 1e-6 && (a[1] - 20.0).abs() < 1e-6 && (a[2] - 10.0).abs() < 1e-6, "{a:?}");
        // A pure yaw (the old page's only turn) reads back as Custom yaw.
        assert_eq!(stock_axis_of(crate::geometry::quat_from_yaw(30f64.to_radians())).0, StockAxis::Custom);
        assert_eq!(stock_axis_rot(StockAxis::Custom, [f64::NAN, 0.0, 0.0]), None);
        assert_eq!(stock_axis_of([0.0; 4]), (StockAxis::Z, [0.0; 3]), "garbage reads as upright, never a panic");
    }
}

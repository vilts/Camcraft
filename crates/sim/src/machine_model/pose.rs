//! The pose of each part frame of the generated machine model (#2584)
//! for a set of axis values: forward kinematics along the serial chain.
//!
//! Provenance: the product of rigid transforms along a kinematic chain
//! (J. J. Craig, "Introduction to Robotics", ch. 2-3) and the rotation
//! about an axis by Rodrigues' formula (any kinematics text); written
//! from the published formulas.

use super::check::{Joint, Link};
use super::vec::{add, dot, scale, sub, V3};
use super::MAX_DIM;

/// The largest angle a pose may state (degrees).
const MAX_ANGLE: f64 = 1.0e6;

/// A rigid transform: `p -> r * p + t`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Frame {
    r: [V3; 3],
    t: V3,
}

impl Frame {
    const IDENTITY: Frame = Frame { r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], t: [0.0; 3] };

    fn shift(t: V3) -> Frame {
        Frame { t, ..Frame::IDENTITY }
    }

    /// A turn of `degrees` about the unit `axis` through `pivot`,
    /// right-handed.
    fn turn(axis: V3, degrees: f64, pivot: V3) -> Frame {
        let (s, c) = degrees.to_radians().sin_cos();
        let [x, y, z] = axis;
        let v = 1.0 - c;
        let r = [
            [c + x * x * v, x * y * v - z * s, x * z * v + y * s],
            [y * x * v + z * s, c + y * y * v, y * z * v - x * s],
            [z * x * v - y * s, z * y * v + x * s, c + z * z * v],
        ];
        let turned = [dot(r[0], pivot), dot(r[1], pivot), dot(r[2], pivot)];
        Frame { r, t: sub(pivot, turned) }
    }

    pub(super) fn apply(&self, p: V3) -> V3 {
        add([dot(self.r[0], p), dot(self.r[1], p), dot(self.r[2], p)], self.t)
    }

    /// `self` after `inner`: `p -> self(inner(p))`.
    fn after(&self, inner: &Frame) -> Frame {
        let col = |k: usize| [inner.r[0][k], inner.r[1][k], inner.r[2][k]];
        let row = |i: usize| [dot(self.r[i], col(0)), dot(self.r[i], col(1)), dot(self.r[i], col(2))];
        Frame { r: [row(0), row(1), row(2)], t: self.apply(inner.t) }
    }
}

/// The value of each joint, by part index. Each linear and rotary part
/// must be stated one time; a fixed part takes no value.
fn joint_values(links: &[Link], values: &[(&str, f64)]) -> Result<Vec<Option<f64>>, String> {
    let mut out = vec![None; links.len()];
    for &(id, value) in values {
        let Some(i) = links.iter().position(|l| l.id == id) else {
            return Err(format!("machine model: pose: no part {id} is in the model"));
        };
        let limit = match links[i].joint {
            Joint::Fixed => {
                return Err(format!("machine model: pose: part {id} is fixed and takes no axis value"));
            }
            Joint::Linear { .. } => MAX_DIM,
            Joint::Rotary { .. } => MAX_ANGLE,
        };
        if !value.is_finite() {
            return Err(format!("machine model: pose: part {id}: the axis value must be finite (got {value})"));
        }
        if value.abs() > limit {
            return Err(format!("machine model: pose: part {id}: the axis value {value} is over {limit}"));
        }
        if out[i].replace(value).is_some() {
            return Err(format!("machine model: pose: part {id} has two values"));
        }
    }
    Ok(out)
}

/// The frame of each part in the machine frame. A part frame is its
/// parent's frame, then the part's `offset`, then its own joint.
pub(super) fn frames(links: &[Link], values: &[(&str, f64)]) -> Result<Vec<Frame>, String> {
    let stated = joint_values(links, values)?;
    let mut out: Vec<Frame> = Vec::with_capacity(links.len());
    for (link, value) in links.iter().zip(stated) {
        let missing = || format!("machine model: pose: part {}: no axis value is stated", link.id);
        let joint = match link.joint {
            Joint::Fixed => Frame::IDENTITY,
            Joint::Linear { axis } => Frame::shift(scale(axis, value.ok_or_else(missing)?)),
            Joint::Rotary { axis, pivot } => Frame::turn(axis, value.ok_or_else(missing)?, pivot),
        };
        // `check::links` proved that a parent is ahead of its part.
        let parent = link.parent.and_then(|p| out.get(p)).copied().unwrap_or(Frame::IDENTITY);
        out.push(parent.after(&Frame::shift(link.offset)).after(&joint));
    }
    Ok(out)
}

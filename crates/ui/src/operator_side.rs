//! The operator side of the work envelope (#2632): an arrow at the edge
//! of the envelope that points out of it toward the operator, and the
//! text "Operator side" at its tip. Drawn only with the envelope, and only
//! while the View door's "Show operator side" box is on. Display only -
//! view state, never a command, never saved; nothing that checks or posts
//! reads it.
//!
//! Frame cost (crates/ui/AGENTS.md): the arrow is built when its input
//! changes - the placed envelope and the side - and read from the kept
//! answer on each other frame. The text is screen space: it follows the
//! camera without a rebuild and always faces the viewer.
//! Provenance: our own view-state design and our own arrow (issue #2632);
//! no vendor material.

use camcraft_app::geometry::Point3;
use camcraft_app::machine::envelope::EnvelopeShape;
use camcraft_app::machine::{self, OperatorSide};

use crate::interact::{project_uv, push_line};
use crate::state::State;
use crate::work_envelope::ENVELOPE_RGB;

/// The text at the arrow: plain English, the same in each view.
pub(crate) const LABEL: &str = "Operator side";
/// The label of the switch in the View door.
pub(crate) const SWITCH: &str = "Show operator side";
/// The tooltip of the switch.
pub(crate) const TIP: &str = "An arrow at the work envelope points to the operator. The side comes from the machine file (general.operator_side), or from the machine type.";
/// The tooltip of the switch while the envelope is not shown.
pub(crate) const TIP_OFF: &str = "Show the work envelope to show the operator side.";

/// Arrow length as a part of the envelope's larger horizontal extent.
const LENGTH_PART: f64 = 0.2;
/// The shortest and the longest arrow, mm (a degenerate or huge envelope).
const LENGTH_RANGE: (f64, f64) = (1.0, 1.0e6);

/// The input of one arrow: the envelope where the viewport draws it, and
/// the side.
type Input = (EnvelopeShape, OperatorSide);

/// One built arrow.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Marker {
    /// The overlay segments: shaft and head.
    pub lines: Vec<[f32; 6]>,
    /// The world point of the text, just past the tip.
    pub label_at: [f64; 3],
}

/// The kept arrow of a [`State`] with the input that built it.
#[derive(Debug, Default)]
pub struct Cache {
    input: Option<Input>,
    marker: Option<Marker>,
    /// Arrows built so far (the counter tests read it).
    builds: u64,
}

impl Cache {
    fn update(&mut self, input: Option<Input>) -> Option<&Marker> {
        if self.input != input {
            self.marker = input.and_then(build);
            self.builds = self.builds.saturating_add(u64::from(self.marker.is_some()));
            self.input = input;
        }
        self.marker.as_ref()
    }
}

/// The arrow segments for the envelope `shape` that the scene draws
/// (`None`: no envelope is drawn). Empty when the switch is off.
#[must_use]
pub(crate) fn lines(state: &State, shape: Option<EnvelopeShape>) -> Vec<[f32; 6]> {
    let input = shape.filter(|_| state.show_operator_side).and_then(|shape| {
        let (_, mounted) = machine::of(&state.world);
        Some((shape, OperatorSide::of(&mounted?)))
    });
    // A poisoned lock only means a draw panicked mid-update: the cache is
    // display data, built again from its input.
    let mut cache = state.operator_side_cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.update(input).map(|m| m.lines.clone()).unwrap_or_default()
}

/// The text of the arrow that the scene drew last, in viewport uv; `None`
/// when no arrow is drawn or its tip is behind the camera.
#[must_use]
pub(crate) fn label(state: &State, mvp: &[[f32; 4]; 4]) -> Option<([f32; 2], [f32; 3], String)> {
    if !(state.show_work_envelope && state.show_operator_side) {
        return None;
    }
    let cache = state.operator_side_cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let uv = project_uv(mvp, cache.marker.as_ref()?.label_at)?;
    Some((uv, ENVELOPE_RGB, LABEL.to_owned()))
}

/// Arrows built for `state` so far.
#[cfg(test)]
pub(crate) fn builds(state: &State) -> u64 {
    state.operator_side_cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner).builds
}

/// The arrow of one input: it starts at the middle of the lower edge of
/// the operator-side face (the lower quadrant for a cylinder) and points
/// out of the envelope. `None` for an envelope whose figures are not finite.
fn build((shape, side): Input) -> Option<Marker> {
    let (axis, sign) = side.outward();
    // The bounds, and the axis whose low end is the bottom of the picture:
    // a mill box stands on the floor (-Z); the lathe view draws -X down.
    let (min, max, down) = match shape {
        EnvelopeShape::Box { min, max } => (min, max, 2),
        EnvelopeShape::Cylinder { cx, cy, radius, z0, z1 } => {
            ([cx - radius, cy - radius, z0.min(z1)], [cx + radius, cy + radius, z0.max(z1)], usize::from(axis == 0))
        },
    };
    if min.iter().chain(max.iter()).any(|v| !v.is_finite()) {
        return None;
    }
    let along = 3 - axis - down;
    let mut anchor = [0.0; 3];
    anchor[axis] = if sign < 0.0 { min[axis] } else { max[axis] };
    anchor[down] = min[down];
    anchor[along] = 0.5 * (min[along] + max[along]);
    if let EnvelopeShape::Cylinder { cx, cy, radius, .. } = shape {
        // The lower-side quadrant lies on the cylinder, unlike its box corner.
        let center = [cx, cy];
        let offset = radius * std::f64::consts::FRAC_1_SQRT_2;
        anchor[axis] = center[axis] + sign * offset;
        anchor[down] = center[down] - offset;
    }
    let extent = (max[axis] - min[axis]).max(max[along] - min[along]);
    let len = (extent * LENGTH_PART).clamp(LENGTH_RANGE.0, LENGTH_RANGE.1);
    let at = |out: f64, across: f64| {
        let mut p = anchor;
        p[axis] += sign * out * len;
        p[along] += across * len;
        p
    };
    let point = |p: [f64; 3]| Point3::new(p[0], p[1], p[2]);
    let (tip, left, right) = (at(1.0, 0.0), at(0.65, 0.2), at(0.65, -0.2));
    let mut lines = Vec::with_capacity(8);
    for (a, b) in [(anchor, tip), (tip, left), (tip, right), (left, right)] {
        push_line(&mut lines, point(a), point(b), ENVELOPE_RGB);
    }
    Some(Marker { lines, label_at: at(1.15, 0.0) })
}

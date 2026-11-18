//! The work envelope in the viewport (#1567): a thin outline of the
//! mounted machine's travel volume, drawn only while the Browser's "Show
//! work envelope" box is on. Display only - view state, not undoable, never
//! saved, and it never reaches posting, simulation or the collision and
//! travel checks. X/Y use the setup work origin and the stated travels.
//! A mill box stands on the viewport floor (#2172, `crate::floor`: the
//! lowest point of the model, the stock and the fixtures) and keeps its
//! full Z travel;
//! the program-space envelope and all safety checks stay unchanged.
//! Provenance: our own view-state design (issue #1567); no vendor material.

use camcraft_app::geometry::Point3;
use camcraft_app::machine::envelope::EnvelopeShape;
use camcraft_app::machine;

use crate::interact::push_line;
use crate::state::State;

/// Outline colour: a faint steel blue that stays behind the toolpaths.
const ENVELOPE_RGB: [f32; 3] = [0.35, 0.55, 0.70];
/// Segments of a lathe envelope's end circles.
const RING_SEGMENTS: usize = 48;

/// Why the box cannot show an envelope right now, `None` when it can.
#[must_use]
pub(crate) fn envelope_problem(state: &State) -> Option<String> {
    machine::work_envelope(&state.world).err()
}

/// Is a machine mounted on the active setup (the box is enabled)?
#[must_use]
pub(crate) fn machine_mounted(state: &State) -> bool {
    machine::of(&state.world).1.is_some()
}

/// The tooltip of the checkbox.
#[must_use]
pub(crate) fn tooltip(state: &State) -> String {
    let what = "Outline of the mounted machine travel. Mill boxes stand on the floor under the model, the stock and the fixtures (the grid); X/Y and lathe cylinders use the setup work origin. This display does not change travel checks.";
    match envelope_problem(state) {
        None => what.to_owned(),
        Some(why) => format!("Nothing to draw: {why}. {what}"),
    }
}

/// The overlay segments of the envelope; empty when the box is off or the
/// machine states no envelope.
#[must_use]
pub(crate) fn envelope_lines(state: &State) -> Vec<[f32; 6]> {
    if !state.show_work_envelope {
        return Vec::new();
    }
    let Ok(shape) = machine::work_envelope(&state.world) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    match shape {
        EnvelopeShape::Box { mut min, mut max } => {
            // #1963: the box stands on the table, not on program Z (travel
            // can straddle the origin). #2172: the table is the floor.
            let floor = crate::floor::floor_z(state);
            max[2] = floor + (max[2] - min[2]);
            min[2] = floor;
            box_lines(&mut out, min, max);
        },
        EnvelopeShape::Cylinder { cx, cy, radius, z0, z1 } => cylinder_lines(&mut out, cx, cy, radius, z0, z1),
    }
    out
}

fn box_lines(out: &mut Vec<[f32; 6]>, min: [f64; 3], max: [f64; 3]) {
    let c = |i: usize| Point3::new(if i & 1 == 0 { min[0] } else { max[0] }, if i & 2 == 0 { min[1] } else { max[1] }, if i & 4 == 0 { min[2] } else { max[2] });
    for i in 0..8usize {
        for bit in [1usize, 2, 4] {
            if i & bit == 0 {
                push_line(out, c(i), c(i | bit), ENVELOPE_RGB);
            }
        }
    }
}

fn cylinder_lines(out: &mut Vec<[f32; 6]>, cx: f64, cy: f64, r: f64, z0: f64, z1: f64) {
    let at = |k: usize, z: f64| {
        let a = std::f64::consts::TAU * (k % RING_SEGMENTS) as f64 / RING_SEGMENTS as f64;
        Point3::new(cx + r * a.cos(), cy + r * a.sin(), z)
    };
    for k in 0..RING_SEGMENTS {
        push_line(out, at(k, z0), at(k + 1, z0), ENVELOPE_RGB);
        push_line(out, at(k, z1), at(k + 1, z1), ENVELOPE_RGB);
    }
    for q in 0..4 {
        let k = q * RING_SEGMENTS / 4;
        push_line(out, at(k, z0), at(k, z1), ENVELOPE_RGB);
    }
}

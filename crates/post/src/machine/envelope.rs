//! The work envelope of a machine (#1567): the volume its stated linear
//! travels allow the tool tip, placed in world coordinates from the work
//! origin. Display data only - it reads the very `travel` figures and axis
//! directions the program's stroke check reads, so the picture cannot
//! disagree with a refusal, and it checks nothing itself.
//! Provenance: general machine-tool practice (a mill's stroke is a box of
//! X/Y/Z travel, a lathe's is a cylinder on the spindle axis whose radius is
//! the X slide's reach); the unstated-limit law of `machine/mod.rs` - a
//! machine without the figures has no envelope, never a guessed one.

use super::{kind, Kind, Machine};

/// The shape of an envelope, in world coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EnvelopeShape {
    /// An axis-aligned box, `min` to `max`.
    Box { min: [f64; 3], max: [f64; 3] },
    /// A cylinder on the world Z axis through `(cx, cy)`, from `z0` to `z1`.
    Cylinder { cx: f64, cy: f64, radius: f64, z0: f64, z1: f64 },
}

/// The envelope of `machine` with its program origin at `origin` (world
/// mm). `Err` names the missing figure; nothing is guessed. Mirrors the
/// stroke check: a program value `v` on axis `k` sits at world
/// `origin[k] + v * sign` (`sign` is -1 on an inverted axis).
pub fn envelope(machine: &Machine, origin: [f64; 3]) -> Result<EnvelopeShape, String> {
    let lathe = kind(Some(machine)) == Kind::Lathe;
    let mut lo = [0.0; 3];
    let mut hi = [0.0; 3];
    for (k, name) in ["X", "Y", "Z"].iter().enumerate() {
        // A lathe has no Y slide: its envelope is turned about the axis.
        if lathe && k == 1 {
            continue;
        }
        let Some(ax) = machine.axis(k) else {
            return Err(format!("the machine states no {name} axis"));
        };
        let Some((a, b)) = ax.travel else {
            return Err(format!("the machine states no {name} travel"));
        };
        if !a.is_finite() || !b.is_finite() || a > b {
            return Err(format!("the {name} travel {a}..{b} is not a range"));
        }
        let (p, q) = (origin[k] + a * ax.direction.sign(), origin[k] + b * ax.direction.sign());
        lo[k] = p.min(q);
        hi[k] = p.max(q);
    }
    if !lathe {
        return Ok(EnvelopeShape::Box { min: lo, max: hi });
    }
    // X on a lathe is the radius from the spindle centreline; the stroke
    // check reads it the same way, so the reach is the larger end.
    let Some((a, b)) = machine.axis(0).and_then(|x| x.travel) else {
        return Err("the machine states no X travel".into());
    };
    let radius = a.abs().max(b.abs());
    if radius <= 0.0 {
        return Err("the X travel has no reach".into());
    }
    Ok(EnvelopeShape::Cylinder { cx: origin[0], cy: origin[1], radius, z0: lo[2], z1: hi[2] })
}

#[cfg(test)]
mod tests {
    use super::super::{AxisDirection, AxisLimits};
    use super::*;

    fn ax(name: &str, travel: Option<(f64, f64)>) -> AxisLimits {
        AxisLimits { name: name.into(), travel, ..AxisLimits::default() }
    }

    fn mill() -> Machine {
        Machine {
            axes: vec![ax("X", Some((-100.0, 200.0))), ax("Y", Some((-50.0, 50.0))), ax("Z", Some((-30.0, 0.0)))],
            ..Machine::default()
        }
    }

    #[test]
    fn a_mill_is_the_travel_box_about_the_origin() {
        let e = envelope(&mill(), [10.0, 20.0, 5.0]).expect("envelope");
        assert_eq!(e, EnvelopeShape::Box { min: [-90.0, -30.0, -25.0], max: [210.0, 70.0, 5.0] });
    }

    #[test]
    fn an_inverted_axis_flips_about_the_origin() {
        let mut m = mill();
        m.axes[2].direction = AxisDirection::Inverted;
        let Ok(EnvelopeShape::Box { min, max }) = envelope(&m, [0.0, 0.0, 0.0]) else { panic!("box") };
        assert_eq!((min[2], max[2]), (0.0, 30.0));
    }

    #[test]
    fn an_inverted_axis_with_two_nonzero_ends_flips_both_bounds() {
        // X travel -100..200 about origin 10: sign -1 gives 10+100 .. 10-200.
        let mut m = mill();
        m.axes[0].direction = AxisDirection::Inverted;
        let Ok(EnvelopeShape::Box { min, max }) = envelope(&m, [10.0, 20.0, 5.0]) else { panic!("box") };
        assert_eq!((min[0], max[0]), (-190.0, 110.0));
        // The other axes keep their upright bounds.
        assert_eq!((min[1], max[1]), (-30.0, 70.0));
        assert_eq!((min[2], max[2]), (-25.0, 5.0));
    }

    #[test]
    fn a_missing_travel_names_the_axis() {
        let mut m = mill();
        m.axes[1].travel = None;
        assert_eq!(envelope(&m, [0.0; 3]), Err("the machine states no Y travel".into()));
        assert!(envelope(&Machine::default(), [0.0; 3]).is_err());
    }

    #[test]
    fn a_lathe_is_a_cylinder_on_the_spindle_axis() {
        let m = Machine {
            kind: Some("lathe".into()),
            axes: vec![ax("X", Some((-60.0, 40.0))), ax("Z", Some((-200.0, 0.0)))],
            ..Machine::default()
        };
        let e = envelope(&m, [1.0, 2.0, 3.0]).expect("envelope");
        assert_eq!(e, EnvelopeShape::Cylinder { cx: 1.0, cy: 2.0, radius: 60.0, z0: -197.0, z1: 3.0 });
    }
}

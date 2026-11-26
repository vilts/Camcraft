//! #3084: the first traverse over work that the machine's own work-turning rotaries can leave at any pose (two rotaries that turn the work, or a multi-axis kind on a machine whose table tilts) - our own reading of the shop rule "never traverse through uncut stock or steel" (machine-tool handbooks) and plain rotation geometry, no vendor material consulted.
//!
//! After a tool change the post sends Z, then X/Y at that height, and
//! the rotary words come only in the move after it. So the X/Y traverse
//! runs with each work-turning rotary at the angle the previous program
//! left, which this program does not know. The law reads the work, and
//! the steel that rides it, through every such pose:
//! - one work-turning rotary: a full turn about its line ([`PosedTurn::
//!   Line`]); a point runs a circle about its foot on the line;
//! - two or more: each turn moves the other's line, so the only figure
//!   that every pose keeps is the distance from the point where the
//!   lines meet ([`PosedTurn::About`]); a point stays on that sphere.
//!
//! A full turn and a full sphere contain every stated turning range,
//! so the read is never lower than the true top. Heights are read along
//! the frame's unit height `up` from `base` (`up = +Z`, `base = 0` for
//! an upright path).

use super::first_traverse::box_top;
use super::types::Warning;
use crate::fixture::FixtureBox;
use crate::geometry::Point3;

/// Where every pose of the work-turning rotaries keeps a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PosedTurn {
    /// One rotary turns the work about the line through `origin` along
    /// `dir` (not unit: normalized here).
    Line { origin: Point3, dir: [f64; 3] },
    /// Two or more turn it; every line passes through `origin`.
    About { origin: Point3 },
}

/// The work that turns, read in one frame: the start stock's and the
/// part's points, and the tagged steel that rides the rotaries.
#[derive(Clone, Copy, Debug)]
pub struct PosedWork<'a> {
    pub turn: PosedTurn,
    pub rides: &'a [Point3],
    pub steel: &'a [FixtureBox],
}

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (n.is_finite() && n > 1e-12).then(|| [v[0] / n, v[1] / n, v[2] / n])
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: Point3, b: Point3) -> [f64; 3] {
    [a.x - b.x, a.y - b.y, a.z - b.z]
}

/// The highest height along unit `up` (from `base`) that `q` reaches in
/// any pose of `turn`. A line circle's top stands `r * sqrt(1 - (up .
/// d)^2)` over its foot; a sphere's top stands its radius over its
/// centre. A line with no direction reads as the sphere (the wider set).
#[must_use]
pub fn posed_top(q: Point3, turn: &PosedTurn, up: [f64; 3], base: Point3) -> f64 {
    let up = unit(up).unwrap_or([0.0, 0.0, 1.0]);
    match *turn {
        PosedTurn::Line { origin, dir } if unit(dir).is_some() => {
            let d = unit(dir).unwrap_or([0.0, 0.0, 1.0]);
            let w = sub(q, origin);
            let along = dot(w, d);
            let perp = [w[0] - along * d[0], w[1] - along * d[1], w[2] - along * d[2]];
            let r = dot(perp, perp).sqrt();
            let c = dot(up, d);
            dot(sub(origin, base), up) + along * c + r * (1.0 - c * c).max(0.0).sqrt()
        }
        PosedTurn::Line { origin, .. } | PosedTurn::About { origin } => {
            let w = sub(q, origin);
            dot(sub(origin, base), up) + dot(w, w).sqrt()
        }
    }
}

/// The posed first-traverse law (#3084): the traverse runs at `height`
/// (along `up` from `base`: the height the post sends before X/Y) over
/// `work` read through every pose ([`posed_top`] of each point, and of
/// each box corner: the distance to a point or a line is convex, so a
/// box's top over all poses is at a corner). `None` when `height` stands
/// at or over both tops within `z_tol`, or is not finite. A hit names
/// the higher top, as [`Warning::FirstTraverseLow`] does.
#[must_use]
pub fn first_traverse_low_posed(height: f64, work: &PosedWork<'_>, up: [f64; 3], base: Point3, z_tol: f64) -> Option<Warning> {
    if !height.is_finite() {
        return None;
    }
    let top = |q: Point3| posed_top(q, &work.turn, up, base);
    let finite_max = |it: &mut dyn Iterator<Item = f64>| it.filter(|z| z.is_finite()).fold(f64::NEG_INFINITY, f64::max);
    let stock = finite_max(&mut work.rides.iter().map(|q| top(*q)));
    let steel = finite_max(&mut work.steel.iter().map(|b| box_top(b, top)));
    let (top, fixture) = if steel > stock { (steel, true) } else { (stock, false) };
    (top.is_finite() && height < top - z_tol).then_some(Warning::FirstTraverseLow { at: 0, height_mm: height, top_mm: top, fixture, tilted: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    const UP: [f64; 3] = [0.0, 0.0, 1.0];
    const O: Point3 = Point3 { x: 0.0, y: 0.0, z: 0.0 };

    /// A 100 x 100 x 50 block on a trunnion table, the lines through the
    /// WCS origin at the table face's centre.
    fn block() -> Vec<Point3> {
        (0..8).map(|i| Point3::new([-50.0, 50.0][(i >> 2) & 1], [-50.0, 50.0][(i >> 1) & 1], [0.0, 50.0][i & 1])).collect()
    }

    #[test]
    fn two_work_turning_rotaries_read_the_block_on_its_sphere() {
        let about = PosedWork { turn: PosedTurn::About { origin: O }, rides: &block(), steel: &[] };
        let r = (3.0f64 * 2500.0).sqrt();
        // Upright the block's top is Z50: Z60 clears it. Tilted 90 deg
        // with C turned, a top corner stands at its radius 86.603.
        let Some(Warning::FirstTraverseLow { height_mm, top_mm, fixture: false, .. }) = first_traverse_low_posed(60.0, &about, UP, O, 1e-6) else {
            panic!("a start under the sphere top must be named");
        };
        assert!((height_mm - 60.0).abs() < 1e-12 && (top_mm - r).abs() < 1e-9, "{top_mm}");
        assert_eq!(first_traverse_low_posed(r + 0.01, &about, UP, O, 1e-6), None);
        assert_eq!(first_traverse_low_posed(f64::NAN, &about, UP, O, 1e-6), None);
    }

    #[test]
    fn one_tilting_table_reads_a_full_turn_about_its_line() {
        let a = PosedTurn::Line { origin: O, dir: [3.0, 0.0, 0.0] };
        // About X a corner (x, 50, 50) turns on radius 70.711.
        let r = 50.0 * 2f64.sqrt();
        assert!((posed_top(Point3::new(50.0, 50.0, 50.0), &a, UP, O) - r).abs() < 1e-12);
        let work = PosedWork { turn: a, rides: &block(), steel: &[] };
        assert!(first_traverse_low_posed(70.0, &work, UP, O, 1e-6).is_some());
        assert_eq!(first_traverse_low_posed(71.0, &work, UP, O, 1e-6), None);
        // A vertical line keeps every height; a line with no direction
        // reads as the sphere.
        let c = PosedTurn::Line { origin: O, dir: [0.0, 0.0, 1.0] };
        assert_eq!(posed_top(Point3::new(50.0, 50.0, 50.0), &c, UP, O), 50.0);
        let none = PosedTurn::Line { origin: O, dir: [0.0, 0.0, 0.0] };
        assert!((posed_top(Point3::new(0.0, 30.0, 40.0), &none, UP, O) - 50.0).abs() < 1e-12);
    }

    #[test]
    fn steel_that_rides_the_table_is_read_at_its_farthest_corner() {
        let vise = FixtureBox { min: Point3::new(-80.0, -20.0, 0.0), max: Point3::new(-60.0, 20.0, 30.0) };
        let work = PosedWork { turn: PosedTurn::About { origin: O }, rides: &[], steel: &[vise] };
        let far = (80.0f64 * 80.0 + 20.0 * 20.0 + 30.0 * 30.0).sqrt();
        let Some(Warning::FirstTraverseLow { top_mm, fixture: true, .. }) = first_traverse_low_posed(50.0, &work, UP, O, 1e-6) else {
            panic!("the vise sweeps over Z50");
        };
        assert!((top_mm - far).abs() < 1e-9, "{top_mm}");
    }

    #[test]
    fn a_tilted_frame_reads_its_heights_along_its_own_axis() {
        // Heights along +X from x = 10: the sphere's top there is the
        // centre's offset (-10) plus the radius.
        let work = PosedWork { turn: PosedTurn::About { origin: O }, rides: &[Point3::new(0.0, 0.0, 40.0)], steel: &[] };
        let base = Point3::new(10.0, 0.0, 0.0);
        let Some(Warning::FirstTraverseLow { top_mm, .. }) = first_traverse_low_posed(20.0, &work, [2.0, 0.0, 0.0], base, 1e-6) else {
            panic!("the sphere top along +X is 30");
        };
        assert!((top_mm - 30.0).abs() < 1e-12, "{top_mm}");
        assert_eq!(first_traverse_low_posed(30.0, &work, [1.0, 0.0, 0.0], base, 1e-6), None);
    }
}

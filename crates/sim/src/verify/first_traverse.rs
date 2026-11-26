//! #2901: the first traverse of a milling operation, from an X/Y the machine does not know after a tool change, must ride above the whole start stock and every fixture - our own reading of the shop rule "never traverse through uncut stock or steel" (machine-tool handbooks), no vendor material consulted.
//!
//! After a tool change the control does not know where the tool stands
//! in X/Y. The post sends Z to the plane the path starts on, then X/Y
//! at that plane: that one traverse can cross any point of the setup.
//! No line of it is known, so no swept law can judge it. The law here
//! judges the plane instead: the path's start height must stand at or
//! over the top of the start stock (the full block, never the
//! in-process stock, because the line is not known) and the top of each
//! fixture, all read in the path's own frame (a 3+2 plane reads heights
//! along its tool axis). A start under either top is one blocking
//! verdict at move 0 that names both heights.

use super::rotary::RotaryAxis;
use super::types::Warning;
use crate::cl::Toolpath;
use crate::fixture::FixtureBox;
use crate::geometry::Point3;

/// The highest point of `b` along the frame's height `z_of` (the
/// largest of its eight corners: an affine map takes its max there).
#[must_use]
pub fn box_top(b: &FixtureBox, z_of: impl Fn(Point3) -> f64) -> f64 {
    (0..8)
        .map(|i| {
            z_of(Point3::new(
                [b.min.x, b.max.x][(i >> 2) & 1],
                [b.min.y, b.max.y][(i >> 1) & 1],
                [b.min.z, b.max.z][i & 1],
            ))
        })
        .fold(f64::NEG_INFINITY, f64::max)
}

/// The first-traverse law: `stock_top` is the top of the start stock in
/// the path's frame (`-inf` when there is none), `steel` the fixtures'
/// boxes in the world, `z_of` the world-to-frame height (`|p| p.z` for
/// an upright path). `None` when the path's start stands at or over
/// both tops within `z_tol`, or when the path is empty or its start is
/// not finite (the non-finite law names that).
#[must_use]
pub fn first_traverse_low(
    path: &Toolpath,
    stock_top: f64,
    steel: &[FixtureBox],
    z_of: impl Fn(Point3) -> f64,
    z_tol: f64,
) -> Option<Warning> {
    let start = path.start()?;
    if !start.z.is_finite() {
        return None;
    }
    let fixture_top = steel.iter().map(|b| box_top(b, &z_of)).filter(|z| z.is_finite()).fold(f64::NEG_INFINITY, f64::max);
    verdict(start.z, stock_top, fixture_top, z_tol)
}

/// One verdict over the two tops: the higher one names the cause.
fn verdict(height: f64, stock_top: f64, fixture_top: f64, z_tol: f64) -> Option<Warning> {
    let stock_top = if stock_top.is_finite() { stock_top } else { f64::NEG_INFINITY };
    let fixture_top = if fixture_top.is_finite() { fixture_top } else { f64::NEG_INFINITY };
    let (top, fixture) = if fixture_top > stock_top { (fixture_top, true) } else { (stock_top, false) };
    (height.is_finite() && top.is_finite() && height < top - z_tol)
        .then_some(Warning::FirstTraverseLow { at: 0, height_mm: height, top_mm: top, fixture, tilted: false })
}

/// The unit of `v`, or `None` when it has no finite direction.
fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (n.is_finite() && n > 1e-12).then(|| [v[0] / n, v[1] / n, v[2] / n])
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// #2965: the highest world Z that `q` reaches while the work turns a
/// full turn about `ax`. The point runs a circle of radius `r` about
/// its foot on the axis; the circle's top stands `r * sqrt(1 - dz^2)`
/// over that foot (`dz` the axis's Z component). A horizontal axis
/// gives the foot's Z plus the radius; a vertical one gives `q.z`. An
/// axis with no direction reads as vertical (no turn lifts a point).
#[must_use]
pub fn swept_top(q: Point3, ax: &RotaryAxis) -> f64 {
    let d = unit(ax.dir).unwrap_or([0.0, 0.0, 1.0]);
    let w = [q.x - ax.origin.x, q.y - ax.origin.y, q.z - ax.origin.z];
    let along = dot(w, d);
    let perp = [w[0] - along * d[0], w[1] - along * d[1], w[2] - along * d[2]];
    let r = dot(perp, perp).sqrt();
    ax.origin.z + along * d[2] + r * (1.0 - d[2] * d[2]).max(0.0).sqrt()
}

/// The steel of a rotary setup, split as [`super::Workholding`] splits
/// it: `turning` rides the rotary with the part (a chuck), `fixed`
/// stands still in the machine frame (the table, a tailstock).
#[derive(Clone, Copy, Debug)]
pub struct SweptSteel<'a> {
    pub turning: &'a [FixtureBox],
    pub fixed: &'a [FixtureBox],
}

/// #2965: the first-traverse law of a path whose work turns about `ax`
/// (the rotary kinds). After a tool change the post sends Z, then X/Y
/// at that height, and the rotary words come only in the move after
/// it: the traverse runs with the work at an angle the program does
/// not state. So the work and the steel that rides it are read as
/// swept through a full turn ([`swept_top`] of each point and corner:
/// the distance to a line is convex, so a box's swept top is at a
/// corner), and the fixed steel as it stands in the machine frame.
///
/// `start` and `tool_axis` are the path's first point and its tool
/// axis in the part frame (`[0, 0, 1]` for an upright move). The
/// machine stands the tool upright by a turn about `ax`, which keeps
/// the axis line and maps `tool_axis` onto +Z: the start's machine
/// height is `origin.z + tool_axis . (start - origin)`. A radial tool
/// on a horizontal axis reads the start's radius over the axis line.
/// A tool axis that no turn about `ax` stands upright reads the lowest
/// height the turn can carry the start to.
/// The post rapids Z to the higher of `clearance` (the operation's
/// clearance height, a machine Z; `-inf` when none) and that start
/// height before the X/Y traverse, so the law reads the higher one.
/// `rides` are the start stock's and the part's points in the part
/// frame. `None` when the start stands at or over both tops within
/// `z_tol`, or when the start is not finite.
#[must_use]
pub fn first_traverse_low_swept(
    start: Point3,
    tool_axis: [f64; 3],
    clearance: f64,
    ax: &RotaryAxis,
    rides: &[Point3],
    steel: SweptSteel<'_>,
    z_tol: f64,
) -> Option<Warning> {
    if !(start.x.is_finite() && start.y.is_finite() && start.z.is_finite()) {
        return None;
    }
    let t = unit(tool_axis).unwrap_or([0.0, 0.0, 1.0]);
    let d = unit(ax.dir).unwrap_or([0.0, 0.0, 1.0]);
    let o = ax.origin;
    let w = [start.x - o.x, start.y - o.y, start.z - o.z];
    // A turn about the axis maps `t` onto +Z only when both make the
    // same angle with it. Else the tool stands at a station the turn
    // cannot reach (a live tool beside a C axis): the start is read at
    // the lowest height that the turn can carry it to.
    let height = if (dot(t, d) - d[2]).abs() <= 1e-6 {
        o.z + dot(t, w)
    } else {
        let along = dot(w, d);
        let perp = [w[0] - along * d[0], w[1] - along * d[1], w[2] - along * d[2]];
        o.z + along * d[2] - dot(perp, perp).sqrt() * (1.0 - d[2] * d[2]).max(0.0).sqrt()
    };
    let finite_max = |it: &mut dyn Iterator<Item = f64>| it.filter(|z| z.is_finite()).fold(f64::NEG_INFINITY, f64::max);
    let stock_top = finite_max(&mut rides.iter().map(|q| swept_top(*q, ax)));
    let turning = finite_max(&mut steel.turning.iter().map(|b| box_top(b, |p| swept_top(p, ax))));
    let fixed = finite_max(&mut steel.fixed.iter().map(|b| box_top(b, |p| p.z)));
    // The post rapids Z to the higher of the clearance height and the
    // start, and traverses X/Y there.
    let height = if clearance.is_finite() { height.max(clearance) } else { height };
    verdict(height, stock_top, turning.max(fixed), z_tol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::{MoveKind, PathBuilder};

    fn opening(z: f64) -> Toolpath {
        let mut b = PathBuilder::new(Point3::new(5.0, 5.0, z));
        b.push(MoveKind::Rapid, Point3::new(5.0, 5.0, 1.0), None);
        b.push(MoveKind::Plunge, Point3::new(5.0, 5.0, -1.0), Some(100.0));
        b.finish()
    }

    fn clamp(top: f64) -> FixtureBox {
        FixtureBox { min: Point3::new(200.0, 200.0, 0.0), max: Point3::new(210.0, 210.0, top) }
    }

    #[test]
    fn a_start_under_the_stock_top_is_named_with_both_heights() {
        let w = first_traverse_low(&opening(8.0), 10.0, &[], |p| p.z, 1e-6);
        assert_eq!(w, Some(Warning::FirstTraverseLow { at: 0, height_mm: 8.0, top_mm: 10.0, fixture: false, tilted: false }));
        assert!(w.is_some_and(|w| w.blocks_post()));
    }

    #[test]
    fn a_far_fixture_taller_than_the_start_is_named() {
        let w = first_traverse_low(&opening(15.0), 10.0, &[clamp(12.0), clamp(20.0)], |p| p.z, 1e-6);
        assert_eq!(w, Some(Warning::FirstTraverseLow { at: 0, height_mm: 15.0, top_mm: 20.0, fixture: true, tilted: false }));
    }

    #[test]
    fn a_start_at_or_over_every_top_passes() {
        assert_eq!(first_traverse_low(&opening(25.0), 10.0, &[clamp(20.0)], |p| p.z, 1e-6), None);
        assert_eq!(first_traverse_low(&opening(10.0), 10.0, &[clamp(10.0)], |p| p.z, 1e-6), None);
        assert_eq!(first_traverse_low(&opening(5.0), f64::NEG_INFINITY, &[], |p| p.z, 1e-6), None);
        assert_eq!(first_traverse_low(&Toolpath::new(Vec::new()), 10.0, &[], |p| p.z, 1e-6), None);
        assert_eq!(first_traverse_low(&opening(f64::NAN), 10.0, &[], |p| p.z, 1e-6), None);
    }

    #[test]
    fn a_tilted_frame_reads_the_highest_corner_along_its_axis() {
        // A frame whose height runs along world +X: the clamp's far
        // face (x 210) is its top there, whatever its Z.
        let along_x = |p: Point3| p.x;
        assert_eq!(box_top(&clamp(1.0), along_x), 210.0);
        let w = first_traverse_low(&opening(150.0), 100.0, &[clamp(1.0)], along_x, 1e-6);
        assert_eq!(w, Some(Warning::FirstTraverseLow { at: 0, height_mm: 150.0, top_mm: 210.0, fixture: true, tilted: false }));
    }

    /// #2991: a cache saved before the `tilted` field reads as upright,
    /// with the upright text it had.
    #[test]
    fn a_verdict_saved_before_tilted_reads_as_upright() {
        let old = r#"{"first_traverse_low":{"at":0,"height_mm":8.0,"top_mm":10.0,"fixture":false}}"#;
        let w: Warning = serde_json::from_str(old).expect("an old cache loads");
        assert_eq!(w, Warning::FirstTraverseLow { at: 0, height_mm: 8.0, top_mm: 10.0, fixture: false, tilted: false });
        assert!(w.to_string().contains("raise the clearance height above Z10.000"), "{w}");
    }

    /// #2991: the 3+2 text says that the heights are along the tilted tool
    /// axis, names the raise, and has no Z value or axis letters.
    #[test]
    fn a_tilted_verdict_names_the_raise_not_a_z_value() {
        let w = Warning::FirstTraverseLow { at: 0, height_mm: 30.0, top_mm: 31.114, fixture: true, tilted: true };
        let text = w.to_string();
        assert_eq!(
            text,
            "the first rapid (move 0) comes from an unknown position at the clearance height 30.000 mm, \
             under the fixture top at 31.114 mm (both measured along the tilted tool axis) - raise the \
             clearance height by at least 1.114 mm"
        );
        let json = serde_json::to_string(&w).expect("serializes");
        assert_eq!(serde_json::from_str::<Warning>(&json).expect("round trip"), w);
    }

    /// #2965: a 40 x 40 square bar along X, turned about the X axis.
    const A: RotaryAxis = RotaryAxis { origin: Point3 { x: 0.0, y: 0.0, z: 0.0 }, dir: [2.0, 0.0, 0.0] };

    fn square_bar() -> Vec<Point3> {
        (0..8).map(|i| Point3::new([0.0, 80.0][(i >> 2) & 1], [-20.0, 20.0][(i >> 1) & 1], [-20.0, 20.0][i & 1])).collect()
    }

    const NO_STEEL: SweptSteel<'static> = SweptSteel { turning: &[], fixed: &[] };
    const NONE: f64 = f64::NEG_INFINITY;

    #[test]
    fn a_turned_bar_is_read_at_its_corner_radius_not_its_upright_top() {
        let top = 20.0 * 2f64.sqrt();
        assert!((swept_top(Point3::new(0.0, 20.0, 20.0), &A) - top).abs() < 1e-12);
        // Upright the bar's top is Z20, so Z25 clears it; turned 45 deg
        // its edge stands at Z28.284 and the traverse runs through it.
        let w = first_traverse_low_swept(Point3::new(5.0, 0.0, 25.0), [0.0, 0.0, 1.0], NONE, &A, &square_bar(), NO_STEEL, 1e-6);
        let Some(Warning::FirstTraverseLow { at: 0, height_mm, top_mm, fixture: false, tilted: false }) = w else { panic!("{w:?}") };
        assert!((height_mm - 25.0).abs() < 1e-12 && (top_mm - top).abs() < 1e-12, "{w:?}");
        assert!(w.is_some_and(|w| w.blocks_post()));
        assert_eq!(first_traverse_low_swept(Point3::new(5.0, 0.0, 30.0), [0.0, 0.0, 1.0], NONE, &A, &square_bar(), NO_STEEL, 1e-6), None);
    }

    #[test]
    fn a_radial_start_reads_its_radius_over_the_axis_line() {
        let s = 0.5f64.sqrt();
        let radial = [0.0, s, s];
        for (r, refused) in [(25.0, true), (30.0, false)] {
            let start = Point3::new(5.0, r * s, r * s);
            let w = first_traverse_low_swept(start, radial, NONE, &A, &square_bar(), NO_STEEL, 1e-6);
            match w {
                Some(Warning::FirstTraverseLow { height_mm, fixture: false, .. }) if refused => assert!((height_mm - r).abs() < 1e-9, "{w:?}"),
                None if !refused => {}
                _ => panic!("radius {r}: {w:?}"),
            }
        }
    }

    #[test]
    fn a_chuck_turns_with_the_work_and_a_tailstock_stands_still() {
        let chuck = FixtureBox { min: Point3::new(-30.0, -50.0, -50.0), max: Point3::new(-20.0, 50.0, 50.0) };
        let tail = FixtureBox { min: Point3::new(100.0, -10.0, -45.0), max: Point3::new(110.0, 10.0, 45.0) };
        let start = Point3::new(5.0, 0.0, 60.0);
        let up = [0.0, 0.0, 1.0];
        let turning = SweptSteel { turning: &[chuck], fixed: &[] };
        let w = first_traverse_low_swept(start, up, NONE, &A, &square_bar(), turning, 1e-6);
        let Some(Warning::FirstTraverseLow { top_mm, fixture: true, .. }) = w else { panic!("{w:?}") };
        assert!((top_mm - 50.0 * 2f64.sqrt()).abs() < 1e-9, "the chuck's corner radius: {top_mm}");
        // The same box standing still: its top is Z45, not its radius 46.1.
        let fixed = SweptSteel { turning: &[], fixed: &[tail] };
        assert_eq!(first_traverse_low_swept(Point3::new(5.0, 0.0, 45.5), up, NONE, &A, &square_bar(), fixed, 1e-6), None);
        let w = first_traverse_low_swept(Point3::new(5.0, 0.0, 40.0), up, NONE, &A, &square_bar(), fixed, 1e-6);
        assert_eq!(w, Some(Warning::FirstTraverseLow { at: 0, height_mm: 40.0, top_mm: 45.0, fixture: true, tilted: false }));
    }

    #[test]
    fn the_swept_top_follows_the_axis_tilt_and_refuses_no_input() {
        let vertical = RotaryAxis { origin: Point3::new(0.0, 0.0, 0.0), dir: [0.0, 0.0, 1.0] };
        assert_eq!(swept_top(Point3::new(30.0, 0.0, 7.0), &vertical), 7.0);
        // A radial live tool beside a C axis: no turn stands it upright,
        // so its start is read at its own Z, which a C turn keeps.
        let w = first_traverse_low_swept(Point3::new(30.0, 0.0, 7.0), [1.0, 0.0, 0.0], NONE, &vertical, &[Point3::new(0.0, 0.0, 10.0)], NO_STEEL, 1e-6);
        assert_eq!(w, Some(Warning::FirstTraverseLow { at: 0, height_mm: 7.0, top_mm: 10.0, fixture: false, tilted: false }));
        // The same start under a clearance height of 12: the post
        // traverses at Z12, over the top.
        assert_eq!(first_traverse_low_swept(Point3::new(30.0, 0.0, 7.0), [1.0, 0.0, 0.0], 12.0, &vertical, &[Point3::new(0.0, 0.0, 10.0)], NO_STEEL, 1e-6), None);
        let leaning = RotaryAxis { origin: Point3::new(0.0, 0.0, 0.0), dir: [1.0, 0.0, 1.0] };
        assert!((swept_top(Point3::new(0.0, 10.0, 0.0), &leaning) - 10.0 * 0.5f64.sqrt()).abs() < 1e-12);
        let nan = Point3::new(f64::NAN, 0.0, 0.0);
        assert_eq!(first_traverse_low_swept(nan, [0.0, 0.0, 1.0], NONE, &A, &square_bar(), NO_STEEL, 1e-6), None);
        assert_eq!(first_traverse_low_swept(Point3::new(0.0, 0.0, 1.0), [0.0, 0.0, 1.0], NONE, &A, &[], NO_STEEL, 1e-6), None);
    }
}

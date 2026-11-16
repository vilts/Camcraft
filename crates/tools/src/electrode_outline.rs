//! The sinker electrode's outline (#1152): the closed section of an
//! electrode BODY, in mm about the burn point, extruded along Z - and
//! the law that says when a list of points is one.
//!
//! Provenance: general die-sink EDM practice from public machine-tool
//! handbooks (the electrode is the cavity's own section made smaller).
//! The shape law is textbook planar geometry: the shoelace area, and
//! the orientation test for two segments that meet (Cormen, Leiserson,
//! Rivest & Stein, "Introduction to Algorithms", ch. 33.1). The bounds
//! and the refusals are our own design (#290 D7). No vendor material.
//!
//! The record: `Tool::outline` holds the points; `diameter` and `width`
//! stay the outline's X and Y extents, and the outline is centred on
//! the burn point, so every law that reads the two sizes (the envelope,
//! the size law of #543) reads the body's true bounds.

use crate::tools::Tool;

/// The most points an outline holds: the stock sim sweeps every edge
/// through every column under the electrode.
pub const MAX_POINTS: usize = 1024;
/// Largest coordinate, mm: half the largest face side a record holds.
const MAX_REACH: f64 = 250.0;
/// The shortest edge, mm: under it two points are one point.
const MIN_EDGE: f64 = 1e-9;
/// The least area an outline encloses, mm².
const MIN_AREA: f64 = 1e-9;
/// Slack between the outline's extents and the record's two sizes, mm.
pub const EXTENT_SLACK: f64 = 1e-6;

fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Is `p`, known collinear with `a`-`b`, within the segment's box?
fn on(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> bool {
    p[0] >= a[0].min(b[0]) && p[0] <= a[0].max(b[0]) && p[1] >= a[1].min(b[1]) && p[1] <= a[1].max(b[1])
}

/// Do the closed segments `a`-`b` and `c`-`d` share a point?
fn meet(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let (d1, d2, d3, d4) = (cross(c, d, a), cross(c, d, b), cross(a, b, c), cross(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return true;
    }
    (d1 == 0.0 && on(c, d, a)) || (d2 == 0.0 && on(c, d, b)) || (d3 == 0.0 && on(a, b, c)) || (d4 == 0.0 && on(a, b, d))
}

/// The outline's bounds `(lo, hi)`; `None` for no points.
#[must_use]
pub fn bounds(o: &[[f64; 2]]) -> Option<([f64; 2], [f64; 2])> {
    let first = *o.first()?;
    Some(o.iter().fold((first, first), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])
    }))
}

/// Why `o` is no electrode section, by name; `None` for a simple closed
/// outline (either winding; the closing edge runs last to first).
#[must_use]
pub fn shape_refusal(o: &[[f64; 2]]) -> Option<String> {
    let n = o.len();
    if n < 3 {
        return Some(format!("a sinker electrode's outline needs at least 3 points (it states {n})"));
    }
    if n > MAX_POINTS {
        return Some(format!(
            "a sinker electrode's outline has {n} points, more than the {MAX_POINTS} the stock sim sweeps - an outline that fine is not modelled"
        ));
    }
    if let Some(i) = o.iter().position(|p| !(p[0].is_finite() && p[1].is_finite() && p[0].abs() <= MAX_REACH && p[1].abs() <= MAX_REACH)) {
        return Some(format!(
            "a sinker electrode's outline point {i} is not a finite position within {MAX_REACH} mm of the burn point"
        ));
    }
    let at = |i: usize| o.get(i % n).copied().unwrap_or([0.0, 0.0]);
    if let Some(i) = (0..n).find(|&i| (at(i)[0] - at(i + 1)[0]).hypot(at(i)[1] - at(i + 1)[1]) < MIN_EDGE) {
        return Some(format!("a sinker electrode's outline repeats a point (points {i} and {})", (i + 1) % n));
    }
    let area = 0.5 * (0..n).map(|i| at(i)[0] * at(i + 1)[1] - at(i + 1)[0] * at(i)[1]).sum::<f64>().abs();
    if area < MIN_AREA {
        return Some("a sinker electrode's outline encloses no area".into());
    }
    for i in 0..n {
        // The next edge shares one point; it must not fold back on it.
        let (a, b, c) = (at(i), at(i + 1), at(i + 2));
        let back = (b[0] - a[0]) * (c[0] - b[0]) + (b[1] - a[1]) * (c[1] - b[1]) < 0.0;
        if cross(a, b, c) == 0.0 && back {
            return Some(format!("a sinker electrode's outline folds back on itself at point {}", (i + 1) % n));
        }
        // Every edge further on (but the one that closes onto this one).
        for j in (i + 2)..n {
            if i == 0 && j + 1 == n {
                continue;
            }
            if meet(a, b, at(j), at(j + 1)) {
                return Some(format!(
                    "a sinker electrode's outline crosses itself (edge {i} meets edge {j}) - it bounds no one section"
                ));
            }
        }
    }
    None
}

/// Why the record's outline is refused, by name: its shape, and its
/// agreement with the two sizes the record states. `None` for a record
/// with no outline (a rectangular face) and for a sound one.
#[must_use]
pub fn record_refusal(t: &Tool) -> Option<String> {
    if t.outline.is_empty() {
        return None;
    }
    if let Some(why) = shape_refusal(&t.outline) {
        return Some(why);
    }
    let (lo, hi) = bounds(&t.outline)?;
    let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
    let off = (0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1]));
    let near = |a: f64, b: f64| (a - b).abs() <= EXTENT_SLACK;
    if !(near(w, t.diameter) && near(h, t.width) && near(off.0, 0.0) && near(off.1, 0.0)) {
        return Some(format!(
            "a sinker electrode's outline spans {w:.6} x {h:.6} centred ({:.6}, {:.6}) from the burn point, but the record states {} x {} - the diameter and the width are the outline's X and Y extents about the burn point (remove the outline to state a rectangular face)",
            off.0, off.1, t.diameter, t.width
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electrode::{electrode_outline_tool, electrode_tool};

    /// An L: 10 x 8 with a 6 x 4 notch out of its upper right, centred.
    fn ell() -> Vec<[f64; 2]> {
        vec![[-5.0, -4.0], [5.0, -4.0], [5.0, 0.0], [-1.0, 0.0], [-1.0, 4.0], [-5.0, 4.0]]
    }

    #[test]
    fn electrode_outline_shape_is_refused_by_name() {
        assert_eq!(shape_refusal(&ell()), None);
        let mut cw = ell();
        cw.reverse();
        assert_eq!(shape_refusal(&cw), None, "either winding");
        let why = |o: &[[f64; 2]]| shape_refusal(o).expect("refused");
        assert!(why(&[[0.0, 0.0], [1.0, 0.0]]).contains("at least 3 points"));
        let many: Vec<[f64; 2]> = (0..=MAX_POINTS).map(|i| {
            let a = i as f64 * std::f64::consts::TAU / (MAX_POINTS + 1) as f64;
            [5.0 * a.cos(), 5.0 * a.sin()]
        }).collect();
        assert!(why(&many).contains("more than the 1024"), "{}", why(&many));
        assert_eq!(shape_refusal(many.get(1..).expect("one fewer")), None, "the bound itself is allowed");
        assert!(why(&[[0.0, 0.0], [f64::NAN, 0.0], [1.0, 1.0]]).contains("point 1 is not a finite"));
        assert!(why(&[[0.0, 0.0], [300.0, 0.0], [1.0, 1.0]]).contains("point 1 is not a finite"));
        assert!(why(&[[0.0, 0.0], [1.0, 0.0], [1.0, 0.0], [0.0, 1.0]]).contains("repeats a point"));
        assert!(why(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]]).contains("encloses no area"));
        // A bow tie with unequal lobes (equal ones enclose no net area).
        assert!(why(&[[0.0, 0.0], [4.0, 0.0], [1.0, 3.0], [4.0, 3.0]]).contains("crosses itself"));
        // A spike: out along an edge and straight back.
        assert!(why(&[[0.0, 0.0], [4.0, 0.0], [2.0, 0.0], [2.0, 3.0]]).contains("folds back"));
        // A vertex resting on a far edge.
        assert!(why(&[[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [2.0, 0.0], [0.0, 4.0]]).contains("crosses itself"));
    }

    #[test]
    fn electrode_outline_record_agrees_with_its_sizes() {
        let e = electrode_outline_tool(&ell(), 16.0, 15.0);
        assert_eq!((e.diameter, e.width, e.flute_len, e.shank_len), (10.0, 8.0, 16.0, 15.0));
        assert!(e.is_sane() && e.torch_refusal().is_none(), "{:?}", e.torch_refusal());
        // No outline: the rectangular record, untouched.
        assert_eq!(record_refusal(&electrode_tool(10.0, 6.0, 40.0, 15.0)), None);
        // A size edited away from the outline.
        let wide = Tool { diameter: 12.0, ..e.clone() };
        assert!(!wide.is_sane());
        assert!(wide.torch_refusal().is_some_and(|w| w.contains("the outline's X and Y extents")), "{:?}", wide.torch_refusal());
        let tall = Tool { width: 9.0, ..e.clone() };
        assert!(!tall.is_sane() && tall.torch_refusal().is_some_and(|w| w.contains("X and Y extents")));
        // An outline that is not centred on the burn point.
        let moved: Vec<[f64; 2]> = ell().iter().map(|p| [p[0] + 1.0, p[1]]).collect();
        let off = Tool { outline: moved, ..e.clone() };
        assert!(!off.is_sane() && off.torch_refusal().is_some_and(|w| w.contains("centred (1.000000, 0.000000)")), "{:?}", off.torch_refusal());
        let up: Vec<[f64; 2]> = ell().iter().map(|p| [p[0], p[1] - 0.5]).collect();
        let off = Tool { outline: up, ..e.clone() };
        assert!(!off.is_sane() && off.torch_refusal().is_some_and(|w| w.contains("centred (0.000000, -0.500000)")), "{:?}", off.torch_refusal());
        // A garbled shape is named through the record's door too.
        let tie = Tool { outline: vec![[-5.0, -4.0], [5.0, -4.0], [-3.0, 4.0], [5.0, 4.0]], ..e };
        assert!(!tie.is_sane() && tie.torch_refusal().is_some_and(|w| w.contains("crosses itself")), "{:?}", tie.torch_refusal());
    }
}

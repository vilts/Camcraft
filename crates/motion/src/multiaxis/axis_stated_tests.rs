//! The stated-axis law's own tests (#2772 parts 1-2): a revolve holds
//! its stated line and reads the same axis the fit reads; a stated
//! line the part does not turn about refuses by name; a helical part
//! refuses with the freeform's 4-axis wording - the radial kinds stay
//! locked until the radial drop slice can hold them.
//!
//! Provenance: our own fixtures; nothing of vendor binaries.

use super::*;
use crate::geometry::{Point3, Triangle};

const TAU: f64 = std::f64::consts::TAU;

/// The turned bar's meridian (rho, s): wall r6, shoulder, neck r5.
const MERIDIAN: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];

/// The bar revolved about `axis` (0 = world X, laid; 1 = world Y;
/// 2 = world Z, standing), `n` facets around.
fn bar(axis: usize, n: usize) -> Vec<Triangle> {
    let pt = |rho: f64, s: f64, j: usize| {
        let th = TAU * j as f64 / n as f64;
        match axis {
            0 => Point3::new(s, rho * th.cos(), rho * th.sin()),
            1 => Point3::new(rho * th.cos(), s, rho * th.sin()),
            _ => Point3::new(rho * th.cos(), rho * th.sin(), s),
        }
    };
    let mut f = Vec::new();
    for w in MERIDIAN.windows(2) {
        let ((ra, za), (rb, zb)) = (w[0], w[1]);
        for j in 0..n {
            let k = (j + 1) % n;
            let (a1, a2, b1, b2) = (pt(ra, za, j), pt(ra, za, k), pt(rb, zb, j), pt(rb, zb, k));
            if ra <= 1e-12 {
                f.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                f.push(Triangle::new(a1, a2, b2));
            } else {
                f.push(Triangle::new(a1, a2, b2));
                f.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    f
}

/// The reporter's kind of part (#2772): a 4-lobe helical sweep about
/// world X - the compressor rotor's lobes turning as x advances -
/// clamped ON the stated A line through the origin. Its normals share
/// no rotation axis (the free fit's own refusal).
fn rotor(turns: f64) -> Vec<Triangle> {
    let rate = turns * TAU / 32.0;
    let (nth, nx) = (96usize, 32usize);
    let rho = |th: f64| 5.0 + 1.5 * (4.0 * th).cos();
    let pt = |th_i: usize, x: f64| {
        let th = TAU * th_i as f64 / nth as f64;
        let (s, c) = (rate * x).sin_cos();
        let r = rho(th);
        let (bs, bc) = th.sin_cos();
        Point3::new(x, r * (bc * c - bs * s), r * (bs * c + bc * s))
    };
    let mut f = Vec::new();
    for thi in 0..nth {
        let tk = (thi + 1) % nth;
        for st in 0..nx {
            let (x0, x1) = (-16.0 + 32.0 * st as f64 / nx as f64, -16.0 + 32.0 * (st + 1) as f64 / nx as f64);
            let (a1, a2, b1, b2) = (pt(thi, x0), pt(tk, x0), pt(thi, x1), pt(tk, x1));
            f.push(Triangle::new(a1, a2, b2));
            f.push(Triangle::new(a1, b2, b1));
        }
    }
    let cap = |f: &mut Vec<Triangle>, x: f64, outward: bool| {
        let c = Point3::new(x, 0.0, 0.0);
        for thi in 0..nth {
            let tk = (thi + 1) % nth;
            let (a, b) = (pt(thi, x), pt(tk, x));
            f.push(if outward { Triangle::new(c, a, b) } else { Triangle::new(c, b, a) });
        }
    };
    cap(&mut f, -16.0, false);
    cap(&mut f, 16.0, true);
    f
}

/// A stated line through the world origin along `dir`, the work
/// offset's own position when the document sets none.
fn line(dir: [f64; 3]) -> Axis {
    canonical(dir, Point3::new(0.0, 0.0, 0.0)).expect("a direction")
}

/// Every vertex of `facets` moved by `off`: a common model or WCS
/// translation (the review of PR #3674).
fn moved(facets: &[Triangle], off: [f64; 3]) -> Vec<Triangle> {
    let at = |p: Point3| Point3::new(p.x + off[0], p.y + off[1], p.z + off[2]);
    facets
        .iter()
        .map(|t| Triangle::new(at(t.a), at(t.b), at(t.c)))
        .collect()
}

#[test]
fn a_revolve_holds_its_stated_line_and_reads_the_fitted_axis() {
    let facets = bar(0, 16);
    let stated = line([1.0, 0.0, 0.0]);
    let held = holds(&facets, stated).expect("the bar turns about the stated line");
    let fitted = super::super::axis::fit(&facets).expect("the bar is a revolve");
    assert_eq!(held, fitted, "the stated line reads the very axis the fit reads");
    // The same judgement over several lines (a trunnion's pair) picks
    // the one that holds, in the caller's order.
    let standing = bar(2, 16);
    let held = holds_any(
        &standing,
        &[line([1.0, 0.0, 0.0]), line([0.0, 0.0, 1.0])],
    )
    .expect("the standing bar turns about the vertical line");
    assert_eq!(held, super::super::axis::fit(&standing).expect("the standing bar is a revolve"));
}

#[test]
fn the_canonical_line_is_signed_and_pinned_like_the_fit() {
    // Either sign of a stated direction is one line, and the origin is
    // pinned to the fit's own foot through the anchor.
    for dir in [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [2.0, 0.0, 0.0]] {
        let ax = canonical(dir, Point3::new(30.0, 12.0, -8.0)).expect("a direction");
        assert_eq!(ax.dir, Point3::new(1.0, 0.0, 0.0), "{dir:?} signs one way");
        assert_eq!(ax.origin, Point3::new(0.0, 12.0, -8.0), "the line runs through the anchor, pinned to the fit's own foot");
    }
    let up = canonical([0.0, 0.0, -5.0], Point3::new(4.0, 5.0, 9.0)).expect("a direction");
    assert_eq!(up.dir, Point3::new(0.0, 0.0, 1.0));
    assert_eq!(up.origin, Point3::new(4.0, 5.0, 0.0), "the origin pins to the fit's foot");
    assert!(canonical([0.0, 0.0, 0.0], Point3::new(0.0, 0.0, 0.0)).is_none(), "zero is no direction");
    assert!(canonical([f64::NAN, 0.0, 0.0], Point3::new(0.0, 0.0, 0.0)).is_none(), "garbage is no direction");
}

#[test]
fn a_stated_line_the_part_does_not_turn_about_refuses_by_name() {
    // The laid bar with the line stated along Z: a revolve placed off
    // the stated line - the refusal names the part's own axis and the
    // acts that fix it, never a silent fall back to the fit.
    let facets = bar(0, 16);
    let e = judged_any(&facets, &[("A".into(), line([0.0, 0.0, 1.0]))]).expect_err("the bar does not turn about Z");
    let e = e.to_string();
    assert!(e.contains("disagree with the stated rotary line (A)"), "{e}");
    assert!(e.contains("they turn about their own axis"), "{e}");
    assert!(e.contains("Align the part on the stated line"), "{e}");
    assert!(e.contains("move the work offset"), "{e}");
    // A stated line off the part by a hair of the work offset's own
    // anchor still refuses: the line moved, not the part.
    let moved = canonical([1.0, 0.0, 0.0], Point3::new(0.0, 30.0, 0.0)).expect("a direction");
    let e = judged_any(&facets, &[("A".into(), moved)]).expect_err("the line stands 30 mm off the part");
    assert!(e.to_string().contains("they turn about their own axis"), "{e}");
}

#[test]
fn a_helical_part_refuses_the_stated_line_with_the_4_axis_wording() {
    // The reporter's kind of part: clamped ON the stated A line, its
    // lobes helical. The raster stays locked (the radial pose would
    // gouge), so the refusal keeps the freeform's wording - the fit
    // refuses the mesh too, which the message says honestly.
    let facets = rotor(2.0);
    assert!(super::super::axis::fit(&facets).is_err(), "the helical lobes share no rotation axis");
    let e = judged_any(&facets, &[("A".into(), line([1.0, 0.0, 0.0]))]).expect_err("the radial kinds stay locked on helical lobes");
    let e = e.to_string();
    assert!(e.contains("disagree with the stated rotary line (A)"), "{e}");
    assert!(e.contains("share no rotation axis"), "{e}");
    assert!(e.contains("not supported yet (#2772)"), "{e}");
    assert!(e.contains("5-Axis"), "{e}");
}

#[test]
fn an_empty_mesh_refuses_like_the_fit_does() {
    let e = judged_any(&[], &[("A".into(), line([1.0, 0.0, 0.0]))]).expect_err("no surface");
    assert!(e.to_string().contains("no surface"), "{e}");
    assert!(holds_any(&[], &[line([1.0, 0.0, 0.0])]).is_none());
}

#[test]
fn a_common_translation_moves_the_hold_with_the_part() {
    // The radius the judge scales its gates by is read ABOUT THE
    // STATED LINE, never off the world origin (the review of PR
    // #3674): a part moved with its line - a common model or WCS
    // translation - holds exactly as it did at home, on every
    // coordinate axis, and reads the same axis the fit reads.
    for (axis, dir) in [(0usize, [1.0, 0.0, 0.0]), (1, [0.0, 1.0, 0.0]), (2, [0.0, 0.0, 1.0])] {
        for off in [
            [0.0, 0.0, 0.0],
            [10000.0, 0.0, 0.0],
            [0.0, 10000.0, 0.0],
            [0.0, 0.0, 10000.0],
            [-2500.0, 30.0, 480.0],
        ] {
            let facets = moved(&bar(axis, 16), off);
            let stated = canonical(dir, Point3::new(off[0], off[1], off[2])).expect("a direction");
            let held = holds(&facets, stated).expect("the moved part turns about its moved line");
            let fitted = super::super::axis::fit(&facets).expect("the moved bar is still a revolve");
            let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
            assert!(
                near(held.dir.x, fitted.dir.x)
                    && near(held.dir.y, fitted.dir.y)
                    && near(held.dir.z, fitted.dir.z)
                    && near(held.origin.x, fitted.origin.x)
                    && near(held.origin.y, fitted.origin.y)
                    && near(held.origin.z, fitted.origin.z),
                "{off:?} axis {axis}: the stated line reads the axis the fit reads"
            );
        }
    }
}

#[test]
fn a_wrong_line_refuses_on_every_plane_wherever_the_part_sits() {
    // The review of PR #3674: the same geometric disagreement gives
    // the same refusal wherever the part sits. A wrong line a hair
    // off the part's true axis - one on each coordinate plane -
    // refuses at home and after any common translation; the old
    // world-origin radius let the 1 mm error pass behind +10000 mm.
    let facets = bar(0, 16); // the true axis: world X through the origin
    let wrong = [
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), // parallel, 1 mm off in Y: the XY plane
        ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]), // parallel, 1 mm off in Z: the XZ plane
        ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]), // across the bar, in the YZ plane
    ];
    for off in [
        [0.0, 0.0, 0.0],
        [10000.0, 0.0, 0.0],
        [0.0, 10000.0, 0.0],
        [0.0, 0.0, 10000.0],
        [-2500.0, 30.0, 480.0],
    ] {
        let moved_facets = moved(&facets, off);
        for (dir, at) in wrong {
            let stated =
                canonical(dir, Point3::new(at[0] + off[0], at[1] + off[1], at[2] + off[2])).expect("a direction");
            let e = judged_any(&moved_facets, &[("A".into(), stated)])
                .expect_err("a wrong line refuses wherever the part sits");
            let e = e.to_string();
            assert!(
                e.contains("disagree with the stated rotary line (A)"),
                "{off:?} {dir:?} at {at:?}: {e}"
            );
            assert!(
                e.contains("they turn about their own axis"),
                "{off:?} {dir:?} at {at:?}: {e}"
            );
        }
        // The part's own line, moved with it, still holds.
        let own = canonical([1.0, 0.0, 0.0], Point3::new(off[0], off[1], off[2])).expect("a direction");
        assert!(holds(&moved_facets, own).is_some(), "{off:?}: the part keeps its own moved line");
    }
}

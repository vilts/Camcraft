//! Pins of the overhang backstop's reading (#1313 review of the #1303
//! port, carried onto #1316's swept cells): the backstop reads main's
//! #1303 column - the rim's exact dip, lowered by the span floor - and a
//! hidden-stock probe asserts the exact verdict on both sides of that
//! reading. Our own fixture; no vendor material.

use super::*;
use crate::geometry::Point3;

/// A stand-in for [`super::super::hidden`]'s `hidden_in_cover`: one
/// column of stock hidden under a ledge at `p`, its top at `top`. An ask
/// meets it when `p` is in the ask's box and footprint (the kerf
/// excluded by the footprint's own test) and the ask's lowest plane `h`,
/// or some height the band's bottom takes over `p`, stands under its top.
fn hidden_post(p: Point2, top: f64) -> Box<Hidden<'static>> {
    Box::new(move |(lo, hi), h, _probes, heights, _upto| {
        let covered = heights(p, &mut |_, _| true);
        p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y && covered && (h < top || heights(p, &mut |a, _| a < top))
    })
}

/// The #1324 review's counter-probe stand-in: a cavity at `p`, stock
/// hidden only at the planes strictly inside (`lo`, `hi`) - part
/// material (a floor) below, the ledge above. The real law over such a
/// column: the lowest plane `h` of an ask reads it only if it stands
/// in the cavity; the range reading, if some height the band's bottom
/// takes over `p` does.
fn cavity(p: Point2, lo: f64, hi: f64) -> Box<Hidden<'static>> {
    Box::new(move |(a, b), h, _probes, heights, _upto| {
        let inside = |z: f64| z > lo && z < hi;
        let covered = heights(p, &mut |_, _| true);
        let swept = heights(p, &mut |z0, z1| z0 < hi && z1 > lo);
        p.x >= a.x && p.x <= b.x && p.y >= a.y && p.y <= b.y && covered && (inside(h) || swept)
    })
}

/// The #1324 review's counter-probe: A 0 -> 2 degrees on one straight
/// plunge z 0 -> -40 (band 20 up, radius 10, 30 long; cutter radius 2),
/// a post at (5, 0). The band's bottom passes over the post from z 20
/// down to z -20.4. The hidden law is NOT monotone in the plane: the
/// cavity's stock is hidden only between its floor and its ledge, and
/// the single ask's lowest plane (-20.37, the deep end) stands in the
/// floor's part material - reading that one plane passed a band that
/// went through the stock on the way down. Every height in the range
/// convicts, wherever the cavity sits in it; one above or below does not.
#[test]
fn a_cavity_the_band_passes_on_the_way_down_is_convicted_at_any_height() {
    let (from, to) = ([0.0, 0.0, 0.0], [2.0, 0.0, 0.0]);
    let bands = [(20.0, 10.0, 30.0)];
    let (r, tol) = (2.0, 0.01);
    let m = Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 0.0), Point3::new(0.0, 0.0, -40.0), Some(300.0));
    let p = Point2::new(5.0, 0.0);
    let verdict = |lo: f64, hi: f64| {
        let mut memo = std::collections::HashMap::new();
        let none_in = |_: Point2| false;
        let flat = |_: Point2| -> Steps { unreachable!("no sample column to read") };
        swing_hit(&m, 1, from, to, &bands, r, (tol, 0.0), 0.01, &[], &mut memo, &none_in, &flat, &none_in, &cavity(p, lo, hi))
    };
    // The review's cavity: the old 4 x 5 grid read a plane in it, the lowest plane does not.
    assert_eq!(verdict(2.64, 4.64), Some(false), "the review's counter-probe");
    // No fixed set of planes passes by luck: thin cavities all down the sweep.
    for k in 0..400 {
        let lo = -20.0 + 0.0975 * f64::from(k);
        assert_eq!(verdict(lo, lo + 0.05), Some(false), "a 0.05 mm cavity at {lo}");
    }
    assert_eq!(verdict(18.9, 19.0), Some(false), "near the top of the sweep");
    assert_eq!(verdict(-20.3, -20.2), Some(false), "near its deep end");
    // Clean twins: the band's bottom never stands at these heights over the post.
    assert_eq!(verdict(20.2, 24.0), None, "a cavity over the whole sweep");
    assert_eq!(verdict(-30.0, -20.5), None, "a cavity under its deep end");

    // The range itself, over the post: from the lowest plane up to the start pose's bottom.
    let kerf2 = (r + tol) * (r + tol);
    let asks = super::super::turn_hidden::backstop_asks(&m.segments3(0.01), from, to, &bands, r, 0.0);
    let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
    for a in &asks {
        a.heights(p, kerf2, &mut |z0, z1| {
            assert!(z0 >= a.plane && z1 >= z0, "a range under its ask's plane: {z0}..{z1} < {}", a.plane);
            (low, high) = (low.min(z0), high.max(z1));
            false
        });
    }
    assert!((-20.5..-20.3).contains(&low) && (20.0..20.1).contains(&high), "the band's bottom over the post: {low}..{high}");
}

/// A 100 -> 260 passes the tool straight down (A 180, the middle sampled
/// pose). Band 20 up, radius 10, 30 long (L = 50); cutter radius 2. The
/// hidden post at (5, 0) sits inside the A 180 column (radius 10 about
/// the tip) and outside the reach of the poses whose span floors run
/// lower (A 140 / A 220 lean 22 mm or more off the tip along Y), so the
/// lowest reading over it is A 180's:
/// - its own exact column: -L, the rim dips R·sin 180 = 0 (the cells
///   about it read a little lower, by their sag and kappa bound);
/// - the span floor toward A 140 / A 220: -L - R·sin 40 = -56.43 (the
///   lerp between the samples leans as far as 40 degrees off down);
/// - the whole radius (the reading before #1303): -L - R = -60.
///
/// The backstop must read -56.43: the post topped at -53 is convicted
/// (only the span floor reaches it - deleting the floor reads about
/// -51.5 and clears it), the post topped at -58 is clean (the whole
/// radius would read -60 and convict it). A revert either way is caught.
#[test]
fn the_backstop_reads_the_exact_dip_lowered_by_the_span_floor() {
    let (from, to) = ([100.0, 0.0, 0.0], [260.0, 0.0, 0.0]);
    let (above, band_r, band_len) = (20.0, 10.0, 30.0);
    let bands = [(above, band_r, band_len)];
    let (r, tol) = (2.0, 0.01);
    let o = Point3::new(0.0, 0.0, 0.0);
    let m = Move::new(MoveKind::Feed, o, o, Some(300.0));
    let p = Point2::new(5.0, 0.0);
    let l = above + band_len;
    let floor = -l - band_r * 40f64.to_radians().sin();

    let verdict = |top: f64| {
        let mut memo = std::collections::HashMap::new();
        let none_in = |_: Point2| false;
        // No sample columns: the column law reads nothing, the backstop is asked.
        let flat = |_: Point2| -> Steps { unreachable!("no sample column to read") };
        swing_hit(&m, 1, from, to, &bands, r, (tol, 0.0), 0.01, &[], &mut memo, &none_in, &flat, &none_in, &hidden_post(p, top))
    };
    assert_eq!(verdict(-53.0), Some(false), "only the span floor reaches the post at -53");
    assert_eq!(verdict(-58.0), None, "the exact dip stands clear of the post at -58 (the whole radius would not)");

    // The reading itself: the lowest plane of any ask whose footprint holds the post.
    let kerf2 = (r + tol) * (r + tol);
    let asks = super::super::turn_hidden::backstop_asks(&m.segments3(0.01), from, to, &bands, r, 0.0);
    let read = asks.iter().filter(|a| a.covers(p, kerf2)).map(|a| a.plane).fold(f64::INFINITY, f64::min);
    assert!((read - floor).abs() < 1e-9, "the backstop reads {read} over the post, the exact dip and span floor {floor}");
}

/// #1404: a blade-lying swarf link turns B +90 -> -90 at a fixed tip, the
/// band swinging through vertical (band 60 up, radius 3, 40 long; cutter
/// radius 3 - the release lane's full-band swarf fixture). Every letter
/// stays in the closed cap [-90, 90], so no span can lean past its more
/// tilted end: the horizontal ends' `-R` reading belongs to the ends'
/// own footprints. Before the closed cap, the B 45 pose read that `-R`
/// floor (its neighbour stood at exactly 90) over its own footprint,
/// where the band stands some 34 mm over the tip, and a post 47 mm out
/// topped at +10 was refused - the 18 false hits. The twins keep the
/// real refusals: a post under a horizontal end's band (70 mm out, top
/// -2, the band's bottom there -3) and one the swing passes through.
#[test]
fn a_swing_through_vertical_reads_no_floor_from_its_horizontal_ends() {
    let bands = [(60.0, 3.0, 40.0)];
    let (r, tol) = (3.0, 0.01);
    let o = Point3::new(0.0, 0.0, 0.0);
    let m = Move::new(MoveKind::Feed, o, o, Some(300.0));
    for (from, to) in [([0.0, 90.0, 0.0], [0.0, -90.0, 0.0]), ([0.0, -90.0, 0.0], [0.0, 90.0, 0.0])] {
        for k in 0..5 {
            assert_eq!(band_floors(from, to, bands[0])[k].1, f64::INFINITY, "pose {k}: no span floor in the closed cap");
        }
        let verdict = |x: f64, top: f64| {
            let mut memo = std::collections::HashMap::new();
            let none_in = |_: Point2| false;
            let flat = |_: Point2| -> Steps { unreachable!("no sample column to read") };
            swing_hit(&m, 1, from, to, &bands, r, (tol, 0.0), 0.01, &[], &mut memo, &none_in, &flat, &none_in, &hidden_post(Point2::new(x, 0.0), top))
        };
        for x in [-47.0, 47.0] {
            assert_eq!(verdict(x, 10.0), None, "{from:?} -> {to:?}: the band stands ~34 mm over the tip {x} mm out");
            assert_eq!(verdict(x, 40.0), Some(false), "{from:?} -> {to:?}: the swing passes through a post topped at 40, {x} mm out");
        }
        for x in [-70.0, 70.0] {
            assert_eq!(verdict(x, -2.0), Some(false), "{from:?} -> {to:?}: a horizontal end's band (bottom -3) over a post topped at -2, {x} mm out");
        }
    }
}

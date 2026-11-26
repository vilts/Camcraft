//! Unit tests of the spun profile (#93): refusals by name, parametric
//! profiles that match the tool's own cylinders, exact stepped bands
//! and conservative arcs.

use super::*;
use crate::holder_profile::HolderStack;
use crate::tools::catalog;

fn p(z: f64, r: f64) -> SpunPoint {
    SpunPoint::line(z, r)
}

fn stepped() -> Vec<SpunPoint> {
    vec![p(40.0, 0.0), p(40.0, 12.0), p(60.0, 12.0), p(60.0, 20.0), p(90.0, 20.0), p(90.0, 0.0)]
}

#[test]
fn a_drawn_profile_is_refused_by_name() {
    let refuse = |pts: Vec<SpunPoint>, want: SpunError| {
        assert_eq!(SpunProfile::new(pts), Err(want));
    };
    refuse(vec![p(0.0, 0.0), p(1.0, 0.0)], SpunError::TooFew);
    refuse((0..=MAX_POINTS).map(|k| p(k as f64, if k == 0 || k == MAX_POINTS { 0.0 } else { 5.0 })).collect(), SpunError::TooMany);
    let mut nan = stepped();
    nan[2].r = f64::NAN;
    refuse(nan, SpunError::NonFinite(3));
    let mut neg = stepped();
    neg[1].r = -1.0;
    refuse(neg, SpunError::Negative(2));
    let mut below = stepped();
    below[0].z = -1.0;
    refuse(below, SpunError::BelowTip(1));
    let mut open = stepped();
    open[0].r = 3.0;
    refuse(open, SpunError::OpenStart);
    let mut open = stepped();
    open[5].r = 3.0;
    refuse(open, SpunError::OpenEnd);
    let mut pinch = stepped();
    pinch[2].r = 0.0;
    refuse(pinch, SpunError::TouchesAxis(3));
    let mut dup = stepped();
    dup.insert(2, dup[1]);
    refuse(dup, SpunError::Short(2));
    let mut tight = stepped();
    tight[2].arc = 3.0; // chord 8 mm: radius must be >= 4
    refuse(tight, SpunError::ArcTooTight(3));
    let mut back = stepped();
    back[3].z = 50.0;
    refuse(back, SpunError::RunsBack(3));
    refuse(vec![p(5.0, 0.0), p(5.0, 3.0), p(5.0, 0.0)], SpunError::Flat);
    refuse(vec![p(0.0, 0.0), p(0.0, 600.0), p(10.0, 600.0), p(10.0, 0.0)], SpunError::TooLarge);
    // Every refusal reads as a sentence naming the point.
    assert!(SpunError::RunsBack(3).to_string().contains("point 3"));
}

#[test]
fn a_stepped_profile_reads_its_bands_exactly() {
    let s = SpunProfile::new(stepped()).expect("a sane profile");
    let b = s.bands();
    assert_eq!(
        b,
        vec![HolderBand { above: 40.0, len: 20.0, r: 12.0 }, HolderBand { above: 60.0, len: 30.0, r: 20.0 }]
    );
    assert_eq!(s.holder_profile().bands(), &b[..]);
}

/// Slack (mm) for the holder stack comparison: float dust only. A stack
/// read from the spun profile subtracts two heights, so a band length can
/// differ from the cylinder's in the last bits (#3369: a 17.4625 mm flute
/// under a 32 mm shank gives a 25.000000000000007 mm holder band).
const STACK_DUST: f64 = 1e-9;

fn bands_agree(a: &[HolderBand], b: &[HolderBand]) -> bool {
    let near = |x: f64, y: f64| (x - y).abs() <= STACK_DUST;
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| near(p.above, q.above) && near(p.len, q.len) && near(p.r, q.r))
}

#[test]
fn a_parametric_holder_stacks_as_the_cylinder() {
    for (name, t) in catalog() {
        if is_lathe(t.kind) {
            assert!(SpunProfile::holder_of(&t).is_none(), "{name}: turning tools are not spun");
            assert!(SpunProfile::cutter_of(&t).is_none(), "{name}: turning tools are not spun");
            continue;
        }
        let g = t.geom();
        let Some(h) = SpunProfile::holder_of(&t) else {
            assert!(g.holder_len <= 0.0 || g.holder_radius <= 0.0, "{name}: a holder must draw");
            continue;
        };
        let (have, want) = (h.holder_profile().stack(&g), HolderStack::cylinder(&g));
        let same = bands_agree(&have.bands, &want.bands)
            && bands_agree(&have.neck, &want.neck)
            && (have.top - want.top).abs() <= STACK_DUST;
        assert!(same, "{name}: {have:?} is not {want:?}");
    }
}

#[test]
fn parametric_cutter_and_holder_draw_the_tool_profile() {
    for (name, t) in catalog() {
        let (Some(c), Some(h)) = (SpunProfile::cutter_of(&t), SpunProfile::holder_of(&t)) else {
            continue;
        };
        // The tool's own outline is the cutter (without its closing
        // axis point) then the holder (without its opening one).
        let mut joined: Vec<(f64, f64)> = c.outline();
        joined.pop();
        joined.extend(h.outline().into_iter().skip(1));
        assert_eq!(joined, t.profile(), "{name}");
    }
}

#[test]
fn arcs_never_fall_inside_the_drawn_solid() {
    // A convex quarter-round corner and a concave waist: every band must reach the true arc at its heights.
    let quarter = SpunProfile::new(vec![p(0.0, 0.0), SpunPoint { z: 0.0, r: 0.5, arc: 0.0 }, SpunPoint { z: 10.0, r: 10.5, arc: 10.0 }, p(20.0, 10.5), p(20.0, 0.0)])
        .expect("a quarter-round corner");
    // The true arc: centre (10, 0.5), radius 10, r = 0.5 + sqrt(100 - (z - 10)^2).
    for band in quarter.bands().iter().filter(|b| b.above < 10.0) {
        let z_top = (band.above + band.len).min(10.0);
        let truth = 0.5 + (100.0 - (z_top - 10.0).powi(2)).max(0.0).sqrt();
        assert!(band.r >= truth - 1e-9, "band {band:?} under the arc ({truth})");
    }
    let waist = SpunProfile::new(vec![p(0.0, 0.0), p(0.0, 10.0), SpunPoint { z: 10.0, r: 10.0, arc: -8.0 }, p(26.0, 10.0), p(26.0, 0.0)])
        .expect("a concave waist");
    let out = waist.outline();
    // Concave: chords on the arc, so the flattened points sit ON it.
    assert!(out.iter().all(|q| q.1 <= 10.0 + 1e-9), "a concave arc never reaches past its chord");
    assert!(out.iter().any(|q| q.1 < 10.0 - 1.0), "the waist is drawn");
}

#[test]
fn an_envelope_redraws_as_a_profile() {
    let s = SpunProfile::new(stepped()).expect("sane");
    let again = SpunProfile::from_bands(&s.bands()).expect("bands redraw");
    assert_eq!(again.bands(), s.bands());
}

/// Every outline point sits inside a band touching its height.
fn bands_cover_the_outline(s: &SpunProfile) {
    let bands = s.bands();
    for (z, r) in s.outline() {
        let held = bands.iter().any(|b| b.above - 1e-9 <= z && z <= b.above + b.len + 1e-9 && r <= b.r + 1e-9);
        assert!(held, "outline point (z {z}, r {r}) is outside every band: {bands:?}");
    }
    let wide = bands.iter().fold(0.0f64, |m, b| m.max(b.r));
    assert!((wide - s.max_radius()).abs() < 1e-9, "the envelope's widest band {wide} is the outline's {}", s.max_radius());
}

#[test]
fn a_zero_height_spike_stays_in_the_envelope() {
    // At the top: out to r 35 and back to the axis at z 90 (the
    // hands-on finding - the summary said Ø70, the envelope Ø50).
    let top = SpunProfile::new(vec![p(55.0, 0.0), p(55.0, 25.0), p(90.0, 25.0), p(90.0, 35.0), p(90.0, 0.0)]).expect("valid");
    bands_cover_the_outline(&top);
    assert_eq!(top.bands(), vec![HolderBand { above: 55.0, len: 35.0, r: 35.0 }]);
    assert!(top.holder_profile().bands().iter().any(|b| b.r >= 35.0));
    // At the foot: out to r 30 at z 40 before stepping in to r 12.
    let foot = SpunProfile::new(vec![p(40.0, 0.0), p(40.0, 30.0), p(40.0, 12.0), p(60.0, 12.0), p(60.0, 0.0)]).expect("valid");
    bands_cover_the_outline(&foot);
    // Mid-body: a flange drawn out and back at one height.
    let mut mid = stepped();
    mid.insert(3, p(60.0, 28.0));
    let mid = SpunProfile::new(mid).expect("valid");
    bands_cover_the_outline(&mid);
    // A plain step still reads exactly (no band grows past its own).
    bands_cover_the_outline(&SpunProfile::new(stepped()).expect("valid"));
    assert_eq!(SpunProfile::new(stepped()).expect("valid").bands()[0].r, 12.0);
}

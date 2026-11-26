//! #3367: the tool meridian - shapes from each mill kind, the outer and
//! inner bounds of a concave arc, and the named refusals.

use super::{Bound, Meridian, MeridianAtom, MeridianError};
use crate::spun::SpunPoint;
use crate::tools::{Tool, ToolKind};
use crate::undercut::cutter;

fn mill(kind: ToolKind, d: f64, corner_r: f64, tip_angle: f64, fl: f64) -> Tool {
    Tool {
        corner_r,
        ..cutter(kind, d, fl, d * 0.5, 30.0, tip_angle)
    }
}

fn arc(z: f64, r: f64, a: f64) -> SpunPoint {
    SpunPoint { z, r, arc: a }
}

/// The largest atom radius at `z` (the union's radius).
fn union_rho(atoms: &[MeridianAtom], z: f64) -> f64 {
    atoms
        .iter()
        .filter(|a| a.z0 - 1e-12 <= z && z <= a.z1 + 1e-12)
        .fold(0.0, |m, a| m.max(a.rho(z)))
}

#[test]
fn meridian_agrees_with_profile_on_the_cutting_band() {
    let tools = [
        mill(ToolKind::EndMill, 10.0, 0.0, 0.0, 20.0),
        mill(ToolKind::BallNose, 10.0, 5.0, 0.0, 20.0),
        mill(ToolKind::BallNose, 10.0, 5.0, 0.0, 3.0),
        mill(ToolKind::BullNose, 10.0, 2.0, 0.0, 20.0),
        mill(ToolKind::Drill, 8.0, 0.0, 118.0, 30.0),
        mill(ToolKind::Taper, 10.0, 0.0, 90.0, 20.0),
        mill(ToolKind::Taper, 10.0, 0.0, 30.0, 5.0),
        cutter(ToolKind::TSlot, 20.0, 5.0, 10.0, 10.0, 0.0),
        cutter(ToolKind::Keyseat, 25.0, 3.0, 10.0, 15.0, 0.0),
        cutter(ToolKind::Dovetail, 20.0, 5.0, 8.0, 10.0, 60.0),
        cutter(ToolKind::Lollipop, 10.0, 6.0, 4.0, 20.0, 0.0),
    ];
    for t in tools {
        let m = t.meridian().unwrap_or_else(|e| panic!("{}: {e}", t.kind));
        assert!(
            m.cutting().iter().all(|c| *c),
            "{}: every segment of a mill cuts",
            t.kind
        );
        let atoms = m
            .atoms(Bound::Outer, 1e-3)
            .unwrap_or_else(|e| panic!("{}: {e}", t.kind));
        let prof = t.profile();
        for &(z, _) in prof.iter().filter(|p| p.0 > 0.0 && p.0 <= t.flute_len) {
            let want = union_rho(&atoms, z);
            let hit = prof
                .iter()
                .any(|q| (q.0 - z).abs() <= 1e-12 && (q.1 - want).abs() <= 1e-9);
            assert!(
                hit,
                "{}: the meridian reads {want} at z {z}, the profile does not",
                t.kind
            );
        }
    }
}

/// A roundover-style meridian: a non-cutting bearing band, then a
/// concave quarter arc (radius 6, centre (4, 9.5)) that cuts.
fn roundover() -> Meridian {
    let pts = vec![
        SpunPoint::line(0.0, 0.0),
        SpunPoint::line(0.0, 3.5),
        arc(4.0, 3.5, -6.0),
        SpunPoint::line(10.0, 9.5),
    ];
    Meridian::new(pts, vec![false, false, true]).expect("a sound roundover")
}

fn arc_rho(z: f64) -> f64 {
    9.5 - (36.0 - (z - 4.0) * (z - 4.0)).max(0.0).sqrt()
}

/// The nearest and farthest distance from the arc centre (4, 9.5) to a
/// frustum atom's generator segment.
fn centre_distances(a: &MeridianAtom) -> (f64, f64) {
    let (p, q) = ((a.z0, a.rho(a.z0)), (a.z1, a.rho(a.z1)));
    let d = (q.0 - p.0, q.1 - p.1);
    let w = (4.0 - p.0, 9.5 - p.1);
    let t = ((w.0 * d.0 + w.1 * d.1) / (d.0 * d.0 + d.1 * d.1)).clamp(0.0, 1.0);
    let near = (w.0 - t * d.0).hypot(w.1 - t * d.1);
    let far = w.0.hypot(w.1).max((4.0 - q.0).hypot(9.5 - q.1));
    (near, far)
}

#[test]
fn concave_arc_outer_bound_contains_and_inner_bound_stays_inside() {
    let m = roundover();
    for lim in [0.1, 0.01, 0.001] {
        let outer = m.atoms(Bound::Outer, lim).expect("outer");
        let inner = m.atoms(Bound::Inner, lim).expect("inner");
        let (oa, ia): (Vec<_>, Vec<_>) = (
            outer.iter().filter(|a| a.cutting).copied().collect(),
            inner.iter().filter(|a| a.cutting).copied().collect(),
        );
        assert!(oa.len() >= 2 && ia.len() >= 2, "the arc splits");
        // The bearing band is the one non-cutting atom.
        assert_eq!(outer.iter().filter(|a| !a.cutting).count(), 1);
        assert_eq!(inner.iter().filter(|a| !a.cutting).count(), 1);
        for i in 0..=600 {
            let z = 4.0 + 6.0 * f64::from(i) / 600.0;
            let want = arc_rho(z);
            assert!(
                union_rho(&oa, z) >= want - 1e-12,
                "outer under the arc at z {z} (sag {lim})"
            );
            assert!(
                union_rho(&ia, z) <= want + 1e-12,
                "inner over the arc at z {z} (sag {lim})"
            );
        }
        for a in &oa {
            let (near, _) = centre_distances(a);
            assert!(
                6.0 - near <= lim + 1e-12,
                "a chord sags {} over {lim}",
                6.0 - near
            );
        }
        for a in &ia {
            let (_, far) = centre_distances(a);
            assert!(
                far - 6.0 <= lim + 1e-12,
                "a tangent stands off {} over {lim}",
                far - 6.0
            );
        }
    }
    let coarse = m.atoms(Bound::Outer, 0.1).expect("coarse").len();
    let fine = m.atoms(Bound::Outer, 0.001).expect("fine").len();
    assert!(
        fine > coarse,
        "a finer sag limit splits the arc into more atoms"
    );
}

#[test]
fn degenerate_meridians_are_refused_by_name() {
    let l = SpunPoint::line;
    let new = |p: Vec<SpunPoint>| {
        let n = p.len().saturating_sub(1);
        Meridian::new(p, vec![true; n])
    };
    assert_eq!(new(vec![l(0.0, 0.0)]), Err(MeridianError::TooFew));
    assert_eq!(
        new(vec![l(0.0, 0.0), l(0.0, 0.0), l(5.0, 1.0)]),
        Err(MeridianError::Short(1))
    );
    assert_eq!(
        new(vec![l(0.0, 0.0), l(0.0, -1.0), l(5.0, 1.0)]),
        Err(MeridianError::Negative(2))
    );
    assert_eq!(
        new(vec![l(0.0, 0.0), l(f64::NAN, 1.0)]),
        Err(MeridianError::NonFinite(2))
    );
    assert_eq!(
        new(vec![l(0.0, 0.0), arc(0.0, 1.0, f64::INFINITY), l(5.0, 1.0)]),
        Err(MeridianError::NonFinite(2))
    );
    assert_eq!(
        new(vec![l(0.0, 0.0), arc(0.0, 1.0, 1.0), l(5.0, 1.0)]),
        Err(MeridianError::ArcTooTight(2))
    );
    assert_eq!(
        new(vec![l(0.0, 0.0), l(5.0, 1.0), l(4.0, 1.0)]),
        Err(MeridianError::RunsBack(2))
    );
    // A convex arc over a flat chord bulges below the tip.
    assert_eq!(
        new(vec![arc(0.0, 0.0, 3.0), l(0.0, 5.0)]),
        Err(MeridianError::ArcFolds(1))
    );
    // A concave arc that dips through the axis.
    assert_eq!(
        new(vec![arc(0.0, 1.0, -5.0), l(8.0, 1.0)]),
        Err(MeridianError::Negative(1))
    );
    assert_eq!(
        Meridian::new(vec![l(0.0, 0.0), l(5.0, 1.0)], vec![]),
        Err(MeridianError::FlagCount)
    );
    let m = roundover();
    assert_eq!(m.atoms(Bound::Outer, 0.0), Err(MeridianError::BadSag));
    assert_eq!(m.atoms(Bound::Inner, f64::NAN), Err(MeridianError::BadSag));
    assert_eq!(
        m.atoms(Bound::Outer, 1e-12),
        Err(MeridianError::SagTooFine(3))
    );
}

#[test]
fn tool_meridian_refuses_bad_tools_by_name() {
    let tap = mill(ToolKind::Tap, 6.0, 0.0, 0.0, 20.0);
    assert_eq!(tap.meridian(), Err(MeridianError::NotAMill(ToolKind::Tap)));
    let nan = mill(ToolKind::EndMill, f64::NAN, 0.0, 0.0, 20.0);
    assert!(matches!(nan.meridian(), Err(MeridianError::NonFinite(_))));
    let neg = mill(ToolKind::EndMill, -6.0, 0.0, 0.0, 20.0);
    assert!(matches!(neg.meridian(), Err(MeridianError::Negative(_))));
    let flat = mill(ToolKind::EndMill, 0.0, 0.0, 0.0, 20.0);
    assert!(matches!(flat.meridian(), Err(MeridianError::Short(_))));
    let fat = mill(ToolKind::BullNose, 10.0, 6.0, 0.0, 20.0);
    assert!(matches!(fat.meridian(), Err(MeridianError::Unsound(_))));
    let narrow = cutter(ToolKind::Dovetail, 20.0, 5.0, 19.0, 10.0, 60.0);
    assert!(matches!(narrow.meridian(), Err(MeridianError::Unsound(_))));
}

#[test]
fn tool_meridian_checks_inputs_before_shape_fallbacks() {
    for kind in [ToolKind::Drill, ToolKind::Taper] {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for t in [
                mill(kind, bad, 0.0, 118.0, 20.0),
                mill(kind, 10.0, 0.0, 118.0, bad),
                mill(kind, 10.0, 0.0, bad, 20.0),
            ] {
                assert!(
                    matches!(t.meridian(), Err(MeridianError::NonFinite(_))),
                    "{t:?}"
                );
            }
        }
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let t = mill(ToolKind::BullNose, 10.0, bad, 0.0, 20.0);
        assert!(matches!(t.meridian(), Err(MeridianError::NonFinite(_))));
    }
    let t = mill(ToolKind::BullNose, 10.0, -1.0, 0.0, 20.0);
    assert!(matches!(t.meridian(), Err(MeridianError::Negative(_))));
}

#[test]
fn accepted_extreme_meridians_have_finite_correct_radii() {
    let m = Meridian::new(
        vec![SpunPoint::line(0.0, 1.0), SpunPoint::line(1e308, 10.0)],
        vec![true],
    )
    .expect("the segment length is finite");
    let atoms = m.atoms(Bound::Outer, 0.001).expect("line atoms");
    for (z, want) in [(0.0, 1.0), (2.5e307, 3.25), (5e307, 5.5), (1e308, 10.0)] {
        let rho = atoms[0].rho(z);
        assert!(rho.is_finite());
        assert_eq!(rho, want);
    }
    let rad = super::MAX_ARC_COORD;
    let m = Meridian::new(
        vec![arc(0.0, 0.0, rad), SpunPoint::line(rad, rad)],
        vec![true],
    )
    .expect("the circle arithmetic stays in range");
    let atoms = m.atoms(Bound::Outer, 0.001).expect("circle atom");
    for (z, want) in [(0.0, 0.0), (rad * 0.5, 0.75_f64.sqrt()), (rad, 1.0)] {
        let rho = atoms[0].rho(z);
        assert!(rho.is_finite());
        assert!((rho / rad - want).abs() < 1e-12);
    }
}

#[test]
fn meridian_refuses_overflowing_lengths_and_circle_ranges() {
    let l = SpunPoint::line;
    for pts in [
        vec![l(-1e308, 1.0), l(1e308, 2.0)],
        vec![l(0.0, 0.0), l(1.3e308, 1.3e308)],
        vec![arc(0.0, 0.0, 1e200), l(1e200, 1e200)],
        vec![arc(1e200, 1.0, 1.0), l(1e200, 2.0)],
    ] {
        assert_eq!(
            Meridian::new(pts, vec![true]),
            Err(MeridianError::OutOfRange(1))
        );
    }
    let ball = mill(ToolKind::BallNose, 1e200, 0.0, 0.0, 1e199);
    assert_eq!(ball.meridian(), Err(MeridianError::OutOfRange(1)));
}

/// The sharp taper's profile before #3368, kept here as the pin: the
/// apex, the cone to the full radius (or the flute top), the flute side,
/// the shank and holder bands.
fn sharp_profile_before_3368(t: &Tool) -> Vec<(f64, f64)> {
    let r = t.diameter.max(0.0) * 0.5;
    let zc = t.flute_len.max(0.0);
    let zs = zc + t.shank_len.max(0.0);
    let zh = zs + t.holder_len.max(0.0);
    let mut p = vec![(0.0, 0.0)];
    let tan_half = (t.tip_angle.to_radians() * 0.5).tan();
    if tan_half > 0.0 && tan_half.is_finite() {
        let zr = (r / tan_half).min(zc);
        crate::tools::add_pt(&mut p, zr, (zr * tan_half).min(r));
    }
    crate::tools::add_pt(&mut p, zc, r);
    let sr = t.shank_d.max(0.0) * 0.5;
    if zs > zc {
        crate::tools::add_pt(&mut p, zc, sr);
        crate::tools::add_pt(&mut p, zs, sr);
    }
    let hr = t.holder_d.max(0.0) * 0.5;
    if zh > zs {
        crate::tools::add_pt(&mut p, zs, hr);
        crate::tools::add_pt(&mut p, zh, hr);
    }
    crate::tools::add_pt(&mut p, zh, 0.0);
    p
}

/// #3368 (safety floor): a sharp taper - every shipped one, and one that
/// carries an inert corner radius - keeps its profile, its meridian and
/// its shape identity bit for bit. Only a flat or ball tip draws anew.
#[test]
fn a_sharp_taper_keeps_its_profile_and_digest() {
    let mut tapers: Vec<Tool> = crate::tools::catalog()
        .into_iter()
        .map(|(_, t)| t)
        .filter(|t| t.kind == ToolKind::Taper && t.tip_form.is_sharp())
        .collect();
    assert!(tapers.len() >= 7, "the shipped sharp tapers");
    tapers.push(mill(ToolKind::Taper, 10.0, 2.0, 90.0, 20.0));
    tapers.push(mill(ToolKind::Taper, 10.0, 0.0, 30.0, 5.0));
    for t in tapers {
        assert_eq!(t.tip_diameter, 0.0);
        assert_eq!(t.profile(), sharp_profile_before_3368(&t), "{t:?}");
        let tan = (t.tip_angle.to_radians() * 0.5).tan();
        let (r, fl) = (t.diameter * 0.5, t.flute_len);
        let top = (r / tan).min(fl);
        let mut want = vec![SpunPoint::line(0.0, 0.0), SpunPoint::line(top, top * tan)];
        // #3938: a flute that ends on the cone adds the radial shoulder.
        if fl > top || r - top * tan >= super::MIN_LEN {
            want.push(SpunPoint::line(fl, r));
        }
        assert_eq!(t.meridian().map(|m| m.points().to_vec()), Ok(want), "{t:?}");
        // A tip form edit is a new shape; the sharp tool is its own.
        assert!(crate::cutter_fit::same_shape(&t, &t.clone()));
        let flat = Tool { tip_form: crate::taper::TaperTip::Flat, tip_diameter: 1.0, ..t.clone() };
        assert!(!crate::cutter_fit::same_shape(&t, &flat));
    }
}

/// #3938 (safety floor): every mill kind whose profile closes its flute
/// at the full radius ends its meridian there too - a flute shorter than
/// the cone (sharp taper, drill) or the corner (ball, bull) steps out by
/// the radial shoulder #3929 gave the flat and ball tips: a face, no atom.
#[test]
fn a_short_flute_meridian_has_the_profile_shoulder() {
    let tools = [
        mill(ToolKind::Taper, 10.0, 0.0, 30.0, 5.0), // cone reaches r 18.7 mm up
        mill(ToolKind::Drill, 8.0, 0.0, 118.0, 1.5), // point reaches r 2.4 mm up
        mill(ToolKind::BallNose, 10.0, 5.0, 0.0, 3.0),
        mill(ToolKind::BullNose, 10.0, 3.0, 0.0, 2.0),
        mill(ToolKind::EndMill, 10.0, 0.0, 0.0, 0.5),
        cutter(ToolKind::TSlot, 20.0, 5.0, 10.0, 10.0, 0.0),
    ];
    for t in tools {
        let (r, fl) = (t.diameter * 0.5, t.flute_len);
        let prof = t.profile().into_iter().filter(|q| q.0 == fl).fold(0.0, |m: f64, q| m.max(q.1));
        assert_eq!(prof, r, "{t:?}: the profile's flute top");
        let m = t.meridian().unwrap_or_else(|e| panic!("{t:?}: {e}"));
        let last = m.points().last().map(|p| (p.z, p.r));
        assert_eq!(last, Some((fl, prof)), "{t:?}: the meridian ends at {last:?}, the profile at r {prof}");
        if matches!(t.kind, ToolKind::Drill | ToolKind::Taper) {
            let tan = (t.tip_angle.to_radians() * 0.5).tan();
            let cone = m.points().iter().rev().nth(1).map(|p| (p.z, p.r));
            assert_eq!(cone, Some((fl, fl * tan)), "{t:?}: the cone still ends at the flute top");
        }
        let atoms = m.atoms(Bound::Outer, 1e-3).unwrap_or_else(|e| panic!("{t:?}: {e}"));
        assert!(atoms.iter().all(|a| a.z1 - a.z0 > 0.0), "{t:?}: a face is no atom");
    }
}

/// #3368: the flat and ball tips' meridian agrees with the drawn profile
/// on the cutting band, as every other mill's does.
#[test]
fn a_flat_and_a_ball_taper_meridian_agrees_with_the_profile() {
    use crate::taper::TaperTip;
    let tools = [
        Tool { tip_form: TaperTip::Flat, tip_diameter: 1.0, ..mill(ToolKind::Taper, 6.0, 0.0, 90.0, 6.0) },
        Tool { tip_form: TaperTip::Flat, tip_diameter: 4.0, ..mill(ToolKind::Taper, 10.0, 0.5, 60.0, 15.0) },
        Tool { tip_form: TaperTip::Ball, ..mill(ToolKind::Taper, 1.7, 0.5, 2.0, 20.0) },
        Tool { tip_form: TaperTip::Ball, ..mill(ToolKind::Taper, 6.0, 1.0, 90.0, 8.0) },
        // Short flutes that end on the cone (#3929 review): the radial shoulder.
        Tool { tip_form: TaperTip::Flat, tip_diameter: 1.0, ..mill(ToolKind::Taper, 6.0, 0.0, 90.0, 1.0) },
        Tool { tip_form: TaperTip::Ball, ..mill(ToolKind::Taper, 6.0, 0.5, 90.0, 1.0) },
    ];
    for t in tools {
        let m = t.meridian().unwrap_or_else(|e| panic!("{t:?}: {e}"));
        let atoms = m.atoms(Bound::Outer, 1e-3).unwrap_or_else(|e| panic!("{t:?}: {e}"));
        let prof = t.profile();
        for &(z, _) in prof.iter().filter(|p| p.0 > 0.0 && p.0 <= t.flute_len) {
            let want = union_rho(&atoms, z);
            let hit = prof.iter().any(|q| (q.0 - z).abs() <= 1e-12 && (q.1 - want).abs() <= 1e-9);
            assert!(hit, "{t:?}: the meridian reads {want} at z {z}, the profile does not");
        }
    }
}

//! The per-ruling widening's laws (#1170): a punch stands at the kerf
//! offset on its flat flanks and is widened only where its surface
//! twists; the uniform widening holds every ruling at the worst one's;
//! a twist past the tolerance is refused where it stands.
//!
//! The wire, gap and machine are SYNTHETIC (#290 D4, D10).

use super::ring::pt_ring;
use super::ruled_tests::{cut, rect, refusal, round, ruled, rulings, sync, H, K};
use super::*;

/// How far past the kerf offset each ruling's lower end stands off the
/// lower profile, with the ruling.
fn standing(path: &Toolpath, lower: &Loop) -> Vec<(f64, (Point2, Point2))> {
    rulings(path).into_iter().map(|r| (pt_ring(r.0, lower.points()) - K, r)).collect()
}

/// What a flat flank leaning as this ruling does needs past the kerf
/// offset on the planes: `K / cos(lean) - K` (#536's tapered cut).
fn lean_need((l, u): (Point2, Point2)) -> f64 {
    K / (u.x - l.x).hypot(u.y - l.y).atan2(H).cos() - K
}

/// Done means: a square-to-round punch stands at the kerf offset on its
/// straight stretches and is widened only at the twist. By arc length
/// the middle of each side does not twist (the round's tangent is the
/// square's there): it stands where a plain taper of its lean stands.
/// Toward the corners the twist grows, and the widening with it. The
/// uniform widening holds every ruling at the worst one's.
#[test]
fn a_square_to_round_punch_is_widened_only_where_it_twists() {
    let (sq, rd) = (rect(10.0, 10.0), round(8.0, 64));
    let per = cut(&rd, &sq, &ruled(WirePairing::ArcLength)).expect("per ruling");
    let uni = cut(&rd, &sq, &WireRuled { widening: WireWidening::Uniform, ..ruled(WirePairing::ArcLength) }).expect("uniform");
    let (per_r, uni_r) = (standing(&per, &sq), standing(&uni, &sq));
    assert_eq!(per_r.len(), uni_r.len(), "the same rulings, widened differently");
    // The middle of each side: the lean's own need, no twist's.
    let mid: Vec<_> = per_r.iter().filter(|(_, (l, _))| l.x.abs() < 1e-9 || l.y.abs() < 1e-9).collect();
    assert!(mid.len() >= 4, "a ruling at the middle of each side: {}", mid.len());
    for (more, r) in &mid {
        assert!((more - lean_need(*r)).abs() < 2e-4, "a side's middle stands as a plain taper does: {more} vs {}", lean_need(*r));
        assert!(*more < 0.003, "{more}");
    }
    // The last ruling before each corner: the twist's widening.
    let near: Vec<_> = per_r.iter().filter(|(_, (l, _))| l.x.abs().min(l.y.abs()) > 8.5 && l.x.abs().min(l.y.abs()) < 9.5).collect();
    assert!(near.len() >= 8, "{}", near.len());
    assert!(near.iter().all(|(more, _)| *more > 0.015 && *more <= 0.05), "{near:?}");
    // Never nearer than the kerf offset, on either plane.
    assert!(per_r.iter().all(|(more, (_, u))| *more >= 0.0 && pt_ring(*u, rd.points()) >= K - 1e-9));
    // Uniform: every ruling stands at the worst one's widening - seven
    // times the middle's.
    let worst = uni_r.iter().map(|u| u.0).fold(f64::INFINITY, f64::min);
    assert!((worst - 0.0207).abs() < 2e-4, "{worst}");
    assert!(per_r.iter().zip(&uni_r).all(|(p, u)| p.0 <= u.0 + 1e-6), "per ruling never stands farther than uniform");
    let (said, plain) = (per.notes.join("\n"), uni.notes.join("\n"));
    assert!(said.contains("each ruling is held off both profiles") && said.contains("stands on the profile planes"), "{said}");
    assert!(said.contains("at most 0.0207 mm more"), "{said}");
    assert!(plain.contains("the wire is held 0.1857 mm off both profiles"), "{plain}");
}

/// A punch with three upright flat flanks and one twisted: the lower
/// profile a rectangle, the upper the same with one corner drawn in.
fn one_twisted_flank() -> (Loop, Loop, WireRuled) {
    let lo = rect(20.0, 10.0);
    let up = Loop::new(vec![Point2::new(-20.0, -10.0), Point2::new(20.0, -10.0), Point2::new(10.0, 10.0), Point2::new(-20.0, 10.0)])
        .unwrap();
    let pairs = sync(&[
        ((-20.0, -10.0), (-20.0, -10.0)),
        ((20.0, -10.0), (20.0, -10.0)),
        ((10.0, 10.0), (20.0, 10.0)),
        ((-20.0, 10.0), (-20.0, 10.0)),
    ]);
    (up, lo, WireRuled { max_deg: Some(45.0), ..ruled(pairs) })
}

/// Done means: the flat flanks stand at the kerf offset, to the wire's
/// own arithmetic; only the twisted flank is widened, and the note says
/// how many rulings stand at the kerf offset.
#[test]
fn only_the_twisted_flank_of_a_punch_is_widened() {
    let (up, lo, r) = one_twisted_flank();
    let path = cut(&up, &lo, &r).expect("per ruling");
    let read = standing(&path, &lo);
    // The flat flanks: left (x = -20), bottom (y = -10) and top
    // (y = 10), away from the twisted flank's two corners.
    let flat: Vec<_> = read.iter().filter(|(_, (l, _))| l.x < 19.5 && (l.x < -20.0 || l.y.abs() > 10.0)).collect();
    assert!(flat.len() >= 8, "{}", flat.len());
    for (more, (l, u)) in &flat {
        let corner = l.x < -20.0 && l.y.abs() > 10.0;
        let want = if corner { K * (2f64.sqrt() - 1.0) } else { 0.0 };
        assert!((more - want).abs() < 1e-6, "a flat flank stands at the kerf offset: {more} at {l:?}");
        assert!((pt_ring(*u, up.points()) - K - want).abs() < 1e-6, "and on the upper plane: {u:?}");
    }
    // The twisted flank (x = 20 below, drawn in above): widened.
    let twisted: Vec<_> = read.iter().filter(|(_, (l, _))| l.x > 20.0 && l.y.abs() < 9.5).collect();
    assert!(!twisted.is_empty());
    assert!(twisted.iter().all(|(more, _)| *more > 0.01 && *more <= 0.05 + 1e-9), "{twisted:?}");
    let said = path.notes.join("\n");
    assert!(said.contains("9 of 13 rulings stand within 0.0001 mm of the kerf offset"), "{said}");
    // The same punch under a tolerance its twist needs more than: the
    // refusal names the twisted flank, not a flat one.
    let why = refusal(&up, &lo, &WireRuled { tolerance: 0.02, ..r });
    assert!(why.contains("twists the surface too far for a straight wire near X20.") && why.contains("tolerance is 0.0200 mm"), "{why}");
}

/// A transition ruling is laid a millimetre in from each end of a long
/// span and dropped again where the widening is constant: a flat taper
/// through the per-ruling path adds no move (`ruled_tests` reads it
/// against #536's cut, move for move).
#[test]
fn a_twisted_flank_ends_its_widening_at_the_transition_rulings() {
    let (up, lo, r) = one_twisted_flank();
    let read = standing(&cut(&up, &lo, &r).expect("per ruling"), &lo);
    // On the twisted flank the two transitions carry different
    // widenings: the flank's twist grows with the upper profile's draw.
    let on: Vec<f64> = read.iter().filter(|(_, (l, _))| l.x > 20.0 && l.y.abs() < 9.5).map(|r| r.0).collect();
    assert_eq!(on.len(), 2, "{on:?}");
    assert!((on[0] - on[1]).abs() > 0.01, "{on:?}");
    // Beyond a corner of the twisted flank, the next flat ruling is one
    // transition (1 mm) away and already at the kerf offset.
    let next = read.iter().find(|(_, (l, _))| l.y < -10.0 && l.x > 19.0 && l.x < 19.5).expect("the bottom flank's transition");
    assert!(next.0.abs() < 1e-6, "{next:?}");
}

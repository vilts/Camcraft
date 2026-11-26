//! #3368: the taper tip - the tip numbers of the #895 research, the sanity
//! rules by name, and the drawn profile and meridian of a flat and a ball
//! tip.

use super::{TaperTip, is_unread_tip, refusal, tip};
use crate::tools::{Tool, ToolKind, catalog};
use crate::undercut::cutter;

/// A taper of diameter `d`, included cone angle `a`, flute `fl`.
fn taper(d: f64, a: f64, fl: f64, form: TaperTip, td: f64, cr: f64) -> Tool {
    Tool { corner_r: cr, tip_form: form, tip_diameter: td, ..cutter(ToolKind::Taper, d, fl, d, 30.0, a) }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn a_square_flat_tip_starts_the_cone_at_its_own_radius() {
    let t = taper(6.0, 90.0, 6.0, TaperTip::Flat, 1.0, 0.0);
    let g = tip(&t).expect("a flat tip");
    assert!(near(g.r0, 0.5) && near(g.r_flat, 0.5) && near(g.r_t, 0.5) && near(g.z_t, 0.0), "{g:?}");
    // The cone reaches the 3 mm radius 2.5 mm up a 90-degree cone.
    assert!(near(g.cone_z(3.0), 2.5), "{g:?}");
    assert_eq!(t.why_not_sane(), None);
}

#[test]
fn a_rounded_flat_tip_keeps_the_cone_line_through_r0() {
    // theta = 30 degrees: r_flat = r0 - cr (1 - sin) / cos, r_t = r_flat + cr cos.
    let t = taper(10.0, 60.0, 15.0, TaperTip::Flat, 4.0, 0.5);
    let g = tip(&t).expect("a rounded flat tip");
    let (s, c) = (0.5_f64, 3.0_f64.sqrt() * 0.5);
    assert!(near(g.r_flat, 2.0 - 0.5 * (1.0 - s) / c), "{g:?}");
    assert!(near(g.r_t, g.r_flat + 0.5 * c), "{g:?}");
    assert!(near(g.z_t, 0.5 * (1.0 - s)), "{g:?}");
    // The tangent point is on the cone line r = r0 + z tan(theta).
    assert!(near(g.r_t, g.r0 + g.z_t * g.tan_half), "{g:?}");
    assert_eq!(t.why_not_sane(), None);
}

#[test]
fn a_ball_tip_is_tangent_to_the_cone() {
    let t = taper(1.7, 2.0, 20.0, TaperTip::Ball, 0.0, 0.5);
    let g = tip(&t).expect("a ball tip");
    let (s, c) = (1.0_f64.to_radians().sin(), 1.0_f64.to_radians().cos());
    assert!(near(g.r_flat, 0.0) && near(g.r_t, 0.5 * c) && near(g.z_t, 0.5 * (1.0 - s)), "{g:?}");
    assert!(near(g.r0, 0.5 * (1.0 - s) / c), "{g:?}");
    assert!(near(g.r_t, g.r0 + g.z_t * g.tan_half), "the tangent point is on the cone: {g:?}");
    // The tangent point is on the ball (centre on the axis, corner_r up).
    assert!(near(g.r_t.hypot(0.5 - g.z_t), 0.5), "{g:?}");
    assert_eq!(t.why_not_sane(), None);
    assert_eq!(Tool { tip_diameter: 1.0, ..t.clone() }.why_not_sane(), None, "a tip diameter of twice the ball radius");
}

#[test]
fn a_sharp_taper_has_no_tip_and_is_not_refused() {
    let t = taper(6.0, 90.0, 6.0, TaperTip::Sharp, 0.0, 0.0);
    assert_eq!(tip(&t), None);
    assert!(!is_unread_tip(&t));
    assert_eq!(refusal(&t), None);
    // An inert corner radius on a sharp taper loads as before.
    assert_eq!(refusal(&Tool { corner_r: 1.0, ..t }), None);
}

#[test]
fn bad_tip_numbers_are_refused_by_name() {
    let flat = taper(6.0, 90.0, 6.0, TaperTip::Flat, 1.0, 0.0);
    let why = |t: Tool| t.why_not_sane().unwrap_or_default();
    assert!(why(Tool { tip_diameter: f64::NAN, ..flat.clone() }).contains("tip diameter"));
    assert!(why(Tool { tip_diameter: -1.0, ..flat.clone() }).contains("negative"));
    assert!(why(Tool { tip_diameter: 0.0, ..flat.clone() }).contains("must be over 0"));
    assert!(why(Tool { tip_diameter: 6.0, ..flat.clone() }).contains("under the 6 mm diameter"));
    assert!(why(Tool { corner_r: 2.0, ..flat.clone() }).contains("does not fit"));
    assert!(why(Tool { tip_form: TaperTip::Sharp, ..flat.clone() }).contains("sharp chamfer mill has no tip diameter"));
    let ball = taper(6.0, 90.0, 6.0, TaperTip::Ball, 0.0, 1.0);
    assert!(why(Tool { corner_r: 0.0, ..ball.clone() }).contains("ball radius"));
    assert!(why(Tool { tip_diameter: 1.0, ..ball.clone() }).contains("twice its"));
    // A rounded flat tip that meets the cone at or past the diameter.
    assert!(why(Tool { tip_diameter: 5.9, corner_r: 1.0, ..flat.clone() }).contains("meets the cone"));
    assert!(why(Tool { flute_len: 0.2, ..ball }).contains("flute length"));
    // Only a taper states a tip.
    let em = cutter(ToolKind::EndMill, 6.0, 15.0, 6.0, 30.0, 0.0);
    assert!(why(Tool { tip_form: TaperTip::Flat, ..em.clone() }).contains("only a chamfer mill"));
    assert!(why(Tool { tip_diameter: 1.0, ..em }).contains("only a chamfer mill"));
}

#[test]
fn a_flat_and_a_ball_tip_draw_their_face_arc_and_cone() {
    let flat = taper(10.0, 60.0, 15.0, TaperTip::Flat, 4.0, 0.5);
    let g = tip(&flat).expect("tip");
    let p = flat.profile();
    assert_eq!(p[0], (0.0, 0.0));
    assert!(near(p[1].0, 0.0) && near(p[1].1, g.r_flat), "the flat face: {p:?}");
    assert!(p.iter().any(|q| near(q.0, g.z_t) && near(q.1, g.r_t)), "the tangent point: {p:?}");
    let zc = g.cone_z(5.0);
    assert!(p.iter().any(|q| near(q.0, zc) && near(q.1, 5.0)), "the cone meets the diameter: {p:?}");
    // Every arc point is on the corner circle.
    for q in p.iter().filter(|q| q.0 > 0.0 && q.0 < g.z_t) {
        assert!(near((q.1 - g.r_flat).hypot(q.0 - g.corner_r), g.corner_r), "{q:?}");
    }
    let ball = taper(1.7, 2.0, 20.0, TaperTip::Ball, 0.0, 0.5);
    let p = ball.profile();
    assert_eq!(p[0], (0.0, 0.0));
    assert!(p[1].0 > 0.0, "a ball tip has no flat face: {p:?}");
    for w in p.windows(2).filter(|w| w[1].0 <= ball.flute_len) {
        assert!(w[1].0 >= w[0].0 && w[1].1 >= w[0].1 - 1e-12, "the tip only widens upward: {w:?}");
    }
}

#[test]
fn the_meridian_of_a_flat_and_a_ball_tip_is_one_arc_and_the_cone() {
    let flat = taper(10.0, 60.0, 15.0, TaperTip::Flat, 4.0, 0.5);
    let g = tip(&flat).expect("tip");
    let m = flat.meridian().expect("a flat-tip meridian");
    let pts = m.points();
    assert_eq!((pts[0].z, pts[0].r), (0.0, 0.0));
    assert!(near(pts[1].r, g.r_flat) && near(pts[1].arc, 0.5), "{pts:?}");
    assert!(near(pts[2].z, g.z_t) && near(pts[2].r, g.r_t), "{pts:?}");
    let ball = taper(1.7, 2.0, 20.0, TaperTip::Ball, 0.0, 0.5);
    let m = ball.meridian().expect("a ball-tip meridian");
    assert!(near(m.points()[0].arc, 0.5), "the ball starts on the axis: {:?}", m.points());
    // A tip that breaks a rule is refused by name, not drawn.
    let bad = Tool { corner_r: 3.0, ..ball };
    assert!(bad.meridian().is_err());
}

#[test]
fn the_catalog_ships_a_flat_a_ball_and_a_flat_tip_chamfer_taper() {
    let forms: Vec<TaperTip> = catalog().into_iter().filter(|(_, t)| is_unread_tip(t)).map(|(_, t)| t.tip_form).collect();
    assert_eq!(forms.iter().filter(|f| **f == TaperTip::Flat).count(), 2, "{forms:?}");
    assert_eq!(forms.iter().filter(|f| **f == TaperTip::Ball).count(), 1, "{forms:?}");
    for (name, t) in catalog().into_iter().filter(|(_, t)| is_unread_tip(t)) {
        assert_eq!(t.why_not_sane(), None, "{name}");
        assert!(t.meridian().is_ok(), "{name}");
    }
}

#[test]
fn the_words_round_trip() {
    for f in TaperTip::ALL {
        assert_eq!(TaperTip::from_word(f.word()), Some(f));
        assert_eq!(serde_json::to_value(f).ok(), Some(serde_json::json!(f.word())));
    }
    assert_eq!(TaperTip::from_word("round"), None);
    assert_eq!(TaperTip::default(), TaperTip::Sharp);
}

/// The widest radius the drawn profile reaches at the flute top, and the
/// last point of the meridian: a short flute ends on the cone, and both
/// must step out to the full radius there.
fn flute_top_radii(t: &Tool) -> (f64, (f64, f64)) {
    let fl = t.flute_len;
    let prof = t.profile().into_iter().filter(|q| near(q.0, fl)).fold(0.0, |m: f64, q| m.max(q.1));
    let m = t.meridian().unwrap_or_else(|e| panic!("{t:?}: {e}"));
    let last = m.points().last().map(|p| (p.z, p.r)).unwrap_or_default();
    (prof, last)
}

#[test]
fn short_flat_profile_and_meridian_agree() {
    // The cone of a 1 mm flat tip reaches the 3 mm radius 2.5 mm up; the
    // flute ends 1 mm up, on the cone.
    let t = taper(6.0, 90.0, 1.0, TaperTip::Flat, 1.0, 0.0);
    assert_eq!(t.why_not_sane(), None);
    let (prof, last) = flute_top_radii(&t);
    assert!(near(prof, 3.0), "the profile's flute top: {prof}");
    assert!(near(last.0, 1.0) && near(last.1, prof), "the meridian ends at {last:?}, the profile at r {prof}");
}

#[test]
fn short_ball_profile_and_meridian_agree() {
    let t = taper(6.0, 90.0, 1.0, TaperTip::Ball, 0.0, 0.5);
    assert_eq!(t.why_not_sane(), None);
    let (prof, last) = flute_top_radii(&t);
    assert!(near(prof, 3.0), "the profile's flute top: {prof}");
    assert!(near(last.0, 1.0) && near(last.1, prof), "the meridian ends at {last:?}, the profile at r {prof}");
}

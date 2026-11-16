//! Unit tests of the parametric cutter edit (#93 slice 3): a drawn
//! outline carried onto new numbers still agrees with them for every
//! catalog milling kind, a hand-drawn neck keeps its shape, and the neck
//! parameter round-trips and is refused by name when it cannot fit.

use super::*;
use crate::cutter_profile::check;
use crate::tools::{ToolKind, catalog};

fn p(z: f64, r: f64) -> SpunPoint {
    SpunPoint::line(z, r)
}

fn em10() -> Tool {
    catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).unwrap()
}

/// EM-1000-4FL (Ø10 x 22 flutes, Ø10 x 40 shank) with a Ø8 neck from
/// 22 to 30.
fn necked_by_hand() -> Vec<SpunPoint> {
    vec![p(0.0, 0.0), p(0.0, 5.0), p(22.0, 5.0), p(22.0, 4.0), p(30.0, 4.0), p(30.0, 5.0), p(62.0, 5.0), p(62.0, 0.0)]
}

#[test]
fn a_drawn_cutter_follows_every_shape_number_and_keeps_agreeing() {
    let old = em10();
    let edits = [
        Tool { diameter: 12.0, ..old.clone() },
        Tool { diameter: 8.5, ..old.clone() },
        Tool { flute_len: 30.0, ..old.clone() },
        Tool { flute_len: 15.0, ..old.clone() },
        Tool { shank_len: 55.0, ..old.clone() },
        Tool { shank_d: 12.0, ..old.clone() },
        Tool { kind: ToolKind::BallNose, corner_r: 5.0, ..old.clone() },
        Tool { kind: ToolKind::BullNose, corner_r: 1.5, ..old.clone() },
        Tool { kind: ToolKind::Drill, tip_angle: 118.0, ..old.clone() },
        Tool { kind: ToolKind::Taper, tip_angle: 90.0, ..old.clone() },
    ];
    for new in edits {
        let fitted = refit(&old, &new, &necked_by_hand());
        let s = check(&new, fitted.clone()).unwrap_or_else(|e| panic!("{new:?}: {e} - {fitted:?}"));
        // The neck survives: a Ø8 band 8 mm long right above the flutes.
        let zc = new.flute_len;
        assert!(s.points().iter().any(|q| (q.z - (zc + 8.0)).abs() < 1e-9 && (q.r - 4.0).abs() < 1e-9), "{new:?}: {fitted:?}");
        // The top is the new shank top, at the new shank radius.
        let top = s.points().last().map(|q| q.z).unwrap();
        assert!((top - (zc + new.shank_len)).abs() < 1e-9);
        assert!(s.points().iter().any(|q| (q.z - top).abs() < 1e-9 && (q.r - new.shank_d * 0.5).abs() < 1e-9));
    }
}

#[test]
fn unchanged_numbers_leave_the_drawing_alone_and_a_narrower_shank_clamps_it() {
    let t = em10();
    assert_eq!(refit(&t, &Tool { rpm: 1.0, feed: 2.0, ..t.clone() }, &necked_by_hand()), necked_by_hand());
    // A Ø6 shank: the Ø8 neck can no longer stand above the flutes.
    let thin = Tool { shank_d: 6.0, ..t.clone() };
    let fitted = refit(&t, &thin, &necked_by_hand());
    assert!(check(&thin, fitted.clone()).is_ok(), "{fitted:?}");
    assert!(fitted.iter().filter(|q| q.z > 22.0 + 1e-6).all(|q| q.r <= 3.0 + 1e-9), "{fitted:?}");
}

#[test]
fn every_milling_catalog_tool_carries_its_own_outline_onto_new_numbers() {
    for (name, t) in catalog() {
        let pts = parametric(&t);
        if pts.is_empty() {
            continue;
        }
        let new = Tool { diameter: t.diameter * 1.2, flute_len: t.flute_len + 3.0, shank_len: t.shank_len + 5.0, ..t.clone() };
        let fitted = refit(&t, &new, &pts);
        assert!(check(&new, fitted.clone()).is_ok(), "{name}: {fitted:?}");
    }
}

#[test]
fn a_neck_round_trips_and_is_refused_by_name_when_it_cannot_fit() {
    let t = em10();
    let pts = necked(&t, Neck { d: 8.0, len: 8.0 }).unwrap();
    assert_eq!(pts, necked_by_hand());
    assert!(check(&t, pts.clone()).is_ok());
    assert_eq!(neck_of(&t, &pts), Some(Neck { d: 8.0, len: 8.0 }));
    assert_eq!(neck_of(&t, &parametric(&t)), None, "the numbers' own outline has no neck");
    let mut odd = pts.clone();
    odd[4].arc = 3.0;
    assert_eq!(neck_of(&t, &odd), None, "a hand-shaped neck is not the parameter's");
    // A neck follows a number edit through `refit` and is read back.
    let longer = Tool { flute_len: 25.0, ..t.clone() };
    assert_eq!(neck_of(&longer, &refit(&t, &longer, &pts)), Some(Neck { d: 8.0, len: 8.0 }));
    assert_eq!(necked(&t, Neck { d: 10.0, len: 8.0 }), Err(NeckError::Diameter { d: 10.0, shank: 10.0 }));
    assert_eq!(necked(&t, Neck { d: 0.0, len: 8.0 }), Err(NeckError::Diameter { d: 0.0, shank: 10.0 }));
    assert_eq!(necked(&t, Neck { d: 8.0, len: 40.0 }), Err(NeckError::Length { len: 40.0, shank: 40.0 }));
    assert!(matches!(necked(&t, Neck { d: 8.0, len: f64::NAN }), Err(NeckError::Length { .. })));
    let lathe = Tool { kind: ToolKind::Turning, ..t };
    assert_eq!(necked(&lathe, Neck { d: 8.0, len: 8.0 }), Err(NeckError::NoCutter));
    assert!(NeckError::Diameter { d: 10.0, shank: 10.0 }.to_string().contains("below the shank's Ø 10.000"));
}

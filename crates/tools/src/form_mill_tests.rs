//! #3369: the form mill - each preset writes a valid outline whose
//! cutting radius is its catalogue numbers', the pilot does not cut, an
//! undercut and a disagreeing outline are refused by name, and the side
//! view never falls inside the form.

use super::*;
use crate::meridian::Bound;
use crate::undercut::cutter;

/// A form mill of `preset` over a pilot of `pilot_len`, settled.
fn mill(preset: FormPreset, pilot_len: f64) -> Tool {
    let mut t = cutter(ToolKind::FormMill, 1.0, 1.0, 12.7, 30.0, 0.0);
    t.form = Some(FormSpec { preset: Some(preset), pilot_len, points: Vec::new() });
    settle(&mut t);
    t
}

/// The widest radius the meridian's cutting segments reach.
fn cutting_radius(t: &Tool) -> f64 {
    let m = t.meridian().unwrap_or_else(|e| panic!("{}: {e}", t.kind));
    m.points().windows(2).zip(m.cutting()).filter(|(_, c)| **c).fold(0.0_f64, |r, (w, _)| r.max(w[0].r).max(w[1].r))
}

#[test]
fn each_preset_meets_its_catalogue_radius() {
    // (preset, pilot length, cutting radius, flute length): the radius is
    // the pilot radius plus the form across the axis.
    let cases = [
        (FormPreset::Roundover { radius: 6.35, pilot_radius: 6.35, height: 12.7 }, 0.1875 * 25.4, 12.7, 0.6875 * 25.4),
        (FormPreset::Roundover { radius: 6.0, pilot_radius: 6.5, height: 6.0 }, 0.0, 12.5, 6.0),
        (FormPreset::Cove { radius: 6.0, pilot_radius: 6.5, height: 13.0 }, 5.0, 12.5, 18.0),
        (FormPreset::Ogee { radius: 4.0, radius2: 3.0, step: 1.5, pilot_radius: 6.35, height: 12.7 }, 5.0, 14.85, 17.7),
        (FormPreset::VGroove { angle: 90.0, tip_radius: 0.0, diameter: 12.7, height: 6.35 }, 0.0, 6.35, 6.35),
        (FormPreset::VGroove { angle: 60.0, tip_radius: 0.5, diameter: 12.0, height: 15.0 }, 0.0, 6.0, 15.0),
        (FormPreset::Bead { radius: 6.35, step: 1.6, pilot_radius: 6.35, height: 9.5 }, 5.0, 14.3, 14.5),
    ];
    for (preset, pilot, r, fl) in cases {
        let t = mill(preset, pilot);
        assert_eq!(t.why_not_sane(), None, "{}: {t:?}", preset.name());
        assert!((cutting_radius(&t) - r).abs() < 1e-9, "{}: cuts at {} not {r}", preset.name(), cutting_radius(&t));
        assert!((t.diameter - 2.0 * r).abs() < 1e-9, "{}: diameter {}", preset.name(), t.diameter);
        assert!((t.flute_len - fl).abs() < 1e-9, "{}: flute length {}", preset.name(), t.flute_len);
        // The solid atoms of the meridian build without a refusal.
        let m = t.meridian().unwrap_or_else(|e| panic!("{e}"));
        assert!(m.atoms(Bound::Outer, 1e-3).is_ok(), "{}", preset.name());
    }
}

#[test]
fn the_pilot_band_does_not_cut() {
    let t = mill(FormPreset::Roundover { radius: 6.35, pilot_radius: 6.35, height: 12.7 }, 0.1875 * 25.4);
    let m = t.meridian().unwrap_or_else(|e| panic!("{e}"));
    // The tip face and the bearing's side are the pilot; the arc and the
    // body above it cut.
    assert_eq!(m.cutting(), &[false, false, true, true]);
    assert!(m.points()[1..3].iter().all(|p| (p.r - 6.35).abs() < 1e-12), "the bearing is a band of its own radius");
    let none = mill(FormPreset::Roundover { radius: 6.35, pilot_radius: 6.35, height: 12.7 }, 0.0);
    assert!(none.meridian().map(|m| m.cutting().iter().all(|c| *c)).unwrap_or(false), "no pilot: every segment cuts");
}

#[test]
fn bad_preset_numbers_are_refused_by_name() {
    let why = |p: FormPreset, pilot: f64| refusal(&mill(p, pilot)).unwrap_or_default();
    assert!(why(FormPreset::Roundover { radius: 6.0, pilot_radius: 6.0, height: 3.0 }, 0.0).contains("cutting height 3 mm is under"));
    assert!(why(FormPreset::Cove { radius: 0.0, pilot_radius: 6.0, height: 9.0 }, 0.0).contains("radius 0 must be over 0"));
    assert!(why(FormPreset::Bead { radius: 6.0, step: 1.0, pilot_radius: 0.0, height: 9.0 }, 0.0).contains("pilot radius 0 mm"));
    assert!(why(FormPreset::VGroove { angle: 90.0, tip_radius: 0.0, diameter: 12.0, height: 6.0 }, 2.0).contains("no pilot bearing"));
    assert!(why(FormPreset::VGroove { angle: 180.0, tip_radius: 0.0, diameter: 12.0, height: 6.0 }, 0.0).contains("included angle"));
    assert!(why(FormPreset::VGroove { angle: 90.0, tip_radius: 9.0, diameter: 12.0, height: 9.0 }, 0.0).contains("tip radius 9 mm"));
    assert!(why(FormPreset::Ogee { radius: f64::NAN, radius2: 3.0, step: 1.0, pilot_radius: 6.0, height: 9.0 }, 0.0).contains("finite"));
}

/// A drawn outline: on the axis, out to 5, up, then the radius narrows -
/// an undercut, refused by name and left to #896.
#[test]
fn an_undercut_outline_is_refused_by_name() {
    let mut t = cutter(ToolKind::FormMill, 12.0, 10.0, 6.0, 30.0, 0.0);
    let drawn = |pts: Vec<[f64; 3]>| FormSpec { preset: None, pilot_len: 0.0, points: pts };
    t.form = Some(drawn(vec![[0.0, 0.0, 0.0], [0.0, 6.0, 0.0], [5.0, 6.0, 0.0], [10.0, 4.0, 0.0]]));
    let why = refusal(&t).unwrap_or_default();
    assert!(why.contains("narrows upward from point 3") && why.contains("#896"), "{why}");
    // A convex arc past its widest point narrows too (a half bead).
    t.form = Some(drawn(vec![[0.0, 0.0, 0.0], [0.0, 3.0, 3.0], [6.0, 3.0, 0.0]]));
    t.diameter = 12.0;
    t.flute_len = 6.0;
    assert!(refusal(&t).unwrap_or_default().contains("narrows upward from point 2"));
    // A sound drawn outline passes.
    t.form = Some(drawn(vec![[0.0, 0.0, 0.0], [0.0, 3.0, 3.0], [3.0, 6.0, 0.0], [10.0, 6.0, 0.0]]));
    t.flute_len = 10.0;
    assert_eq!(t.why_not_sane(), None);
}

#[test]
fn the_outline_owns_the_diameter_and_the_kind_owns_the_form() {
    let mut t = mill(FormPreset::Cove { radius: 6.0, pilot_radius: 6.5, height: 13.0 }, 5.0);
    t.diameter = 24.0;
    assert!(refusal(&t).unwrap_or_default().contains("25.0000 mm and 18.0000 mm, not 24 mm"));
    t.diameter = 25.0;
    if let Some(f) = t.form.as_mut() {
        f.points[3][1] += 0.5;
    }
    assert!(refusal(&t).unwrap_or_default().contains("not the one its Cove numbers write"));
    let mut em = cutter(ToolKind::EndMill, 6.0, 20.0, 6.0, 30.0, 0.0);
    em.form = mill(FormPreset::starter(0), 5.0).form;
    assert!(em.why_not_sane().unwrap_or_default().contains("only a form mill has a form outline"));
    let bare = cutter(ToolKind::FormMill, 6.0, 20.0, 6.0, 30.0, 0.0);
    assert!(bare.why_not_sane().unwrap_or_default().contains("needs a form outline"));
    assert!(bare.meridian().is_err(), "no outline, no meridian");
}

#[test]
fn the_side_view_never_falls_inside_the_form() {
    for i in 0..FormPreset::NAMES.len() {
        let pilot = if i == 3 { 0.0 } else { 5.0 };
        let t = mill(FormPreset::starter(i), pilot);
        let prof = t.profile();
        let m = t.meridian().unwrap_or_else(|e| panic!("{e}"));
        let atoms = m.atoms(Bound::Outer, 1e-4).unwrap_or_else(|e| panic!("{e}"));
        // At each profile vertex inside the flutes, the profile's radius
        // is at least the form's (a convex arc is drawn by its tangent
        // polygon, a concave one by its chords).
        for &(z, _) in prof.iter().filter(|p| p.0 > 1e-9 && p.0 < t.flute_len - 1e-9) {
            // A flat step has two vertices at one z: the wider one counts.
            let r = prof.iter().filter(|q| (q.0 - z).abs() < 1e-12).fold(0.0_f64, |m, q| m.max(q.1));
            let want = atoms.iter().filter(|a| a.z0 - 1e-12 <= z && z <= a.z1 + 1e-12).fold(0.0_f64, |m, a| m.max(a.rho(z)));
            assert!(r >= want - 1e-3, "{}: the side view is {r} at z {z}, the form {want}", FormPreset::NAMES[i]);
        }
        let top = prof.iter().filter(|p| p.0 <= t.flute_len + 1e-9).fold(0.0_f64, |m, p| m.max(p.1));
        assert!((top - t.diameter * 0.5).abs() < 1e-9, "{}: the widest cut is the diameter", FormPreset::NAMES[i]);
    }
}

#[test]
fn a_number_edit_keeps_the_drawn_form() {
    let t = mill(FormPreset::starter(0), 5.0);
    let longer = Tool { shank_len: t.shank_len + 10.0, ..t.clone() };
    assert_eq!(longer.form, t.form, "a shank edit keeps the outline");
    assert_eq!(longer.why_not_sane(), None);
    assert!(!crate::cutter_fit::same_shape(&t, &Tool { form: mill(FormPreset::starter(1), 5.0).form, ..t.clone() }));
    // Settling again changes nothing: the preset and its outline agree.
    let mut again = t.clone();
    settle(&mut again);
    assert_eq!(again, t);
}

#[test]
fn the_wire_form_round_trips() {
    let t = mill(FormPreset::Ogee { radius: 4.0, radius2: 3.0, step: 1.5, pilot_radius: 6.35, height: 12.7 }, 5.0);
    let json = serde_json::to_string(&t.form).unwrap_or_default();
    assert!(json.contains("\"kind\":\"ogee\"") && json.contains("\"pilot_len\":5.0"), "{json}");
    let back: Option<FormSpec> = serde_json::from_str(&json).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(back, t.form);
}

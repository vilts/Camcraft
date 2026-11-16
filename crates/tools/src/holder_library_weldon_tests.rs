//! #2018: one Weldon pick for each taper, its bore taken from the
//! tool's shank; the resolved holder is the old bore entry, band for
//! band.

use super::*;
use crate::holder_library::{BT30, BT40, CAT40, TTS};
use crate::spun::SpunProfile;

fn weldon_rows() -> Vec<(String, Vec<f64>)> {
    picker_rows()
        .into_iter()
        .filter_map(|r| match &r {
            PickerRow::Weldon { bores, .. } => Some((r.name(), bores.clone())),
            PickerRow::Entry(_) => None,
        })
        .collect()
}

/// The picker shows one Weldon row for each taper and no single-bore
/// Weldon entry; every ER entry stays a row of its own.
#[test]
fn the_picker_shows_one_weldon_row_for_each_taper() {
    let names: Vec<String> = weldon_rows().into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["BT30 Weldon", "BT40 Weldon", "CAT40 Weldon", "TTS Weldon"]);
    let rows = picker_rows();
    assert!(rows.iter().all(|r| !matches!(r, PickerRow::Entry(e) if matches!(e.clamp, Clamp::Weldon { .. }))));
    let ers = library().into_iter().filter(|e| matches!(e.clamp, Clamp::Er(_))).count();
    assert_eq!(rows.iter().filter(|r| matches!(r, PickerRow::Entry(_))).count(), ers);
    // Each row stands for every bore the library has for its taper.
    let bores = |t: &Taper| {
        library().iter().filter(|e| e.taper == Some(*t)).filter_map(|e| match e.clamp {
            Clamp::Weldon { bore, .. } => Some(bore),
            Clamp::Er(_) => None,
        }).collect::<Vec<_>>()
    };
    let rows = weldon_rows();
    for (i, t) in [BT30, BT40, CAT40, TTS].iter().enumerate() {
        assert_eq!(rows[i].1, bores(t), "{}", t.name);
    }
}

/// A 10 mm shank on a BT40 Weldon gets the 10 mm bore and its 35 mm
/// body - the old "BT40 Weldon 10 mm" entry exactly.
#[test]
fn a_10mm_shank_gets_the_10mm_bore_and_its_body() {
    let e = resolve("BT40 Weldon", 10.0).expect("a standard bore");
    assert_eq!(e.clamp, Clamp::Weldon { bore: 10.0, body_d: 35.0 });
    assert_eq!(e.name(), "BT40 Weldon 10 mm");
    assert_eq!(e, find("BT40 Weldon 10 mm").expect("entry"));
    // Case and spacing are ignored, and the inch bores resolve alike.
    assert_eq!(resolve("  bt40   weldon ", 12.7).map(|e| e.name()), Ok("BT40 Weldon 1/2 in".to_owned()));
    assert_eq!(resolve("TTS Weldon", 7.9375).map(|e| e.name()), Ok("TTS Weldon 5/16 in".to_owned()));
    // Any other name is the entry it names.
    assert_eq!(resolve("BT30 ER20", 10.0), find("BT30 ER20"));
}

/// A shank with no standard bore is refused by name, with the fit
/// check's text and the bores there are.
#[test]
fn a_shank_with_no_standard_bore_is_refused_by_name() {
    let err = resolve("BT30 Weldon", 7.0).expect_err("no 7 mm bore");
    assert_eq!(
        err.to_string(),
        "a Ø7.000 mm shank does not fit - the holder takes a BT30 Weldon bore: \
         Ø6.000, Ø8.000, Ø10.000, Ø12.000, Ø16.000, Ø6.350, Ø9.525, Ø12.700 mm"
    );
    // BT30's side-lock bodies stop at 16 mm: a 20 mm shank is refused there.
    assert!(matches!(resolve("BT30 Weldon", 20.0), Err(LibraryError::Shank { .. })));
    assert!(matches!(resolve("BT30 Weldon", f64::NAN), Err(LibraryError::Shank { .. })));
    assert_eq!(resolve("ER20 Weldon", 10.0), Err(LibraryError::Unknown("ER20 Weldon".into())));
}

/// The collision bands of a holder picked through the Weldon row are
/// those of the old bore entry, for every bore of every taper.
#[test]
fn a_picked_weldon_draws_the_old_entrys_collision_bands() {
    for old in library().into_iter().filter(|e| matches!(e.clamp, Clamp::Weldon { .. })) {
        let (Some(t), Clamp::Weldon { bore, .. }) = (old.taper, old.clamp) else { continue };
        let picked = resolve(&weldon_name(&t), bore).expect("its own bore");
        let bands = |e: &HolderEntry| {
            let pts = e.points(40.0, e.gauge_len).expect("draws");
            SpunProfile::new(pts).expect("sane").holder_profile().bands().to_vec()
        };
        assert_eq!(bands(&picked), bands(&old), "{}", old.name());
        assert!(!bands(&old).is_empty());
    }
}

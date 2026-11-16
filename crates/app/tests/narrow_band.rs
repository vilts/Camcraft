//! #1655: a pocket-family op that clears the stock around the part (a
//! whole-model source, #12, or the stock source, #1554) names each band
//! of that area narrower than the tool. Before the fix, a 3 mm stock
//! margin beside a 10 mm tool got no pass while the pocket in the part
//! was cut, and the op came back with no note: the outside of the part
//! stayed uncut and nothing said so.
//!
//! Provenance: our own fixtures (a block with a blind square pocket in a
//! box stock); no vendor material consulted.
//!
//! Gate map: covers app::operation::keep_law (narrow_noted),
//! app::operation::generate (the #1655 note), motion::machining2d
//! (unreached_note).

mod area_support;
mod pocket_support;

use area_support::*;
use pocket_support::{block_with_pocket, rounded_rect};

/// A `2a x 2a x [0, 10]` block with a 30 x 30 pocket to Z 2, in the
/// 70 x 70 x 10 stock: the stock margin is `35 - a` on each side.
fn world(a: f64) -> World {
    let ring = rounded_rect(15.0, 15.0, 0.0, 1);
    world_with(Some(ImportModel::from_mesh("part", block_with_pocket(a, a, &ring, 2.0, 10.0))), 0.0, 10.0)
}

fn adaptive() -> OpParams {
    let mut p = operation::retype_params(&pocket(10.0, 2.0), OpType::Adaptive2D as usize);
    if let OpParams::Adaptive2D { top_z, bottom_z, stepdown, stepover, .. } = &mut p {
        *top_z = Height::fixed(10.0);
        *bottom_z = Height::fixed(2.0);
        *stepdown = 4.0;
        *stepover = 4.0;
    }
    assert_eq!(p.kind(), OpType::Adaptive2D);
    p
}

fn band_notes(t: &Toolpath) -> Vec<&String> {
    t.notes.iter().filter(|n| n.contains("narrower than the") && n.contains("no pass cuts")).collect()
}

#[test]
fn a_model_source_adaptive_names_the_outside_band_narrower_than_the_tool() {
    // 3 mm margin, 10 mm tool: the band erodes to nothing, the pocket
    // does not. The op still cuts the pocket - and now says the band.
    let w = world(32.0);
    for (label, params) in [("2D Adaptive", adaptive()), ("2D Pocket", pocket(10.0, 2.0))] {
        for src in [SrcRef::Entity("part".into()), SrcRef::Stock] {
            let (t, _) = run(&w, &op(src.clone(), params.clone(), AreaControls::default()))
                .unwrap_or_else(|e| panic!("{label} {src:?}: {e}"));
            let pts = feeds(&t);
            assert!(!pts.is_empty(), "{label} {src:?}: the pocket is cut");
            assert!(pts.iter().all(|p| p.x.abs() < 15.0 + EPS && p.y.abs() < 15.0 + EPS), "{label} {src:?}: no pass in the band");
            let notes = band_notes(&t);
            assert_eq!(notes.len(), 1, "{label} {src:?}: {:?}", t.notes);
            let n = notes[0];
            assert!(n.starts_with(label), "{n}");
            assert!(n.contains("1 band") && n.contains("10.000 mm tool"), "{n}");
            assert!(n.contains("X -35.000 mm to 35.000 mm, Y -35.000 mm to 35.000 mm"), "{n}");
        }
    }
}

#[test]
fn a_band_the_tool_fits_is_cut_and_says_nothing() {
    // 11 mm margin, 10 mm tool: the band erodes from both sides by the
    // 5 mm radius and keeps a 1 mm wide center area.
    let w = world(24.0);
    let (t, _) = run(&w, &op(SrcRef::Entity("part".into()), adaptive(), AreaControls::default())).expect("generate");
    assert!(band_notes(&t).is_empty(), "{:?}", t.notes);
    assert!(feeds(&t).iter().any(|p| p.x.abs() > 24.0), "the band is cut");
}

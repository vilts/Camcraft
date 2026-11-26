//! Provenance: our own tests over our own single-line engraving font (issue 2679). The expected
//! strokes of the golden string are read from the glyph table by hand.
//! Gate map (M4.4): covers camcraft_geometry::stroke_text, camcraft_geometry::sketch_data.

use camcraft_geometry::dxf::{Unit, Units};
use camcraft_geometry::geo2d::Point2;
use camcraft_geometry::sketch_data::SketchData;
use camcraft_geometry::stroke_text::{self, CHARSET, LAYER, MAX_CHARS, MAX_HEIGHT, MIN_HEIGHT, TextError, TextSpec};

fn spec(text: &str, height: f64) -> TextSpec<'_> {
    TextSpec { text, height, letter_spacing: 0.0 }
}

fn sketch(text: &str, height: f64) -> SketchData {
    stroke_text::sketch(&spec(text, height)).unwrap_or_else(|e| panic!("'{text}' at {height} mm: {e}"))
}

/// Each chain is usable: nothing rejected, and each closed loop has a nesting.
fn assert_clean(data: &SketchData, what: &str) {
    let chains = data.chains();
    assert!(chains.rejected.is_empty(), "{what}: rejected {:?}", chains.rejected);
    for chain in &chains.closed {
        assert!(chain.nesting.is_ok(), "{what}: {:?}", chain.nesting);
    }
    assert!(chains.closed.len() + chains.open.len() >= 1, "{what}: no chain");
}

/// One stroke for each line: `x,y x,y ...`, three decimals.
fn dump(strokes: &[Vec<Point2>]) -> String {
    let line = |s: &Vec<Point2>| s.iter().map(|p| format!("{:.3},{:.3}", p.x, p.y)).collect::<Vec<_>>().join(" ");
    strokes.iter().map(line).collect::<Vec<_>>().join("\n")
}

/// `SN-0471` at a cap height of 6 mm: one font unit is 1 mm, a glyph is 4 mm wide and the
/// gap between glyphs is 2 mm.
const GOLDEN_SN_0471: &str = "\
4.000,5.000 3.000,6.000 1.000,6.000 0.000,5.000 0.000,4.000 1.000,3.000 3.000,3.000 4.000,2.000 4.000,1.000 3.000,0.000 1.000,0.000 0.000,1.000
6.000,0.000 6.000,6.000 10.000,0.000 10.000,6.000
12.500,3.000 15.500,3.000
19.000,0.000 21.000,0.000 22.000,1.000 22.000,5.000 21.000,6.000 19.000,6.000 18.000,5.000 18.000,1.000 19.000,0.000
19.000,1.500 21.000,4.500
27.000,0.000 27.000,6.000 24.000,2.000 28.000,2.000
30.000,6.000 34.000,6.000 31.500,0.000
37.000,5.000 38.000,6.000 38.000,0.000
37.000,0.000 39.000,0.000";

#[test]
fn a_known_string_gives_its_golden_strokes() {
    let strokes = stroke_text::strokes(&spec("SN-0471", 6.0)).expect("strokes");
    assert_eq!(dump(&strokes), GOLDEN_SN_0471);
    // The same call gives the same strokes.
    assert_eq!(strokes, stroke_text::strokes(&spec("SN-0471", 6.0)).expect("strokes again"));
}

#[test]
fn each_glyph_chains_with_no_rejection_at_the_extreme_heights() {
    for ch in CHARSET.chars().filter(|c| *c != ' ') {
        for height in [MIN_HEIGHT, 6.0, MAX_HEIGHT] {
            let text = ch.to_string();
            assert_clean(&sketch(&text, height), &format!("'{ch}' at {height} mm"));
        }
    }
}

#[test]
fn the_whole_character_set_in_one_string_chains_clean() {
    for height in [MIN_HEIGHT, 3.0] {
        let data = sketch(CHARSET, height);
        assert_clean(&data, "the character set");
        // Twice over, so each glyph also stands next to its own repeat.
        let doubled: String = CHARSET.chars().flat_map(|c| [c, c]).collect();
        assert_clean(&sketch(&doubled, height), "the doubled character set");
    }
}

#[test]
fn the_sketch_is_in_millimetres_on_one_layer_with_the_cap_height_stated() {
    let data = sketch("H8", 10.0);
    assert_eq!(data.layers(), [LAYER]);
    assert_eq!(data.units().stated, Units::Known(Unit::Millimeters));
    assert_eq!(data.units().applied, Unit::Millimeters);
    assert_eq!(data.skipped_total(), 0);
    assert_eq!(data.summary().entities, data.primitives().len());
    let (lo, hi) = data.bounds().expect("bounds");
    assert!(lo.x.abs() < 1e-12 && lo.y.abs() < 1e-12, "starts at the baseline origin: {lo:?}");
    assert!((hi.y - 10.0).abs() < 1e-12, "the cap height is the stated height: {hi:?}");
    // Two glyphs of 4 units and one gap of 2 units, at 10/6 mm for each unit.
    assert!((hi.x - 10.0 * 10.0 / 6.0).abs() < 1e-9, "{hi:?}");
    // H is three open strokes; 8 is a closed lower loop and an open upper bowl.
    assert_eq!((data.chains().open.len(), data.chains().closed.len()), (4, 1));
}

#[test]
fn height_scales_and_letter_spacing_moves_only_the_later_glyphs() {
    let small = stroke_text::strokes(&spec("A1", 6.0)).expect("small");
    let large = stroke_text::strokes(&spec("A1", 12.0)).expect("large");
    assert_eq!(small.len(), large.len());
    for (s, l) in small.iter().flatten().zip(large.iter().flatten()) {
        assert!((l.x - 2.0 * s.x).abs() < 1e-12 && (l.y - 2.0 * s.y).abs() < 1e-12, "{s:?} {l:?}");
    }
    let spaced = stroke_text::strokes(&TextSpec { text: "A1", height: 6.0, letter_spacing: 1.5 }).expect("spaced");
    // A has two strokes: they stay, and the strokes of 1 move by the spacing.
    assert_eq!(spaced[..2], small[..2]);
    for (s, p) in small[2..].iter().flatten().zip(spaced[2..].iter().flatten()) {
        assert!((p.x - s.x - 1.5).abs() < 1e-12 && p.y == s.y, "{s:?} {p:?}");
    }
    // A space cuts nothing and still advances.
    let gap = stroke_text::strokes(&spec("A 1", 6.0)).expect("gap");
    assert_eq!(gap.len(), small.len());
    assert!(gap[2][0].x > small[2][0].x);
}

#[test]
fn bad_input_is_refused_by_name() {
    let refused = |text: &str, height: f64, spacing: f64| {
        stroke_text::sketch(&TextSpec { text, height, letter_spacing: spacing }).expect_err("refused")
    };
    assert_eq!(refused("", 6.0, 0.0), TextError::Empty);
    assert_eq!(refused("   ", 6.0, 0.0), TextError::Empty);
    // The strokes refuse too: an empty stroke list is never an answer.
    for blank in ["", "   "] {
        assert_eq!(stroke_text::strokes(&spec(blank, 6.0)), Err(TextError::Empty));
    }
    let why = refused("SN-0471a", 6.0, 0.0);
    assert_eq!(why, TextError::NoGlyph { ch: 'a', position: 8 });
    let text = why.to_string();
    assert!(text.contains("'a'") && text.contains("character 8") && text.contains("capital letters"), "{text}");
    assert!(matches!(refused("A\nB", 6.0, 0.0), TextError::NoGlyph { ch: '\n', position: 2 }));
    assert!(matches!(refused("É", 6.0, 0.0), TextError::NoGlyph { ch: 'É', position: 1 }));
    for height in [0.0, -1.0, MIN_HEIGHT / 2.0, MAX_HEIGHT * 2.0, f64::NAN, f64::INFINITY] {
        let why = refused("A", height, 0.0);
        assert!(matches!(why, TextError::Height { .. }), "{height}: {why}");
        assert!(why.to_string().contains("text height"), "{why}");
    }
    for spacing in [-0.1, f64::NAN, f64::INFINITY, MAX_HEIGHT * 2.0] {
        let why = refused("AB", 6.0, spacing);
        assert!(matches!(why, TextError::Spacing { .. }), "{spacing}: {why}");
    }
    let long = "A".repeat(MAX_CHARS + 1);
    assert_eq!(refused(&long, 6.0, 0.0), TextError::TooLong { chars: MAX_CHARS + 1, max: MAX_CHARS });
    assert!(stroke_text::sketch(&spec(&"A".repeat(MAX_CHARS), 6.0)).is_ok());
}

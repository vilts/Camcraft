//! #3072: typed text in a TrueType outline font becomes closed shapes - holes nest in their
//! letters, curves are flattened inside the chord tolerance, composite glyphs are resolved, and
//! every font or text that cannot give clear shapes is refused by name.
//!
//! Provenance: our own tests over our own reader, on the generated font of
//! `outline_text::sample_font` (our own glyphs, no font file).
//!
//! Gate map: covers camcraft_geometry::outline_text.

use camcraft_geometry::outline_text::sample_font::{Glyph, build_font, sample_font};
use camcraft_geometry::outline_text::{
    CHORD_TOLERANCE, FontError, MAX_VERTICES, OutlineSpec, OutlineTextError, check_font, outlines,
    sketch,
};
use camcraft_geometry::sketch_chains::LoopRole;

fn spec<'a>(font: &'a [u8], text: &'a str) -> OutlineSpec<'a> {
    OutlineSpec { font, text, height: 7.0, letter_spacing: 0.0 }
}

fn roles(font: &[u8], text: &str) -> (usize, usize) {
    let data = sketch(&spec(font, text)).unwrap_or_else(|e| panic!("{text}: {e}"));
    let closed = &data.chains().closed;
    let holes = closed.iter().filter(|c| matches!(c.nesting, Ok(n) if n.role == LoopRole::Hole)).count();
    assert!(data.chains().open.is_empty() && data.chains().rejected.is_empty());
    (closed.len() - holes, holes)
}

#[test]
fn letters_become_closed_shapes_with_their_holes() {
    let font = sample_font();
    // H and I are one shape each; O is a shape with a hole.
    assert_eq!(roles(&font, "HI"), (2, 0));
    assert_eq!(roles(&font, "O"), (1, 1));
    assert_eq!(roles(&font, "HOD O"), (4, 2));
    // The composite 8 is two half-size Os: two shapes, two holes.
    assert_eq!(roles(&font, "8"), (2, 2));
}

#[test]
fn the_cap_height_and_the_advances_place_the_letters_in_mm() {
    let font = sample_font();
    // H is 700 units tall: at a cap height of 7 mm one unit is 0.01 mm.
    let lines = outlines(&spec(&font, "H I")).expect("H I");
    let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    for p in lines.iter().flatten() {
        lo = (lo.0.min(p.x), lo.1.min(p.y));
        hi = (hi.0.max(p.x), hi.1.max(p.y));
    }
    // H advances 600, the space 300; I is 100 wide.
    assert_eq!((lo, hi), ((0.0, 0.0), (10.0, 7.0)));
    // Letter spacing adds its gap after each character.
    let spaced = outlines(&OutlineSpec { letter_spacing: 1.5, ..spec(&font, "II") }).expect("II");
    let right = spaced.iter().flatten().map(|p| p.x).fold(f64::MIN, f64::max);
    assert!((right - (2.0 + 1.5 + 1.0)).abs() < 1e-9, "{right}");
}

#[test]
fn curves_are_flattened_inside_the_chord_tolerance() {
    let font = sample_font();
    // D's right side is two quadratic pieces through (6, 3.5); each point lies on the curve.
    let lines = outlines(&spec(&font, "D")).expect("D");
    let curve: Vec<_> = lines[0].iter().filter(|p| p.x > 3.0 + 1e-9).collect();
    assert!(curve.len() > 8, "the curve is flattened into many lines, got {}", curve.len());
    assert!(curve.iter().any(|p| (p.x - 6.0).abs() < 1e-9 && (p.y - 3.5).abs() < 1e-9));
    // The upper piece is (3, 7) (6, 7) (6, 3.5): each flattened point lies on it.
    for p in curve.iter().filter(|p| p.y > 3.5) {
        // B(s) = (3 + 3 (2s - s^2), 7 - 3.5 s^2): find s from y, then check x.
        let s = ((7.0 - p.y) / 3.5).sqrt();
        assert!((3.0 + 3.0 * (2.0 * s - s * s) - p.x).abs() < 1e-6, "{p:?} is not on the curve");
    }
    // The contour of o has no on-curve point: it still closes as one smooth shape.
    let o = outlines(&spec(&font, "o")).expect("o");
    assert_eq!(o.len(), 1);
    assert_eq!(o[0].first(), o[0].last());
    assert!(o[0].len() > 16 && CHORD_TOLERANCE <= 0.005);
}

#[test]
fn a_font_that_cannot_be_read_is_refused_by_name() {
    let font = sample_font();
    let why = |bytes: &[u8]| match check_font(bytes) {
        Ok(()) => panic!("must be refused"),
        Err(e) => e,
    };
    assert_eq!(why(b"not a font at all"), OutlineTextError::Font(FontError::NotAFont));
    assert_eq!(why(b"OTTO\0\0\0\0\0\0\0\0"), OutlineTextError::Font(FontError::CffOutlines));
    assert_eq!(why(b"ttcf\0\0\0\0\0\0\0\0"), OutlineTextError::Font(FontError::Collection));
    assert!(why(&font[..40]).to_string().starts_with("the font cannot be read"));
    // Every cut of the file is refused or read, never a panic.
    for len in 0..font.len() {
        let _ = sketch(&spec(&font[..len], "HO8Do"));
    }
    // Every flipped byte is refused or read, never a panic or a hang.
    for at in 0..font.len() {
        let mut bad = font.clone();
        bad[at] ^= 0xA5;
        let _ = sketch(&spec(&bad, "HO8Do X"));
    }
}

#[test]
fn bad_text_and_overlapping_outlines_are_refused_by_name() {
    let font = sample_font();
    let err = |s: OutlineSpec| sketch(&s).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err(spec(&font, "  ")).contains("the text is empty"));
    assert!(err(spec(&font, "HZ")).contains("'Z' at character 2 is not in the font"));
    assert!(err(OutlineSpec { height: 0.1, ..spec(&font, "H") }).contains("cap height from 0.5 mm"));
    assert!(err(OutlineSpec { letter_spacing: -1.0, ..spec(&font, "H") }).contains("letter spacing"));
    // X's two bars cross: a pocket of them is not clear.
    assert!(err(spec(&font, "X")).contains("the text outlines cross or touch"), "{}", err(spec(&font, "X")));
}

#[test]
fn large_curves_measure_inside_the_chord_tolerance() {
    let font = sample_font();
    for height in [7.0, 1000.0] {
        let scale = height / 700.0;
        let lines = outlines(&OutlineSpec {
            height,
            ..spec(&font, "D")
        })
        .expect("large D");
        let mut chords = 0;
        for pair in lines[0].windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if a.x < 300.0 * scale || b.x < 300.0 * scale {
                continue;
            }
            // Recover each endpoint's parameter on either of D's two quadratic pieces.
            let upper = a.y >= 350.0 * scale && b.y >= 350.0 * scale;
            let lower = a.y <= 350.0 * scale && b.y <= 350.0 * scale;
            if !(upper || lower) {
                continue;
            }
            let parameter = |y: f64| {
                if upper {
                    ((height - y) / (350.0 * scale)).max(0.0).sqrt()
                } else {
                    1.0 - (y / (350.0 * scale)).max(0.0).sqrt()
                }
            };
            let (ta, tb) = (parameter(a.y), parameter(b.y));
            for i in 1..16 {
                let t = ta + (tb - ta) * f64::from(i) / 16.0;
                let (x, y) = if upper {
                    (300.0 + 300.0 * (2.0 * t - t * t), 700.0 - 350.0 * t * t)
                } else {
                    (600.0 - 300.0 * t * t, 350.0 * (1.0 - t).powi(2))
                };
                let (x, y) = (x * scale, y * scale);
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let u = (((x - a.x) * dx + (y - a.y) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                let error = (x - a.x - u * dx).hypot(y - a.y - u * dy);
                assert!(
                    error <= CHORD_TOLERANCE,
                    "height {height}, chord {a:?}..{b:?}, error {error}"
                );
            }
            chords += 1;
        }
        assert!(chords > 8);
    }
}

fn table_record(font: &[u8], tag: &[u8; 4]) -> usize {
    (12..12 + 16 * usize::from(u16::from_be_bytes(font[4..6].try_into().unwrap())))
        .step_by(16)
        .find(|&at| &font[at..at + 4] == tag)
        .expect("table")
}

fn table_offset(font: &[u8], tag: &[u8; 4]) -> usize {
    let rec = table_record(font, tag);
    u32::from_be_bytes(font[rec + 8..rec + 12].try_into().unwrap()) as usize
}

#[test]
fn composite_offsets_use_the_component_transform() {
    let mut font = sample_font();
    let loca = table_offset(&font, b"loca");
    let glyph = u32::from_be_bytes(font[loca + 28..loca + 32].try_into().unwrap()) as usize;
    let at = table_offset(&font, b"glyf") + glyph + 10;
    font[at..at + 2].copy_from_slice(&0x080b_u16.to_be_bytes());
    font[at + 4..at + 6].copy_from_slice(&100_i16.to_be_bytes());
    font[at + 6..at + 8].copy_from_slice(&360_i16.to_be_bytes());
    let lines = outlines(&spec(&font, "8")).expect("scaled offset");
    let min_x = lines
        .iter()
        .flatten()
        .map(|p| p.x)
        .fold(f64::INFINITY, f64::min);
    let min_y = lines
        .iter()
        .flatten()
        .map(|p| p.y)
        .fold(f64::INFINITY, f64::min);
    assert_eq!((min_x, min_y), (0.5, 1.8));
    assert_eq!(roles(&font, "8"), (1, 1));
    font[at..at + 2].copy_from_slice(&0x180b_u16.to_be_bytes());
    assert!(
        outlines(&spec(&font, "8"))
            .unwrap_err()
            .to_string()
            .contains("conflicting offset flags")
    );
}

/// Write the x coordinates of the eight points of `O` (outer box, then inner box) into the
/// font as deltas. The y coordinates stay: each box keeps its corners, in a new direction.
fn set_o_xs(font: &mut [u8], xs: [i16; 8]) {
    let loca = table_offset(font, b"loca");
    let glyph = u32::from_be_bytes(font[loca + 16..loca + 20].try_into().unwrap()) as usize;
    // Header 10, two contour ends 4, instruction length 2, eight flags 8.
    let at = table_offset(font, b"glyf") + glyph + 24;
    let mut last = 0_i16;
    for (k, x) in xs.into_iter().enumerate() {
        font[at + 2 * k..at + 2 * k + 2].copy_from_slice(&(x - last).to_be_bytes());
        last = x;
    }
}

#[test]
fn nested_outlines_follow_the_nonzero_winding_fill() {
    let mut font = sample_font();
    // The unchanged O: the box runs clockwise and its hole counterclockwise.
    set_o_xs(&mut font, [0, 0, 600, 600, 450, 450, 150, 150]);
    assert_eq!(roles(&font, "O"), (1, 1));
    // The two directions swapped are still a box with a hole under nonzero fill.
    set_o_xs(&mut font, [600, 600, 0, 0, 150, 150, 450, 450]);
    assert_eq!(roles(&font, "O"), (1, 1));
    // The inner box in the same direction as the outer one: nonzero fill makes it filled.
    set_o_xs(&mut font, [0, 0, 600, 600, 150, 150, 450, 450]);
    let err = sketch(&spec(&font, "O")).expect_err("a same-direction inner box is not a hole");
    assert!(matches!(err, OutlineTextError::Winding { .. }), "{err}");
    assert!(err.to_string().contains("runs in the same direction"), "{err}");
    // Side by side, two outlines in the same direction stay two shapes.
    assert_eq!(roles(&sample_font(), "II"), (2, 0));
}

#[test]
fn declared_table_lengths_bound_reads_with_trailing_bytes() {
    let font = sample_font();
    for (tag, lengths) in [
        (b"head", &[0_u32, 19, 51][..]),
        (b"maxp", &[0, 5][..]),
        (b"hhea", &[0, 35][..]),
        (b"hmtx", &[0, 19][..]),
        (b"loca", &[0, 23][..]),
        (b"cmap", &[0, 11, 27][..]),
    ] {
        for &length in lengths {
            let mut bad = font.clone();
            let rec = table_record(&bad, tag);
            bad[rec + 12..rec + 16].copy_from_slice(&length.to_be_bytes());
            assert!(
                matches!(sketch(&spec(&bad, "HO")), Err(OutlineTextError::Font(_))),
                "{tag:?} length {length} was accepted"
            );
        }
    }
}

#[test]
fn declared_cmap_subtable_lengths_bound_reads() {
    let mut font = sample_font();
    let cmap = table_offset(&font, b"cmap");
    let sub = cmap + 12;
    font[sub + 2..sub + 4].copy_from_slice(&16_u16.to_be_bytes());
    assert!(matches!(
        sketch(&spec(&font, "HO")),
        Err(OutlineTextError::Font(_))
    ));
    // Point the directory at a new format 12 cmap. The bytes after its short declared
    // subtable length remain in the file, including the complete group for H.
    let mut font = sample_font();
    let rec = table_record(&font, b"cmap");
    let offset = u32::try_from(font.len()).unwrap();
    let mut cmap = vec![0, 0, 0, 1, 0, 3, 0, 10, 0, 0, 0, 12];
    cmap.extend_from_slice(&12_u16.to_be_bytes());
    cmap.extend_from_slice(&0_u16.to_be_bytes());
    cmap.extend_from_slice(&28_u32.to_be_bytes());
    cmap.extend_from_slice(&0_u32.to_be_bytes());
    cmap.extend_from_slice(&1_u32.to_be_bytes());
    cmap.extend_from_slice(&u32::from('H').to_be_bytes());
    cmap.extend_from_slice(&u32::from('H').to_be_bytes());
    cmap.extend_from_slice(&2_u32.to_be_bytes());
    font[rec + 8..rec + 12].copy_from_slice(&offset.to_be_bytes());
    font[rec + 12..rec + 16].copy_from_slice(&u32::try_from(cmap.len()).unwrap().to_be_bytes());
    font.extend_from_slice(&cmap);
    assert!(
        sketch(&spec(&font, "H")).is_ok(),
        "complete format 12 group"
    );
    let length_at = offset as usize + 12 + 4;
    font[length_at..length_at + 4].copy_from_slice(&16_u32.to_be_bytes());
    assert!(matches!(
        sketch(&spec(&font, "H")),
        Err(OutlineTextError::Font(_))
    ));
}

/// Glyphs 1 to 8 are composites of 64 parts, each part the next glyph; glyph 9 is empty. A
/// full walk is 64^8 visits that add no point. `H` is the tree below glyph 1, for the cap
/// height (64^7 visits, inside the nesting limit).
fn part_bomb() -> Vec<(Option<char>, u16, Glyph)> {
    let mut table = vec![(None, 500, Glyph::Empty)];
    for gid in 1..=8_u16 {
        let ch = (gid == 1).then_some('A');
        table.push((ch, 500, Glyph::Composite(vec![(gid + 1, 0, 0, 0x4000); 64])));
    }
    table.push((None, 500, Glyph::Empty));
    table.push((Some('H'), 500, Glyph::Composite(vec![(2, 0, 0, 0x4000)])));
    table
}

#[test]
fn a_composite_tree_of_empty_parts_is_refused_without_a_walk() {
    // With an OS/2 cap height, the text glyph walks the tree; without it, the cap height does.
    for cap in [Some(700), None] {
        let font = build_font(&part_bomb(), cap);
        let err = outlines(&spec(&font, "A")).expect_err("a composite part bomb");
        assert!(matches!(err, OutlineTextError::Font(FontError::Corrupt(_))), "{err}");
        assert!(err.to_string().contains("too many parts"), "{cap:?}: {err}");
    }
}

#[test]
fn the_flattened_points_of_a_text_are_bounded_before_they_are_made() {
    // A cap height of one unit at 1000 mm, and a zigzag of off-curve points 67 units deep,
    // out and back: each curve needs about 2000 lines, so the text needs far more than the
    // budget, and no single curve passes the per-curve limit.
    let out = (0..2000_i16).map(|i| (i, 67 * (i % 2), false));
    let back = (0..2000_i16).map(|i| (1999 - i, 150 + 67 * (i % 2), false));
    let zigzag: Vec<_> = out.chain(back).collect();
    let table = vec![(None, 500, Glyph::Empty), (Some('A'), 500, Glyph::Simple(vec![zigzag]))];
    let font = build_font(&table, Some(1));
    let big = OutlineSpec { height: 1000.0, ..spec(&font, "A") };
    let err = outlines(&big).expect_err("a vertex bomb");
    assert_eq!(err, OutlineTextError::TooManyVertices { max: MAX_VERTICES });
    assert!(err.to_string().contains("outline points"), "{err}");
    // The same glyph at a small height stays inside the budget.
    assert!(outlines(&OutlineSpec { height: 0.5, ..big }).is_ok());
}

/// A simple glyph of one contour whose x deltas are written as words, as given.
fn raw_glyph(dxs: &[i16]) -> Glyph {
    let mut g = Vec::new();
    for v in [1_i16, 0, 0, 0, 0, i16::try_from(dxs.len() - 1).unwrap(), 0] {
        g.extend_from_slice(&v.to_be_bytes());
    }
    // On the curve, x a word, y the same as before.
    g.extend(dxs.iter().map(|_| 0x01 | 0x20));
    dxs.iter().for_each(|d| g.extend_from_slice(&d.to_be_bytes()));
    Glyph::Raw(g)
}

#[test]
fn a_coordinate_outside_the_16_bit_range_is_refused() {
    let font = |dxs: &[i16]| build_font(&[(None, 500, Glyph::Empty), (Some('A'), 500, raw_glyph(dxs))], Some(700));
    // Inside the range, the deltas read as written.
    assert!(matches!(outlines(&spec(&font(&[0, 32767, -32767]), "A")), Err(OutlineTextError::NoShape)));
    // Two deltas of +32767 sum to 65534: not a font coordinate, so not a silent wrap.
    let err = outlines(&spec(&font(&[0, 32767, 32767]), "A")).expect_err("out of range");
    assert!(err.to_string().contains("16-bit range"), "{err}");
}

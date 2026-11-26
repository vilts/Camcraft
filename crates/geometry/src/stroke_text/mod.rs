//! Stroke text: a typed string made into single-line strokes for engraving, and into sketch
//! data that the document places like a DXF drawing.
//!
//! Provenance: our own design for issue 2679. The glyphs are our own drawing (`glyphs`); the
//! layout is plain left-to-right advance arithmetic. No font file and no vendor material was
//! consulted.
//!
//! Contract. The strokes are tool-centreline paths in millimetres. The origin is the left end
//! of the baseline, and `height` is the cap height. Nothing is guessed: a character with no
//! glyph, an empty text and a size outside the limits are refused by name. The sketch that
//! [`sketch`] returns has no rejected chain, or the call fails. This module has no file I/O and
//! no application state.

mod glyphs;

use crate::dxf::{ImportSummary, LayeredPrimitive, Primitive, Unit, Units};
use crate::geo2d::Point2;
use crate::sketch_data::{SketchData, SketchError, SourceUnits};
use glyphs::{CAP, GAP, GLYPHS, Glyph};

/// Each character that has a glyph, in table order.
pub const CHARSET: &str = " ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.:/#+=()";
/// The layer of each stroke.
pub const LAYER: &str = "text";
/// The smallest cap height, mm. The shortest stroke (half a font unit) is then 0.04 mm long,
/// above the default join tolerance of 0.01 mm.
pub const MIN_HEIGHT: f64 = 0.5;
/// The largest cap height and the largest letter spacing, mm.
pub const MAX_HEIGHT: f64 = 1000.0;
/// The largest number of characters in one text.
pub const MAX_CHARS: usize = 256;

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum TextError {
    #[error("the text is empty: type one or more characters to engrave")]
    Empty,
    #[error("the text has {chars} characters: the limit is {max}")]
    TooLong { chars: usize, max: usize },
    #[error(
        "{ch:?} at character {position} has no glyph: the engraving font has capital letters, \
         digits, the space and - _ . : / # + = ( ) only"
    )]
    NoGlyph { ch: char, position: usize },
    #[error("the text height {height} mm is not usable: state a cap height from 0.5 mm to 1000 mm")]
    Height { height: f64 },
    #[error("the letter spacing {spacing} mm is not usable: state 0 mm to 1000 mm")]
    Spacing { spacing: f64 },
    /// A defect of the glyph table or of the layout, never of the input: the strokes did not
    /// chain clean, so no sketch is made.
    #[error("the engraving font gave strokes that do not chain: {0}")]
    Font(String),
}

/// What to write: the string, the cap height in mm, and the extra gap between two characters
/// in mm (0 is the font's own gap).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextSpec<'a> {
    pub text: &'a str,
    pub height: f64,
    pub letter_spacing: f64,
}

fn glyph(ch: char) -> Option<&'static Glyph> {
    GLYPHS.iter().find(|g| g.ch == ch)
}

/// The strokes of `spec` as polylines in millimetres, in reading order. A closed stroke
/// repeats its first point as its last point.
pub fn strokes(spec: &TextSpec) -> Result<Vec<Vec<Point2>>, TextError> {
    let TextSpec { text, height, letter_spacing } = *spec;
    if text.trim().is_empty() {
        return Err(TextError::Empty);
    }
    let chars = text.chars().count();
    if chars > MAX_CHARS {
        return Err(TextError::TooLong { chars, max: MAX_CHARS });
    }
    if !(height.is_finite() && (MIN_HEIGHT..=MAX_HEIGHT).contains(&height)) {
        return Err(TextError::Height { height });
    }
    if !(letter_spacing.is_finite() && (0.0..=MAX_HEIGHT).contains(&letter_spacing)) {
        return Err(TextError::Spacing { spacing: letter_spacing });
    }
    let unit = height / CAP;
    let mut out = Vec::new();
    let mut pen = 0.0_f64;
    for (index, ch) in text.chars().enumerate() {
        let glyph = glyph(ch).ok_or(TextError::NoGlyph { ch, position: index + 1 })?;
        for stroke in glyph.strokes {
            out.push(stroke.iter().map(|&(x, y)| Point2::new(pen + x * unit, y * unit)).collect());
        }
        pen += (glyph.width + GAP) * unit + letter_spacing;
    }
    Ok(out)
}

/// The strokes of `spec` as sketch data: line primitives in millimetres on the layer
/// [`LAYER`], chained with the default join tolerance.
pub fn sketch(spec: &TextSpec) -> Result<SketchData, TextError> {
    let strokes = strokes(spec)?;
    let mut primitives = Vec::new();
    let mut vertices = 0_usize;
    for stroke in &strokes {
        vertices = vertices.saturating_add(stroke.len());
        for pair in stroke.windows(2) {
            if let [start, end] = *pair {
                primitives.push(LayeredPrimitive { layer: LAYER.into(), primitive: Primitive::Line { start, end } });
            }
        }
    }
    let units = SourceUnits { stated: Units::Known(Unit::Millimeters), applied: Unit::Millimeters };
    let summary = ImportSummary { entities: primitives.len(), vertices, skipped: Default::default() };
    let data = SketchData::from_parts(primitives, vec![LAYER.into()], units, None, summary).map_err(|e| match e {
        SketchError::Empty { .. } => TextError::Empty,
        other => TextError::Font(other.to_string()),
    })?;
    let chains = data.chains();
    if let Some(rejected) = chains.rejected.first() {
        return Err(TextError::Font(rejected.error.to_string()));
    }
    if let Some(error) = chains.closed.iter().find_map(|chain| chain.nesting.as_ref().err()) {
        return Err(TextError::Font(error.to_string()));
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_character_set_is_the_glyph_table() {
        let table: String = GLYPHS.iter().map(|g| g.ch).collect();
        assert_eq!(table, CHARSET);
    }

    #[test]
    fn each_glyph_stays_in_its_box() {
        for g in GLYPHS {
            for &(x, y) in g.strokes.iter().copied().flatten() {
                assert!((0.0..=g.width).contains(&x) && (0.0..=CAP).contains(&y), "'{}': ({x}, {y})", g.ch);
            }
        }
    }
}

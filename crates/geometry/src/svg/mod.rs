//! Bounded SVG import (#1880): an SVG drawing read as the same 2D primitives as a DXF drawing,
//! so the sketch chaining, nesting, profile entity and import command are shared.
//! Provenance: the W3C SVG 1.1 Recommendation (structure, coordinate systems, paths, basic
//! shapes and the arc implementation notes) and the CSS absolute-length table; original
//! implementation, no vendor code or output consulted.
//!
//! Contract. The output is a [`Document`]: when the file states a physical size (`width` and
//! `height` in mm, cm, in, pt, pc or px) its primitives are in millimetres and its units are
//! [`Units::Known`] millimetres. Otherwise the size is never guessed: the units are
//! [`Units::MissingOrUnitless`] and one drawing unit is the caller's unit, so
//! [`crate::sketch_data::SketchData::from_dxf`] refuses the drawing until the caller states
//! one. Y points up (the page's bottom edge on y = 0), as the drawing looks in an SVG editor.
//! An elliptical arc that maps to a circular arc stays an analytic arc; Beziers and other
//! ellipses become lines within [`CHORD_TOLERANCE_MM`]. Text, images, clipping, masks, markers,
//! styles and references to other files are not geometry: each is counted in
//! `summary.skipped`. What can hide, move or resize a drawn shape and is not applied (a style
//! rule that is not paint, an animation element, a DOCTYPE entity) is refused by name.
//! Malformed input is refused by name. This module has no file I/O and no application state.

mod css;
mod emit;
mod facts;
mod number;
mod path;
mod reference;
mod shapes;
mod sheet;
mod style;
mod transform;
mod viewport;
mod walk;
mod xml;

use crate::dxf::{Document, Limits, Unit, Units};
use crate::sketch_chains::DEFAULT_JOIN_TOLERANCE;

/// The largest distance of a flattened curve from the true curve, in millimetres.
pub const CHORD_TOLERANCE_MM: f64 = 0.005;

/// A refusal of an SVG file, in words a user can act on.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum SvgError {
    #[error("SVG {0}")]
    Invalid(String),
    /// The file states its size and the caller stated a unit as well.
    #[error(
        "the SVG file states its size in physical units (width and height); \
         leave the unit out, the file's size is used"
    )]
    SizeStated,
}

pub(crate) fn invalid(message: impl Into<String>) -> SvgError {
    SvgError::Invalid(message.into())
}

/// [`parse_with_limits`] under the default import budget.
pub fn parse(data: &[u8], unit: Option<Unit>) -> Result<Document, SvgError> {
    parse_with_limits(data, unit, Limits::default())
}

/// Read an SVG file. `unit` is the caller's unit for one drawing unit of a file that states no
/// physical size; a file that states one refuses it ([`SvgError::SizeStated`]).
pub fn parse_with_limits(data: &[u8], unit: Option<Unit>, limits: Limits) -> Result<Document, SvgError> {
    if data.len() > limits.max_bytes {
        return Err(invalid(format!(
            "file is {} bytes, larger than the import budget of {} bytes",
            data.len(),
            limits.max_bytes
        )));
    }
    if data.starts_with(&[0x1f, 0x8b]) {
        return Err(invalid("file is compressed (.svgz): save it as plain SVG"));
    }
    let text = std::str::from_utf8(data).map_err(|e| invalid(format!("file is not UTF-8 text: {e}")))?;
    let tree = xml::parse(text.trim_start_matches('\u{feff}'), limits.max_entities)?;
    let root = tree.nodes.first().ok_or_else(|| invalid("the file has no <svg> root element"))?;
    let view = viewport::read(root)?;
    if view.physical && unit.is_some() {
        return Err(SvgError::SizeStated);
    }
    // The tolerances in output units: millimetres, or the caller's unit (millimetres when the
    // caller has not answered yet; that drawing is refused for its units in any case).
    let per_unit = match (view.physical, unit) {
        (false, Some(u)) => u.millimeters_per_unit(),
        _ => 1.0,
    };
    let sink = emit::Sink {
        map: view.map,
        chord: CHORD_TOLERANCE_MM / per_unit,
        min_len: DEFAULT_JOIN_TOLERANCE * 1.5 / per_unit,
        layer: walk::DEFAULT_LAYER.to_owned(),
        max_vertices: limits.max_vertices,
        out: Vec::new(),
    };
    let mut walk = walk::Walk::new(&tree, sink, &limits);
    walk.root(view.map)?;
    walk.summary.vertices = walk.sink.out.len();
    Ok(Document {
        units: if view.physical { Units::Known(Unit::Millimeters) } else { Units::MissingOrUnitless },
        layers: Vec::new(),
        primitives: walk.sink.out,
        annotations: Vec::new(),
        elevation: None,
        summary: walk.summary,
        curve_tolerance: None,
    })
}

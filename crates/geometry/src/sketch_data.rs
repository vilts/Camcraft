//! Sketch data: one DXF drawing made ready for the document - its primitives in millimetres,
//! its layer names, the units it was read in, its chains and the import summary.
//!
//! Provenance: our own design for issue 1862 over this crate's DXF reader (issue 1860) and
//! sketch chaining (issue 1861). The unit factors are the `$INSUNITS` table of Autodesk's
//! published DXF Reference. No vendor code or output was consulted.
//!
//! Contract. A drawing with no stated unit is never scaled by a guess: the caller states the
//! unit or gets [`SketchError::MissingUnits`]. The chains are always the chains of the stored
//! primitives under the stored tolerance: the fields are private and both constructors chain,
//! so a `SketchData` cannot carry chains that its primitives do not give. This module has no
//! file I/O and no application state.

use crate::dxf::{
    CURVE_TOLERANCE_MM, Document, ImportSummary, LayeredPrimitive, Limits, Primitive, Unit, Units,
};
use crate::geo2d::Point2;
use crate::sketch_chains::{ChainError, ChainOptions, DEFAULT_JOIN_TOLERANCE, SketchChains, chain};
use std::collections::BTreeSet;
use std::f64::consts::{FRAC_PI_2, TAU};

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum SketchError {
    /// `$INSUNITS` is absent or 0 and the caller stated no unit. The extents are in drawing
    /// units, so the caller can show them in each candidate unit before it asks.
    #[error(
        "the DXF file states no units ($INSUNITS is absent or 0): state millimetres or inches. \
         The drawing is {width} x {height} drawing units"
    )]
    MissingUnits { width: f64, height: f64 },
    #[error(
        "the DXF file states $INSUNITS {code}, which is not a supported unit: state millimetres or inches"
    )]
    UnsupportedUnits { code: i32 },
    #[error(
        "the DXF file has no line, arc or circle to place ({skipped} other entities were skipped)"
    )]
    Empty { skipped: usize },
    #[error("invalid DXF import summary: counts or skipped total exceed the import limits")]
    InvalidSummary,
    /// The drawing's SPLINE or ELLIPSE curves were laid as arcs for a smaller unit than the
    /// one applied, so their bound in millimetres would pass the stated one (#1865).
    #[error(
        "the DXF file's SPLINE and ELLIPSE curves were read within {used_mm} mm in this unit, more \
         than the {CURVE_TOLERANCE_MM} mm bound: state the unit in the file ($INSUNITS) before its curves"
    )]
    CurveTolerance { used_mm: f64 },
    #[error(transparent)]
    Chain(#[from] ChainError),
}

impl SketchError {
    /// The caller can clear this refusal with a stated unit.
    pub fn needs_units(&self) -> bool {
        matches!(
            self,
            SketchError::MissingUnits { .. } | SketchError::UnsupportedUnits { .. }
        )
    }
}

/// The units of a sketch's source drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceUnits {
    /// What the file's `$INSUNITS` said.
    pub stated: Units,
    /// The unit the coordinates were read in: the stated unit, or the caller's answer.
    pub applied: Unit,
}

impl SourceUnits {
    /// The caller's answer was used, not the file's own unit.
    pub fn overridden(&self) -> bool {
        self.stated != Units::Known(self.applied)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SketchData {
    primitives: Vec<LayeredPrimitive>,
    primitive_signature: u64,
    layers: Vec<String>,
    units: SourceUnits,
    join_tolerance: f64,
    chains: SketchChains,
    summary: ImportSummary,
    skipped_total: usize,
}

impl SketchData {
    /// Read a parsed drawing. `unit_override` wins over the file's `$INSUNITS`; without it a
    /// file that states no usable unit is refused. `join_tolerance` (mm) defaults to
    /// [`DEFAULT_JOIN_TOLERANCE`].
    pub fn from_dxf(
        doc: Document,
        unit_override: Option<Unit>,
        join_tolerance: Option<f64>,
    ) -> Result<SketchData, SketchError> {
        let skipped = checked_summary(&doc.summary)?;
        if doc.primitives.is_empty() {
            return Err(SketchError::Empty { skipped });
        }
        let applied = match (unit_override, doc.units) {
            (Some(unit), _) | (None, Units::Known(unit)) => unit,
            (None, Units::MissingOrUnitless) => {
                let (lo, hi) = bounds(&doc.primitives).unwrap_or_default();
                return Err(SketchError::MissingUnits {
                    width: hi.x - lo.x,
                    height: hi.y - lo.y,
                });
            }
            (None, Units::Unsupported(code)) => return Err(SketchError::UnsupportedUnits { code }),
        };
        let k = applied.millimeters_per_unit();
        if let Some(used) = doc.curve_tolerance {
            let used_mm = used * k;
            if used_mm.is_nan() || used_mm > CURVE_TOLERANCE_MM * (1.0 + 1e-9) {
                return Err(SketchError::CurveTolerance { used_mm });
            }
        }
        let primitives = doc.primitives.iter().map(|p| scaled(p, k)).collect();
        let layers = doc.layers.into_iter().map(|layer| layer.name).collect();
        let units = SourceUnits {
            stated: doc.units,
            applied,
        };
        Self::from_parts(primitives, layers, units, join_tolerance, doc.summary)
    }

    /// Build from stored parts (the project file): `primitives` are in millimetres. The
    /// chains are made again here, so a stored sketch reads exactly as an imported one.
    pub fn from_parts(
        primitives: Vec<LayeredPrimitive>,
        layers: Vec<String>,
        units: SourceUnits,
        join_tolerance: Option<f64>,
        summary: ImportSummary,
    ) -> Result<SketchData, SketchError> {
        let skipped_total = checked_summary(&summary)?;
        if primitives.is_empty() {
            return Err(SketchError::Empty {
                skipped: skipped_total,
            });
        }
        let join_tolerance = join_tolerance.unwrap_or(DEFAULT_JOIN_TOLERANCE);
        let chains = chain(&primitives, ChainOptions { join_tolerance })?;
        let names: BTreeSet<String> = layers
            .into_iter()
            .chain(primitives.iter().map(|p| p.layer.clone()))
            .collect();
        Ok(SketchData {
            primitive_signature: primitive_signature(&primitives),
            primitives,
            layers: names.into_iter().collect(),
            units,
            join_tolerance,
            chains,
            summary,
            skipped_total,
        })
    }

    /// The primitives, in millimetres, in drawing order.
    pub fn primitives(&self) -> &[LayeredPrimitive] {
        &self.primitives
    }

    /// Exact geometry and layer membership, in drawing order. Reading this
    /// immutable signature never visits a primitive.
    pub fn primitive_signature(&self) -> u64 {
        self.primitive_signature
    }

    /// Every layer name of the drawing's layer table and of its primitives, sorted.
    pub fn layers(&self) -> &[String] {
        &self.layers
    }

    pub fn units(&self) -> SourceUnits {
        self.units
    }

    /// The join tolerance the chains were made with, in millimetres.
    pub fn join_tolerance(&self) -> f64 {
        self.join_tolerance
    }

    pub fn chains(&self) -> &SketchChains {
        &self.chains
    }

    pub fn summary(&self) -> &ImportSummary {
        &self.summary
    }

    /// The skipped count, checked before this sketch was constructed.
    pub fn skipped_total(&self) -> usize {
        self.skipped_total
    }

    /// The extents of the primitives in millimetres, arcs included exactly.
    pub fn bounds(&self) -> Option<(Point2, Point2)> {
        bounds(&self.primitives)
    }
}

/// Stored summaries are untrusted, just like the source DXF counts. Validate before
/// even the empty-drawing refusal adds counts, and keep readers free of unchecked sums.
fn checked_summary(summary: &ImportSummary) -> Result<usize, SketchError> {
    let limits = Limits::default();
    let skipped = summary
        .skipped
        .values()
        .try_fold(0_usize, |total, &count| total.checked_add(count));
    match skipped {
        Some(total)
            if summary.entities <= limits.max_entities
                && summary.vertices <= limits.max_vertices
                && total <= limits.max_entities =>
        {
            Ok(total)
        }
        _ => Err(SketchError::InvalidSummary),
    }
}

fn scaled(item: &LayeredPrimitive, k: f64) -> LayeredPrimitive {
    let at = |p: Point2| Point2::new(p.x * k, p.y * k);
    let primitive = match item.primitive {
        Primitive::Line { start, end } => Primitive::Line {
            start: at(start),
            end: at(end),
        },
        Primitive::Arc {
            center,
            radius,
            start_angle,
            sweep_angle,
        } => Primitive::Arc {
            center: at(center),
            radius: radius * k,
            start_angle,
            sweep_angle,
        },
        Primitive::Circle { center, radius } => Primitive::Circle {
            center: at(center),
            radius: radius * k,
        },
    };
    LayeredPrimitive {
        layer: item.layer.clone(),
        primitive,
    }
}

/// The extents of `primitives`; `None` for an empty slice. An arc counts its two ends and each
/// axis-extreme point that its sweep passes.
pub fn bounds(primitives: &[LayeredPrimitive]) -> Option<(Point2, Point2)> {
    let mut range: Option<(Point2, Point2)> = None;
    let mut add = |p: Point2| {
        let (lo, hi) = range.unwrap_or((p, p));
        range = Some((
            Point2::new(lo.x.min(p.x), lo.y.min(p.y)),
            Point2::new(hi.x.max(p.x), hi.y.max(p.y)),
        ));
    };
    for item in primitives {
        match item.primitive {
            Primitive::Line { start, end } => {
                add(start);
                add(end);
            }
            Primitive::Circle { center, radius } => {
                add(Point2::new(center.x - radius, center.y - radius));
                add(Point2::new(center.x + radius, center.y + radius));
            }
            Primitive::Arc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                let on =
                    |a: f64| Point2::new(center.x + radius * a.cos(), center.y + radius * a.sin());
                add(on(start_angle));
                add(on(start_angle + sweep_angle));
                for quarter in 0..4 {
                    let angle = f64::from(quarter) * FRAC_PI_2;
                    let travel = if sweep_angle >= 0.0 {
                        (angle - start_angle).rem_euclid(TAU)
                    } else {
                        (start_angle - angle).rem_euclid(TAU)
                    };
                    if travel <= sweep_angle.abs() {
                        add(on(angle));
                    }
                }
            }
        }
    }
    range
}

#[cfg(test)]
thread_local! {
    static SIGNATURE_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Compute once at the common import and project-load boundary. Store the digest on
/// the same immutable value as its source so sharing or undo never needs invalidation.
fn primitive_signature(primitives: &[LayeredPrimitive]) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    #[cfg(test)]
    SIGNATURE_BUILDS.with(|n| n.set(n.get() + 1));
    let mut h = DefaultHasher::new();
    primitives.len().hash(&mut h);
    for item in primitives {
        item.layer.hash(&mut h);
        match item.primitive {
            Primitive::Line { start, end } => {
                h.write_u8(0);
                [start.x, start.y, end.x, end.y]
                    .map(f64::to_bits)
                    .hash(&mut h);
            }
            Primitive::Arc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                h.write_u8(1);
                [center.x, center.y, radius, start_angle, sweep_angle]
                    .map(f64::to_bits)
                    .hash(&mut h);
            }
            Primitive::Circle { center, radius } => {
                h.write_u8(2);
                [center.x, center.y, radius].map(f64::to_bits).hash(&mut h);
            }
        }
    }
    h.finish()
}

#[cfg(test)]
mod signature_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn primitive_signature_is_built_once_per_drawing_and_reads_exact_coordinates() {
        let make = |r| {
            SketchData::from_parts(
                vec![LayeredPrimitive {
                    layer: "outline".into(),
                    primitive: Primitive::Circle {
                        center: Point2::default(),
                        radius: r,
                    },
                }],
                vec![],
                SourceUnits {
                    stated: Units::Known(Unit::Millimeters),
                    applied: Unit::Millimeters,
                },
                None,
                ImportSummary::default(),
            )
            .expect("circle")
        };
        let before = SIGNATURE_BUILDS.with(Cell::get);
        let original = make(1.0);
        let same = make(1.0);
        let edited = make(1.0000001);
        let shared = std::sync::Arc::new(original);
        for _ in 0..8 {
            assert_eq!(
                shared.clone().primitive_signature(),
                same.primitive_signature()
            );
            assert_ne!(shared.primitive_signature(), edited.primitive_signature());
        }
        assert_eq!(SIGNATURE_BUILDS.with(Cell::get), before + 3);
    }
}

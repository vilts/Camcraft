//! Provenance: neutral import values for Autodesk's public DXF entity definitions.
use super::Units;
use crate::geo2d::Point2;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("DXF {0}")]
pub struct DxfError(pub String);

pub(super) fn error(context: &str, message: &str) -> DxfError {
    DxfError(format!("{context}: {message}"))
}

/// Limits are checked before collecting records or vertices. No input count is a capacity.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_entities: usize,
    pub max_vertices: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
            max_entities: 100_000,
            max_vertices: 1_000_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    Line {
        start: Point2,
        end: Point2,
    },
    /// Radians, counterclockwise positive. A full circle uses `Circle`.
    Arc {
        center: Point2,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    },
    Circle {
        center: Point2,
        radius: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayeredPrimitive {
    pub layer: String,
    pub primitive: Primitive,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub flags: i32,
    pub color: i32,
    pub line_type: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextAnnotation {
    pub layer: String,
    pub text: String,
    pub pos: Point2,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportSummary {
    /// Includes POLYLINE's VERTEX and SEQEND records, and skipped entities.
    pub entities: usize,
    pub vertices: usize,
    pub skipped: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub units: Units,
    pub layers: Vec<Layer>,
    pub primitives: Vec<LayeredPrimitive>,
    /// Text entities (`TEXT` / `MTEXT`) in drawing units (#744).
    pub annotations: Vec<TextAnnotation>,
    /// Common Z of the extracted geometry, in drawing units.
    pub elevation: Option<f64>,
    pub summary: ImportSummary,
}

pub(super) struct State {
    pub doc: Document,
    pub limits: Limits,
}

impl State {
    pub fn entity(&mut self, kind: &str) -> Result<(), DxfError> {
        if self.doc.summary.entities >= self.limits.max_entities {
            return Err(error(kind, "entity count exceeds limit"));
        }
        self.doc.summary.entities += 1;
        Ok(())
    }

    pub fn vertex(&mut self, kind: &str) -> Result<(), DxfError> {
        if self.doc.summary.vertices >= self.limits.max_vertices {
            return Err(error(kind, "vertex count exceeds limit"));
        }
        self.doc.summary.vertices += 1;
        Ok(())
    }

    pub fn elevation(&mut self, kind: &str, z: f64) -> Result<(), DxfError> {
        if self.doc.elevation.is_some_and(|old| old != z) {
            return Err(error(kind, "varying Z heights are not planar"));
        }
        self.doc.elevation = Some(z);
        Ok(())
    }

    pub fn push(&mut self, layer: &str, primitive: Primitive) {
        self.doc.primitives.push(LayeredPrimitive {
            layer: layer.into(),
            primitive,
        });
    }
}

//! The file mirror of a DXF profile (#1862, `schema::Sketch`).
//! Provenance: our own project format.
//!
//! A profile is an entity row of kind `sketch` carrying this payload; the
//! row's `translation` and `rotation` hold its placement on the stock
//! face. The file stores what was read from the drawing - the primitives
//! in mm, the layer names, the unit codes, the join tolerance and the
//! import counts - and never the chains: they are made again from the
//! primitives on load (`SketchData::from_parts`), so a stored profile
//! cannot carry chains its primitives do not give. The virtual model
//! reference is not stored either; it is read from the world.
//!
//! Every profile is read and chained before any world write
//! ([`read_all`]), so a corrupt one refuses the file by name and leaves
//! no half-loaded document.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use super::*;
use crate::schema::Sketch;
use crate::sketch_data::{SketchData, SourceUnits};
use camcraft_geometry::dxf::{ImportSummary, LayeredPrimitive, Primitive, Units};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SketchDto {
    /// The setup whose stock it lies on; absent - the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
    /// The drawing's `$INSUNITS` code as its file stated it; 0 - none.
    pub insunits: i32,
    /// The `$INSUNITS` code of the unit the drawing was read in.
    pub unit: i32,
    /// The chaining tolerance, mm.
    pub join_tolerance: f64,
    pub layers: Vec<String>,
    pub primitives: Vec<PrimitiveDto>,
    /// The import summary: DXF records read, vertices read, and the
    /// entities skipped, counted by kind.
    #[serde(default)]
    pub entities: usize,
    #[serde(default)]
    pub vertices: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub skipped: BTreeMap<String, usize>,
}

/// One primitive, mm and radians; `layer` indexes `SketchDto::layers`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveDto {
    Line { layer: usize, a: [f64; 2], b: [f64; 2] },
    Arc { layer: usize, c: [f64; 2], r: f64, start: f64, sweep: f64 },
    Circle { layer: usize, c: [f64; 2], r: f64 },
}

/// The payload for a profile row, `None` on every other entity.
pub(super) fn collect(world: &World, entity: &flecs_ecs::prelude::Entity) -> Option<SketchDto> {
    let sketch = crate::sketch::sketch_of(world, entity)?;
    let data = &sketch.data;
    let layers = data.layers().to_vec();
    let index = |name: &str| layers.iter().position(|l| l == name).unwrap_or(0);
    let xy = |p: Point2| [p.x, p.y];
    let primitives = data
        .primitives()
        .iter()
        .map(|item| {
            let layer = index(&item.layer);
            match item.primitive {
                Primitive::Line { start, end } => PrimitiveDto::Line { layer, a: xy(start), b: xy(end) },
                Primitive::Arc { center, radius, start_angle, sweep_angle } => {
                    PrimitiveDto::Arc { layer, c: xy(center), r: radius, start: start_angle, sweep: sweep_angle }
                }
                Primitive::Circle { center, radius } => PrimitiveDto::Circle { layer, c: xy(center), r: radius },
            }
        })
        .collect();
    Some(SketchDto {
        setup: sketch.setup,
        insunits: data.units().stated.code(),
        unit: data.units().applied.code(),
        join_tolerance: data.join_tolerance(),
        layers,
        primitives,
        entities: data.summary().entities,
        vertices: data.summary().vertices,
        skipped: data.summary().skipped.clone(),
    })
}

/// The profile rows' structural law: the payload rides a `sketch` row
/// and nothing else, and a `sketch` row is named, carries nothing but
/// it, and lies on its stock face.
pub(super) fn validate(dto: &EntityDto, kind: EntityKind) -> Result<(), ProjectError> {
    let is_sketch = matches!(kind, EntityKind::Sketch);
    if dto.sketch.is_some() != is_sketch {
        return Err(corrupt(format!(
            "'{}' {}",
            dto.name,
            if is_sketch { "is a DXF profile without its drawing" } else { "carries a DXF profile's drawing but is not one" }
        )));
    }
    if !is_sketch {
        return Ok(());
    }
    if dto.name.trim().is_empty()
        || dto.name.eq_ignore_ascii_case(STOCK_NAME)
        || LEGACY_WCS_NAMES.contains(&dto.name.as_str())
        || crate::setup::is_furniture_shape(&dto.name)
    {
        return Err(corrupt(format!("'{}' is not a usable name for a DXF profile", dto.name)));
    }
    if dto.facets.is_some() || dto.stock.is_some() || dto.tool.is_some() || dto.operation.is_some() || dto.setup.is_some() {
        return Err(corrupt(format!("'{}' is a DXF profile, not a solid, tool, operation or setup", dto.name)));
    }
    if dto.sketch.as_ref().is_some_and(|s| s.setup.as_deref().is_some_and(str::is_empty)) {
        return Err(corrupt(format!("DXF profile '{}' names an empty setup", dto.name)));
    }
    let at = Location { pos: Point3::new(dto.translation[0], dto.translation[1], dto.translation[2]), rot: dto.rotation };
    if crate::sketch::face_placement(at).is_none() {
        return Err(corrupt(format!(
            "DXF profile '{}' is placed off its stock face - a profile has no Z offset and no tilt",
            dto.name
        )));
    }
    Ok(())
}

/// Read one profile's payload into the component, chains made again.
fn read(name: &str, dto: &SketchDto) -> Result<Sketch, ProjectError> {
    let bad = |why: String| corrupt(format!("DXF profile '{name}': {why}"));
    let Units::Known(applied) = Units::from_code(dto.unit) else {
        return Err(bad(format!("unit code {} is not a unit", dto.unit)));
    };
    let at = |p: [f64; 2]| Point2::new(p[0], p[1]);
    let mut primitives = Vec::with_capacity(dto.primitives.len().min(1 << 16));
    for item in &dto.primitives {
        let (layer, primitive) = match *item {
            PrimitiveDto::Line { layer, a, b } => (layer, Primitive::Line { start: at(a), end: at(b) }),
            PrimitiveDto::Arc { layer, c, r, start, sweep } => {
                (layer, Primitive::Arc { center: at(c), radius: r, start_angle: start, sweep_angle: sweep })
            }
            PrimitiveDto::Circle { layer, c, r } => (layer, Primitive::Circle { center: at(c), radius: r }),
        };
        let layer = dto.layers.get(layer).ok_or_else(|| bad(format!("a primitive names layer {layer}, which the profile does not list")))?;
        primitives.push(LayeredPrimitive { layer: layer.clone(), primitive });
    }
    let units = SourceUnits { stated: Units::from_code(dto.insunits), applied };
    let summary = ImportSummary { entities: dto.entities, vertices: dto.vertices, skipped: dto.skipped.clone() };
    let data = SketchData::from_parts(primitives, dto.layers.clone(), units, Some(dto.join_tolerance), summary)
        .map_err(|e| bad(e.to_string()))?;
    Ok(Sketch { setup: dto.setup.clone(), data: std::sync::Arc::new(data) })
}

/// Every profile of the file, by name, read before any world write.
pub(super) fn read_all(entities: &[EntityDto]) -> Result<HashMap<String, Sketch>, ProjectError> {
    let mut out = HashMap::new();
    for dto in entities {
        if let Some(sketch) = &dto.sketch
            && out.insert(dto.name.clone(), read(&dto.name, sketch)?).is_some()
        {
            return Err(corrupt(format!("two DXF profiles are named '{}'", dto.name)));
        }
    }
    Ok(out)
}

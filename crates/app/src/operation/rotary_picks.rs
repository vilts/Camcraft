//! The rotary kinds' picked faces (#2848), read from the world: a face
//! selection on Rotary Parallel names the faces its passes keep to,
//! and a selection a rotary kind cannot use is refused by name - never
//! cut as the whole model. The axis, the gouge laws and the verify
//! still read every facet of the picked model; only the cut is kept
//! to the picks (`multiaxis::rotary_pick`).
//!
//! Each pick resolves to FACET INDICES of its model, not to points:
//! a one-model selection's material is that model's facets in order
//! (`material_of`), so the indices name the same facets after any
//! frame the job carries the material into (the WCS yaw, #760).
//!
//! Provenance: our own resolution over the feature layer's face and
//! patch ids, the walk `sinker_faces` does for the electrode picks,
//! with the rotary kinds' own refusals. No vendor material.

use flecs_ecs::prelude::World;

use crate::geometry::feature;
use crate::geometry::Triangle;
use crate::machining2d::GenError;
use crate::model;
use crate::schema::Operation;

use super::types::{FeatureSel, GeoSel, OpParams, SrcRef};

/// One picked face: its label, and its facets as indices into the
/// picked model's facet list.
pub(super) type Picked = Vec<(String, Vec<u32>)>;

/// The picks of a rotary op whose source is a selection; None for
/// every other kind and source (the whole-model read, untouched).
pub(super) fn picks_of(world: &World, op: &Operation) -> Option<Result<Picked, GenError>> {
    let SrcRef::Selection(items) = &op.src else { return None };
    match &op.params {
        OpParams::RotaryParallel { .. } => Some(resolve(world, items)),
        OpParams::RotaryContour { .. } => Some(Err(not_yet("Rotary Contour"))),
        OpParams::RotaryOffset { .. } => Some(Err(not_yet("Rotary Offset"))),
        _ => None,
    }
}

/// The refusal of a kind whose path cannot keep to picked faces yet:
/// said by name, so the picks are never silently read as the model.
fn not_yet(kind: &str) -> GenError {
    GenError::Invalid(format!(
        "{kind} cannot yet keep to picked faces - set the geometry to the whole model, or finish the picked faces with Rotary Parallel"
    ))
}

fn resolve(world: &World, items: &[GeoSel]) -> Result<Picked, GenError> {
    const KIND: &str = "Rotary Parallel";
    let Some(first) = items.first() else {
        return Err(GenError::Invalid(format!("{KIND}: the selection is empty - pick faces of the turned surface, or set the geometry to the whole model")));
    };
    if let Some(other) = items.iter().find(|g| g.entity != first.entity) {
        return Err(GenError::Invalid(format!(
            "{KIND}: the picked faces are on different models ('{}' and '{}') - the work turns about one model's axis; pick faces of one model",
            first.entity, other.entity
        )));
    }
    let gone = |label: &str| GenError::Invalid(format!("{KIND}: {label} is no longer in the model: re-pick it"));
    let name = first.entity.as_str();
    let entity = model::find_by_name(world, name)
        .ok_or_else(|| GenError::Invalid(format!("{KIND}: the selection names '{name}', which is gone")))?;
    let features = model::features_of(world, &entity).ok_or_else(|| gone(&format!("'{name}'")))?;
    let facets_of = |fid: feature::FaceId| features.faces.iter().find(|f| f.id == fid).map(|f| f.facets.clone());
    let mut out: Picked = Vec::with_capacity(items.len());
    for g in items {
        let label = crate::pick::geo_label(world, g);
        let idx = match g.feature {
            FeatureSel::Face(fid) => facets_of(fid).ok_or_else(|| gone(&label))?,
            FeatureSel::Patch(pid) => {
                let patch = feature::patches(&features).into_iter().find(|p| p.id == pid).ok_or_else(|| gone(&label))?;
                let mut all = Vec::new();
                for fid in patch.faces {
                    all.extend(facets_of(fid).ok_or_else(|| gone(&label))?);
                }
                all
            }
            FeatureSel::Chain(_) | FeatureSel::Vertex(_) | FeatureSel::SketchChain(_) | FeatureSel::SketchLayer(_) => {
                return Err(GenError::Invalid(format!(
                    "{KIND}: {label} is not a face - the passes keep to picked faces of the turned surface; pick faces, or set the geometry to the whole model"
                )));
            }
        };
        out.push((label, idx));
    }
    Ok(out)
}

/// The picked facets of `material` (the picked model's facets, in the
/// job's own frame), judged against the axis the kernel will turn
/// about - `ax`, the stated line or the fit the caller resolved
/// (#2772 parts 1-2): every pick must look away from that axis, and
/// the picks must touch. None is the whole model. A pick is never
/// dropped: one the radial tool cannot reach refuses the whole
/// selection by name.
pub(super) fn usable(
    picks: Option<Result<Picked, GenError>>,
    material: &[Triangle],
    ax: &crate::multiaxis::axis::Axis,
) -> Result<Option<Vec<Triangle>>, GenError> {
    const KIND: &str = "Rotary Parallel";
    let Some(picks) = picks else { return Ok(None) };
    let picks = picks?;
    let mut all: Vec<Triangle> = Vec::new();
    for (label, idx) in &picks {
        let face: Option<Vec<Triangle>> = idx.iter().map(|&i| material.get(i as usize).copied()).collect();
        let Some(face) = face else {
            return Err(GenError::Invalid(format!("{KIND}: {label} is no longer in the model: re-pick it")));
        };
        if !crate::multiaxis::rotary_pick::faces_outward(ax, &face) {
            return Err(GenError::Invalid(format!(
                "{KIND}: {label} does not face away from the work's axis - the tool stands on the radial, so it cannot reach an end face or the wall of a bore; pick faces of the turned surface"
            )));
        }
        all.extend(face);
    }
    let n = crate::multiaxis::rotary_pick::groups(&all);
    if n != 1 {
        return Err(GenError::Invalid(format!(
            "{KIND}: the picked faces are not connected - they form {n} separate groups; pick faces that touch, or make one operation for each group"
        )));
    }
    Ok(Some(all))
}

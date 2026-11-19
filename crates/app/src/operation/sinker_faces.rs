//! Picked part faces read as a sinker EDM cavity (#547): each pick
//! resolved by its stable feature id into a world-space face (outward
//! normal by the facets' winding, outer boundary ring), the cavity and
//! its electrode derived by the motion crate, and the model checked for
//! anything standing in the electrode's way.
//!
//! Provenance: our own glue over this repo's feature topology
//! (`geometry::feature`), the one pick rule (#692, `pick::feature_sel`)
//! and the motion crate's `sinker_cavity` / `sinker_electrode`, whose
//! module heads cite their public sources (the depression-feature rule,
//! the polygon offset, general die-sink EDM practice). No vendor
//! material.

use flecs_ecs::prelude::World;

use crate::geometry::feature::{self, FaceId, FaceKind, LoopKind};
use crate::geometry::{transform_point, Point3};
use crate::machine::{ConditionField, Kind};
use crate::machining2d::GenError;
use crate::model;
use crate::sinker_cavity::{cavity_of, welded_wall, PickedFace};
use crate::sinker_electrode::{derive, standing_in, ElectrodeBox, ElectrodeSpec};

use super::types::{FeatureSel, GeoSel};

/// A generator refusal as its own words (the command adds its prefix).
fn words(e: GenError) -> String {
    match e {
        GenError::Invalid(m) | GenError::Empty(m) => m,
        other => other.to_string(),
    }
}

/// The one model every pick is on.
///
/// # Errors
/// Picks on different models, or on something that is no part.
fn model_of(world: &World, picks: &[GeoSel]) -> Result<Option<String>, String> {
    let Some(first) = picks.first() else { return Ok(None) };
    if let Some(other) = picks.iter().find(|g| g.entity != first.entity) {
        return Err(format!(
            "electrode from faces: the picks are on different models ('{}' and '{}') - one cavity is cut in one part",
            first.entity, other.entity
        ));
    }
    let name = &first.entity;
    let e = model::find_by_name(world, name).ok_or_else(|| format!("electrode from faces: no model named '{name}'"))?;
    if !model::is_model(world, &e) || model::is_fixture(world, &e) {
        return Err(format!("electrode from faces: '{name}' is not a part - pick the faces of the part's cavity"));
    }
    Ok(Some(name.clone()))
}

/// One pick as world-space faces: a planar face as itself, a welded
/// smooth face (#1808) as its planar strips, held by the motion crate's
/// `welded_wall` to be vertical. A strip also picked on its own is the
/// same wall twice, which the cavity read takes as one.
fn faces_of(world: &World, setup: Option<&str>, g: &GeoSel) -> Result<Vec<PickedFace>, String> {
    let label = crate::pick::geo_label(world, g);
    let pid = match g.feature {
        FeatureSel::Face(fid) => return Ok(vec![face_of(world, setup, g, fid, label)?]),
        FeatureSel::Patch(pid) => pid,
        FeatureSel::Chain(_) | FeatureSel::Vertex(_) => {
            return Err(format!(
                "electrode from faces: {label} is not a face - an electrode is generated from the cavity's floor and wall faces"
            ));
        }
    };
    // #692: the one pick rule - an id is taken only if a click could land on it.
    crate::pick::check_pickable(world, g).map_err(|why| format!("electrode from faces: {why}"))?;
    let gone = || format!("electrode from faces: {label} is no longer in the model: re-pick it");
    let e = model::find_by_name(world, &g.entity).ok_or_else(gone)?;
    let features = model::features_of(world, &e).ok_or_else(gone)?;
    let patch = feature::patches(&features).into_iter().find(|p| p.id == pid).ok_or_else(gone)?;
    let mut strips = Vec::with_capacity(patch.faces.len());
    for &fid in &patch.faces {
        let s = GeoSel { entity: g.entity.clone(), feature: FeatureSel::Face(fid) };
        let strip = format!("{} (a strip of {label})", crate::pick::geo_label(world, &s));
        strips.push(face_of(world, setup, &s, fid, strip)?);
    }
    welded_wall(&label, strips).map_err(words)
}

/// The planar-face pick `g` (face `fid`) as a setup-frame face, named
/// `label`. A face whose rings refused to close comes back marked open:
/// the cavity read refuses it by name.
fn face_of(world: &World, setup: Option<&str>, g: &GeoSel, fid: FaceId, label: String) -> Result<PickedFace, String> {
    let blank = |open: bool| PickedFace { label: label.clone(), normal: Point3::new(0.0, 0.0, 1.0), curved: false, open, outer: Vec::new() };
    // #692: the one pick rule - an id is taken only if a click could land on it.
    crate::pick::check_pickable(world, g).map_err(|why| format!("electrode from faces: {why}"))?;
    let gone = || format!("electrode from faces: {label} is no longer in the model: re-pick it");
    let e = model::find_by_name(world, &g.entity).ok_or_else(gone)?;
    let features = model::features_of(world, &e).ok_or_else(gone)?;
    let f = features.faces.iter().find(|f| f.id == fid).ok_or_else(gone)?;
    if !matches!(f.kind, FaceKind::Planar) {
        return Err(format!(
            "electrode from faces: {label} is a degenerate face (zero-area or non-finite facets) - it bounds nothing"
        ));
    }
    // The outside by the facets' winding; an open, inside-out or
    // non-manifold body is refused by name there (#366).
    let n = crate::pick::align::outward_normal(world, g, |w, entity| crate::setup_align::pose_in(w, entity, setup))
        .map_err(|why| format!("electrode from faces: {why}"))?
        .ok_or_else(|| format!("electrode from faces: {label} has no usable plane - re-pick it"))?;
    let Some(outer) = f.loops.iter().find(|l| matches!(l.kind, LoopKind::Outer)) else {
        return Ok(blank(true));
    };
    if f.loops.iter().any(|l| matches!(l.kind, LoopKind::Open)) {
        return Ok(blank(true));
    }
    let loc = crate::setup_align::pose_in(world, &e, setup).unwrap_or_default();
    let ring: Option<Vec<Point3>> = outer
        .verts
        .iter()
        .map(|&v| features.verts.get(v as usize).map(|p| transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot)))
        .collect();
    let Some(ring) = ring else { return Ok(blank(true)) };
    Ok(PickedFace { label, normal: Point3::new(n[0], n[1], n[2]), curved: false, open: false, outer: ring })
}

/// Derive an electrode in the active setup (defaulting to document frame
/// when no setup exists).
///
/// # Errors
/// Every refusal of [`electrode_from_faces_in`].
pub fn electrode_from_faces(world: &World, picks: &[GeoSel], spec: &ElectrodeSpec) -> Result<ElectrodeBox, String> {
    let active = crate::setup::active(world);
    electrode_from_faces_in(world, active.as_deref(), picks, spec)
}

/// The electrode the picked cavity faces call for in `setup`'s frame:
/// one level floor and the vertical walls standing on every edge of it,
/// offset inward by the spec's undersize (and orbit), extended by its stub.
///
/// # Errors
/// Every refusal of the cavity read and the derivation, by name; picks
/// on different models or off a part; a stale id; and a model that
/// stands in the cavity's own column (a boss, a rib, an overhang).
pub(crate) fn electrode_from_faces_in(
    world: &World,
    setup: Option<&str>,
    picks: &[GeoSel],
    spec: &ElectrodeSpec,
) -> Result<ElectrodeBox, String> {
    let name = model_of(world, picks)?;
    let mut seen: Vec<&GeoSel> = Vec::new();
    let mut faces = Vec::with_capacity(picks.len());
    for g in picks {
        // The same face picked twice is one face.
        if seen.contains(&g) {
            continue;
        }
        seen.push(g);
        faces.extend(faces_of(world, setup, g)?);
    }
    let cavity = cavity_of(&faces).map_err(words)?;
    let electrode = derive(&cavity, spec).map_err(words)?;
    // The obstruction check reads the part's facets; a part whose
    // facets cannot be read is refused by name, never checked against an
    // empty list (review of #1156: never silence).
    let facets = name
        .as_deref()
        .and_then(|n| model::find_by_name(world, n))
        .and_then(|e| crate::setup_align::facets_in(world, &e, setup))
        .ok_or_else(|| {
            "electrode from faces: the part's facets could not be read, so the cavity's column cannot be checked for a boss, a rib or an overhang - no electrode is made".to_owned()
        })?;
    if let Some(at) = standing_in(&electrode, &facets).map_err(words)? {
        return Err(format!(
            "electrode from faces: the part stands inside the cavity above its floor near ({:.3}, {:.3}, {:.3}) - a boss, a rib or an overhang is in the electrode's way, and an electrode shaped round it is not modelled",
            at.x, at.y, at.z
        ));
    }
    Ok(electrode)
}

/// The electrode undersize per side of condition `id` on the setup's
/// machine (#531's table, #543's reading): the door for "size the
/// electrode for this condition". `setup` `None` is the active setup.
///
/// # Errors
/// By name: a machine that is not a sinker EDM, no condition named, no
/// EDM section, a condition that is missing or states no undersize.
pub fn condition_undersize(world: &World, setup: Option<&str>, id: &str) -> Result<f64, String> {
    let active = crate::setup::active(world);
    let setup = setup.or(active.as_deref());
    let kind = crate::machine::kind_of_setup(world, setup);
    if kind != Kind::SinkerEdm {
        return Err(format!(
            "a sinker electrode's undersize is read from a sinker EDM's condition; the setup's machine is a {}",
            kind.word()
        ));
    }
    let mounted = crate::machine::of_setup(world, setup);
    let label = crate::setup_mount::machine_label(&mounted);
    let id = id.trim();
    if id.is_empty() {
        return Err(format!("no EDM condition is named - name one of machine {label}'s conditions (its undersize sizes the electrode)"));
    }
    let edm = mounted
        .1
        .as_ref()
        .and_then(|m| m.edm.as_ref())
        .ok_or_else(|| format!("machine {label} states no EDM section, so no condition {id:?}"))?;
    let c = edm.condition(&label, id, &[ConditionField::Undersize])?;
    c.undersize.ok_or_else(|| format!("machine {label}: EDM condition {id:?} states no undersize"))
}

/// A warning by name for a stated undersize of 0 (review of #1156): the
/// electrode is then the cavity's full size, and any spark gap burns
/// its walls past the finished size - #543's size law refuses a rough
/// or finish of it. A condition's undersize never needs it: the machine
/// file already refuses a condition undersized below its own gap.
/// `None` otherwise.
#[must_use]
pub fn undersize_warning(undersize: f64) -> Option<String> {
    (undersize == 0.0).then(|| {
        "an undersize of 0 makes the electrode the cavity's full size: any spark gap burns the walls past it, and a Sinker Burn rough or finish of it is refused by the size law (#543) - state the undersize your condition calls for, or read it with \"condition\"".to_owned()
    })
}

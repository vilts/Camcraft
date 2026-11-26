//! "Do not touch" world resolution, overlap checking, rapid routing, and live post gate (#2055).
//! Provenance: our own feature-to-AvoidSpec adapter over `geometry::feature` and `sim::verify::avoid`.

use std::collections::{HashMap, HashSet};

use flecs_ecs::prelude::World;

use super::generate::stock_to_leave_of;
use super::*;
use crate::geometry::feature;
use crate::verify::{AVOID_PAD, AVOID_VERIFY_TOL, AvoidItem, AvoidSpec, Cutter};

fn to_job(loc: &crate::schema::Location, p: Point3, frame: Option<&crate::cl::TiltedFrame>) -> Point3 {
    let wp = transform_point(p, loc.pos, loc.rot);
    match frame {
        Some(f) => f.to_local(wp),
        None => wp,
    }
}

/// Resolve every protected pick into world (or tilted-job-frame) triangles and 3D edges.
pub(super) fn collect_items(
    world: &World,
    setup: Option<&str>,
    dnt: &DoNotTouch,
    frame: Option<&crate::cl::TiltedFrame>,
) -> Result<Vec<AvoidItem>, GenError> {
    if !dnt.is_sane() {
        return Err(GenError::Invalid(
            "'Do not touch' picks or clearance are invalid (use non-negative finite clearance and faces or edges)".into(),
        ));
    }
    type CachedMesh = (std::sync::Arc<[Triangle]>, std::sync::Arc<feature::Features>, crate::schema::Location);
    let mut cache: HashMap<&str, CachedMesh> = HashMap::new();
    let mut out = Vec::with_capacity(dnt.picks.len());
    for g in &dnt.picks {
        let desc = describe_sel(g);
        if matches!(g.feature, FeatureSel::Vertex(_)) {
            return Err(GenError::Invalid(format!(
                "{desc} is a vertex - 'Do not touch' protects faces or edge chains, not vertices"
            )));
        }
        if matches!(g.feature, FeatureSel::SketchChain(_) | FeatureSel::SketchLayer(_)) {
            return Err(GenError::Invalid(format!(
                "{desc} is a sketch pick - 'Do not touch' protects model faces or edge chains, not DXF sketch chains"
            )));
        }
        if !cache.contains_key(g.entity.as_str()) {
            let entity = model::find_by_name(world, &g.entity).ok_or_else(|| {
                GenError::Invalid(format!("'Do not touch' references '{}', which is gone", g.entity))
            })?;
            let facets = model::facets_of(world, &entity).ok_or_else(|| {
                GenError::Invalid(format!("'{}' has no facets for 'Do not touch'", g.entity))
            })?;
            let features = model::features_of(world, &entity).ok_or_else(|| {
                GenError::Invalid(format!("'{}' has no selectable features for 'Do not touch'", g.entity))
            })?;
            let loc = crate::setup_align::pose_in(world, &entity, setup).unwrap_or_default();
            cache.insert(g.entity.as_str(), (facets, features, loc));
        }
        let Some((facets, features, loc)) = cache.get(g.entity.as_str()) else {
            return Err(GenError::Invalid(format!("missing cached entity '{}'", g.entity)));
        };
        let mut tris = Vec::new();
        let mut edges = Vec::new();
        match g.feature {
            FeatureSel::Face(fid) => {
                let fi = features.faces.iter().position(|f| f.id == fid).ok_or_else(|| {
                    GenError::Invalid(format!("'Do not touch' {desc} is no longer in '{}' - re-pick it", g.entity))
                })?;
                for (ti, tri) in facets.iter().enumerate() {
                    if features.facet_face.get(ti) == Some(&(fi as u32)) {
                        tris.push(Triangle::new(
                            to_job(loc, tri.a, frame),
                            to_job(loc, tri.b, frame),
                            to_job(loc, tri.c, frame),
                        ));
                    }
                }
                if tris.is_empty() {
                    return Err(GenError::Invalid(format!("'Do not touch' {desc} has no facets")));
                }
            }
            FeatureSel::Patch(pid) => {
                let patches = feature::patches(features);
                let patch = patches.into_iter().find(|p| p.id == pid).ok_or_else(|| {
                    GenError::Invalid(format!("'Do not touch' {desc} is no longer in '{}' - re-pick it", g.entity))
                })?;
                let face_ids: HashSet<u64> = patch.faces.into_iter().collect();
                let face_idxs: HashSet<u32> = features
                    .faces
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| face_ids.contains(&f.id))
                    .map(|(i, _)| i as u32)
                    .collect();
                for (ti, tri) in facets.iter().enumerate() {
                    if features.facet_face.get(ti).is_some_and(|fi| face_idxs.contains(fi)) {
                        tris.push(Triangle::new(
                            to_job(loc, tri.a, frame),
                            to_job(loc, tri.b, frame),
                            to_job(loc, tri.c, frame),
                        ));
                    }
                }
                if tris.is_empty() {
                    return Err(GenError::Invalid(format!("'Do not touch' {desc} has no facets")));
                }
            }
            FeatureSel::Chain(cid) => {
                let chain = feature::resolve_chain(features, cid).ok_or_else(|| {
                    GenError::Invalid(format!("'Do not touch' {desc} is no longer in '{}' - re-pick it", g.entity))
                })?;
                let pts: Vec<Point3> = feature::chain_points(features, &chain)
                    .into_iter()
                    .map(|p| to_job(loc, Point3::new(p[0], p[1], p[2]), frame))
                    .collect();
                if pts.len() < 2 {
                    return Err(GenError::Invalid(format!("'Do not touch' {desc} has fewer than 2 points")));
                }
                for w in pts.windows(2) {
                    edges.push(([w[0].x, w[0].y, w[0].z], [w[1].x, w[1].y, w[1].z]));
                }
                if chain.closed && pts.len() >= 3 && pts.first() != pts.last()
                    && let (Some(a), Some(b)) = (pts.last(), pts.first())
                {
                    edges.push(([a.x, a.y, a.z], [b.x, b.y, b.z]));
                }
            }
            FeatureSel::Vertex(_) | FeatureSel::SketchChain(_) | FeatureSel::SketchLayer(_) => {
                return Err(GenError::Invalid(format!(
                    "{desc} cannot be protected - 'Do not touch' protects faces or edge chains"
                )));
            }
        }
        out.push(AvoidItem { label: desc, tris, edges });
    }
    Ok(out)
}

/// Check both direct equality and Patch-vs-Face containment against the machining selection/Include boundaries.
#[must_use]
pub fn overlap_issue(
    world: &World,
    picks: &[GeoSel],
    src: Option<&SrcRef>,
    area: &AreaControls,
) -> Option<String> {
    if let Some(msg) = do_not_touch_overlap_issue(picks, src, area) {
        return Some(msg);
    }
    if picks.is_empty() {
        return None;
    }
    let cut_picks: Vec<&GeoSel> = match src {
        Some(SrcRef::Selection(sel)) => sel.iter().collect(),
        _ => Vec::new(),
    }
    .into_iter()
    .chain(area.extra.iter().filter_map(|eb| match (&eb.mode, &eb.shape) {
        (BoundaryMode::Include, BoundaryShape::Picks(bps)) => Some(bps.iter()),
        _ => None,
    }).flatten())
    .collect();
    if cut_picks.is_empty() {
        return None;
    }
    let patch_faces = |g: &GeoSel| -> HashSet<u64> {
        let mut out = HashSet::new();
        if let FeatureSel::Face(fid) = g.feature {
            out.insert(fid);
            return out;
        }
        if let FeatureSel::Patch(pid) = g.feature
            && let Some(e) = model::find_by_name(world, &g.entity)
            && let Some(f) = model::features_of(world, &e)
            && let Some(p) = feature::patches(&f).into_iter().find(|p| p.id == pid)
        {
            out.extend(p.faces);
        }
        out
    };
    for p in picks {
        let pf = patch_faces(p);
        if pf.is_empty() {
            continue;
        }
        for c in &cut_picks {
            if p.entity == c.entity && !pf.is_disjoint(&patch_faces(c)) {
                return Some(format!(
                    "'Do not touch' item {} overlaps machining selection {} - remove it from one list",
                    describe_sel(p),
                    describe_sel(c)
                ));
            }
        }
    }
    None
}

#[must_use]
pub(super) fn spec_of(
    items: Vec<AvoidItem>,
    clearance: f64,
    params: &OpParams,
    cutter: Cutter,
) -> AvoidSpec {
    let stl = stock_to_leave_of(params).max(0.0);
    AvoidSpec {
        items,
        cutter,
        clearance: clearance.max(0.0) + stl,
    }
}

/// Elevate upright rapid moves over protected items; return refusal notes if `z_ceiling` blocks them.
pub fn route_links(path: &mut Toolpath, spec: &AvoidSpec, z_ceiling: Option<f64>) -> Vec<String> {
    verify::elevate_rapid_links(path, spec, z_ceiling, AVOID_PAD)
}

/// Independent move-by-move check against every protected item, returning `(warnings, notes)`.
pub fn verify_avoid(path: &Toolpath, spec: &AvoidSpec) -> (Vec<Warning>, Vec<String>) {
    if spec.is_empty() {
        return (Vec::new(), Vec::new());
    }
    match spec.hits_named(path, AVOID_VERIFY_TOL) {
        Ok(hits) => {
            let mut warns = Vec::with_capacity(hits.len());
            let mut notes = Vec::new();
            let mut noted = HashSet::new();
            for (w, label) in hits {
                if let Warning::AvoidHit { part, short_mm, .. } = w
                    && noted.insert(label.clone())
                {
                    notes.push(format!(
                        "'Do not touch' violated on {label} ({part:?}, {short_mm:.3} mm short of {:.2} mm clearance)",
                        spec.clearance
                    ));
                }
                warns.push(w);
            }
            (warns, notes)
        }
        Err(e) => (Vec::new(), vec![format!("'Do not touch' verification failed: {e}")]),
    }
}

/// Restrict finishing to centres whose full tool clears the protected items
/// throughout every cut, lift and plunge up to the clearance plane.
pub(super) fn finish_centre_area(
    clip: Option<Region>,
    region: &Region,
    spec: Option<&AvoidSpec>,
    fp: &Finish3DParams,
    yaw: Option<super::wcs_yaw::WcsYaw>,
) -> Result<Option<Region>, GenError> {
    let Some(spec) = spec.filter(|s| !s.is_empty()) else { return Ok(clip) };
    let keepout = spec.keepout_region(fp.bottom_z, fp.clearance_z, AVOID_PAD)
        .map_err(|e| GenError::Invalid(format!("3D Finish cannot protect 'Do not touch' {}: {e}", spec.names_list())))?;
    if keepout.is_empty() {
        return Ok(clip);
    }
    // Material and the centre area already use the WCS generation frame.
    // The independent final check keeps the original setup-frame spec.
    let keepout = match yaw {
        Some(yaw) => super::wcs_yaw::WcsYaw { origin: yaw.origin, yaw_rad: -yaw.yaw_rad }.exit_region(&keepout)?,
        None => keepout,
    };
    let allowed = match &clip {
        Some(c) => c.clone(),
        None => region.offset(fp.tool_diameter * 0.5)?,
    };
    if allowed.intersect(&keepout)?.is_empty() {
        return Ok(clip);
    }
    let allowed = allowed.difference(&keepout)?;
    Ok(Some(allowed))
}

/// Elevate upright rapid links and refuse any remaining protected-item contact.
pub(super) fn protect_path(
    path: &mut Toolpath,
    spec: &AvoidSpec,
    upright: bool,
    z_ceiling: Option<f64>,
    kind: OpType,
) -> Result<(), GenError> {
    if upright {
        let notes = route_links(path, spec, z_ceiling);
        path.notes.extend(notes);
    }
    if let Some(problem) = clearance_problems(path, spec).first() {
        return Err(GenError::Invalid(format!("{kind}: {problem}")));
    }
    Ok(())
}

fn clearance_problems(path: &Toolpath, spec: &AvoidSpec) -> Vec<String> {
    let hits = match spec.hits_named(path, AVOID_VERIFY_TOL) {
        Ok(hits) => hits,
        Err(e) => return vec![format!("cannot verify 'Do not touch' on {}: {e}", spec.names_list())],
    };
    let mut problems = Vec::new();
    let mut seen = HashSet::new();
    for (w, label) in hits {
        if let Warning::AvoidHit { at, part, short_mm, .. } = w
            && seen.insert(label.clone())
        {
            problems.push(format!(
                "violates 'Do not touch' {label} at move {} ({part:?}, {short_mm:.3} mm short of {:.2} mm clearance)",
                at + 1,
                spec.clearance
            ));
        }
    }
    problems
}

/// Live post/verify check over every active cached operation with "Do not touch" configured.
#[must_use]
pub fn live_avoid_problems(world: &World) -> Vec<String> {
    let mut problems = Vec::new();
    for (name, e) in crate::setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        if op.suppressed || op.area.do_not_touch.is_default() {
            continue;
        }
        let Some(cache) = model::toolpath_of(world, &e) else {
            continue;
        };
        if let Some(why) = do_not_touch_unsupported(op.params.kind()) {
            problems.push(format!("{name}: {why}"));
            continue;
        }
        if let Some(why) = overlap_issue(world, &op.area.do_not_touch.picks, Some(&op.src), &op.area) {
            problems.push(format!("{name}: {why}"));
            continue;
        }
        let items = match collect_items(world, op.setup.as_deref(), &op.area.do_not_touch, None) {
            Ok(it) => it,
            Err(e) => {
                problems.push(format!("{name}: {e}"));
                continue;
            }
        };
        let (tool, body, _, neck) = match super::tool_gate::job_tool(world, &op) {
            Ok(t) => t,
            Err(e) => {
                problems.push(format!("{name}: {e}"));
                continue;
            }
        };
        let geom = tool.geom();
        let holder = crate::tool_body::stack(&geom, body.as_ref()).with_neck(neck);
        let cutter = Cutter::of(&tool).with_body(&geom, &holder);
        let spec = spec_of(items, op.area.do_not_touch.clearance, &op.params, cutter);
        problems.extend(clearance_problems(&cache.path, &spec).into_iter().map(|why| format!("{name}: {why}")));
    }
    problems
}

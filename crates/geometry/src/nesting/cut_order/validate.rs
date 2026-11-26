//! Provenance: issue #2159 sheet hold-down rules and ordered-phase scheduling.

use std::collections::{BTreeMap, BTreeSet};

use super::{CutFeature, CutGeometry, CutPhase, GeoError, PlannedCut, geometry::Node, invalid};

pub(super) fn requests(features: &[CutFeature]) -> Result<(), GeoError> {
    let mut ids = BTreeSet::new();
    for feature in features {
        if !ids.insert(feature.id) {
            return Err(invalid(format!(
                "duplicate sheet feature id {}",
                feature.id
            )));
        }
        let phases: BTreeSet<_> = feature.passes.iter().collect();
        if phases.is_empty() || phases.len() != feature.passes.len() {
            return Err(invalid(format!(
                "feature {} needs nonempty, distinct pass phases",
                feature.id
            )));
        }
        if let Some(phase) = feature.release_phase
            && (!matches!(feature.geometry, CutGeometry::Closed(_))
                || !matches!(phase, CutPhase::Internal | CutPhase::Breakthrough)
                || !feature.passes.contains(&phase))
        {
            return Err(invalid(format!(
                "feature {} needs a requested closed through pass for release_phase",
                feature.id
            )));
        }
        if matches!(feature.geometry, CutGeometry::Closed(_))
            && feature.passes.contains(&CutPhase::Breakthrough)
            && feature.release_phase.is_none()
        {
            return Err(invalid(format!(
                "contour {} Breakthrough must release material",
                feature.id
            )));
        }
    }
    Ok(())
}

pub(super) fn order(
    features: &[CutFeature],
    nodes: &[Node],
    cuts: &[PlannedCut],
) -> Result<(), GeoError> {
    let indices: BTreeMap<_, _> = features
        .iter()
        .enumerate()
        .map(|(i, f)| (f.id, i))
        .collect();
    let mut seen = BTreeSet::new();
    let mut released = vec![false; features.len()];
    let mut previous = None;
    for cut in cuts {
        let Some(&i) = indices.get(&cut.feature_id) else {
            return Err(invalid(format!(
                "cut names unknown feature {}",
                cut.feature_id
            )));
        };
        if !features[i].passes.contains(&cut.phase) || !seen.insert((cut.feature_id, cut.phase)) {
            return Err(invalid(format!(
                "feature {} has an unrequested or duplicate {:?} pass",
                cut.feature_id, cut.phase
            )));
        }
        if previous.is_some_and(|phase| phase > cut.phase) {
            return Err(invalid(format!(
                "feature {} violates global pass phase order",
                cut.feature_id
            )));
        }
        previous = Some(cut.phase);
        let mut at = Some(i);
        while let Some(j) = at {
            if released[j] {
                return Err(invalid(format!(
                    "feature {} cuts after contour {} freed its material",
                    cut.feature_id, features[j].id
                )));
            }
            at = nodes[j].parent;
        }
        if features[i].release_phase == Some(cut.phase) {
            released[i] = true;
        }
    }
    for feature in features {
        for phase in &feature.passes {
            if !seen.contains(&(feature.id, *phase)) {
                return Err(invalid(format!(
                    "feature {} is missing its {:?} pass",
                    feature.id, phase
                )));
            }
        }
    }
    Ok(())
}

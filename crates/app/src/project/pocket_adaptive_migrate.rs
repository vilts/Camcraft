//! Migration for pre-v69 operations saved as 2D Pocket with the retired
//! `Adaptive` strategy (#1517): opens each as `2D Adaptive` (`Spiral`),
//! carrying every matching field unchanged. Any un-carriable setting
//! (`rest`, `links.approach.predrilled`) is turned off, noted on load,
//! and recorded in `needs_review` so posting refuses until the user
//! reviews the operation.
//!
//! Provenance: our own persistence contract; no vendor material consulted.

use super::{
    AdaptiveMethodDto, AdaptiveTrochReturnDto, ApproachDto, EntityDto, EntityKind, LinksDto,
    ParamsDto, PassStartDto, StrategyDto,
};

/// Convert every pre-v69 `Pocket { strategy: Adaptive }` operation into
/// `Adaptive2D (Spiral)`, returning human-readable load notes.
pub(super) fn settle(entities: &mut [EntityDto], from: u32) -> Vec<String> {
    if from >= 69 {
        return Vec::new();
    }
    let mut notes = Vec::new();
    for dto in entities.iter_mut() {
        if dto.kind != Some(EntityKind::Operation) {
            continue;
        }
        let Some(op) = dto.operation.as_mut() else {
            continue;
        };
        let ParamsDto::Pocket {
            links,
            stepover,
            stock_to_leave,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy: StrategyDto::Adaptive,
            retract_z,
            entry,
            helix_radius,
            max_engagement,
            optimal_load,
            blend_turns,
            blend_radius,
            rest,
            corner_relief,
            tilt,
            // #3354: no retired Adaptive file carries the key; 2D
            // Adaptive takes it in its own issue.
            pass_start: _,
            picks,
        } = op.params.clone()
        else {
            continue;
        };

        let mut dropped = Vec::new();
        let mut next_links = links;
        if let Some(l) = next_links.as_mut()
            && let Some(a) = l.approach.as_mut()
            && a.predrilled.is_some()
        {
            if a.predrilled == Some(true) {
                dropped.push(
                    "pre-drilled entry (2D Adaptive does not support pre-drilled entry yet)"
                        .to_owned(),
                );
            }
            a.predrilled = None;
            if *a == ApproachDto::default() {
                l.approach = None;
            }
            if *l == LinksDto::default() {
                next_links = None;
            }
        }
        if rest.is_some() {
            dropped.push(
                "rest machining (2D Adaptive does not support 2D Pocket rest machining yet)"
                    .to_owned(),
            );
        }

        op.params = ParamsDto::Adaptive2D {
            links: next_links,
            stepover,
            stock_to_leave,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            retract_z,
            entry,
            helix_radius,
            max_engagement: if optimal_load.is_some() { None } else { max_engagement },
            optimal_load,
            method: AdaptiveMethodDto::Spiral,
            troch_return: AdaptiveTrochReturnDto::Auto,
            lift: None,
            return_feed: None,
            loop_width: None,
            blend_turns,
            blend_radius,
            // #1446: the Pocket's rest group is not carried (noted above):
            // the 2D Adaptive reads the previous operations only.
            rest: None,
            corner_relief,
            tilt,
            pass_start: PassStartDto::Auto,
            picks,
        };
        // Adaptive2D uses its own kind tag in the generation stamp digest.
        op.stamp = None;

        if dropped.is_empty() {
            notes.push(format!(
                "operation '{}': migrated from retired 2D Pocket Adaptive strategy to 2D Adaptive (Spiral)",
                dto.name
            ));
        } else {
            // Never retain a cache produced under dropped entry/rest parameters.
            op.cache = None;
            for item in &dropped {
                if !op.needs_review.contains(item) {
                    op.needs_review.push(item.clone());
                }
            }
            notes.push(format!(
                "operation '{}': migrated from retired 2D Pocket Adaptive to 2D Adaptive (Spiral); turned off {} - marked needs review",
                dto.name,
                dropped.join("; ")
            ));
        }
    }
    notes
}

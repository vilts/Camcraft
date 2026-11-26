// Provenance: clean-room classifier mapping operation kinematics intent to `PARAM SectionMode`.

//! Emit `PARAM SectionMode` per operation stream (`3-axis`, `3+2 indexed`, `simultaneous 5-axis`).

use crate::cl::{is_vertical, Toolpath};
use crate::operation::OpParams;
use crate::post::{self, cnc::Record, SectionMode};

/// Classify an operation and its posted toolpath into an explicit [`SectionMode`],
/// returning `None` for non-milling sections (turning, plasma, EDM, live-tool).
#[must_use]
pub fn classify(params: &OpParams, path: &Toolpath) -> Option<SectionMode> {
    match params {
        // Planar operations support an explicit 3+2 index (`tilt`).
        OpParams::Face { .. }
        | OpParams::Contour { .. }
        | OpParams::Pocket { .. }
        | OpParams::Adaptive2D { .. }
        | OpParams::Offset2D { .. }
        | OpParams::VCarve { .. }
        | OpParams::Engrave { .. }
        | OpParams::Chamfer2D { .. }
        | OpParams::Circular { .. }
        | OpParams::Slot { .. }
        | OpParams::Trace { .. } => Some(if params.tilt().is_some() {
            SectionMode::Indexed3Plus2
        } else {
            SectionMode::ThreeAxis
        }),

        // Continuous multi-axis simultaneous operations.
        OpParams::Contour5D { .. }
        | OpParams::Parallel5D { .. }
        | OpParams::Swarf { .. }
        | OpParams::AdvancedSwarf { .. }
        | OpParams::Flow5D { .. }
        | OpParams::MultiAxisContour { .. }
        | OpParams::MultiAxisRough { .. }
        | OpParams::RotaryContour { .. }
        | OpParams::RotaryParallel { .. }
        | OpParams::RotaryOffset { .. }
        // The flow band turns the work about the stated axis on
        // every pass (#2826): simultaneous, the twins' answer.
        | OpParams::RotaryFlow { .. } => {
            if post::dialect::states_live_tool(path) {
                None
            } else {
                Some(SectionMode::Simultaneous5Axis)
            }
        }

        // Fixed-vertical 3-axis milling/hole/probe kinds.
        OpParams::TrochoidalSlot { .. }
        | OpParams::Pocket3D { .. }
        | OpParams::Finish3D { .. }
        | OpParams::Bore { .. }
        | OpParams::Thread { .. }
        | OpParams::Probe { .. } => Some(SectionMode::ThreeAxis),

        OpParams::Deburr { .. } | OpParams::MultiAxisDeburr { .. } => {
            let leans = path
                .moves
                .iter()
                .any(|m| m.axis.is_some_and(|a| !is_vertical(&a)));
            Some(if leans {
                SectionMode::Simultaneous5Axis
            } else {
                SectionMode::ThreeAxis
            })
        }

        OpParams::Drill { cross, .. } => {
            if !cross.is_empty() || post::dialect::states_live_tool(path) {
                None
            } else {
                Some(SectionMode::ThreeAxis)
            }
        }

        // Non-milling (turning, plasma, wire/sinker EDM) stay unannotated.
        OpParams::Facing { .. }
        | OpParams::ProfileRough { .. }
        | OpParams::ProfileFinish { .. }
        | OpParams::GrooveRough { .. }
        | OpParams::GrooveFinish { .. }
        | OpParams::Part { .. }
        | OpParams::Threading { .. }
        | OpParams::Chamfering { .. }
        | OpParams::TaperTurning { .. }
        | OpParams::MultiPassTurning { .. }
        | OpParams::FormTurning { .. }
        | OpParams::BoringTurning { .. }
        | OpParams::PlasmaProfile(_) | OpParams::WaterjetProfile(_)
        | OpParams::WireContour(_)
        | OpParams::SinkerBurn(_)
        | OpParams::SlotUndercut(_)
        | OpParams::SinkerSideBurn(_) => None,
    }
}

/// Annotate a generated operation record stream with `PARAM SectionMode` before `SECTION`.
pub fn insert(params: &OpParams, path: &Toolpath, records: &mut Vec<Record>) {
    if let Some(mode) = classify(params, path) {
        post::set_section_mode(records, mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::{Move, MoveKind};
    use crate::geometry::Point3;
    use crate::machining2d::FaceHoles;
    use crate::operation::{Height, Tilt};

    fn sample_face(tilt: Option<Tilt>) -> OpParams {
        OpParams::Face {
            stepover: 3.0,
            clearance_z: Height::fixed(10.0),
            cut_z: Height::fixed(0.0),
            top_z: None,
            stepdown: None,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            blend_turns: false, blend_radius: None,
            holes: FaceHoles::default(),
            tilt,
            picks: Default::default(),
        }
    }

    #[test]
    fn classifies_3axis_3plus2_and_simultaneous_distinctly() {
        let empty = Toolpath::default();
        assert_eq!(classify(&sample_face(None), &empty), Some(SectionMode::ThreeAxis));
        assert_eq!(
            classify(
                &sample_face(Some(Tilt { basis_version: 1, a_deg: 0.0, b_deg: 45.0, origin: [0.0; 3] })),
                &empty
            ),
            Some(SectionMode::Indexed3Plus2)
        );
        let sim = OpParams::Flow5D {
            tolerance: 0.1,
            stepover: Some(2.0),
            clearance_z: Height::fixed(20.0),
            retract_z: None,
            top_z: Height::fixed(10.0),
            bottom_z: Height::fixed(0.0),
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            lead_deg: 10.0,
            lag_deg: 0.0,
            max_tilt_deg: 45.0,
            shank_avoid: false,
            picks: Default::default(),
        };
        assert_eq!(classify(&sim, &empty), Some(SectionMode::Simultaneous5Axis));

        // Live-tool turning (rotary field populated on Move) stays unannotated.
        let mut live_path = Toolpath::default();
        let mut m = Move::new(
            MoveKind::Feed,
            Point3::ZERO,
            Point3::new(1.0, 0.0, 0.0),
            Some(400.0),
        );
        m.rotary = Some(15.0);
        live_path.moves.push(m);
        let rotary_op = OpParams::RotaryContour {
            tolerance: 0.05,
            clearance_z: Height::fixed(25.0),
            retract_z: None,
            top_z: Height::fixed(15.0),
            bottom_z: Height::fixed(10.0),
            feed: 400.0,
            plunge_feed: 100.0,
            climb: true,
            lead_deg: 0.0,
            lag_deg: 0.0,
            max_tilt_deg: 45.0,
            stated_axis: None,
            picks: Default::default(),
        };
        assert_eq!(classify(&rotary_op, &empty), Some(SectionMode::Simultaneous5Axis));
        assert_eq!(classify(&rotary_op, &live_path), None);
    }
}

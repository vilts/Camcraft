//! Which glyph each operation kind wears - in the ribbon's Create cells,
//! the browser tree's op rows and the operation pages alike.
//! Provenance: our own mapping (#661: every `OpType` its own original
//! drawing, SVG masters in assets/ui/svg/); split out of panes.rs.

use camcraft_app::operation::OpType;

use super::icons::Icon;

/// The op row's glyph. Exhaustive on purpose: a new `OpType` without a
/// glyph stops at the compiler, and the structural tests pin that no two
/// kinds share one (#661 retired the old reuse law, under which Facing,
/// the profile kinds and every turning blade wore the face mill's pixels).
pub(crate) fn op_icon(kind: OpType) -> Icon {
    match kind {
        OpType::Face => Icon::Face,
        OpType::Contour => Icon::Contour,
        OpType::Pocket => Icon::Pocket,
        OpType::Adaptive2D => Icon::Adaptive2D,
        OpType::Offset2D => Icon::Offset2D,
        OpType::Slot => Icon::Slot,
        OpType::TrochoidalSlot => Icon::TrochoidalSlot,
        OpType::Trace => Icon::Trace,
        OpType::Engrave => Icon::Engrave,
        OpType::Chamfer2D => Icon::Chamfer2D,
        OpType::Deburr => Icon::Deburr,
        OpType::Circular => Icon::Circular,
        OpType::PlasmaProfile => Icon::PlasmaProfile,
        OpType::WireContour => Icon::WireContour,
        OpType::SinkerBurn => Icon::SinkerBurn,
        // #545: a lateral burn of one wall - its own glyph.
        OpType::SinkerSideBurn => Icon::SinkerSideBurn,
        OpType::Drill => Icon::Drill,
        OpType::Bore => Icon::Bore,
        OpType::Thread => Icon::Thread,
        OpType::Probe => Icon::Probe,
        OpType::Pocket3D => Icon::Pocket3D,
        OpType::Finish3D => Icon::Finish3D,
        OpType::Contour5D => Icon::Contour5D,
        OpType::Parallel5D => Icon::Parallel5D,
        OpType::Swarf => Icon::Swarf,
        OpType::AdvancedSwarf => Icon::AdvancedSwarf,
        OpType::RotaryContour => Icon::RotaryContour,
        OpType::RotaryParallel => Icon::RotaryParallel,
        OpType::RotaryOffset => Icon::RotaryOffset,
        OpType::MultiAxisRough => Icon::MultiAxisRough,
        OpType::Flow5D => Icon::Flow5D,
        OpType::MultiAxisContour => Icon::MultiAxisContour,
        OpType::Facing => Icon::Facing,
        OpType::ProfileRough => Icon::ProfileRough,
        OpType::ProfileFinish => Icon::ProfileFinish,
        OpType::GrooveRough => Icon::GrooveRough,
        OpType::GrooveFinish => Icon::GrooveFinish,
        OpType::Part => Icon::PartOff,
        OpType::Threading => Icon::Threading,
        OpType::Chamfering => Icon::Chamfering,
        OpType::TaperTurning => Icon::TaperTurning,
        OpType::MultiPassTurning => Icon::MultiPassTurning,
        OpType::FormTurning => Icon::FormTurning,
        OpType::BoringTurning => Icon::BoringTurning,
    }
}

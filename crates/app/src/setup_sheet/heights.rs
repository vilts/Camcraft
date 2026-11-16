//! Setup-sheet depth columns: which four height expressions each
//! operation kind prints as CLR/RET/TOP/BOT (M3.2 stage D). Provenance:
//! our own layout of our own parameter sets - see `setup_sheet/mod.rs`.

use crate::operation::{Height, OpParams};

/// The four depth columns as `(column, field name, height expression)`,
/// in table order. The expression is what `resolve_params`/`stamp_with`
/// feed the same world questions - this only lays them out per kind, so
/// the sheet reads the machinery, never a copy of its rules.
pub(super) fn height_fields(params: &OpParams) -> [(&'static str, &'static str, Option<Height>); 4] {
    match params {
        OpParams::PlasmaProfile(p) => [
            ("CLR", "Clearance Z", Some(p.clearance_z)),
            ("RET", "Retract Z", None),
            ("TOP", "Top of Plate Z", Some(p.top_z)),
            ("BOT", "Bottom of Plate Z", Some(p.bottom_z)),
        ],
        // The wire cut (#532): the two programmed planes; no rapids
        // plane, no hop.
        OpParams::WireContour(p) => [
            ("CLR", "Clearance Z", None),
            ("RET", "Retract Z", None),
            ("TOP", "Top of Workpiece Z", Some(p.top_z)),
            ("BOT", "Bottom of Workpiece Z", Some(p.bottom_z)),
        ],
        OpParams::SinkerBurn(p) | OpParams::SinkerSideBurn(crate::operation::SinkerSideBurnOp { burn: p, .. }) => [
            ("CLR", "Clearance Z", Some(p.clearance_z)),
            ("RET", "Retract Z", None),
            ("TOP", "Work Top Z", Some(p.top_z)),
            ("BOT", "Cavity Floor Z", Some(p.floor_z)),
        ],
        OpParams::Probe { clearance_z, .. } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", None),
            ("TOP", "Top Z", None),
            ("BOT", "Bottom Z", None),
        ],
        OpParams::Face {
            clearance_z,
            cut_z,
            top_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", None),
            ("TOP", "Top Z", *top_z),
            ("BOT", "Cut Z", Some(*cut_z)),
        ],
        OpParams::Contour {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", Some(*retract_z)),
            ("TOP", "Top of Part Z", Some(*top_z)),
            ("BOT", "Bottom of Part Z", Some(*bottom_z)),
        ],
        OpParams::Pocket {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        // The twins' four heights are the pocket's, column for
        // column (M2.4 A).
        | OpParams::Adaptive2D {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Offset2D {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        // The engrave twin and the circular ladder ride the family's
        // four columns, the optional hop among them (M2.4 B).
        | OpParams::Engrave {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Circular {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Slot {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::TrochoidalSlot {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Trace {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Pocket3D {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Finish3D {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        // The 5-axis twins ride the same four rows as the finish -
        // the optional hop included (M5.1 stage C).
        | OpParams::Contour5D {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Parallel5D {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        // The swarf family's rows are the same four (M5.1 stage D).
        | OpParams::Swarf {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::AdvancedSwarf {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        // The rotary twins' too - the world planes the generator
        // reads into axial stations (M5.1 stage E).
        | OpParams::RotaryContour {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::RotaryParallel {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        // The stamped ladder states the pocket family's four rows,
        // the optional hop among them (M5.1 stage E3), and so does
        // flow (M5.1 I).
        | OpParams::MultiAxisRough {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Flow5D {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        // The drive-curve contour states the same four rows; its
        // TOP/BOT are the window the picked chain must stand in, and
        // the Geometry column already spells the chain by its picks
        // (M5.1 J).
        | OpParams::MultiAxisContour {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", *retract_z),
            ("TOP", "Top of Part Z", Some(*top_z)),
            ("BOT", "Bottom of Part Z", Some(*bottom_z)),
        ],
        // The chamfer ride states three rows: the BOT column is empty
        // because the face's depth is DERIVED - width and angle author
        // the floor, and no height stands there to print (M2.4 B,
        // variant docs).
        OpParams::Chamfer2D {
            clearance_z,
            retract_z,
            top_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", *retract_z),
            ("TOP", "Top of Part Z", Some(*top_z)),
            ("BOT", "Bottom of Part Z", None),
        ],
        // The deburr ride states four rows but names its top and
        // bottom as a WINDOW: the tip may stand nowhere else, and
        // there is no RET row - the kernel hops every run to the
        // clearance plane itself (M2.5 E B).
        OpParams::Deburr {
            clearance_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", None),
            ("TOP", "Window Top Z", Some(*top_z)),
            ("BOT", "Window Bottom Z", Some(*bottom_z)),
        ],
        // The rough ladder states the sheet with no RET row - the
        // kernel's hop plane is a law, not a height (M5.1 stage E2).
        OpParams::RotaryOffset {
            clearance_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", None),
            ("TOP", "Top of Part Z", Some(*top_z)),
            ("BOT", "Bottom of Part Z", Some(*bottom_z)),
        ],
        // The facing ladder states the sheet the wrap's way - three
        // rows, no RET: the station rapids are one infeed off the
        // profile, a law and not a height. Its bottom names itself
        // for the axis: every station steps TOWARD it (M5.2 A).
        OpParams::Facing {
            clearance_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", None),
            ("TOP", "Top of Part Z", Some(*top_z)),
            ("BOT", "Facing Plane Z", Some(*bottom_z)),
        ],
        // The profile kinds state the same three rows with the plain
        // names: their bottom is the low end of the station window,
        // not a plane the ladder faces toward (M5.2 B).
        OpParams::ProfileRough {
            clearance_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::ProfileFinish {
            clearance_z,
            top_z,
            bottom_z,
            ..
        }
        // The blade windows state the same four rows (M5.2 C)...
        | OpParams::GrooveRough {
            clearance_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::GrooveFinish {
            clearance_z,
            top_z,
            bottom_z,
            ..
        }
        // Threading states the same four rows - the window of turns
        // by its plain names (M5.2 D) - and the contour rides state
        // them across a face: the window bounds the cone, and the
        // ride names its own reach (M5.2 E).
        | OpParams::Threading {
            clearance_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Chamfering {
            clearance_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::TaperTurning {
            clearance_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::MultiPassTurning {
            clearance_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::FormTurning {
            clearance_z,
            top_z,
            bottom_z,
            ..
        }
        // The boring bar names the same trio - the rapids plane and
        // the window of wall the ride walks (M5.2 F).
        | OpParams::BoringTurning {
            clearance_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", None),
            ("TOP", "Top of Part Z", Some(*top_z)),
            ("BOT", "Bottom of Part Z", Some(*bottom_z)),
        ],
        // ...and parting's bottom names the plane the blade severs at,
        // facing's naming law read as a cutoff (M5.2 C).
        OpParams::Part {
            clearance_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", None),
            ("TOP", "Top of Part Z", Some(*top_z)),
            ("BOT", "Parting Plane Z", Some(*bottom_z)),
        ],
        OpParams::Drill {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        // The boring ride and the thread mill fill the drill's four
        // columns column for column - the R plane and the optional
        // floor are their law too (M2.4 C).
        | OpParams::Bore {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        }
        | OpParams::Thread {
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            ..
        } => [
            ("CLR", "Clearance Z", Some(*clearance_z)),
            ("RET", "Retract Z", *retract_z),
            ("TOP", "R Plane (Top of Part Z)", Some(*top_z)),
            ("BOT", "Bottom Z", *bottom_z),
        ],
    }
}

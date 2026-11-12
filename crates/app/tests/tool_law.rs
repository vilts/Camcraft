//! M10.1 D (the table half): the tool law's registry column. Every kind
//! names the tool kinds that can machine it, every tool kind is claimed
//! by some kind (no orphan cutters in the library), and the family edges
//! hold - a grooving kind refuses an end mill, a drill refuses a ball, a
//! pocket takes an end mill.
//!
//! Provenance: machine-tool handbook tooling practice read against our
//! own generators' tool doors; no vendor material consulted.
//!
//! Gate map (M4.4): covers app::operation (kinds table, tool_law), tools::tools.

use camcraft_app::operation::{tool_law, KindInfo, OpType, Section};
use camcraft_app::tools::ToolKind;

/// Every `ToolKind`, kept honest by the exhaustive match below: a new
/// variant fails to compile here until it joins the list (and so the
/// orphan law).
fn every_tool_kind() -> Vec<ToolKind> {
    let all = vec![
        ToolKind::EndMill,
        ToolKind::BallNose,
        ToolKind::BullNose,
        ToolKind::Drill,
        ToolKind::Tap,
        ToolKind::Bore,
        ToolKind::Turning,
        ToolKind::Groove,
        ToolKind::Thread,
        ToolKind::Form,
        ToolKind::Taper,
        ToolKind::Probe,
        ToolKind::Torch,
    ];
    for k in &all {
        match k {
            ToolKind::EndMill
            | ToolKind::BallNose
            | ToolKind::BullNose
            | ToolKind::Drill
            | ToolKind::Tap
            | ToolKind::Bore
            | ToolKind::Turning
            | ToolKind::Groove
            | ToolKind::Thread
            | ToolKind::Form
            | ToolKind::Taper
            | ToolKind::Probe
            | ToolKind::Torch => {}
        }
    }
    all
}

#[test]
fn every_kind_names_a_tool_set() {
    for row in KindInfo::ALL {
        assert!(!row.tool_kinds.is_empty(), "{:?} names no tool kind", row.ty);
        for (i, k) in row.tool_kinds.iter().enumerate() {
            assert!(
                !row.tool_kinds[..i].contains(k),
                "{:?} lists {k:?} twice",
                row.ty
            );
            assert!(row.accepts(*k), "{:?}: accepts disagrees with its own set", row.ty);
        }
    }
}

#[test]
fn no_orphan_tool_kinds() {
    for k in every_tool_kind() {
        assert!(
            KindInfo::ALL.iter().any(|row| row.accepts(k)),
            "no operation kind can use a {k}"
        );
    }
}

#[test]
fn lathe_blades_stay_on_the_lathe_and_mills_off_it() {
    // The family column and the tool column cannot disagree: a turning
    // kind takes only lathe tooling, a mill kind never a lathe blade.
    // Bore is the one shared kind (the drilling cycle's bar and the
    // turned bore's bar are the same tool).
    let lathe_only = [ToolKind::Turning, ToolKind::Groove, ToolKind::Thread, ToolKind::Form];
    for row in KindInfo::ALL {
        for k in row.tool_kinds {
            if row.section == Section::Turning {
                assert!(
                    lathe_only.contains(k) || *k == ToolKind::Bore,
                    "{:?} (turning) accepts mill tooling {k:?}",
                    row.ty
                );
            } else {
                assert!(!lathe_only.contains(k), "{:?} (mill) accepts lathe blade {k:?}", row.ty);
            }
        }
    }
}

#[test]
fn family_edges_spot_checks() {
    // The hand-test case: an end mill offered to a groove.
    assert!(!OpType::GrooveRough.info().accepts(ToolKind::EndMill));
    assert!(!OpType::GrooveFinish.info().accepts(ToolKind::EndMill));
    assert!(!OpType::Part.info().accepts(ToolKind::EndMill));
    assert!(OpType::GrooveRough.info().accepts(ToolKind::Groove));
    // A drill cycle expands into drill / tap / bar - never a ball.
    let drill = OpType::Drill.info();
    assert!(!drill.accepts(ToolKind::BallNose));
    assert!(drill.accepts(ToolKind::Drill));
    assert!(drill.accepts(ToolKind::Tap));
    assert!(drill.accepts(ToolKind::Bore));
    // A pocket takes an end mill, and no drill.
    assert!(OpType::Pocket.info().accepts(ToolKind::EndMill));
    assert!(!OpType::Pocket.info().accepts(ToolKind::Drill));
    // The generators' shape doors, restated as family: ball finishing,
    // cone chamfering, the tap-form thread mill.
    assert_eq!(OpType::Finish3D.info().tool_kinds, tool_law::BALL);
    assert_eq!(OpType::Chamfer2D.info().tool_kinds, tool_law::CONE);
    assert_eq!(OpType::Deburr.info().tool_kinds, tool_law::CONE);
    assert_eq!(OpType::Thread.info().tool_kinds, tool_law::THREAD_MILL);
    assert_eq!(OpType::Threading.info().tool_kinds, &[ToolKind::Thread]);
    assert_eq!(OpType::FormTurning.info().tool_kinds, &[ToolKind::Form]);
    assert_eq!(OpType::BoringTurning.info().tool_kinds, &[ToolKind::Bore]);
    assert_eq!(OpType::Facing.info().tool_kinds, &[ToolKind::Turning]);
}

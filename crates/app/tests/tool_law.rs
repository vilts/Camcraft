//! M10.1 D (the table half): the tool law's registry column. Every kind
//! names the tool kinds that can machine it, each supported cutting tool
//! is claimed by an operation, staged kinds are refused, and the family edges
//! hold - a grooving kind refuses an end mill, a drill refuses a ball, a
//! pocket takes an end mill.
//!
//! Provenance: machine-tool handbook tooling practice read against our
//! own generators' tool doors; no vendor material consulted.
//!
//! Gate map (M4.4): covers app::operation (kinds table, tool_law), tools::tools,
//! app::operation::tool_gate (#3368: a flat or ball taper tip; #3369: the
//! form mill).

use camcraft_app::commands::{AddTool, History};
use camcraft_app::operation::{tool_gate, tool_law, KindInfo, OpType, Section};
use camcraft_app::tools::{catalog, TaperTip, Tool, ToolKind};

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
        ToolKind::Electrode,
        ToolKind::ThreadMill,
        ToolKind::ThreadMillSingle,
        ToolKind::Waterjet,
        ToolKind::Wire,
        ToolKind::TSlot,
        ToolKind::Keyseat,
        ToolKind::Dovetail,
        ToolKind::Lollipop,
        ToolKind::FormMill,
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
            | ToolKind::Torch
            | ToolKind::Electrode
            | ToolKind::ThreadMill
            | ToolKind::ThreadMillSingle
            | ToolKind::Waterjet
            | ToolKind::Wire
            | ToolKind::TSlot
            | ToolKind::Keyseat
            | ToolKind::Dovetail
            | ToolKind::Lollipop
            | ToolKind::FormMill => {}
        }
    }
    all
}

#[test]
fn every_kind_names_a_tool_set() {
    for row in KindInfo::ALL {
        assert!(row.mounts_tool(), "{:?} names no tool kind", row.ty);
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
        if k == ToolKind::Waterjet {
            // #1883: the Waterjet Profile is the one operation that cuts
            // with a nozzle; every other family refuses it (#1881).
            for row in KindInfo::ALL {
                assert_eq!(
                    row.accepts(k),
                    row.ty == OpType::WaterjetProfile,
                    "{:?}: only the Waterjet Profile machines with a waterjet nozzle",
                    row.ty
                );
            }
            continue;
        }
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
    assert_eq!(OpType::WireContour.info().tool_kinds, tool_law::WIRE);
}

#[test]
fn undercut_cutters_mount_only_the_slot_undercut() {
    // #1444: a head wider than its neck cuts sideways only.
    for k in [ToolKind::TSlot, ToolKind::Keyseat, ToolKind::Dovetail, ToolKind::Lollipop] {
        for row in KindInfo::ALL {
            assert_eq!(row.accepts(k), row.ty == OpType::SlotUndercut, "{:?} with a {k}", row.ty);
        }
    }
    assert_eq!(OpType::SlotUndercut.info().tool_kinds, tool_law::UNDERCUT);
    assert!(!OpType::SlotUndercut.info().accepts(ToolKind::EndMill));
}

/// #3368: until a generator reads a taper's tip, every operation that
/// mounts a chamfer mill refuses a flat or ball tip by name - the tool
/// door at creation, edit and re-tool. A sharp tip passes the same door,
/// and the catalog hint never offers a flat or ball tip.
#[test]
fn every_operation_refuses_a_non_sharp_taper_by_name() {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let Some((_, sharp)) = catalog().into_iter().find(|(n, _)| *n == "CM-0600-90") else {
        panic!("the catalog has CM-0600-90");
    };
    let flat = Tool { tip_form: TaperTip::Flat, tip_diameter: 1.0, ..sharp.clone() };
    let ball = Tool { tip_form: TaperTip::Ball, corner_r: 0.5, ..sharp.clone() };
    for (name, t) in [("SHARP", sharp), ("FLAT", flat), ("BALL", ball)] {
        let cmd = AddTool::new(&world, name, t).unwrap_or_else(|e| panic!("{name}: {e}"));
        h.execute(&world, cmd);
    }
    let rows: Vec<&KindInfo> = KindInfo::ALL.iter().filter(|r| r.accepts(ToolKind::Taper)).collect();
    assert!(rows.len() >= 4, "the chamfer mill's operations: {:?}", rows.iter().map(|r| r.ty).collect::<Vec<_>>());
    for row in rows {
        assert_eq!(tool_gate::tool_refusal(&world, row, "SHARP"), None, "{:?} takes a sharp tip", row.ty);
        for (name, form) in [("FLAT", "flat"), ("BALL", "ball")] {
            let why = tool_gate::tool_refusal(&world, row, name).unwrap_or_else(|| panic!("{:?} took {name}", row.ty));
            assert!(why.contains(&format!("'{name}' has a {form} tip")), "{:?}: {why}", row.ty);
            assert!(why.contains(row.label), "{:?} names itself: {why}", row.ty);
        }
        assert!(!tool_gate::fitting_tools(&world, row).iter().any(|n| n != "SHARP"), "{:?} offers only the sharp tip", row.ty);
        let hint = tool_gate::catalog_suggestion(&catalog(), &[ToolKind::Taper]).unwrap_or_default();
        assert!(!hint.contains("-F10") && !hint.contains("-R05"), "{hint}");
    }
}

/// #3369: until a generator reads a form mill's outline, every operation
/// that has it in its family refuses it by name - the tool door at
/// creation, edit and re-tool - and the 2D Chamfer names the edge form
/// work. No operation offers it, no refusal suggests it, and the catalog
/// hint never names one.
#[test]
fn form_mill_is_refused_by_every_door() {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    for name in ["CM-0600-90", "FM-RO-R6"] {
        let Some((_, t)) = catalog().into_iter().find(|(n, _)| *n == name) else {
            panic!("the catalog has {name}");
        };
        let cmd = AddTool::new(&world, name, t).unwrap_or_else(|e| panic!("{name}: {e}"));
        h.execute(&world, cmd);
    }
    let rows: Vec<&KindInfo> = KindInfo::ALL.iter().filter(|r| r.accepts(ToolKind::FormMill)).collect();
    assert!(rows.iter().any(|r| r.ty == OpType::Chamfer2D), "the form mill is in the 2D Chamfer family");
    for row in KindInfo::ALL {
        assert!(tool_gate::tool_refusal(&world, row, "FM-RO-R6").is_some(), "{:?} took a form mill", row.ty);
        assert!(!tool_gate::fitting_tools(&world, row).iter().any(|n| n == "FM-RO-R6"), "{:?} offers a form mill", row.ty);
    }
    for row in rows {
        let why = tool_gate::tool_refusal(&world, row, "FM-RO-R6").unwrap_or_default();
        assert!(why.contains("'FM-RO-R6' is a form mill"), "{:?}: {why}", row.ty);
        assert!(why.contains(row.label), "{:?} names itself: {why}", row.ty);
        assert_eq!(why.contains("edge form work (#896)"), row.ty == OpType::Chamfer2D, "{:?}: {why}", row.ty);
        assert!(!tool_gate::kinds_phrase(row.tool_kinds).contains("form mill"), "{:?} suggests a form mill", row.ty);
        let hint = tool_gate::catalog_suggestion(&catalog(), row.tool_kinds).unwrap_or_default();
        assert!(!hint.contains("FM-"), "{hint}");
    }
}

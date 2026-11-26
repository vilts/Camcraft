//! #2496: which holes a drill leaves undersize, and the warning's place.
//! Provenance: our own tests over our own law.

use super::*;
use crate::geo2d::Point2;
use crate::tools::catalog;

fn drill() -> Tool {
    let t = catalog().into_iter().find(|(n, _)| *n == "DR-0600").map(|(_, t)| t).expect("catalog DR-0600");
    assert_eq!(t.kind, ToolKind::Drill);
    t
}

fn hole(x: f64, dia: f64) -> Hole {
    Hole { xy: Point2::new(x, 0.0), radius: dia * 0.5, entry_z: 10.0, floor_z: 0.0, through: true, stepped: false }
}

fn drill_op(cycle: CycleChoice, holes: Vec<Hole>) -> OpParams {
    use crate::operation::Height;
    OpParams::Drill {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: None,
        cycle,
        peck: 0.0,
        dwell: 0.0,
        feed: 130.0,
        plunge_feed: 100.0,
        holes,
        cross: Vec::new(),
        break_through: None,
        break_through_from: crate::drilling::BreakThroughFrom::FullDiameter,
        picks: Default::default(),
    }
}

#[test]
fn a_hole_wider_than_the_drill_is_named_and_its_own_size_is_not() {
    let t = drill();
    let p = drill_op(CycleChoice::Auto, vec![hole(0.0, 6.0), hole(20.0, 6.04), hole(30.0, 6.1), hole(35.0, 6.2), hole(40.0, 8.0), hole(60.0, 12.0)]);
    let wide: Vec<f64> = undersize_holes(&t, &p).iter().map(|h| h.radius * 2.0).collect();
    assert_eq!(wide, vec![6.2, 8.0, 12.0], "D6.00, D6.04 and D6.1 are the drill's size; D6.2, D8 and D12 are not");
}

#[test]
fn a_narrower_hole_is_not_this_laws() {
    let t = drill();
    let p = drill_op(CycleChoice::Drill, vec![hole(0.0, 5.0)]);
    assert!(undersize_holes(&t, &p).is_empty(), "a D5 hole under a D6 drill is a gouge, judged by the solid law");
}

#[test]
fn spot_tap_and_bore_cycles_and_other_tool_kinds_size_the_hole_another_way() {
    let t = drill();
    for cycle in [CycleChoice::Spot, CycleChoice::Tap, CycleChoice::FloatingTap, CycleChoice::Bore, CycleChoice::BackBore] {
        assert!(undersize_holes(&t, &drill_op(cycle, vec![hole(0.0, 8.0)])).is_empty(), "{cycle:?}");
    }
    for cycle in [CycleChoice::Auto, CycleChoice::Drill, CycleChoice::Peck, CycleChoice::Ream] {
        assert_eq!(undersize_holes(&t, &drill_op(cycle, vec![hole(0.0, 8.0)])).len(), 1, "{cycle:?}");
    }
    for kind in [ToolKind::Tap, ToolKind::Bore, ToolKind::EndMill, ToolKind::Taper] {
        let other = Tool { kind, ..t.clone() };
        assert!(undersize_holes(&other, &drill_op(CycleChoice::Auto, vec![hole(0.0, 8.0)])).is_empty(), "{kind:?}");
    }
}

#[test]
fn a_bad_number_names_nothing_and_never_panics() {
    let t = drill();
    let p = drill_op(CycleChoice::Auto, vec![hole(0.0, f64::NAN), hole(10.0, f64::INFINITY)]);
    assert!(undersize_holes(&t, &p).is_empty());
    for d in [0.0, -6.0, f64::NAN] {
        let bad = Tool { diameter: d, ..t.clone() };
        assert!(undersize_holes(&bad, &drill_op(CycleChoice::Auto, vec![hole(0.0, 8.0)])).is_empty(), "{d}");
    }
}

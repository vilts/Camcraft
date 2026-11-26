//! #2004 round 5: the app hands the stock top to trace, engrave and the
//! trochoidal slot. With Top Z 2 mm under the stock top (box stock z 0..20,
//! Top Z 18) the first level stays at Top Z, so no first bite is deeper
//! than one stepdown under the material; the old law skipped Top Z and
//! cut 5 mm (stock top 20 to level 15) in one bite.
//!
//! Provenance: our own fixtures; the depth-of-cut ladder is general
//! machining knowledge.
//!
//! Gate map (#2004): covers app::operation (generate, trochoid_op, face),
//! motion::machining2d (slots::trace, trochoid_slot, levels, face, circular).

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::machining2d::TrochoidReturn;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0600-4FL";

/// Generate `params` on a 80x60x20 box stock (z 0..20) and give the path.
fn generated(params: OpParams) -> Toolpath {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 80.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "Op1", TOOL, SrcRef::Stock, params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Op1").expect("generate"));
    let e = model::find_by_name(&world, "Op1").expect("op");
    model::toolpath_of(&world, &e).expect("cached path").path
}

/// The cut levels on the stepdown grid (3 mm from Z 0): the Z of each feed
/// that keeps its Z, highest first. The trochoid's microlift returns (0.3
/// mm over a level) are off the grid and drop out.
fn levels(t: &Toolpath) -> Vec<f64> {
    let mut zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && (m.from.z - m.to.z).abs() < 1e-9)
        .map(|m| m.to.z)
        .filter(|z| (z / 3.0 - (z / 3.0).round()).abs() < 1e-6)
        .collect();
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    zs
}

#[test]
fn a_trace_under_stock_over_top_z_keeps_the_level_at_top_z() {
    let t = generated(OpParams::Trace {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(18.0),
        bottom_z: Height::fixed(12.0),
        stepdown: 3.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        tilt: None,
        picks: Default::default(),
    });
    assert_eq!(levels(&t), vec![18.0, 15.0, 12.0], "the first bite is 2 mm, not 5 mm");
}

#[test]
fn a_trochoidal_slot_under_stock_over_top_z_keeps_the_level_at_top_z() {
    let t = generated(OpParams::TrochoidalSlot {
        a: Point2::new(-15.0, 0.0),
        b: Point2::new(15.0, 0.0),
        width: 10.0,
        loop_width: None,
        step: 0.8,
        max_engagement: 90.0,
        ret: TrochoidReturn::Microlift { height: 0.3 },
        smoothing: 0.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: Some(Height::fixed(22.0)),
        top_z: Height::fixed(18.0),
        bottom_z: Height::fixed(12.0),
        stepdown: 3.0,
        feed: 800.0,
        return_feed: 2000.0,
        plunge_feed: 200.0,
        climb: true,
        picks: Default::default(),
    });
    assert_eq!(levels(&t), vec![18.0, 15.0, 12.0], "the first bite is 2 mm, not 5 mm");
}

/// #2004 review: the staged Face reads the stock top too. On the box
/// (stock top 20) a Face from Top Z 18 to Cut Z 12 starts at Z 18.
#[test]
fn a_staged_face_under_stock_over_top_z_keeps_the_level_at_top_z() {
    let t = generated(OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 4.0,
        clearance_z: Height::fixed(30.0),
        cut_z: Height::fixed(12.0),
        top_z: Some(Height::fixed(18.0)),
        stepdown: Some(3.0),
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    });
    assert_eq!(levels(&t), vec![18.0, 15.0, 12.0], "the first bite is 2 mm, not 5 mm");
}

/// #2004 review: the vault rim ring reads the stock top. On the box
/// (stock top 20) a vault with Top Z 18 cuts its rim ring at Z 18.
#[test]
fn a_vault_under_stock_over_top_z_cuts_its_rim_ring_at_top_z() {
    let t = generated(OpParams::Circular {
        pass_start: Default::default(),
        links: camcraft_app::links::Links::PLANAR,
        center: Point2::new(0.0, 0.0),
        radius: 12.0,
        vault: true,
        stepover: 2.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(18.0),
        bottom_z: Height::fixed(12.0),
        stepdown: 3.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
        picks: Default::default(),
    });
    assert_eq!(levels(&t).first().copied(), Some(18.0), "the rim ring cuts at Top Z");
}

/// Indexed stock caps include rotation and translation, not only origin.z.
fn indexed_face(tilt: camcraft_app::operation::Tilt, stock_top: f64) {
    let frame = tilt.frame().expect("finite frame");
    let top = stock_top - 2.0;
    let t = generated(OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: Some(tilt),
        stepover: 4.0,
        clearance_z: Height::fixed(70.0),
        cut_z: Height::fixed(top - 6.0 + tilt.origin[2]),
        top_z: Some(Height::fixed(top + tilt.origin[2])),
        stepdown: Some(3.0),
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    });
    let mut zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed)
        .filter_map(|m| {
            let (a, b) = (frame.z_local(m.from), frame.z_local(m.to));
            ((a - b).abs() < 1e-8).then_some(b)
        })
        .collect();
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup_by(|a, b| (*a - *b).abs() < 1e-8);
    let expected = [top, top - 3.0, top - 6.0];
    assert_eq!(zs.len(), expected.len(), "cutting-frame levels: {zs:?}");
    for (got, want) in zs.iter().zip(expected) {
        assert!(
            (got - want).abs() < 1e-8,
            "levels {zs:?}, expected {expected:?}"
        );
    }
}

#[test]
fn a_b90_face_keeps_top_z_under_rotated_stock() {
    indexed_face(
        camcraft_app::operation::Tilt {
            basis_version: 1,
            a_deg: 0.0,
            b_deg: 90.0,
            origin: [0.0; 3],
        },
        40.0,
    );
}

#[test]
fn an_oblique_face_keeps_top_z_under_rotated_and_translated_stock() {
    // B45 axis = (1, 0, 1)/sqrt(2): max z_local = (40 + 20 - 4 - 2)/sqrt(2).
    indexed_face(
        camcraft_app::operation::Tilt {
            basis_version: 1,
            a_deg: 0.0,
            b_deg: 45.0,
            origin: [4.0, 0.0, 2.0],
        },
        54.0 / 2.0_f64.sqrt(),
    );
}

//! Relief height bounds retain collisions outside the declared cut volume (#2400).
//! Provenance: original fixtures for the bounded swept-volume exemption.
//! Gate map: covers camcraft_sim::verify::{corner_relief, section, solid_index}.

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::{Point3, Triangle, cube};
use camcraft_sim::verify::{CornerReliefZone, Mode, ToolGeom, VerifyContext, Warning, verify};

fn zone() -> CornerReliefZone {
    CornerReliefZone {
        corner: Point2::new(10.0, 10.0),
        start: Point2::new(7.0, 7.0),
        end: Point2::new(7.8786796564, 7.8786796564),
        radius: 3.0,
        z_bottom: 10.0,
        z_top: 20.0,
    }
}

fn judge(z0: f64, z1: f64, kind: MoveKind, facets: &[Triangle], relieved: bool) -> Vec<Warning> {
    let region = Region::new(vec![
        Loop::new(vec![
            Point2::new(-40.0, -40.0),
            Point2::new(40.0, -40.0),
            Point2::new(40.0, 40.0),
            Point2::new(-40.0, 40.0),
        ])
        .expect("square loop"),
    ])
    .expect("square region");
    let tool = ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 5.0,
        shank_radius: 3.0,
        shank_len: 10.0,
        holder_radius: 6.0,
        holder_len: 10.0,
    };
    let reliefs = [zone()];
    let path = Toolpath {
        moves: vec![Move {
            rotary: None,
            kind,
            from: Point3::new(7.0, 7.0, z0),
            to: Point3::new(7.8786796564, 7.8786796564, z1),
            feed: if kind == MoveKind::Rapid {
                None
            } else {
                Some(500.0)
            },
            arc: None,
            axis: None,
        }],
        ..Default::default()
    };
    verify(
        &path,
        &VerifyContext {
            region: &region,
            stock: Some(&region),
            model: Some(facets),
            tool: &tool,
            mode: Mode::Area,
            stock_top_z: 40.0,
            clearance_z: 45.0,
            gouge_tol: 0.01,
            z_tol: 1e-3,
            corner_reliefs: if relieved { &reliefs } else { &[] },
            ..Default::default()
        },
    )
}

fn block(z0: f64, z1: f64) -> Vec<Triangle> {
    cube(Point3::new(9.5, 7.0, z0), Point3::new(10.5, 9.0, z1))
        .facets
        .to_vec()
}

#[test]
fn cutting_edge_above_relief_top_keeps_gouge() {
    let model = block(31.0, 34.0);
    for relieved in [false, true] {
        assert!(
            judge(30.0, 30.0, MoveKind::Feed, &model, relieved).contains(&Warning::Gouge { at: 0 }),
            "relieved={relieved}"
        );
    }
}

#[test]
fn shank_above_relief_top_keeps_part_collision() {
    let model = block(22.0, 24.0);
    for relieved in [false, true] {
        assert!(
            judge(10.0, 10.0, MoveKind::Feed, &model, relieved)
                .contains(&Warning::ShankHit { at: 0, part: true }),
            "relieved={relieved}"
        );
    }
}

#[test]
fn ramp_crossing_relief_top_keeps_gouge() {
    let model = block(21.0, 24.0);
    for relieved in [false, true] {
        assert!(
            judge(10.0, 30.0, MoveKind::Feed, &model, relieved).contains(&Warning::Gouge { at: 0 }),
            "relieved={relieved}"
        );
    }
}

#[test]
fn enclosed_shank_crossing_relief_top_keeps_part_collision() {
    let model = cube(
        Point3::new(-20.0, -20.0, 0.0),
        Point3::new(20.0, 20.0, 40.0),
    );
    let warnings = judge(10.0, 10.0, MoveKind::Feed, &model.facets, true);
    assert!(
        warnings.contains(&Warning::ShankHit { at: 0, part: true }),
        "{warnings:?}"
    );
}

#[test]
fn contact_inside_relief_remains_allowed_with_flute_above_top() {
    // The flute extends to Z 23, but all contact is within Z 18..20.
    let model = block(18.0, 20.0);
    assert!(judge(18.0, 18.0, MoveKind::Feed, &model, false).contains(&Warning::Gouge { at: 0 }));
    let warnings = judge(18.0, 18.0, MoveKind::Feed, &model, true);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn relief_does_not_allow_a_rapid_through_uncut_stock() {
    for relieved in [false, true] {
        assert!(
            judge(10.0, 10.0, MoveKind::Rapid, &[], relieved)
                .contains(&Warning::RapidInStock { at: 0 }),
            "relieved={relieved}"
        );
    }
}

//! PR #2289: a crossing-safe link must still obey the centre-cutting law.

use std::cell::Cell;

use super::super::level_ctx::{LevelCtx, feed_leg};
use super::super::shell::Field;
use super::super::{Entry3D, Pocket3DParams};
use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::PocketStrategy;
use crate::plunge_law::{Solid, first_solid_plunge};

fn crossing_hop(center_cutting: bool, cleared: bool) -> Toolpath {
    let mut p = Pocket3DParams {
        tool_diameter: 2.0,
        stepover: 1.0,
        stock_to_leave: 0.0,
        clearance_z: 2.0,
        top_z: 0.0,
        bottom_z: -1.0,
        stepdown: 1.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::OffsetRings,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 0.5,
        optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
        links: crate::links::Links::ROUGH3D,
    };
    p.links.approach.center_cutting = center_cutting;
    p.validate().unwrap();
    let cen = Region::new(vec![
        Loop::new(vec![
            Point2::new(-20.0, -20.0),
            Point2::new(20.0, -20.0),
            Point2::new(20.0, 20.0),
            Point2::new(-20.0, 20.0),
        ])
        .unwrap(),
    ])
    .unwrap();
    // No model surface nearby: shell tips stay exactly on the level.
    let field = Field::new(&[]).unwrap();
    let lvl = LevelCtx {
        cen: &cen,
        passes: &[],
        z: -1.0,
        hop_z: 2.0,
        p: &p,
        field: Some(&field),
        tz: 2.0,
        kept: false,
        settle: Cell::new(0.0),
        arcs: None,
    };
    let mut b = PathBuilder::new(Point3::new(-10.0, 0.6, -1.0));
    if cleared {
        b.push(MoveKind::Feed, Point3::new(10.0, 0.6, -1.0), Some(p.feed));
    }
    b.push(MoveKind::Rapid, Point3::new(-10.0, 0.0, -1.0), None);
    b.push(MoveKind::Feed, Point3::new(10.0, 0.0, -1.0), Some(p.feed));
    b.push(MoveKind::Feed, Point3::new(0.0, -1.0, -1.0), Some(p.feed));
    feed_leg(&mut b, &lvl, Point2::new(0.0, 0.6), 0.0);
    let path = b.finish();
    assert!(
        path.moves.iter().any(|m| {
            m.kind == MoveKind::Plunge
                && m.from.z == 2.0
                && m.to.x == 0.0
                && (m.to.y - 0.6).abs() < 1e-12
                && m.to.z == -1.0
        }),
        "the fixture must reach the crossing-hop fallback: {:?}",
        path.moves
    );
    path
}

#[test]
fn crossing_hop_refuses_non_center_cutting_tool_in_uncleared_column() {
    let path = crossing_hop(false, false);
    let (xy, z) = first_solid_plunge(
        &path,
        1.0,
        &Solid {
            top: 0.0,
            footprint: None,
        },
    )
    .expect("the hop lands in uncut stock");
    assert_eq!(xy.x, 0.0);
    assert!((xy.y - 0.6).abs() < 1e-12);
    assert_eq!(z, -1.0);
    let refusal =
        crate::approach::refusal(&path).expect("an unsafe crossing hop must refuse the operation");
    assert!(
        refusal.contains("a crossing hop"),
        "the refusal names the unsafe motion: {refusal}"
    );
}

#[test]
fn crossing_hop_allows_center_cutting_tool_in_uncleared_column() {
    let path = crossing_hop(true, false);
    assert!(
        first_solid_plunge(
            &path,
            1.0,
            &Solid {
                top: 0.0,
                footprint: None
            }
        )
        .is_some()
    );
    assert!(crate::approach::refusal(&path).is_none());
}

#[test]
fn crossing_hop_allows_non_center_cutting_tool_in_cleared_column() {
    let path = crossing_hop(false, true);
    assert!(
        first_solid_plunge(
            &path,
            1.0,
            &Solid {
                top: 0.0,
                footprint: None
            }
        )
        .is_none()
    );
    assert!(crate::approach::refusal(&path).is_none());
}

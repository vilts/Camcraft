//! Regression pins for the tilted cone, the bottom window and the entry sweep (#3283).
use super::{feeds, params, run};
use crate::finishing3d::{DeburrEdges, UnreachableEdges};
use crate::geometry::{Point3, Triangle, cube};
use crate::multiaxis::DeburrAxisMode;
use crate::multiaxis::multiaxis_deburr;

fn brick() -> Vec<Triangle> {
    cube(Point3::ZERO, Point3::new(40.0, 40.0, 10.0))
        .facets
        .to_vec()
}

/// Lowest point of the complete 6 mm, 90-degree cone, independently of
/// the collision guard: the apex or the lowest point on the base disk.
fn cone_bottom(tip: Point3, axis: [f64; 3]) -> f64 {
    tip.z
        .min(tip.z + 3.0 * axis[2] - 3.0 * (1.0 - axis[2] * axis[2]).max(0.0).sqrt())
}

#[test]
fn the_brick_keeps_the_tilted_tip_and_entire_cone_above_the_bottom() {
    let mut p = params(DeburrAxisMode::Axis5, 45.0);
    p.base.top_z = 10.0;
    let path = run(&brick(), &p).expect("the clear brick still cuts");
    let tilted: Vec<_> = feeds(&path).filter(|(_, axis)| axis[2] < 0.99).collect();
    assert!(!tilted.is_empty(), "clear vertical edges must still cut");
    for (tip, axis) in path.moves.iter().filter_map(|m| Some((m.to, m.axis?))) {
        assert!(
            tip.z >= p.base.bottom_z - 1e-8,
            "tip below the bottom: {tip:?} {axis:?}"
        );
        assert!(
            cone_bottom(tip, axis) >= p.base.bottom_z - 1e-8,
            "cone below the bottom: {tip:?} {axis:?}"
        );
    }
}

#[test]
fn a_raised_bottom_clips_the_cone_without_a_part_floor_to_hide_the_error() {
    let mut p = params(DeburrAxisMode::Axis5, 45.0);
    p.base.top_z = 10.0;
    p.base.bottom_z = 6.0;
    let path = run(&brick(), &p).expect("the high portion of the brick still cuts");
    let tilted: Vec<_> = path
        .moves
        .iter()
        .filter_map(|m| Some((m.to, m.axis?)))
        .filter(|(_, axis)| axis[2] < 0.99)
        .collect();
    assert!(
        !tilted.is_empty(),
        "clipping must retain a clear vertical edge"
    );
    for (tip, axis) in tilted {
        assert!(
            cone_bottom(tip, axis) >= 6.0 - 1e-8,
            "cone below the raised window: {tip:?} {axis:?}"
        );
    }
}

#[test]
fn a_boss_on_a_floor_skips_the_unsafe_foot_and_still_cuts_the_clear_edge() {
    let mut facets = brick();
    facets.extend(
        cube(
            Point3::new(-20.0, -20.0, -5.0),
            Point3::new(60.0, 60.0, 0.0),
        )
        .facets
        .iter()
        .copied(),
    );
    let mut p = params(DeburrAxisMode::Axis5, 45.0);
    p.base.bottom_z = -1.0; // The floor, rather than the window, must guard the cone.
    p.base.top_z = 10.0;
    let path = run(&facets, &p).expect("the clear upper edges still cut");
    let boss: Vec<_> = path
        .moves
        .iter()
        .filter_map(|m| Some((m.to, m.axis?)))
        .filter(|(q, a)| a[2] < 0.99 && q.x > -4.0 && q.x < 44.0 && q.y > -4.0 && q.y < 44.0)
        .collect();
    assert!(
        !boss.is_empty(),
        "a clear part of the vertical boss edge must cut"
    );
    for (tip, axis) in boss {
        assert!(
            cone_bottom(tip, axis) >= -1e-8,
            "the cone cuts the floor at the boss foot: {tip:?} {axis:?}"
        );
    }
    assert!(
        path.notes
            .iter()
            .any(|n| n.contains("multi-axis deburr") && n.contains("skipped")),
        "unsafe foot must be named: {:?}",
        path.notes
    );
}

#[test]
fn an_obstacle_on_the_entry_line_is_skipped_while_clear_edges_still_cut() {
    let mut facets = brick();
    facets.extend(
        cube(Point3::new(-2.0, -3.0, 21.0), Point3::new(2.0, 1.0, 23.0))
            .facets
            .iter()
            .copied(),
    );
    let p = params(DeburrAxisMode::Axis5, 0.0);
    let path = run(&facets, &p).expect("unobstructed top edges remain");
    assert!(
        feeds(&path).any(|(q, _)| q.y > 35.0 && q.z < 15.0),
        "the opposite clear edge still cuts"
    );
    let mut prev = None;
    for mov in &path.moves {
        if let Some(from) = prev {
            assert!(
                !tip_enters_box(from, mov.to),
                "the entry or exit tip crosses the obstacle: {from:?} -> {:?}",
                mov.to
            );
        }
        prev = Some(mov.to);
    }
    assert!(
        path.notes
            .iter()
            .any(|n| n.contains("multi-axis deburr") && n.contains("skipped")),
        "unsafe entry must be named: {:?}",
        path.notes
    );
}

fn tip_enters_box(a: Point3, b: Point3) -> bool {
    let mut enter: f64 = 0.0;
    let mut exit: f64 = 1.0;
    for (start, end, lo, hi) in [
        (a.x, b.x, -2.0, 2.0),
        (a.y, b.y, -3.0, 1.0),
        (a.z, b.z, 21.0, 23.0),
    ] {
        if (end - start).abs() < 1e-12 {
            if start < lo || start > hi {
                return false;
            }
        } else {
            let p = (lo - start) / (end - start);
            let q = (hi - start) / (end - start);
            enter = enter.max(p.min(q));
            exit = exit.min(p.max(q));
        }
    }
    enter <= exit
}

#[test]
fn an_unsafe_entry_refuses_by_name_when_skipping_is_disabled() {
    let mut facets = brick();
    facets.extend(
        cube(Point3::new(-2.0, -3.0, 21.0), Point3::new(2.0, 1.0, 23.0))
            .facets
            .iter()
            .copied(),
    );
    let mut p = params(DeburrAxisMode::Axis5, 0.0);
    p.base.unreachable = UnreachableEdges::Refuse;
    let chains = [vec![
        Point3::new(0.0, 0.0, 10.0),
        Point3::new(40.0, 0.0, 10.0),
    ]];
    let picked = DeburrEdges {
        include: &chains,
        exclude: &[],
    };
    let clear =
        multiaxis_deburr(&brick(), &p, &picked).expect("the picked edge cuts without the obstacle");
    assert!(!clear.breaks.is_empty());
    let err =
        multiaxis_deburr(&facets, &p, &picked).expect_err("the entry obstacle cannot be ignored");
    assert!(
        format!("{err:?}").contains("multi-axis deburr") && format!("{err:?}").contains("Refuse"),
        "{err:?}"
    );
}

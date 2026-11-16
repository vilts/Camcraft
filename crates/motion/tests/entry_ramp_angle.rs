//! #1674: a steep link angle must not steepen a Ramp entry.
//! Provenance: independent checks of standard linear and helical ramp slopes (RS-274).
//! Gate map: covers camcraft_motion::machining2d (contour_bore, descent).

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::{Approach, Links};
use camcraft_motion::machining2d::{
    BoreHole, BoreParams, CircularParams, ContourParams, Lead, Side, bore, circular, contour,
};

const FEED: f64 = 500.0;
const PLUNGE: f64 = 250.0;

fn region() -> Region {
    Region::new(vec![
        Loop::new(vec![
            Point2::new(-20.0, -20.0),
            Point2::new(20.0, -20.0),
            Point2::new(20.0, 20.0),
            Point2::new(-20.0, 20.0),
        ])
        .unwrap(),
    ])
    .unwrap()
}

fn links(entry_angle: f64) -> Links {
    Links {
        ramp_angle: 45.0,
        approach: Approach {
            ramp_angle: entry_angle,
            ..Approach::DEFAULT
        },
        ..Links::PLANAR
    }
}

fn contour_path(entry_angle: f64, climb: bool) -> Toolpath {
    contour_with_links(links(entry_angle), climb)
}

fn contour_with_links(links: Links, climb: bool) -> Toolpath {
    contour(
        &region(),
        &ContourParams {
            tool_diameter: 6.0,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            retract_z: 2.0,
            top_z: 0.0,
            bottom_z: -6.0,
            stepdown: 2.0,
            feed: FEED,
            plunge_feed: PLUNGE,
            climb,
            side: Side::Inside,
            lead: Lead::None,
            tolerance: 0.01,
            tab_count: 0,
            tab_width: 0.0,
            tab_height: 0.0,
            entry: Entry3D::Ramp,
            helix_radius: 1.0,
            blend_turns: false,
            links,
        },
    )
    .expect("a legal inside contour")
}

fn circular_path(entry_angle: f64, climb: bool) -> Toolpath {
    circular_with_links(links(entry_angle), climb)
}

fn circular_with_links(links: Links, climb: bool) -> Toolpath {
    circular(
        &region(),
        &CircularParams {
            links,
            center: Point2::new(0.0, 0.0),
            radius: 3.0,
            vault: false,
            bottom_z: -6.0,
            top_z: 0.0,
            tool_diameter: 4.0,
            stock_to_leave: 0.0,
            stepover: 0.5,
            stepdown: 1.0,
            clearance_z: 5.0,
            retract_z: None,
            feed: FEED,
            plunge_feed: PLUNGE,
            climb,
            entry: Entry3D::Ramp,
            helix_radius: 1.0,
        },
    )
    .expect("a legal circular pocket")
}

fn bore_path(entry_angle: f64, climb: bool) -> Toolpath {
    bore(&BoreParams {
        holes: vec![BoreHole {
            xy: Point2::new(0.0, 0.0),
            radius: 6.0,
            top_z: 0.0,
            floor_z: -6.0,
            through: false,
            stepped: false,
        }],
        tool_diameter: 6.0,
        corner_radius: 0.0,
        stock_to_leave: 0.0,
        stepdown: 2.0,
        through_allowance: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        feed: FEED,
        plunge_feed: PLUNGE,
        climb,
        tolerance: 0.01,
        entry: Entry3D::Ramp,
        helix_radius: 1.0,
        links: links(entry_angle),
    })
    .expect("a legal bore")
}

/// Length along the actual XY interpolation, including circular sweeps.
fn xy_len(m: &Move) -> f64 {
    match m.arc {
        None => (m.to.x - m.from.x).hypot(m.to.y - m.from.y),
        Some(a) => {
            let ang =
                |p: camcraft_motion::geometry::Point3| (p.y - a.center.y).atan2(p.x - a.center.x);
            let sweep = if a.ccw {
                ang(m.to) - ang(m.from)
            } else {
                ang(m.from) - ang(m.to)
            };
            let sweep = sweep.rem_euclid(std::f64::consts::TAU);
            let sweep = if sweep == 0.0 {
                std::f64::consts::TAU
            } else {
                sweep
            };
            (m.from.x - a.center.x).hypot(m.from.y - a.center.y) * sweep
        }
    }
}

fn check(t: &Toolpath, angle: f64) -> f64 {
    let slope = angle.to_radians().tan();
    let named = t.notes.iter().any(|n| n.starts_with("entry:"));
    let mut ramps = 0;
    let mut max_slope: f64 = 0.0;
    for m in &t.moves {
        let dz = m.from.z - m.to.z;
        if m.kind == MoveKind::Rapid || dz <= 1e-9 {
            continue;
        }
        let len = xy_len(m);
        if len > 1e-9 {
            assert!(
                dz <= slope * (len + 1e-5),
                "entry at {:.3} degrees exceeds {angle}: {:?} -> {:?}",
                (dz / len).atan().to_degrees(),
                m.from,
                m.to,
            );
            ramps += 1;
            max_slope = max_slope.max(dz / len);
        } else {
            assert_eq!(
                m.feed,
                Some(PLUNGE),
                "axial substitution must use the plunge feed"
            );
            assert!(named, "axial substitution must be named: {:?}", t.notes);
        }
    }
    assert!(ramps > 0, "the fixture must exercise a ramp");
    assert!(
        t.moves.iter().any(|m| m.to.z <= -6.0),
        "the path reaches the floor"
    );
    max_slope.atan().to_degrees()
}

#[test]
fn inside_contour_default_entry_ignores_steep_link_angle() {
    for climb in [true, false] {
        check(&contour_path(0.0, climb), 3.0);
    }
}

#[test]
fn circular_default_entry_ignores_steep_link_angle() {
    for climb in [true, false] {
        check(&circular_path(0.0, climb), 3.0);
    }
}

#[test]
fn bore_default_entry_ignores_steep_link_angle() {
    for climb in [true, false] {
        check(&bore_path(0.0, climb), 3.0);
    }
}

#[test]
fn explicit_entry_angle_remains_independent_of_steep_link_angle() {
    for climb in [true, false] {
        assert!(check(&contour_path(5.0, climb), 5.0) > 3.0);
        assert!(check(&circular_path(5.0, climb), 5.0) > 3.0);
        assert!(check(&bore_path(5.0, climb), 5.0) > 3.0);
    }
}

#[test]
fn explicit_ramp_transition_keeps_its_own_link_angle() {
    for climb in [true, false] {
        let links = Links {
            transition: Some(Entry3D::Ramp),
            ..links(0.0)
        };
        for t in [
            circular_with_links(links, climb),
            contour_with_links(links, climb),
        ] {
            let mut steep_transition = false;
            let mut gentle_entry = false;
            for m in &t.moves {
                let dz = m.from.z - m.to.z;
                let len = xy_len(m);
                if m.kind == MoveKind::Rapid || dz <= 1e-9 || len <= 1e-9 {
                    continue;
                }
                let angle = (dz / len).atan().to_degrees();
                if m.from.z > 0.0 {
                    assert!(angle <= 3.0 + 1e-4, "entry steepened to {angle}");
                    gentle_entry = true;
                } else {
                    assert!(
                        angle <= 45.0 + 1e-4,
                        "transition exceeded its angle: {angle}"
                    );
                    steep_transition |= angle > 3.0 + 1e-4;
                }
            }
            assert!(gentle_entry, "the first entry ramps at its default");
            assert!(
                steep_transition,
                "later transitions still use the 45 degree link setting"
            );
        }
    }
}

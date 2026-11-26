//! Provenance: issue #3443's independently made sign plate mesh.
//! Gate map: covers camcraft_motion::machining2d (contour_depth).

use camcraft_motion::cl::MoveKind;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle, cube, slice};
use camcraft_motion::machining2d::{ContourParams, Lead, Side, contour_on_picked};
use camcraft_motion::machining3d::Entry3D;

#[test]
fn outside_sign_plate_reaches_bottom_with_four_tabs() {
    let raw: Vec<[[f64; 3]; 3]> =
        serde_json::from_str(include_str!("contour_outside_floor/fixtures/sign_plate_3443.json")).expect("mesh");
    let f: Vec<Triangle> = raw
        .into_iter()
        .map(|t| {
            let p = |v: [f64; 3]| Point3::new(v[0], v[1], v[2]);
            Triangle::new(p(t[0]), p(t[1]), p(t[2]))
        })
        .collect();
    let top = slice::slice(&f, 14.99).expect("top section");
    let outer = top
        .loops()
        .iter()
        .max_by(|a, b| a.area().abs().total_cmp(&b.area().abs()))
        .expect("outer");
    let region = Region::from_slices(vec![outer.clone()]).expect("picked ring");
    let p = params();
    let path = contour_on_picked(&region, &f, &[], &p).expect("outside contour");
    for z in [12.0, 9.0, 6.0, 3.0, 0.0] {
        assert!(
            path.moves
                .iter()
                .any(|m| m.kind == MoveKind::Feed && (m.to.z - z).abs() < 1e-6),
            "missing level {z}: {:?}",
            path.notes
        );
    }
    assert!(
        !path
            .notes
            .iter()
            .any(|n| n.contains("levels below skipped")),
        "{:?}",
        path.notes
    );
    let mut runs = Vec::new();
    let mut low: Option<f64> = None;
    for m in &path.moves {
        let z = m.from.z.min(m.to.z);
        if m.kind != MoveKind::Rapid && z < p.tab_height - 1e-6 {
            low = Some(low.map_or(z, |v| v.min(z)));
        } else if let Some(z) = low.take() {
            runs.push(z);
        }
    }
    runs.extend(low);
    assert_eq!(
        runs.iter().filter(|&&z| z.abs() < 1e-6).count(),
        4,
        "four bottom tab gaps"
    );
}

fn params() -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 41.0,
        retract_z: 26.0,
        top_z: 15.0,
        bottom_z: 0.0,
        stepdown: 3.0,
        feed: 2500.0,
        plunge_feed: 600.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::Linear { length: 8.0 },
        tolerance: 0.01,
        tab_count: 4,
        tab_width: 6.0,
        tab_height: 2.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        picks: Default::default(),
    }
}

#[test]
fn picked_ring_refuses_unread_deeper_plane_after_clear_levels() {
    let mut f = Vec::new();
    let mut bodies = Vec::new();
    for (lo, hi) in [
        ([-15.0, 10.0, 0.0], [15.0, 15.0, 15.0]),
        ([-15.0, -15.0, 0.0], [15.0, -10.0, 15.0]),
        ([10.0, -10.0, 0.0], [15.0, 10.0, 15.0]),
        ([-15.0, -10.0, 0.0], [-10.0, 10.0, 15.0]),
    ] {
        let start = f.len();
        let p = |v: [f64; 3]| Point3::new(v[0], v[1], v[2]);
        f.extend(cube(p(lo), p(hi)).facets.iter().copied());
        bodies.push(start..f.len());
    }
    let start = f.len();
    f.push(Triangle::new(
        Point3::new(0.0, 0.0, 2.0),
        Point3::new(2.0, 0.0, 2.0),
        Point3::new(1.0, 0.0, 6.0),
    ));
    bodies.push(start..f.len());
    let region = Region::new(vec![
        Loop::new(vec![
            Point2::new(-10.0, -10.0),
            Point2::new(10.0, -10.0),
            Point2::new(10.0, 10.0),
            Point2::new(-10.0, 10.0),
        ])
        .expect("ring"),
    ])
    .expect("region");
    let p = ContourParams {
        side: Side::Inside,
        tab_count: 0,
        lead: Lead::None,
        ..params()
    };
    let err = contour_on_picked(&region, &f, &bodies, &p)
        .expect_err("unreadable lower section must not produce a partial program")
        .to_string();
    assert!(
        err.contains("cannot be sectioned") && err.contains("3.000"),
        "{err}"
    );
}

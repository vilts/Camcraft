//! Move simulation on real generated paths: the timeline built from
//! contour and drilling caches must obey the same laws the unit tests
//! pin on synthetic ones - and the generators' own feed numbers must
//! round-trip through the sim's time.
//!
//! Gate map (M4.4): covers cl::cl, motion::drilling, geometry::geo2d, motion::machining2d, sim::sim, tools::tools.

use camcraft_app::cl::Toolpath;
use camcraft_app::drilling::{self, CycleParams, CycleChoice, Hole};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::machining2d::{contour, ContourParams, Lead, Side};
use camcraft_app::sim::{script, Readout, Traverse};
use camcraft_app::tools::{catalog, Tool};

fn tr() -> Traverse {
    Traverse {
        xy: 6000.0,
        z: 4000.0,
        ..Default::default()
    }
}

fn tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .unwrap()
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

fn contour_of(r: &Region) -> Toolpath {
    let p = ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 4.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Arc { radius: 3.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    };
    contour(r, &p).expect("contour")
}

fn holes_at(pts: &[(f64, f64)], floor_z: f64, through: bool) -> Vec<Hole> {
    pts.iter()
        .map(|(x, y)| Hole {
            xy: Point2::new(*x, *y),
            radius: 3.0,
            entry_z: 0.0,
            floor_z,
            through,
            stepped: false,
        })
        .collect()
}

#[test]
fn contour_timeline_matches_its_own_geometry() {
    let region = rect(0.0, 0.0, 40.0, 40.0);
    let t = contour_of(&region);
    let sc = script([("C1", &t)], &tr());
    assert!(!sc.is_empty());
    assert!(!sc.truncated);
    assert_eq!(sc.ops.len(), 1);

    // Every straight feed move's seconds are exactly its distance
    // over its feed word; an arc's are at least its chord's (sweep
    // length is longer, and the sim uses the sweep). Rapids pace on
    // the traverse rates. The sim cannot drift from the geometry it
    // walks.
    assert_eq!(t.moves.len(), sc.segs.len());
    for (m, s) in t.moves.iter().zip(&sc.segs) {
        let d = ((m.to.x - m.from.x).powi(2)
            + (m.to.y - m.from.y).powi(2)
            + (m.to.z - m.from.z).powi(2))
        .sqrt();
        use camcraft_app::cl::MoveKind;
        match m.kind {
            MoveKind::Rapid => {
                let axis_secs = (60.0 * (m.to.x - m.from.x).abs() / tr().xy)
                    .max(60.0 * (m.to.y - m.from.y).abs() / tr().xy)
                    .max(60.0 * (m.to.z - m.from.z).abs() / tr().z);
                assert!(
                    (s.secs - axis_secs).abs() <= 1e-9 * s.secs.max(1.0),
                    "rapid paced by its slowest axis"
                );
            }
            MoveKind::Feed | MoveKind::Plunge => {
                let f = m.feed.expect("cutting moves carry a feed");
                if m.arc.is_none() {
                    assert!(
                        (s.secs - 60.0 * d / f).abs() <= 1e-9 * s.secs.max(1.0),
                        "feed seconds = distance over feed"
                    );
                } else {
                    assert!(s.secs >= 60.0 * d / f, "an arc beats its chord's time");
                }
            }
        }
    }

    let e = sc.at(sc.total);
    assert!(e.done);
    let end = t.end().unwrap();
    assert!((e.pos.x - end.x).abs() < 1e-6);
    assert!((e.pos.y - end.y).abs() < 1e-6);
    assert!((e.pos.z - end.z).abs() < 1e-6);
}

#[test]
fn drilling_timeline_carries_every_cycle_dwell() {
    let tool = tool("DR-0600");
    let holes = holes_at(&[(0.0, 0.0), (20.0, 0.0)], -12.0, false);
    let p = CycleParams {
        clearance_z: 10.0,
        r_z: 2.0,
        hop_z: None,
        bottom_z: None,
        cycle: CycleChoice::Peck,
        peck: 4.0,
        dwell: 1.5,
        feed: 200.0,
        plunge_feed: 60.0,
        tool: &tool,
        holes: &holes,
    };
    let t = drilling::drill(&p).expect("drill");
    let sc = script([("D1", &t)], &tr());

    // Sum of the script's dwells equals the sum the cycles carry -
    // none lost, none invented. (Whether Peck records a dwell at all
    // is drilling's law; consistency is the sim's.)
    let want: f64 = t.cycles.iter().map(|c| c.dwell).sum();
    let got: f64 = sc.segs.iter().map(|s| s.dwell).sum();
    assert!((want - got).abs() < 1e-12, "no dwell is lost or invented");
    // And the timeline is exactly travel plus standstill.
    let travel: f64 = sc.segs.iter().map(|s| s.secs).sum();
    assert!((sc.total - (travel + got)).abs() < 1e-9);
    // Each dwell window really reads as a dwell at its middle.
    for (i, s) in sc.segs.iter().enumerate() {
        if s.dwell > 0.0 {
            if let Readout::Dwell(d) = sc.at(sc.cum[i] - s.dwell / 2.0).readout {
                assert!((d - s.dwell).abs() < 1e-12);
            } else {
                panic!("a dwell window must read as Dwell at its middle");
            }
        }
    }
}

#[test]
fn two_operations_form_one_continuous_timeline() {
    // Contour then a drill far away: the connector rapid is
    // attributed to the arriving op, and the path never teleports -
    // every window boundary is a shared point.
    let region = rect(0.0, 0.0, 40.0, 40.0);
    let c = contour_of(&region);

    let tool = tool("DR-0600");
    let holes = holes_at(&[(60.0, 60.0)], -8.0, false);
    let dp = CycleParams {
        clearance_z: 10.0,
        r_z: 2.0,
        hop_z: None,
        bottom_z: None,
        cycle: CycleChoice::Drill,
        peck: 0.0,
        dwell: 0.0,
        feed: 200.0,
        plunge_feed: 60.0,
        tool: &tool,
        holes: &holes,
    };
    let d = drilling::drill(&dp).expect("drill");

    let sc = script([("Contour1", &c), ("Drill1", &d)], &tr());
    assert_eq!(sc.ops.len(), 2);
    for w in sc.segs.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        assert!(
            (a.to.x - b.from.x).abs() < 1e-9
                && (a.to.y - b.from.y).abs() < 1e-9
                && (a.to.z - b.from.z).abs() < 1e-9,
            "the timeline never teleports"
        );
    }
    let op1_start = sc.segs.iter().position(|s| s.op == 1).unwrap();
    assert!(op1_start > 0);
    // The frame mid-connector is Rapid, named for op 1, and it was
    // not a zero-time hop.
    let conn = &sc.segs[op1_start];
    assert!(conn.secs > 0.0);
    assert_eq!(conn.feed, None);
    let f = sc.at(sc.cum[op1_start] - conn.secs / 2.0);
    assert_eq!(f.readout, Readout::Rapid);
    assert_eq!(f.op, Some(1));
}

#[test]
fn a_posed_move_paces_on_its_rotary_slot_through_app_sim() {
    // The app seam of M5.1 stage G: the machine file's B rate reaches
    // the timeline through `app::sim`'s own Traverse, and a move
    // paced by its turn shows both clocks - XY holding at arrival
    // while the head is still mid-swing.
    use camcraft_app::cl::{Move, MoveKind};
    use camcraft_app::geometry::Point3;
    use camcraft_app::sim::RotRate;
    let mut t = tr();
    t.rot[1] = RotRate {
        normal: Some(1000.0),
        rapid: None,
    };
    let path = Toolpath::new(vec![
        Move::new(
            MoveKind::Rapid,
            Point3::new(-20.0, -20.0, 5.0),
            Point3::new(0.0, 0.0, 5.0),
            None,
        ),
        Move::new(
            MoveKind::Feed,
            Point3::new(0.0, 0.0, 5.0),
            Point3::new(30.0, 0.0, 5.0),
            Some(6000.0),
        )
        .with_axis([1.0, 0.0, 0.0]), // B = 90
    ]);
    let sc = script([("Tilt1", &path)], &t);
    assert!((sc.segs[1].secs - 5.4).abs() < 1e-9, "60*90/1000 paces it, not 60*30/6000");
    // Half a turn in, the table has long held at its destination.
    let f = sc.at(sc.cum[0] + 2.7);
    assert!((f.pos.x - 30.0).abs() < 1e-9, "XY holds at arrival");
    let s = std::f64::consts::FRAC_1_SQRT_2;
    assert!(
        (f.axis[0] - s).abs() < 1e-9 && (f.axis[2] - s).abs() < 1e-9,
        "the head is exactly mid-swing: {:?}",
        f.axis
    );
}

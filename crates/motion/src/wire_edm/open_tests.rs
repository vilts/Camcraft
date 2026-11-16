//! The wire's open run (#1109): a cut-off across a plate and a slot
//! from the stock's edge stand exactly the kerf offset off the run on
//! the stated side, threaded outside the stock by the clearance; each
//! refusal is named. The wire and gap are SYNTHETIC - a plausible
//! size, from no machine's manual (#290 D4, D10).

use super::ring::pt_poly;
use super::*;

const K: f64 = 0.165;

fn plate() -> Region {
    let pts = vec![Point2::new(0.0, 0.0), Point2::new(100.0, 0.0), Point2::new(100.0, 50.0), Point2::new(0.0, 50.0)];
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

fn params(side: WireSide) -> WireParams {
    WireParams {
        wire_diameter: 0.25,
        gap: 0.04,
        condition: "C110".into(),
        side,
        start: None,
        plane_z: -12.0,
        stock: Some(plate()),
        clearance: 1.0,
        tabs: None,
    }
}

fn run(pts: &[(f64, f64)]) -> Vec<Vec<Point2>> {
    vec![pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()]
}

fn refusal(chains: &[Vec<Point2>], p: &WireParams) -> String {
    wire_contour(&[], chains, p).expect_err("refused").to_string()
}

fn pts(path: &Toolpath) -> Vec<Point2> {
    let mut v = vec![Point2::new(path.moves[0].from.x, path.moves[0].from.y)];
    v.extend(path.moves.iter().map(|m| Point2::new(m.to.x, m.to.y)));
    v
}

/// Done means: a cut-off across the plate stands exactly the kerf offset
/// off the line on the stated side, threaded and run out the clearance
/// past the kerf outside the stock; left and right stand a kerf apart.
#[test]
fn a_cut_off_across_a_plate_stands_the_kerf_offset_on_the_stated_side() {
    let line = run(&[(60.0, 0.0), (60.0, 50.0)]);
    let right = wire_contour(&[], &line, &params(WireSide::Right)).unwrap();
    let left = wire_contour(&[], &line, &params(WireSide::Left)).unwrap();
    for (path, x) in [(&right, 60.0 + K), (&left, 60.0 - K)] {
        let v = pts(path);
        assert!(v.iter().all(|q| (q.x - x).abs() < 1e-9), "{v:?}");
        assert!((v[0].y + (K + 1.0)).abs() < 1e-6, "threaded {:?}", v[0]);
        assert!((v[v.len() - 1].y - (50.0 + K + 1.0)).abs() < 1e-6, "run out {:?}", v[v.len() - 1]);
        assert!(path.moves.iter().all(|m| m.kind == MoveKind::Feed && m.feed.is_none() && m.to.z == -12.0));
        assert!(path.notes.iter().any(|n| n.contains("open run, kerf on the")) && path.notes.iter().any(|n| n.contains("no start hole")));
    }
    assert!((right.moves[0].from.x - left.moves[0].from.x - params(WireSide::Left).kerf_width()).abs() < 1e-9);
}

/// A slot entered from the stock's edge (2 mm inside corners, the kerf
/// inside the U): every wire point stands at least the offset off the
/// run, and the ride along it exactly at it.
#[test]
fn a_slot_from_the_edge_rides_inside_at_the_kerf_offset() {
    let mut u = vec![Point2::new(0.0, 10.0)];
    for (cx, cy, a0) in [(28.0, 12.0, -90.0_f64), (28.0, 18.0, 0.0)] {
        for s in 0..=30 {
            let t = (a0 + 3.0 * f64::from(s)).to_radians();
            u.push(Point2::new(cx + 2.0 * t.cos(), cy + 2.0 * t.sin()));
        }
    }
    u.push(Point2::new(0.0, 20.0));
    let path = wire_contour(&[], std::slice::from_ref(&u), &params(WireSide::Left)).unwrap();
    let v = pts(&path);
    let ride = &v[1..v.len() - 1];
    for q in ride {
        assert!((pt_poly(*q, &u) - K).abs() < 1e-6, "{q:?} stands {}", pt_poly(*q, &u));
        assert!(q.y > 10.0 && q.y < 20.0 && q.x < 30.0, "{q:?} left the slot");
    }
    assert!((v[0].x + K + 1.0).abs() < 1e-6 && (v[v.len() - 1].x + K + 1.0).abs() < 1e-6, "{:?}", (v[0], v[v.len() - 1]));
    // The same U with a sharp corner is refused by name on the wire's
    // inside; the kerf outside the U mitres it and cuts.
    let sharp = run(&[(0.0, 10.0), (30.0, 10.0), (30.0, 20.0), (0.0, 20.0)]);
    let why = refusal(&sharp, &params(WireSide::Left));
    assert!(why.contains("tighter than the wire can cut on the left side") && why.contains("0.1650"), "{why}");
    assert!(wire_contour(&[], &sharp, &params(WireSide::Right)).is_ok());
}

#[test]
fn every_open_run_refusal_is_named() {
    let line = run(&[(60.0, 0.0), (60.0, 50.0)]);
    let why = refusal(&line, &params(WireSide::Die));
    assert!(why.contains("no punch or die side") && why.contains("left or right"), "{why}");
    let why = refusal(&line, &WireParams { stock: None, ..params(WireSide::Left) });
    assert!(why.contains("states no stock"), "{why}");
    let why = refusal(&run(&[(60.0, 10.0), (60.0, 50.0)]), &params(WireSide::Left));
    assert!(why.contains("start at X60.000 Y10.000 stops 10.000 mm inside the stock"), "{why}");
    let why = refusal(&run(&[(60.0, 0.0), (60.0, 40.0)]), &params(WireSide::Left));
    assert!(why.contains("end at X60.000 Y40.000 stops"), "{why}");
    let why = refusal(&run(&[(-5.0, -5.0), (105.0, -5.0)]), &params(WireSide::Left));
    assert!(why.contains("never enters the stock"), "{why}");
    let crossed = run(&[(50.0, -1.0), (50.0, 30.0), (40.0, 20.0), (60.0, 20.0), (60.0, 51.0)]);
    let why = refusal(&crossed, &params(WireSide::Left));
    assert!(why.contains("crosses itself"), "{why}");
    let why = refusal(&run(&[(60.0, -1.0), (60.0, 30.0), (60.0, -1.0)]), &params(WireSide::Left));
    assert!(why.contains("crosses itself") || why.contains("doubles back"), "{why}");
    let why = refusal(&line, &WireParams { clearance: f64::NAN, ..params(WireSide::Left) });
    assert!(why.contains("clearance"), "{why}");
    let two = vec![line[0].clone(), run(&[(70.0, 0.0), (70.0, 50.0)])[0].clone()];
    assert!(refusal(&two, &params(WireSide::Left)).contains("2 open run(s)"));
    // Left / right on a closed profile: a closed profile states die or punch.
    let ring = Loop::new(vec![Point2::new(0.0, 0.0), Point2::new(10.0, 0.0), Point2::new(10.0, 10.0), Point2::new(0.0, 10.0)]).unwrap();
    let why = wire_contour(&[ring], &[], &WireParams { start: Some(Point2::new(5.0, 5.0)), ..params(WireSide::Right) })
        .expect_err("refused")
        .to_string();
    assert!(why.contains("profile is closed") && why.contains("die or punch"), "{why}");
    // Tabs on an open run (#539): refused by name, never silently dropped.
    let tabs = TabParams { count: Some(1), width: Some(2.0), at: vec![Point2::new(60.0, 25.0)], cutoff: None };
    let why = refusal(&line, &WireParams { tabs: Some(tabs), ..params(WireSide::Left) });
    assert!(why.contains("tabs are stated on an open run"), "{why}");
}

/// #536 x #1109 (M3's review): a tapered open run is refused by name.
/// Leaning its polyline as a ring closed it with a chord from the run's
/// last point back to its first, across the part. Fixture: a 13-point
/// semicircle entered from the stock's edge, at 3 degrees.
#[test]
fn a_tapered_open_run_is_refused_by_name_not_closed_across_the_part() {
    let semi: Vec<(f64, f64)> = (0..=12)
        .map(|s| {
            let t = (180.0 - 15.0 * f64::from(s)).to_radians();
            (50.0 + 20.0 * t.cos(), 20.0 * t.sin())
        })
        .collect();
    let chains = run(&semi);
    let p = params(WireSide::Left);
    assert!(wire_contour(&[], &chains, &p).is_ok(), "the vertical run cuts");
    let taper = WireTaper { angle_deg: 3.0, top_z: 0.0, max_deg: Some(15.0), machine: "M".into(), corner: WireCorner::Mitre };
    for side in [WireSide::Left, WireSide::Right] {
        let p = params(side);
        let why = wire_contour_tapered(&[], &chains, &p, &taper).expect_err("refused").to_string();
        assert!(why.contains("3.000 degree taper is asked on an open run"), "{why}");
    }
    // Zero taper is the vertical cut itself, open run included.
    let zero = WireTaper { angle_deg: 0.0, ..taper };
    assert_eq!(wire_contour_tapered(&[], &chains, &p, &zero).unwrap().moves, wire_contour(&[], &chains, &p).unwrap().moves);
}

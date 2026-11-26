//! Issue #323 kernel controls: stock-aware exits for all three contour kinds.
//! Provenance: the existing turning_stock fixture and #229 exit law.
//! Gate map (M4.4): covers camcraft_motion::turning (contour, stock, prof).
//! Lane shape (M4.4): small revolved fixtures, no slow verdict lanes.

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::turning::profile::Frame;
use camcraft_motion::turning::{
    ChamferParams, MultiPassParams, StockEnv, TaperParams, chamfer, chamfer_in, multi_pass,
    multi_pass_in, taper, taper_in,
};

fn revolve(meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
            let (a, b, c, d) = (
                v(j, ra, za),
                v(j + 1, ra, za),
                v(j, rb, zb),
                v(j + 1, rb, zb),
            );
            if ra <= 1e-12 {
                facets.push(Triangle::new(a, d, c));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a, b, d));
            } else {
                facets.push(Triangle::new(a, b, d));
                facets.push(Triangle::new(a, d, c));
            }
        }
    }
    facets
}

fn rho(p: Point3) -> f64 {
    p.x.hypot(p.y)
}

fn exits(path: &Toolpath, top: f64) -> Vec<&Move> {
    path.moves
        .iter()
        .filter(|m| {
            (m.from.z - top).abs() < 1e-8
                && (m.to.z - top).abs() < 1e-8
                && rho(m.to) > rho(m.from) + 1e-6
        })
        .collect()
}

fn controls(part: &[Triangle], blind: Toolpath, ride: impl Fn(Option<StockEnv>, f64) -> Toolpath) {
    let frame = Frame::about(part).expect("axis");
    let stock = |top| {
        Some(StockEnv {
            r_out: 10.0,
            clearance: 1.0,
            s_top: frame.station_of(top),
            s_bot: f64::NAN,
        })
    };
    let short = ride(stock(40.0), 40.0);
    assert_eq!(
        blind.moves,
        ride(None, 40.0).moves,
        "compatibility entry point"
    );
    assert_eq!(
        blind.moves, short.moves,
        "same-sized, same-end stock keeps the old path"
    );
    let long = ride(stock(48.0), 40.0);
    assert_eq!(short.moves.len(), long.moves.len());
    let out = exits(&long, 40.0);
    assert!(
        !out.is_empty()
            && out
                .iter()
                .all(|m| m.kind == MoveKind::Feed && m.feed == Some(50.0))
    );
    assert!(
        exits(&short, 40.0)
            .iter()
            .all(|m| m.kind == MoveKind::Rapid)
    );
    for (a, b) in short.moves.iter().zip(&long.moves) {
        let mut expected = *a;
        if exits(&short, 40.0).contains(&a) {
            expected.kind = MoveKind::Feed;
            expected.feed = Some(50.0);
        }
        assert_eq!(expected, *b, "only the exit's kind/feed changes");
    }
    // The bar need not exceed the part: an interior window also ends in retained stock.
    let window = ride(stock(40.0), 39.0);
    let out = exits(&window, 39.0);
    assert!(
        !out.is_empty()
            && out
                .iter()
                .all(|m| m.kind == MoveKind::Feed && m.feed == Some(50.0))
    );
    // Stock-aware axial travel must be clear before feeding out radially.
    let wide = ride(
        Some(StockEnv {
            r_out: 14.0,
            clearance: 1.0,
            s_top: frame.station_of(48.0),
            s_bot: f64::NAN,
        }),
        40.0,
    );
    let axial: Vec<_> = wide
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid && (m.from.z - m.to.z).abs() > 1e-8)
        .collect();
    assert!(!axial.is_empty());
    assert!(
        axial
            .iter()
            .all(|m| rho(m.from) >= 15.8 - 1e-8 && rho(m.to) >= 15.8 - 1e-8)
    );
}

#[test]
fn chamfer_stock_exits_preserve_geometry_and_cover_interior_windows() {
    let part = revolve(&[
        (0.0, -16.0),
        (10.0, -16.0),
        (10.0, 38.0),
        (8.0, 40.0),
        (0.0, 40.0),
    ]);
    let p = ChamferParams {
        tolerance: 0.05,
        clearance_z: 56.0,
        top_z: 40.0,
        bottom_z: 38.0,
        angle_deg: 45.0,
        feed: 200.0,
        plunge_feed: 50.0,
        nose_radius: 0.8,
    };
    controls(&part, chamfer(&part, &p).expect("blind"), |s, top| {
        chamfer_in(&part, &ChamferParams { top_z: top, ..p }, s).expect("chamfer")
    });
}

#[test]
fn taper_stock_exits_preserve_geometry_and_cover_interior_windows() {
    let part = revolve(&[(0.0, -16.0), (10.0, -16.0), (8.0, 40.0), (0.0, 40.0)]);
    let p = TaperParams {
        tolerance: 0.05,
        clearance_z: 56.0,
        top_z: 40.0,
        bottom_z: -16.0,
        bottom_dia: 20.0,
        top_dia: 16.0,
        feed: 200.0,
        plunge_feed: 50.0,
        nose_radius: 0.8,
    };
    controls(&part, taper(&part, &p).expect("blind"), |s, top| {
        taper_in(
            &part,
            &TaperParams {
                top_z: top,
                top_dia: 20.0 - 4.0 * (top + 16.0) / 56.0,
                ..p
            },
            s,
        )
        .expect("taper")
    });
}

#[test]
fn multipass_stock_exits_preserve_geometry_and_cover_interior_windows() {
    let part = revolve(&[
        (0.0, -16.0),
        (10.0, -16.0),
        (10.0, 2.0),
        (6.0, 2.0),
        (6.0, 40.0),
        (0.0, 40.0),
    ]);
    let p = MultiPassParams {
        tolerance: 0.05,
        clearance_z: 56.0,
        top_z: 40.0,
        bottom_z: -16.0,
        stepdown: 0.5,
        stock_to_leave: 0.25,
        passes: 3,
        feed: 200.0,
        plunge_feed: 50.0,
        nose_radius: 0.8,
    };
    controls(&part, multi_pass(&part, &p).expect("blind"), |s, top| {
        multi_pass_in(&part, &MultiPassParams { top_z: top, ..p }, s).expect("multi")
    });
}

#[test]
fn a_wide_stock_ladder_drops_air_rungs_outside_a_restricted_window() {
    let part = revolve(&[
        (0.0, -16.0),
        (6.0, -16.0),
        (6.0, 0.0),
        (10.0, 0.0),
        (10.0, 40.0),
        (0.0, 40.0),
    ]);
    let frame = Frame::about(&part).expect("axis");
    let p = MultiPassParams {
        tolerance: 0.05,
        clearance_z: 56.0,
        top_z: 40.0,
        bottom_z: 2.0,
        stepdown: 0.5,
        stock_to_leave: 0.25,
        passes: 3,
        feed: 200.0,
        plunge_feed: 50.0,
        nose_radius: 0.8,
    };
    let path = multi_pass_in(
        &part,
        &p,
        Some(StockEnv {
            r_out: 14.0,
            clearance: 1.0,
            s_top: frame.station_of(48.0),
            s_bot: f64::NAN,
        }),
    )
    .expect("windowed multipass");
    let plunges: Vec<_> = path
        .moves
        .windows(2)
        .filter(|w| w[0].kind == MoveKind::Rapid && w[1].kind == MoveKind::Feed)
        .map(|w| &w[1])
        .collect();
    assert!(
        plunges.len() > p.passes as usize,
        "roughing plus all springs"
    );
    assert!(
        plunges.iter().all(|m| (m.to.z - 2.0).abs() < 1e-8),
        "the window excludes the narrow section"
    );
    let first = rho(plunges[0].to) - p.nose_radius;
    assert!(
        first < 14.0 && first >= 14.0 - p.stepdown - 1e-8,
        "first roughing rung must engage the stock: {first}"
    );
    assert!(
        plunges.iter().all(|m| rho(m.to) - p.nose_radius < 14.0),
        "no air-only rungs"
    );
    let springs = &plunges[plunges.len() - p.passes as usize..];
    for (i, m) in springs.iter().enumerate() {
        let expected = 10.0 + p.nose_radius + p.stock_to_leave * (2 - i) as f64 / 3.0;
        assert!(
            (rho(m.to) - expected).abs() < 1e-8,
            "all finishing rides remain"
        );
    }
}

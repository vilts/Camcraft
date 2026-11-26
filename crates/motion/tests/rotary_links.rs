//! M13.8 rotary link law tests: how the three rotary kinds START and
//! how they LINK. A rotary program's first record is a clearance pose
//! on the first cut's own radial - never the planning frame's origin
//! column (on the axis, or at azimuth zero mid-bar); Rotary Parallel
//! leaves every pass along the radius to its clearance cylinder,
//! indexes on that cylinder and comes back in along the radius - never
//! a world-Z lift, which on a turned bar is a sideways sweep of a
//! radially posed tool; Rotary Offset peels from the STOCK's outer
//! radius about the axis, not from the part's.
//!
//! Gate map (M4.4): covers camcraft_motion::multiaxis (rotary,
//! rotary_offset, wrap_exit, rotary_links) on this crate's own revolve
//! fixtures.
//! Lane shape (M4.4): all tests here run in seconds.
//! Provenance: radial retract / index / radial plunge linking of 4-axis
//! rotary milling from general machining practice; our own fixtures.

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::finishing3d::{Finish3DParams, FinishPattern};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams};
use camcraft_motion::multiaxis::{rotary_contour, rotary_offset, rotary_offset_in, rotary_parallel, AxisControl};

const TAU: f64 = std::f64::consts::TAU;
const R: f64 = 1.0; // both param sets below carry a 2 mm ball

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn finish(top_z: f64, bottom_z: f64, clearance_z: f64) -> Finish3DParams {
    Finish3DParams {
        gap_link: camcraft_motion::finishing3d::GapLink::Jump,
        pattern: FinishPattern::Waterline,
        stepover: None,
        tool_diameter: 2.0 * R,
        tolerance: 0.05,
        clearance_z,
        retract_z: None,
        top_z,
        bottom_z,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
        waterlines: true,
    }
}

fn rough(v_top: f64, v_bot: f64) -> Pocket3DParams {
    Pocket3DParams {
        pass_start: Default::default(),
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: 2.0 * R,
        stepover: 0.8,
        stock_to_leave: 0.0,
        clearance_z: 100.0,
        top_z: v_top,
        bottom_z: v_bot,
        stepdown: 1.0,
        feed: 400.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

const STILL: AxisControl = AxisControl {
    lead_deg: 0.0,
    lag_deg: 0.0,
    max_tilt_deg: None,
};

/// A closed lathe solid swept about Z (the twins' fixture).
fn revolve(n_circ: usize, meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / n_circ as f64;
        p3(r * th.cos(), r * th.sin(), z)
    };
    let mut t = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..n_circ {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                t.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                t.push(Triangle::new(a1, a2, b2));
            } else {
                t.push(Triangle::new(a1, a2, b2));
                t.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    t
}

/// The stepped shaft: skin max r2.5 at stations 21..28.
fn lathe_bar() -> Vec<Triangle> {
    revolve(16, &[(0.0, 0.0), (2.0, 0.0), (2.0, 6.0), (1.2, 8.5), (1.2, 18.0), (2.5, 21.0), (2.5, 28.0), (0.0, 28.0)])
}

fn laid(q: Point3) -> Point3 {
    p3(q.z, q.x, q.y)
}

fn laid_bar() -> Vec<Triangle> {
    lathe_bar().into_iter().map(|f| Triangle::new(laid(f.a), laid(f.b), laid(f.c))).collect()
}

/// (station, radius, azimuth) of a world point about the fixture's
/// axis: world z standing, world x laid.
fn cyl(p: Point3, lying: bool) -> (f64, f64, f64) {
    let (v, a, b) = if lying { (p.x, p.y, p.z) } else { (p.z, p.x, p.y) };
    (v, (a * a + b * b).sqrt(), b.atan2(a))
}

fn dang(a: f64, b: f64) -> f64 {
    (a - b + std::f64::consts::PI).rem_euclid(TAU) - std::f64::consts::PI
}

/// The ball center a posed tip stands under (the M13 pivot law).
fn center_at(p: Point3, axis: Option<[f64; 3]>) -> Point3 {
    let a = axis.unwrap_or([0.0, 0.0, 1.0]);
    p3(p.x + R * a[0], p.y + R * a[1], p.z + R * a[2])
}

/// The pose stated at `p` is the pure radial of its own ball center.
fn radial_pose(p: Point3, axis: Option<[f64; 3]>, lying: bool) -> bool {
    let Some(a) = axis else { return false };
    let c = center_at(p, axis);
    let (n, l) = if lying {
        ([0.0, c.y, c.z], (c.y * c.y + c.z * c.z).sqrt())
    } else {
        ([c.x, c.y, 0.0], (c.x * c.x + c.y * c.y).sqrt())
    };
    l > 1e-9 && (a[0] * n[0] + a[1] * n[1] + a[2] * n[2]) / l > 1.0 - 1e-9
}

/// Where the program starts: its first record is a RAPID, it starts
/// off the axis, on the radial (station + azimuth) of the first cut's
/// approach, holding the pure radial there - and the caller's check
/// says the start is on the kind's clearance.
fn starts_on_the_first_cut(t: &Toolpath, lying: bool, same_station: bool) -> (f64, f64, f64) {
    let m0 = &t.moves[0];
    assert_eq!(m0.kind, MoveKind::Rapid, "the first record positions at rapid");
    let fc: &Move = t.moves.iter().find(|m| m.kind != MoveKind::Rapid).expect("a cutting move");
    let s = cyl(m0.from, lying);
    let f = cyl(fc.from, lying);
    assert!(s.1 > 0.5, "the first move starts ON the axis: {:?}", m0.from);
    assert!(dang(s.2, f.2).abs() < 1e-6, "start azimuth {:.4} off the first cut's {:.4}: {:?}", s.2, f.2, m0.from);
    if same_station {
        assert!((s.0 - f.0).abs() < 1e-6, "start station {} off the first cut's {}", s.0, f.0);
    }
    assert!(radial_pose(m0.from, m0.axis, lying), "the start does not hold the radial: {:?}", m0.axis);
    s
}

#[test]
fn the_rotary_contour_starts_above_its_first_ring() {
    let t = rotary_contour(&lathe_bar(), &finish(28.0, 0.0, 33.0), &STILL).expect("rings");
    let (v, _, _) = starts_on_the_first_cut(&t, false, false);
    assert!(v >= 33.0 - 1e-9, "the start sits below the clearance station: {v}");
}

#[test]
fn the_rotary_parallel_starts_on_its_clearance_radius() {
    let t = rotary_parallel(&laid_bar(), &finish(3.0, -3.0, 10.0), &STILL).expect("passes");
    let (_, rho, _) = starts_on_the_first_cut(&t, true, true);
    assert!(rho >= 10.0 - 1e-6, "the start sits inside the clearance radius: {rho}");
}

#[test]
fn the_rotary_offset_starts_on_its_clearance_radius() {
    for lying in [false, true] {
        let bar = if lying { laid_bar() } else { lathe_bar() };
        let t = rotary_offset(&bar, &rough(28.0, 0.0), &STILL).expect("the wrap ladder");
        let (_, rho, _) = starts_on_the_first_cut(&t, lying, true);
        // rho_max 2.5 + one infeed + one tool radius.
        assert!(rho >= 4.5 - 1e-6, "the start sits inside the wrap's clearance: {rho}");
    }
}

/// The link law of a radial kind: every rapid ENDS on the clearance
/// cylinder (`rc`, tip radius); a rapid starting inside it, every
/// plunge, and every cut-side move climbing onto it is RADIAL - its
/// two ends share station and azimuth.
fn links_are_radial(t: &Toolpath, lying: bool, rc: f64) -> usize {
    let radial = |m: &Move| {
        let (a, b) = (cyl(m.from, lying), cyl(m.to, lying));
        (a.0 - b.0).abs() < 1e-6 && dang(a.2, b.2).abs() < 1e-6
    };
    let mut retracts = 0usize;
    for (i, m) in t.moves.iter().enumerate() {
        let (from, to) = (cyl(m.from, lying), cyl(m.to, lying));
        match m.kind {
            MoveKind::Rapid => {
                assert!(to.1 >= rc - 1e-6, "rapid {i} ends inside the clearance radius: {:.4}", to.1);
                if from.1 < rc - 1e-6 {
                    assert!(radial(m), "rapid {i} climbs off the radius: {:?} -> {:?}", m.from, m.to);
                    retracts += 1;
                }
            }
            MoveKind::Plunge => {
                assert!(radial(m), "plunge {i} comes in off the radius: {:?} -> {:?}", m.from, m.to);
            }
            MoveKind::Feed if to.1 >= rc - 1e-6 && from.1 < rc - 1e-6 => {
                assert!(radial(m), "retract {i} is not radial: {:?} -> {:?}", m.from, m.to);
                assert!(radial_pose(m.to, m.axis, lying), "retract {i} ends off the pure radial");
                retracts += 1;
            }
            MoveKind::Feed => {}
        }
    }
    retracts
}

#[test]
fn the_rotary_parallel_retracts_along_the_radius() {
    let t = rotary_parallel(&laid_bar(), &finish(3.0, -3.0, 10.0), &STILL).expect("passes");
    let n = links_are_radial(&t, true, 10.0);
    assert!(n > 5, "a raster of passes retracts many times, got {n}");
}

#[test]
fn the_rotary_parallel_hops_on_its_retract_radius() {
    let p = Finish3DParams { retract_z: Some(5.0), ..finish(3.0, -3.0, 10.0) };
    let t = rotary_parallel(&laid_bar(), &p, &STILL).expect("passes");
    let n = links_are_radial(&t, true, 5.0);
    assert!(n > 5, "a raster of passes retracts many times, got {n}");
    let hops = t.moves.iter().filter(|m| m.kind == MoveKind::Rapid && cyl(m.to, true).1 < 10.0 - 1e-6).count();
    assert!(hops > 5, "the hops ride the retract cylinder, not the clearance one: {hops}");
}

#[test]
fn the_rotary_parallel_refuses_a_clearance_inside_the_work() {
    let e = rotary_parallel(&laid_bar(), &finish(3.0, -3.0, 2.0), &STILL).expect_err("a clearance radius inside the bar");
    assert!(e.to_string().contains("radius"), "{e}");
}

/// A coaxial round bar of radius `rho` over the shaft's stations.
fn round_stock(rho: f64) -> Vec<Triangle> {
    revolve(32, &[(0.0, -1.0), (rho, -1.0), (rho, 29.0), (0.0, 29.0)])
}

#[test]
fn the_wrap_ladder_peels_from_the_stock_radius() {
    let bar = lathe_bar();
    let stock = round_stock(4.0);
    let t = rotary_offset_in(&bar, Some(&stock), &rough(28.0, 0.0), &STILL).expect("the wrap ladder");
    let cut = |m: &&Move| m.kind != MoveKind::Rapid;
    let top_cut = t.moves.iter().filter(cut).map(|m| cyl(m.to, false).1).fold(0.0f64, f64::max);
    let low_rapid = t.moves.iter().filter(|m| m.kind == MoveKind::Rapid).map(|m| cyl(m.to, false).1).fold(f64::INFINITY, f64::min);
    // Levels from the stock's r4 inward by the 1 mm infeed: the first
    // level peels skin the part's r2.5 never names.
    assert!((3.0 - 1e-6..=4.0 + 1e-6).contains(&top_cut),"the top level is not the stock's: {top_cut}");
    // Every rapid tip stands outside the uncut bar (the ladder's own
    // hops ride a level's infeed above it, the opening one infeed and a
    // radius out).
    assert!(low_rapid > 4.0 + 1e-6, "a rapid inside the stock: {low_rapid}");
    let (_, rho, _) = starts_on_the_first_cut(&t, false, true);
    assert!(rho >= 6.0 - 1e-6, "the start sits inside the stock's clearance: {rho}");
    // No stock, or one inside the part's own radius: the part's skin
    // stands, exactly as before.
    let plain = rotary_offset(&bar, &rough(28.0, 0.0), &STILL).expect("plain");
    let tight = rotary_offset_in(&bar, Some(&round_stock(2.0)), &rough(28.0, 0.0), &STILL).expect("tight");
    assert_eq!(plain.moves, tight.moves, "a stock inside the part changes nothing");
    let plain_top = plain.moves.iter().filter(cut).map(|m| cyl(m.to, false).1).fold(0.0f64, f64::max);
    assert!(plain_top <= 2.5 + 1e-6, "the plain ladder tops at the part's skin: {plain_top}");
}

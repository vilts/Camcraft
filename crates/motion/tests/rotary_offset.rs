//! M5.1 stage E2 rotary offset law tests: the pocket ladder run in
//! wrap space. What "right" means here: every cut tip stands ON a
//! cylinder the plan named (its ball center one tool radius outside
//! it - the M13 pivot law), the ladder
//! peels from the skin inward and never past the floor a column
//! legally allows, a bar roughs the same lying as standing, the
//! world heights window the STATIONS (and refuse when they window
//! nothing), and the developed seam is crossed by closed rings, not
//! left as a rest ridge.
//!
//! Gate map (M4.4): covers camcraft_motion::multiaxis (rotary offset
//! composition on the axis fit and the pocket ladder) with this
//! crate's own revolve fixtures.
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams};
use camcraft_motion::multiaxis::{rotary_offset, AxisControl};

const TAU: f64 = std::f64::consts::TAU;
const R: f64 = 1.0; // the tool radius the params below spell out

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The wrap kind's pocket numbers; the height fields are the
/// STATION window here, the ρ ladder the kernel derives itself.
fn params(v_top: f64, v_bot: f64) -> Pocket3DParams {
    Pocket3DParams {
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: 2.0 * R,
        stepover: 0.8,
        stock_to_leave: 0.0,
        clearance_z: 100.0, // replaced by the wrap's own plane
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

fn control(lead: f64, lag: f64, clamp: Option<f64>) -> AxisControl {
    AxisControl {
        lead_deg: lead,
        lag_deg: lag,
        max_tilt_deg: clamp,
    }
}

/// A closed lathe solid: the meridian polyline (rho, z), ascending
/// in z, swept about Z at n_circ turns (the twins' fixture, verbatim
/// - including the DIAGONAL split that keeps wall patches off the
///   bowtie a side split would open in a slicing law).
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

/// The stepped shaft (the twins' bar): foot r2.0, shoulder, cone,
/// barrel r1.2, barrel r2.5, crown - skin max 2.5 at v 21..28.
fn lathe_bar(n_circ: usize) -> Vec<Triangle> {
    revolve(
        n_circ,
        &[
            (0.0, 0.0),
            (2.0, 0.0),
            (2.0, 6.0),
            (1.2, 8.5),
            (1.2, 18.0),
            (2.5, 21.0),
            (2.5, 28.0),
            (0.0, 28.0),
        ],
    )
}

fn laid(q: Point3) -> Point3 {
    p3(q.z, q.x, q.y)
}

fn laid_bar(n_circ: usize) -> Vec<Triangle> {
    lathe_bar(n_circ)
        .into_iter()
        .map(|f| Triangle::new(laid(f.a), laid(f.b), laid(f.c)))
        .collect()
}

/// A grooved sleeve: skin r3.0 with a r2.0 groove at v 11..13 -
/// 2 stepdowns of peel above the floor, so the last legal level of
/// a groove column is EXACTLY the floor. The length is load-bearing:
/// a short fat revolve puts the caps' ẑ-normals near parity with the
/// walls' r̂-normals, and stage E's ISO eigen-guard (a sphere answers
/// every axis) refuses it before the concurrency test is tried.
fn grooved(n_circ: usize) -> Vec<Triangle> {
    revolve(
        n_circ,
        &[
            (0.0, 0.0),
            (3.0, 0.0),
            (3.0, 10.0),
            (2.0, 11.0),
            (2.0, 13.0),
            (3.0, 14.0),
            (3.0, 24.0),
            (0.0, 24.0),
        ],
    )
}

/// Distance of a world TIP from the standing Z axis: the tangent
/// cylinder the move actually turned to. M13 pivot law: the stream
/// locates the tip, and a radial ball's tip stands ON the level's
/// cylinder - its center one radius further out, never the tip.
fn tangent(p: Point3) -> f64 {
    (p.x * p.x + p.y * p.y).sqrt()
}

fn tangent_xz(p: Point3) -> f64 {
    (p.y * p.y + p.z * p.z).sqrt()
}

/// A rapid states the pure radial the head holds while it laps the
/// bar (the post's tilt is modal - M13), measured off its own ball
/// center; on the axis it may hold what it held.
fn rapid_radial(m: &camcraft_motion::cl::Move) {
    let Some(a) = m.axis else { return };
    let c = p3(m.to.x + R * a[0], m.to.y + R * a[1], m.to.z + R * a[2]);
    let l = (c.x * c.x + c.y * c.y).sqrt();
    if l > 1e-6 {
        let av = p3(a[0], a[1], a[2]);
        assert!(unit_angle(av, p3(c.x / l, c.y / l, 0.0)) < 1e-6, "a rapid off the radial at {:?}", m.to);
    }
}

fn unit_angle(a: Point3, b: Point3) -> f64 {
    let d = (a.x * b.x + a.y * b.y + a.z * b.z)
        / ((a.x * a.x + a.y * a.y + a.z * a.z).sqrt() * (b.x * b.x + b.y * b.y + b.z * b.z).sqrt());
    d.clamp(-1.0, 1.0).acos()
}

// ---- the ladder law ------------------------------------------------

#[test]
fn a_standing_bar_peels_from_the_skin_inward() {
    let got = rotary_offset(&lathe_bar(16), &params(28.0, 0.0), &control(0.0, 0.0, None))
        .expect("a turned bar takes the wrap ladder");
    let mut cuts = 0usize;
    let mut deepest = f64::INFINITY;
    for m in &got.moves {
        match m.kind {
            MoveKind::Rapid => rapid_radial(m),
            _ => {
                cuts += 1;
                let t = tangent(m.to);
                // Never above the skin's own level (the skim is the
                // top), and the tool stands on a named level plane.
                assert!(t <= 2.5 + 1e-6, "cut above the wrap top: {t} at {:?}", m.to);
                assert!(t >= -1e-6, "the tool crossed the axis: {t} at {:?}", m.to);
                let a = m.axis.expect("a radial cut move poses");
                let av = p3(a[0], a[1], a[2]);
                assert!(av.z.abs() < 1e-9, "the pose leaned off the radial plane: {av:?}");
                // The radial of the ball CENTER (r out along the pose):
                // a cap's tip may stand on the axis itself (M13).
                let c = p3(m.to.x + R * a[0], m.to.y + R * a[1], m.to.z + R * a[2]);
                let l = (c.x * c.x + c.y * c.y).sqrt();
                assert!(
                    unit_angle(av, p3(c.x / l, c.y / l, 0.0)) < 1e-6, // acos near 0 amplifies the center's rebuild dust
                    "the pose is not the radial at {:?}",
                    m.to
                );
                deepest = deepest.min(t);
            }
        }
    }
    assert!(cuts > 50, "a whole bar roughs in plenty of moves, got {cuts}");
    // The barrel (r1.2, v 8.5..18) and cone columns peel one stepdown
    // under the skin: the deepest legal floor is 1.5.
    assert!((deepest - 1.5).abs() < 1e-6, "deepest cut tangent {deepest}");
}

#[test]
fn the_groove_floor_is_turned_to_its_radius_and_no_deeper() {
    let got = rotary_offset(&grooved(16), &params(24.0, 0.0), &control(0.0, 0.0, None))
        .expect("a grooved sleeve roughs to its floor");
    let mut deepest = f64::INFINITY;
    let mut floor_cut = 0usize;
    for m in &got.moves {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        let t = tangent(m.to);
        assert!(t <= 3.0 + 1e-6, "cut above the OD skim: {t}");
        if (t - 2.0).abs() < 1e-6 {
            floor_cut += 1;
        }
        deepest = deepest.min(t);
    }
    // 2.0 is an exact stepdown count under 3.0: the groove floor is
    // reached dead-on, and nothing cuts under it.
    assert!((deepest - 2.0).abs() < 1e-6, "deepest tangent {deepest}");
    assert!(floor_cut > 8, "a groove lap, not a nick: {floor_cut} moves");
}

#[test]
fn the_rapids_stand_clear_of_the_whirl() {
    let got = rotary_offset(&lathe_bar(16), &params(28.0, 0.0), &control(0.0, 0.0, None)).unwrap();
    // Wrap clearance = rho_max + stepdown + r = 4.5 of plan height,
    // so rapid TIPS sit at radius 4.5 (M13 pivot law: the plan's
    // height is the tip's, the ball center one radius out) and cut
    // tips at the 2.5 skim and below.
    let max_cut = got
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .map(|m| (m.to.x * m.to.x + m.to.y * m.to.y).sqrt())
        .fold(0.0f64, f64::max);
    let min_rapid = got
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid)
        .map(|m| (m.to.x * m.to.x + m.to.y * m.to.y).sqrt())
        .fold(f64::INFINITY, f64::min);
    assert!(max_cut <= 2.5 + 1e-6, "cut tip above the skim: {max_cut}");
    assert!(
        min_rapid > max_cut,
        "the rapids clear the cut: {min_rapid} vs {max_cut}"
    );
    assert!(got.all_finite());
}

#[test]
fn the_seam_closes_into_a_full_ring() {
    let got = rotary_offset(&lathe_bar(16), &params(28.0, 0.0), &control(0.0, 0.0, None)).unwrap();
    // #2004: the wrap top (rho 2.5) is the material top and takes no
    // level now; the first ring is one stepdown under it (rho 1.5), round
    // the barrel. Its positions must cover every
    // azimuth. An open seam would leave a rest ridge there and show
    // as an angular gap far wider than the 16-turn chord step.
    let mut ths: Vec<f64> = got
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && (tangent(m.to) - 1.5).abs() < 1e-6)
        .map(|m| m.to.y.atan2(m.to.x))
        .collect();
    ths.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(ths.len() > 16, "a skim ring, not a few nicks: {}", ths.len());
    let mut worst = 0.0f64;
    for w in ths.windows(2) {
        worst = worst.max(w[1] - w[0]);
    }
    worst = worst.max(TAU + ths[0] - ths[ths.len() - 1]);
    assert!(worst < 0.8, "an angular hole in the skim ring: {worst}");
}

// ---- the stance law --------------------------------------------------

#[test]
fn a_laid_bar_takes_the_same_ladder_on_its_side() {
    let stood = rotary_offset(&lathe_bar(16), &params(28.0, 0.0), &control(0.0, 0.0, None)).unwrap();
    let lain = rotary_offset(
        &laid_bar(16),
        &params(28.0, 0.0),
        &control(0.0, 0.0, None),
    )
    .expect("the developed cylinder is neutral to which way the axis points");
    let deep = |p: &camcraft_motion::cl::Toolpath, f: fn(Point3) -> f64| {
        p.moves
            .iter()
            .filter(|m| m.kind != MoveKind::Rapid)
            .map(|m| f(m.to))
            .fold(f64::INFINITY, f64::min)
    };
    let (ds, dl) = (deep(&stood, tangent), deep(&lain, tangent_xz));
    assert!(
        (ds - dl).abs() < 1e-6,
        "the stance changed the ladder: {ds} vs {dl}"
    );
    let cuts = lain
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .count();
    assert!(cuts > 50, "the lying bar roughs as well: {cuts} cuts");
    for m in lain.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        let a = m.axis.expect("laid cuts pose too");
        assert!(a[0].abs() < 1e-9, "the laid pose leaned along the world z: {a:?}");
    }
}

#[test]
fn the_height_window_names_the_stations_and_refuses_an_empty_one() {
    // Window above the bar: no station survives, and it says so.
    let e = rotary_offset(&lathe_bar(16), &params(48.0, 38.0), &control(0.0, 0.0, None))
        .expect_err("heights that cover nothing refuse");
    assert!(e.to_string().contains("no station"), "{e}");
    // #2004: the window v 0..8 holds only the foot (rho 2.0) under the
    // wrap top (rho 2.5). Its one open level was the air plane AT the
    // wrap top; with that level gone the ladder refuses by name.
    let air = rotary_offset(&lathe_bar(16), &params(8.0, 0.0), &control(0.0, 0.0, None))
        .expect_err("a window whose only open level is the wrap top is air");
    assert!(air.to_string().contains("nothing to cut"), "{air}");
    // A lower window with material: the rect's v range ends AT station
    // 12 (no pad in v - the stations are the contract), over the thin
    // barrel (rho 1.2), so no cut may run past it.
    let got = rotary_offset(&lathe_bar(16), &params(12.0, 0.0), &control(0.0, 0.0, None))
        .expect("the lower bar roughs alone");
    let cuts = got
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .count();
    assert!(cuts > 20, "half a bar is still a bar: {cuts} cuts");
    for m in got.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        assert!(
            m.to.z <= 12.0 + 1e-6,
            "a cut ran past the window station: {:?}",
            m.to
        );
    }
    // And the upper half, for the mirror. #2004: v 19..28 holds only the
    // cone top and the r2.5 barrel; its one open level was the air plane
    // on the wrap top, so it refuses by name. The window v 10..28 holds
    // the thin barrel (rho 1.2) and must stay at or above station 10.
    let air = rotary_offset(&lathe_bar(16), &params(28.0, 19.0), &control(0.0, 0.0, None))
        .expect_err("the upper window's only open level is the wrap top");
    assert!(air.to_string().contains("nothing to cut"), "{air}");
    let up = rotary_offset(&lathe_bar(16), &params(28.0, 10.0), &control(0.0, 0.0, None)).unwrap();
    let up_cuts = up.moves.iter().filter(|m| m.kind != MoveKind::Rapid).count();
    assert!(up_cuts > 20, "the upper window still roughs the barrel: {up_cuts} cuts");
    for m in up.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        assert!(
            m.to.z >= 10.0 - 1e-6,
            "a cut ran past the upper window: {:?}",
            m.to
        );
    }
}

// ---- the refusals ----------------------------------------------------

#[test]
fn a_sphere_and_a_flat_say_no_to_the_wrap() {
    let (r, nlat, nlon) = (5.0, 12, 24);
    let pt = |i: usize, j: usize| {
        let (ph, th) = (
            std::f64::consts::PI * (i as f64 / nlat as f64 - 0.5),
            TAU * j as f64 / nlon as f64,
        );
        p3(r * ph.cos() * th.cos(), r * ph.cos() * th.sin(), r * ph.sin())
    };
    let mut s = Vec::new();
    for i in 0..nlat {
        for j in 0..nlon {
            s.push(Triangle::new(pt(i, j), pt(i + 1, j), pt(i + 1, j + 1)));
            s.push(Triangle::new(pt(i, j), pt(i + 1, j + 1), pt(i, j + 1)));
        }
    }
    let e = rotary_offset(&s, &params(5.0, -5.0), &control(0.0, 0.0, None))
        .expect_err("a dome has no ONE axis to develop about");
    assert!(e.to_string().contains("equally"), "{e}");
    // Plain ground: the fit's own refusal rides through unchanged.
    let plate = revolve(16, &[(0.0, 5.0), (4.0, 5.0)]);
    assert!(
        rotary_offset(&plate, &params(6.0, 4.0), &control(0.0, 0.0, None)).is_err(),
        "a facing alone names no axis"
    );
    // No facets at all, and a degenerate mesh: named, not crashed.
    assert!(rotary_offset(&[], &params(1.0, 0.0), &control(0.0, 0.0, None)).is_err());
    let sliver = vec![Triangle::new(p3(0.0, 0.0, 0.0), p3(0.0, 0.0, 1.0), p3(0.0, 0.0, 2.0))];
    assert!(
        rotary_offset(&sliver, &params(2.0, 0.0), &control(0.0, 0.0, None)).is_err(),
        "a mesh of no radius cannot be developed"
    );
}

#[test]
fn a_lean_poses_off_the_radial_by_the_stated_angle() {
    let got = rotary_offset(&lathe_bar(16), &params(28.0, 0.0), &control(15.0, 0.0, None))
        .expect("a leaning radial rough");
    let mut body = 0usize;
    for m in got.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        let a = m.axis.expect("cut moves pose");
        // The lean is measured off the radial of the ball center the
        // tip stands under (M13 pivot law).
        let c = p3(m.to.x + R * a[0], m.to.y + R * a[1], m.to.z + R * a[2]);
        let l = (c.x * c.x + c.y * c.y).sqrt();
        let av = p3(a[0], a[1], a[2]);
        let n = p3(c.x / l, c.y / l, 0.0);
        assert!(
            (unit_angle(av, n) - 15f64.to_radians()).abs() < 1e-6,
            "the lead law broke at {:?}",
            m.to
        );
        body += 1;
    }
    assert!(body > 50, "a whole bar of leaned moves, got {body}");
}

#[test]
fn a_ramp_entry_runs_the_ladder_too() {
    let p = Pocket3DParams {
        entry: Entry3D::Ramp,
        ..params(28.0, 0.0)
    };
    let got = rotary_offset(&lathe_bar(16), &p, &control(0.0, 0.0, None))
        .expect("the pocket's own entry laws ride the wrap");
    // The ramp's law on the revolved solid, post-M8.1: every level
    // crossing either descends diagonally - a Feed whose radius
    // changes, the slope mapped back into the world - or substitutes
    // the axial dive and SAYS so. On a bar the band of open floor
    // narrows spiral-ward with depth, so the destination's chord
    // almost never clears and the honest verdict is the substitution;
    // what must never happen is silence. (M8.1 gap this test pins:
    // wrap_exit rebuilt the path and dropped the notes channel, so
    // the verdict the plan spoke vanished across the map.)
    let rad = |q: Point3| (q.x * q.x + q.y * q.y).sqrt();
    let descends = got
        .moves
        .iter()
        .any(|m| m.kind == MoveKind::Feed && (rad(m.to) - rad(m.from)).abs() > 1e-6);
    let declared = got
        .notes
        .iter()
        .any(|n| n.contains("ramp's chord is blocked"));
    assert!(
        descends || declared,
        "no ramp descent and no substitution verdict: the entry law went silent on the wrap"
    );
    assert!(declared, "on this narrowing bar the substitution must be readable");
    let axial_dive = got
        .moves
        .iter()
        .any(|m| m.kind == MoveKind::Plunge && (rad(m.to) - rad(m.from)).abs() > 1e-6);
    assert!(axial_dive, "the substituted dive stands on the floor's own room, radially");
}

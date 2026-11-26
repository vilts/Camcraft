//! The swarf pose-continuity law (the "swarf stamp flips hemispheres
//! mid-program" finding, RECOMMENDED.md): a ruling direction is a
//! LINE, so both signs of it are "the ruling" - and the stamp must
//! pick the sign that continues the pose the machine holds coming in,
//! never a fixed hemisphere. On the capped blade the fixed rule posed
//! the body +X whatever the lean law had landed on beside it, so the
//! program held four exact 180-degree reversals between consecutive
//! 4-6 mm feeds on the body and a 179-degree swing at the ring/raster
//! hand-off - a joint-interpolating controller swings the head
//! end-over-end at feed with the ball 2.9 mm off the blade mid-swing.
//! What is measured HERE is the program's own turn ladder on that
//! blade: two consecutive blade-lying cuts never change end (they turn
//! within 10 degrees), no cut of either swarf kind turns past the link
//! threshold off the pose held coming in (every wider hand-off - the
//! raster reversals, the ring's passages round the flat caps - rides
//! an air link: lift, swing on the plunge, re-enter on the same ball
//! centre), and the guard's named refusal (`stamp_link::guard_turns`,
//! unit-tested beside the stamp) is what keeps it so.
//!
//! Provenance: continuity of the tool-axis vector along a 5-axis
//! program is standard practice (a tilt-axis reversal is a linking
//! fault, not a cut); the fixture is the app's blade at debug size.
//! Written from our own design notes only.
//!
//! Gate map (M4.4): covers camcraft_motion::multiaxis (stamp, swarf).
//! Lane shape (M4.4): seconds.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::finishing3d::{Finish3DParams, FinishPattern};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::multiaxis::{swarf5d, swarf5d_advanced, AxisControl};

const TAU: f64 = std::f64::consts::TAU;
const CLEARANCE: f64 = 25.0;
/// The link threshold (`stamp_link::LINK_TURN_DEG`), mirrored: a cut
/// the generator hands back turns no further than this off the pose
/// held coming in; wider turns ride the air links.
const GUARD_DEG: f64 = 30.0;
/// Two blade-lying poses in a row hold one end of the ruling.
const LYING_TURN_DEG: f64 = 10.0;
/// Level, to float dust: no stated pose points under it (#17; the
/// law's band was sin 10 degrees, -0.1737, before the rim dip's cure).
const LEVEL: f64 = -1e-9;

/// The app's blade at debug size: a circular tube about X, radius
/// 12, 16 rings x 6 stations, capped flat at both ends.
fn blade() -> Vec<Triangle> {
    let (radius, x_len, n_circ, n_x) = (12.0, 24.0, 16usize, 6usize);
    let v = |j: usize, i: usize| {
        let th = TAU * j as f64 / n_circ as f64;
        Point3::new(
            -x_len + 2.0 * x_len * i as f64 / n_x as f64,
            radius * th.cos(),
            radius * th.sin(),
        )
    };
    let mut facets = Vec::new();
    for j in 0..n_circ {
        for i in 0..n_x {
            let (a, b, c, d) = (v(j, i), v(j + 1, i), v(j + 1, i + 1), v(j, i + 1));
            facets.push(Triangle::new(a, b, c));
            facets.push(Triangle::new(a, c, d));
        }
    }
    let cap = |x: f64| Point3::new(x, 0.0, 0.0);
    for j in 0..n_circ {
        facets.push(Triangle::new(cap(x_len), v(j, n_x), v(j + 1, n_x)));
        facets.push(Triangle::new(cap(-x_len), v(j + 1, 0), v(j, 0)));
    }
    facets
}

/// The app test's swarf parameters, the debug band (11 down to 1).
fn params(bottom_z: f64) -> Finish3DParams {
    Finish3DParams {
        gap_link: camcraft_motion::finishing3d::GapLink::Jump,
        pattern: FinishPattern::Parallel,
        stepover: Some(2.0),
        tool_diameter: 6.0,
        tolerance: 0.1,
        clearance_z: CLEARANCE,
        retract_z: None,
        top_z: 11.0,
        bottom_z,
        feed: 500.0,
        plunge_feed: 100.0,
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

fn control() -> AxisControl {
    AxisControl {
        lead_deg: 15.0,
        lag_deg: 0.0,
        max_tilt_deg: Some(60.0),
    }
}

fn deg_between(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    d.clamp(-1.0, 1.0).acos().to_degrees()
}

fn lying(a: [f64; 3]) -> bool {
    a[0].abs() > 0.995
}

/// The turn ladder: the angle between the pose of each cutting move
/// and the pose of the cutting move before it (a move stating no
/// pose is upright; rapids are links). Returns (max turn, its move,
/// count over 90, count over the guard, worst lying-to-lying turn).
fn turn_ladder(path: &Toolpath) -> (f64, usize, usize, usize, f64) {
    let mut held: Option<[f64; 3]> = None;
    let (mut max, mut at, mut over90, mut flips, mut lying_max) = (0.0f64, 0usize, 0usize, 0usize, 0.0f64);
    for (i, m) in path.moves.iter().enumerate() {
        let a = m.axis.unwrap_or([0.0, 0.0, 1.0]);
        let p = held.unwrap_or([0.0, 0.0, 1.0]);
        held = m.axis.or(held);
        if m.kind == MoveKind::Rapid || m.from.z >= CLEARANCE - 1e-9 {
            continue; // links and entries swing in air
        }
        let t = deg_between(p, a);
        if t > max {
            max = t;
            at = i;
        }
        over90 += usize::from(t > 90.0);
        flips += usize::from(t > GUARD_DEG);
        if lying(p) && lying(a) {
            lying_max = lying_max.max(t);
        }
    }
    (max, at, over90, flips, lying_max)
}

/// Air links: the rapid swings - a rapid whose stated pose turns past
/// the guard off the pose held coming in (the link law retracts along
/// the contact normal, turns the head there in air on a rapid, and
/// re-enters along the normal; the plunge that follows holds the pose
/// the swing already stated).
fn air_links(path: &Toolpath) -> usize {
    let mut held: Option<[f64; 3]> = None;
    let mut n = 0usize;
    for m in &path.moves {
        let p = held.unwrap_or([0.0, 0.0, 1.0]);
        // #3414: a zero-travel rapid is the standstill rotary index,
        // not a link - it swings in place to the pose the next descend
        // cuts under, and travels no air.
        if m.kind == MoveKind::Rapid
            && m.from != m.to
            && let Some(a) = m.axis
            && deg_between(p, a) > GUARD_DEG
        {
            n += 1;
        }
        held = m.axis.or(held);
    }
    n
}

/// The air-link ratchet (#17): the most links each blade run may write.
/// Before #17 the debug band linked 68 / 106 times (swarf / ladder) and
/// the full band 93 / 145: half the tube read unruled (the curvature fit
/// reached across the cap's crease), every raster end turned the lying
/// tool upright and back, and the rim column's dip kept the hold from
/// qualifying. #17 took them to 34 / 51 and 55 / 84 (main had crept to
/// 34 / 53 and 55 / 86 by #48). #48: each raster line's exit rise is a
/// rapid holding the lying pose where its flank is proved in air, so a
/// raster end no longer turns upright and back - 23 / 34 and 44 / 67 on
/// macOS. #446: the two-pass ladder may need two more air links at coarse
/// raster ends on Linux; both are guarded turns in air, not cutting turns,
/// so the advanced pins carry that allowance (+2). The full flank and turn
/// checks below still apply to every move. Any other rise fails.
const LINKS_DEBUG: (usize, usize) = (23, 36);
const LINKS_FULL: (usize, usize) = (44, 69);

fn assert_continuous(name: &str, path: &Toolpath, max_links: usize) {
    let (max, at, over90, flips, lying_max) = turn_ladder(path);
    let links = air_links(path);
    eprintln!(
        "{name}: {} moves, {links} air links, max cut turn {max:.2} deg at move {at}, {over90} over 90 deg, \
         {flips} past the guard, lying-to-lying worst {lying_max:.2} deg",
        path.moves.len(),
    );
    assert!(links > 0, "{name}: the blade's hand-offs ride air links");
    assert!(links <= max_links, "{name}: {links} air links, the ratchet holds {max_links}");
    assert_eq!(flips, 0, "{name}: a program the stamp handed back turns past its guard");
    assert!(max <= GUARD_DEG, "{name}: the worst turn is {max:.1} deg at move {at}");
    assert!(
        lying_max <= LYING_TURN_DEG,
        "{name}: two blade-lying cuts in a row changed end ({lying_max:.1} deg)"
    );
}

#[test]
fn swarf_poses_never_flip_between_consecutive_cuts() {
    let facets = blade();
    let path = swarf5d(&facets, &params(1.0), &control()).expect("swarf on the blade");
    assert_continuous("swarf", &path, LINKS_DEBUG.0);
}

#[test]
fn advanced_swarf_poses_never_flip_between_consecutive_cuts() {
    let facets = blade();
    let path = swarf5d_advanced(&facets, &params(1.0), &control(), 2).expect("ladder on the blade");
    assert_continuous("advanced swarf", &path, LINKS_DEBUG.1);
}

/// #17: the ruling reads the WHOLE tube ruled, end columns included -
/// the curvature fit no longer reaches across the cap's crease (the
/// near half of the tube read curved along its ruling before, and lost
/// its swarf pose mid-line) - and its verdict has one slot per facet
/// (a facet the fit could not answer used to push none, shifting every
/// later verdict and running the direction scan off the end).
#[test]
fn the_blade_reads_ruled_end_to_end_with_one_verdict_per_facet() {
    let facets = blade();
    let f = camcraft_motion::multiaxis::ruling::analyze(&facets).expect("the blade has a ruling");
    assert_eq!(f.ruled.len(), facets.len(), "one verdict per facet");
    let tube = 16 * 6 * 2;
    let unruled = f.ruled[..tube].iter().filter(|r| !**r).count();
    assert_eq!(unruled, 0, "{unruled} of the tube's {tube} facets read unruled");
    assert!(f.ux.abs() > 0.999, "the ruling runs along X: ({}, {})", f.ux, f.uy);
}

/// The exact distance from a point to the blade's solid (the 16-gon
/// prism the fixture's facets bound: its cross-section is the polygon
/// on the circle's vertices, flat between them), negative inside.
fn blade_distance(q: Point3) -> f64 {
    let (radius, x_len, n) = (12.0, 24.0, 16usize);
    let v = |j: usize| {
        let th = TAU * j as f64 / n as f64;
        (radius * th.cos(), radius * th.sin())
    };
    let (mut inside, mut edge) = (true, f64::INFINITY);
    for j in 0..n {
        let ((ay, az), (by, bz)) = (v(j), v(j + 1));
        let (ey, ez) = (by - ay, bz - az);
        inside &= ey * (q.z - az) - ez * (q.y - ay) >= 0.0;
        let t = (((q.y - ay) * ey + (q.z - az) * ez) / (ey * ey + ez * ez)).clamp(0.0, 1.0);
        edge = edge.min(f64::hypot(q.y - ay - t * ey, q.z - az - t * ez));
    }
    let dp = if inside { -edge } else { edge };
    let dx = q.x.abs() - x_len;
    match (dp > 0.0, dx > 0.0) {
        (true, true) => f64::hypot(dp, dx),
        (true, false) => dp,
        (false, true) => dx,
        (false, false) => dp.max(dx),
    }
}

/// The "upright in the wall" law, on the whole band: no cutting move
/// under the tube's equator stands upright (the stamp used to read
/// the rings' chords at the cap rims as air and state upright, and an
/// upright flute at the end column under the equator stands 0.8 mm
/// inside the tube), no stated pose points under the horizon, and
/// every cutting move's flank - the flute's column from the ball
/// centre along its pose, out to the blade's shell - clears the solid
/// by the ball's own radius less the tolerance. The continuity laws
/// hold on the full band as on the debug one.
#[test]
fn no_swarf_pose_under_the_equator_is_upright_and_every_flank_clears_the_blade() {
    let facets = blade();
    let (r, tol) = (3.0, 0.1);
    for (name, path, links) in [
        ("swarf", swarf5d(&facets, &params(-11.0), &control()).expect("swarf, full band"), LINKS_FULL.0),
        (
            "advanced swarf",
            swarf5d_advanced(&facets, &params(-11.0), &control(), 2).expect("ladder, full band"),
            LINKS_FULL.1,
        ),
    ] {
        assert_continuous(name, &path, links);
        let (mut under, mut checked) = (0usize, 0usize);
        for (i, m) in path.moves.iter().enumerate() {
            let a = m.axis.unwrap_or([0.0, 0.0, 1.0]);
            assert!(a[2] >= LEVEL, "{name}: a pose points under level at move {i}: {a:?}");
            if m.kind == MoveKind::Rapid || m.from.z >= CLEARANCE - 1e-9 {
                continue;
            }
            let c = Point3::new(m.to.x + a[0] * r, m.to.y + a[1] * r, m.to.z + a[2] * r);
            if c.z < -0.5 && blade_distance(c) < r + tol {
                under += 1;
                assert!(
                    m.axis.is_some_and(|a| a[2] < 0.999_999),
                    "{name}: an upright cut under the equator at move {i} (ball centre {c:?})"
                );
            }
            // The flank: half a radius out, to twenty radii (past the
            // blade's longest diagonal), every half radius.
            for k in 1..=40 {
                let s = r * 0.5 * k as f64;
                let q = Point3::new(c.x + a[0] * s, c.y + a[1] * s, c.z + a[2] * s);
                let d = blade_distance(q);
                assert!(
                    d >= r - tol - 1e-6,
                    "{name}: the flank of move {i} stands in the blade: {d:.3} at {s:.1} along {a:?} from {c:?}"
                );
            }
            checked += 1;
        }
        eprintln!("{name}: {checked} cuts' flanks proved, {under} under the equator");
        assert!(under >= 100, "{name}: only {under} cuts under the equator on the full band");
    }
}

/// The sign rule is continuity, not a hemisphere: the blade-lying
/// poses are a real population on BOTH ends of the ruling now (the
/// fixed rule allowed one), and no pose dips under level.
#[test]
fn ruled_poses_ride_both_ends_of_the_ruling_and_never_down() {
    let facets = blade();
    let path = swarf5d(&facets, &params(1.0), &control()).expect("swarf on the blade");
    let (mut plus, mut minus) = (0usize, 0usize);
    for m in &path.moves {
        let Some(a) = m.axis else { continue };
        // #17: a ruling read at a rim column (the contact on the
        // facet's edge) used to project into the contact's plane and
        // dip under level by up to the stamp's 10-degree band (7.3 deg
        // here, 9.4 on the ladder); the facet's own plane holds the
        // ruling level, so no pose dips at all now.
        assert!(a[2] >= LEVEL, "a pose points under level at {:?}: {a:?}", m.to);
        if m.kind != MoveKind::Rapid && lying(a) {
            if a[0] > 0.0 {
                plus += 1;
            } else {
                minus += 1;
            }
        }
    }
    assert!(plus >= 8 && minus >= 8, "the ruling's ends: {plus} +X, {minus} -X");
}

/// #48: a raster line leaves the blade on a RAPID up Z under the pose
/// it cut with - never an upright feed (the turn that linked every
/// raster end) and never a posed feed in air (the verifier's
/// posed-contact law reads that as a cut standing off the part). On
/// the debug blade every line ends that way: a rapid straight up Z
/// out of a lying cut, holding the lying pose, with no feed rising
/// straight up Z anywhere after the program first leans.
#[test]
fn every_raster_exit_rise_is_a_rapid_holding_the_lying_pose() {
    let facets = blade();
    let path = swarf5d(&facets, &params(1.0), &control()).expect("swarf on the blade");
    let mut held: Option<[f64; 3]> = None;
    let mut rises = 0usize;
    for (i, m) in path.moves.iter().enumerate() {
        let vertical_up = (m.to.x - m.from.x).hypot(m.to.y - m.from.y) <= 1e-9 && m.to.z > m.from.z + 1e-9;
        let leaning = held.is_some_and(|a| a[2] < 0.999_999);
        if vertical_up && leaning {
            assert_eq!(m.kind, MoveKind::Rapid, "move {i}: an exit rise off a lean is a {:?}", m.kind);
        }
        if vertical_up && m.kind == MoveKind::Rapid && held.is_some_and(lying) {
            assert_eq!(m.axis, held, "move {i}: the rise turned the pose");
            rises += 1;
        }
        held = m.axis.or(held);
    }
    assert!(rises >= 10, "only {rises} raster exits rose under the lying pose");
}

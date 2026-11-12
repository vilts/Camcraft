//! Rotary field parity on the app's own fixtures (M13.1, the rotary
//! lift): each rotary kind's cached program - Contour and Offset on
//! the standing d12.4 bar, Parallel on the laid one, the ball a
//! BN-0600 (`common::rotary_bar`) - chorded through
//! `Stock::simulate_rotary` about the axis `fixture::rotary_axis_of`
//! fits, against the radial law written independently here: the
//! union of the balls and flutes the controller's lerp places (tip on
//! its chord, the letters each on their own line), sampled densely.
//! Column tops are read from the law the way the field reads them -
//! MARCHED DOWN FROM THE UNCUT SKIN to the first steel (a standing
//! bar's rings bore tunnels under its cap; the sweep's lowest reach
//! over a column is a tunnel floor, not the column's top - measured
//! 2026-11-06 on the Offset ladder at (-1.75, -5.25): reach 3.31
//! under 2 mm of standing steel and the cap). Sphere-cut floors must
//! agree within the pre-pass's centre sag plus `FIELD_VERDICT_EPS`
//! plus the law's own sampling sag; a column the sweep never reaches
//! from above must stand at its uncut top; the steel/air sign at every
//! probed cell centre must agree off the swept skin by more than the
//! flute's chord-class displacement band; both consumers must route
//! (the eye cuts the lifted kinds and refuses the rest by name, the
//! column verdict refuses every kind by name). Provenance: our own
//! fixtures and engines; nothing from vendor binaries.
//!
//! Gate map (M4.4): covers app::stock_sim, sim::stock_sim (plan_program's rotary
//! clause, stock_sim::rotary), app::stock_view, app::fixture, sim::fixture.

mod common;

use camcraft_app::cl::{angles_to_axis, axis_to_angles, Move, MoveKind};
use camcraft_app::geometry::Point3;
use camcraft_app::stock_sim::{
    rotary_kind_lift, rotary_pose_tol, simulate_document, simulate_rotary_slice, SimOp, FIELD_VERDICT_EPS,
    ROTARY_VERDICT_REFUSAL,
};
use camcraft_app::stock_view::field_view;
use common::rotary_bar::{field_stock, params, program, run, world_for, Kind, BAR_R, CELL, HALF_L, R};

/// The verdict-class chord sag at the tool's farthest point.
const POSE_TOL: f64 = 1e-4;
/// The law's sample spacing along the centre curve, mm: its union of
/// balls scallops by `H²/(8R)` - 4.2e-6, under the verdict class.
const H: f64 = 0.01;
/// The flute above the ball the law sweeps: past the bar's skin the
/// engines agree on air, so the stock's own reach is enough.
const FLUTE: f64 = 2.0 * BAR_R;
/// The march's convergence: the exit bracketed to within this.
const MARCH_EPS: f64 = 1e-7;
/// The march's least step inside the sweep: sphere tracing alone
/// stalls beside a grazing side wall (the distance to ANY boundary
/// vanishes while the ray runs on inside), so a step never shrinks
/// under this - a steel sliver thinner than it is no column's top
/// anyway (`Floor::Sliver`).
const MARCH_MIN: f64 = 1e-3;
/// Steps a march may take before it is called unreadable.
const MARCH_STEPS: usize = 4096;

/// The radial law's samples: (ball centre, pose) along every cutting
/// move at spacing `H`, the pose the letters hold at each.
fn centers(moves: &[Move]) -> Vec<(Point3, [f64; 3])> {
    let mut out = Vec::new();
    let mut held = moves.first().and_then(|m| m.axis).unwrap_or([0.0, 0.0, 1.0]);
    for m in moves {
        let to = m.axis.unwrap_or(held);
        let (la, lb) = (axis_to_angles(held), axis_to_angles(to));
        if m.kind != MoveKind::Rapid {
            let len = ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2) + (m.to.z - m.from.z).powi(2)).sqrt();
            let dth = ((la[0] - lb[0]).abs() + (la[1] - lb[1]).abs()).to_radians();
            let n = ((len.max(dth * (BAR_R + 2.0 * R)) / H).ceil() as usize).clamp(1, 40_000);
            for k in 0..=n {
                let s = k as f64 / n as f64;
                let l = [la[0] + (lb[0] - la[0]) * s, la[1] + (lb[1] - la[1]) * s, la[2] + (lb[2] - la[2]) * s];
                let a = angles_to_axis(l).unwrap_or([0.0, 0.0, 1.0]);
                let t = Point3::new(m.from.x + (m.to.x - m.from.x) * s, m.from.y + (m.to.y - m.from.y) * s, m.from.z + (m.to.z - m.from.z) * s);
                out.push((Point3::new(t.x + R * a[0], t.y + R * a[1], t.z + R * a[2]), a));
            }
        }
        held = to;
    }
    out
}

/// Signed distance to the swept tool (ball and flute at every sample),
/// negative where cut, and whether a sphere (not a flute) is nearest.
fn law_cut(cs: &[(Point3, [f64; 3])], p: Point3) -> (f64, bool) {
    let (mut best, mut sphere) = (f64::INFINITY, false);
    for (c, a) in cs {
        let w = [p.x - c.x, p.y - c.y, p.z - c.z];
        let ww = w[0] * w[0] + w[1] * w[1] + w[2] * w[2];
        if ww > (FLUTE + R + 1.0).powi(2) {
            continue;
        }
        let ds = ww.sqrt() - R;
        if ds < best {
            (best, sphere) = (ds, true);
        }
        let t = w[0] * a[0] + w[1] * a[1] + w[2] * a[2];
        let rho = (ww - t * t).max(0.0).sqrt();
        let (dr, dt) = (rho - R, (-t).max(t - FLUTE));
        let d = if dr > 0.0 || dt > 0.0 { dr.max(0.0).hypot(dt.max(0.0)) } else { dr.max(dt) };
        if d < best {
            (best, sphere) = (d, false);
        }
    }
    (best, sphere)
}

/// What the law reads over a column, marched down from its uncut skin.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Floor {
    /// The sweep never reaches the skin: the column stands uncut from
    /// above, with this clearance to the nearest sweep.
    Standing(f64),
    /// The top of the steel the sweep leaves, and whether a sphere set it.
    Cut(f64, bool),
    /// The steel under the exit is thinner than a cell: the field's
    /// own `top` climbs to a material cell CENTRE first, so a sliver
    /// holding none is no column's top on either side - not asserted.
    Sliver,
    /// The march ran out of steps: a grazing flank, no verdict.
    Unreadable,
}

/// The law's column top, read the way the field reads its own: down
/// from the skin through the swept union - sphere-traced (inside a
/// union, the deepest member's depth never exceeds the distance to the
/// union's skin, so a step of that depth stays inside), never under
/// `MARCH_MIN` (a grazing side wall stalls the trace while the ray runs
/// on inside: measured 2026-11-06 on Contour at (-3.25, -3.75), the
/// trace halting 0.015 under the cap where the exit stands 0.47 under
/// it) - to the first probe outside, then the exit bisected.
fn law_floor(cs: &[(Point3, [f64; 3])], x: f64, y: f64, skin: f64) -> Floor {
    let at = |z: f64| law_cut(cs, Point3::new(x, y, z));
    let (d0, _) = at(skin);
    if d0 >= 0.0 {
        return Floor::Standing(d0);
    }
    let (mut z_in, mut d_in) = (skin, d0);
    let mut exit = None;
    for _ in 0..MARCH_STEPS {
        let z = z_in + d_in.min(-MARCH_MIN);
        let (d, _) = at(z);
        if d >= 0.0 {
            exit = Some(z);
            break;
        }
        (z_in, d_in) = (z, d);
    }
    let Some(mut z_out) = exit else { return Floor::Unreadable };
    while z_in - z_out > MARCH_EPS {
        let zm = 0.5 * (z_in + z_out);
        if at(zm).0 >= 0.0 {
            z_out = zm;
        } else {
            z_in = zm;
        }
    }
    if at(z_out - 0.5 * CELL).0 <= 0.0 || at(z_out - CELL).0 <= 0.0 {
        return Floor::Sliver;
    }
    Floor::Cut(z_out, at(z_out).1)
}

/// The uncut stock's top over (x, y), `None` outside its footprint.
fn uncut_top(lying: bool, x: f64, y: f64) -> Option<f64> {
    if lying {
        (x.abs() < HALF_L - 1.0 && y.abs() < BAR_R - 2.0 * CELL).then(|| (BAR_R * BAR_R - y * y).sqrt())
    } else {
        // The flat end cap is exact over every column inside the
        // stock polygon; the ring's sphere leaves its floor under the
        // cap only in the annulus just inside the skin.
        (x.hypot(y) < BAR_R - 0.3).then_some(HALF_L)
    }
}

/// Whether a probe sits inside the uncut bar, off its tessellated skin.
fn in_bar(lying: bool, p: Point3) -> bool {
    let (ax, rho) = if lying { (p.x, p.y.hypot(p.z)) } else { (p.z, p.x.hypot(p.y)) };
    ax.abs() < HALF_L - CELL && rho < BAR_R - CELL && rho > BAR_R - 2.5
}

fn parity(kind: Kind) {
    let lying = kind.lying();
    let world = world_for(kind);
    let (moves, _) = program(&world);
    let (f, stats, wall) = run(&world, CELL, POSE_TOL);
    let uncut = field_stock(&world, CELL);
    let law_sag = H * H / (8.0 * R);
    let bound = stats.center_sag + FIELD_VERDICT_EPS + law_sag;
    let band = (R + BAR_R) * stats.half_angle + law_sag + FIELD_VERDICT_EPS;
    let per = wall.as_secs_f64() * 1e3 / stats.sub_chords.max(1) as f64;
    println!(
        "{kind:?}: {} moves, {} turning -> {} sub-chords, half-angle {:.3e}, centre sag {:.3e}, wall {:.2?} ({per:.3} ms/sub-chord)",
        moves.len(), stats.turning_moves, stats.sub_chords, stats.half_angle, stats.center_sag, wall
    );
    assert!(stats.center_sag <= POSE_TOL);
    let cs = centers(&moves);
    let n = (BAR_R / CELL) as i32 + 1;
    let (x_lo, x_hi) = if lying { (-(HALF_L / CELL) as i32, (HALF_L / CELL) as i32) } else { (-n, n) };
    let (mut worst, mut tops, mut flutes, mut standing, mut grazing, mut slivers) = (0.0f64, 0usize, 0usize, 0usize, 0usize, 0usize);
    for j in -n..=n {
        for i in x_lo..=x_hi {
            let (x, y) = ((i as f64 + 0.5) * CELL, (j as f64 + 0.5) * CELL);
            if uncut_top(lying, x, y).is_none() {
                continue;
            }
            // The march starts on the stock the field cuts - its own
            // uncut top (the cap on its cell plane, the laid bar's
            // tessellated crown), never an analytic circle above it.
            let skin = uncut.top(x, y);
            let got = f.top(x, y);
            assert!(got.is_finite() && skin.is_finite(), "{kind:?}: no top at ({x}, {y})");
            match law_floor(&cs, x, y, skin) {
                // Untouched from above by more than the flute's chord
                // band: the field must leave the column's own top.
                Floor::Standing(clear) if clear > band => {
                    let e = (got - skin).abs();
                    assert!(e <= bound, "{kind:?}: standing top at ({x}, {y}): field {got:.7} vs uncut {skin:.7}, |delta| {e:.3e} > {bound:.3e}");
                    standing += 1;
                }
                Floor::Standing(_) => {}
                Floor::Unreadable => grazing += 1,
                Floor::Sliver => slivers += 1,
                Floor::Cut(want, sphere) => {
                    // Steep flanks are no column verdict's to read either.
                    let d = 1e-3;
                    let steep = [(x + d, y), (x - d, y), (x, y + d), (x, y - d)].iter().any(|&(u, v)| {
                        !matches!(law_floor(&cs, u, v, skin), Floor::Cut(t, _) if (t - want).abs() <= 2.0 * d)
                    });
                    if steep || !sphere {
                        flutes += usize::from(!sphere);
                        continue;
                    }
                    let e = (got - want).abs();
                    worst = worst.max(e);
                    tops += 1;
                    assert!(e <= bound, "{kind:?}: floor top at ({x}, {y}): field {got:.7} vs law {want:.7}, |delta| {e:.3e} > {bound:.3e}");
                }
            }
        }
    }
    println!(
        "{kind:?} height parity: {tops} sphere-cut tops, worst |delta| {worst:.3e} (bound {bound:.3e}); {standing} standing tops exact; {flutes} flute-cut, {grazing} grazing, {slivers} sliver columns skipped"
    );
    // A laid bar's crown is read whole from above. A standing bar's
    // rings cut its flanks and bore under its cap, and a column
    // marched from above reads the cap or the top ring's floor in the
    // neck's annulus; the sign scan judges the tunnels.
    if lying {
        assert!(tops > 50, "{kind:?}: the sphere-cut floor must be read, got {tops}");
    } else {
        assert!(tops + standing > 50, "{kind:?}: the cap and the top ring's floor must be read, got {tops} + {standing}");
    }

    let (mut checked, mut misses, mut worst_sign) = (0usize, 0usize, 0.0f64);
    let m = (HALF_L / CELL) as i32;
    for k in (-m..=m).step_by(2) {
        for j in -n..=n {
            for i in -n..=n {
                let (u, v, w) = ((i as f64 + 0.5) * CELL, (j as f64 + 0.5) * CELL, (k as f64 + 0.5) * CELL);
                let p = if lying { Point3::new(w, u, v) } else { Point3::new(u, v, w) };
                if !in_bar(lying, p) {
                    continue;
                }
                let (law, _) = law_cut(&cs, p);
                if law.abs() <= band {
                    continue;
                }
                let d = f.distance(p);
                checked += 1;
                if (d > 0.0) != (law < 0.0) {
                    misses += 1;
                    worst_sign = worst_sign.max(law.abs());
                }
            }
        }
    }
    println!("{kind:?} sign parity: {checked} cell centres, {misses} misses, worst {worst_sign:.3e} (band {band:.3e})");
    assert_eq!(misses, 0, "{kind:?}: sign misses off the band");
    assert!(checked > 500, "{kind:?}: the scan must cover the skin, got {checked}");
}

/// The eye's chord-class cost of a kind's program at the verdict
/// lattice, the number the lift is gated on.
fn eye_cost(kind: Kind) {
    let world = world_for(kind);
    let eye_tol = rotary_pose_tol(CELL);
    let (_, stats, wall) = run(&world, CELL, eye_tol);
    let per = wall.as_secs_f64() * 1e3 / stats.sub_chords.max(1) as f64;
    println!(
        "{kind:?} at the eye's chord class ({eye_tol:.3e}): {} sub-chords, wall {wall:.2?} ({per:.3} ms/sub-chord)",
        stats.sub_chords
    );
}

#[test]
#[ignore = "heavy lane (M13.1 rotary parity): the three kinds' programs at the verdict class (Contour 92,727 and Offset 89,685 sub-chords, the law marched per column) and the eye's chord-class cost, release ~14 min (808 s measured 2026-11-06) - run at milestone close with --release -- --ignored"]
fn every_rotary_kind_reads_the_radial_law_and_its_cost_is_measured() {
    parity(Kind::Parallel);
    parity(Kind::Contour);
    parity(Kind::Offset);
    eye_cost(Kind::Parallel);
    eye_cost(Kind::Contour);
    eye_cost(Kind::Offset);
}

/// The column verdict never judges a rotary program (M13.1): a lifted
/// kind refuses with the verdict's own words, an unmeasured kind with
/// the plan's - by name either way, never an un-chorded posed cut. The
/// eye draws only the lifted kinds; an unmeasured kind's document
/// refuses the picture with the same words, never a silent chord.
#[test]
fn the_column_verdict_refuses_every_rotary_kind_by_name() {
    for kind in [Kind::Parallel, Kind::Contour, Kind::Offset] {
        let world = world_for(kind);
        let err = simulate_document(&world).expect_err("the column verdict does not judge a rotary program");
        let lift = rotary_kind_lift(&params(kind)).unwrap_or_else(|| panic!("{kind:?} is rotary"));
        let want = match lift {
            Ok(()) => ROTARY_VERDICT_REFUSAL,
            Err(why) => why,
        };
        assert!(err.contains(want), "{kind:?}: {err}");
        if let Err(why) = lift {
            let eye = field_view(&world).expect_err("the eye does not draw an unmeasured rotary kind");
            assert!(eye.contains(why), "{kind:?}: {eye}");
        }
    }
}

/// The eye on the laid bar: its stock entity carries a rotation, which
/// the field view refuses by name before any cut (the pre-existing
/// axis-aligned-stock law) - the routed path is proved on the stock's
/// own facets by the parity lane and the slice law below.
#[test]
fn the_eye_refuses_the_laid_bars_rotated_stock_by_name() {
    let world = world_for(Kind::Parallel);
    let err = field_view(&world).expect_err("the rotated stock entity is refused");
    assert!(err.contains("carries a rotation"), "{err}");
}

/// A playback slice turns from where the head stood (M13.1): cutting
/// the program in two slices through `simulate_rotary_slice` leaves
/// the same steel, cell for cell, as cutting it whole.
#[test]
fn a_playback_slice_turns_from_where_the_head_stood() {
    let world = world_for(Kind::Parallel);
    let (moves, ax) = program(&world);
    let cell = 1.0;
    let tol = rotary_pose_tol(cell);
    fn op(m: &[Move]) -> SimOp<'_> {
        SimOp { moves: m, radius: R, ball: true, cone: None, corner: None }
    }
    let mut whole = field_stock(&world, cell);
    simulate_rotary_slice(&mut whole, &op(&moves), &moves, 0, &ax, tol).expect("whole");
    // Split where the pose changes, past the middle: the second
    // slice's first move turns, and must turn from the held pose.
    let k = (moves.len() / 2..moves.len())
        .find(|&k| moves[k].axis.is_some() && moves[k].axis != moves[k - 1].axis)
        .expect("a turn past the middle");
    let mut sliced = field_stock(&world, cell);
    simulate_rotary_slice(&mut sliced, &op(&moves[..k]), &moves, 0, &ax, tol).expect("first slice");
    simulate_rotary_slice(&mut sliced, &op(&moves[k..]), &moves, k, &ax, tol).expect("second slice");
    let n = (BAR_R / cell) as i32 + 1;
    let (mut checked, mut cut) = (0usize, 0usize);
    for i in -(HALF_L / cell) as i32..(HALF_L / cell) as i32 {
        for j in -n..=n {
            for k in -n..=n {
                let p = Point3::new((i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell, (k as f64 + 0.5) * cell);
                if !in_bar(true, p) {
                    continue;
                }
                let (a, b) = (whole.distance(p), sliced.distance(p));
                assert_eq!(a > 0.0, b > 0.0, "at {p:?}: whole {a:.6}, sliced {b:.6}");
                checked += 1;
                cut += usize::from(a > 0.0);
            }
        }
    }
    assert!(checked > 500 && cut > 20, "fixture: {checked} centres, {cut} cut");
}

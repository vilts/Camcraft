//! The per-move judgment helpers the machine checks share (#669 split
//! them out of `mod.rs` when the plasma check became their second
//! reader): a move's extreme points, the axis a feed ceiling belongs
//! to, the C-letter gate, the rigid-tap gate (#786), the tool-pose gate
//! (#797), and the capped reporter.
//!
//! Provenance: general machine-tool practice (an arc bulges to its
//! quarter points; a coordinated feed is capped by its slowest driven
//! axis; a rigid tap needs a spindle the control can synchronise the
//! feed to - RS-274 G84), our own code moved from `machine/mod.rs` plus
//! the tap gate; the 4th-axis pose reading from RS-274's A as a rotation
//! about X and our `cl::pose` law.

use super::{AxisLimits, Machine, MachineCheck, REPORT_CAP};
use crate::geometry::Point3;

/// The arc's four XY quarter points plus both endpoints; a straight
/// move's two endpoints. (A quarter is outside the sweep only when
/// the arc goes the other way around it; checking all four can only
/// over-report a limit the post would then also cross, never hide a
/// violation - and over-reporting a REAL stroke is stage C's honest
/// direction of error.)
pub(super) fn move_extremes(mv: &crate::cl::Move) -> Vec<Point3> {
    let mut v = vec![mv.from, mv.to];
    // XZ / YZ (M11.2 A): the same four quarters, in the arc's plane.
    if let Some(arc) = mv.arc.filter(|a| !a.plane.is_xy()) {
        let pl = arc.plane;
        let (cu, cv) = pl.center_uv(arc.center);
        let (u0, v0, _) = pl.uvw(mv.from);
        let (_, _, w1) = pl.uvw(mv.to);
        let r = ((u0 - cu).powi(2) + (v0 - cv).powi(2)).sqrt();
        v.push(pl.point(cu + r, cv, w1));
        v.push(pl.point(cu - r, cv, w1));
        v.push(pl.point(cu, cv + r, w1));
        v.push(pl.point(cu, cv - r, w1));
        return v;
    }
    if let Some(arc) = &mv.arc {
        let dx = mv.from.x - arc.center.x;
        let dy = mv.from.y - arc.center.y;
        let r = (dx * dx + dy * dy).sqrt();
        v.push(Point3::new(arc.center.x + r, arc.center.y, mv.to.z));
        v.push(Point3::new(arc.center.x - r, arc.center.y, mv.to.z));
        v.push(Point3::new(arc.center.x, arc.center.y + r, mv.to.z));
        v.push(Point3::new(arc.center.x, arc.center.y - r, mv.to.z));
    }
    v
}

pub(super) fn slowest_axis(axes: &[AxisLimits], ceil: f64) -> String {
    axes.iter()
        .find(|a| a.feed == Some(ceil))
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "an axis".into())
}

pub(super) fn say(out: &mut MachineCheck, s: String) {
    if out.problems.len() < REPORT_CAP {
        out.problems.push(s);
    } else {
        out.more += 1;
    }
}

/// The tool-pose gate (M5.1 stage A; #797): what a move leaning its tool
/// to the unit `axis` asks of the machine's rotaries, one sentence per
/// thing it cannot give. Empty when the machine holds the pose.
///
/// A machine whose ONLY rotary is about X (a 4th-axis table) is judged
/// through the tool axis: [`x_rotary_problem`]. Every other machine is
/// judged on the canonical letters, each by its own name - a machine
/// whose tilt lives under another letter (an A trunnion asked for a B
/// turn) is refused by name too; that gate never does IK.
pub(super) fn pose_problems(machine: &Machine, i: usize, axis: [f64; 3]) -> Vec<String> {
    let mut out = Vec::new();
    if !super::can_tilt(machine) {
        out.push(format!("move {i} tilts the tool; {} states no tilting rotary", machine.model));
    }
    let [da, db, dc] = crate::cl::axis_to_angles(axis);
    if let Some(ax) = only_x_rotary(machine) {
        out.extend(x_rotary_problem(machine, ax, i, [da, db, dc]));
        return out;
    }
    for (letter, ang) in [('A', da), ('B', db), ('C', dc)] {
        if ang.abs() <= 1e-9 {
            continue; // that letter holds nothing to check
        }
        match machine
            .axes
            .iter()
            .find(|x| x.rotary && x.name.eq_ignore_ascii_case(&letter.to_string()))
        {
            None => out.push(format!(
                "move {i} commands {letter} {ang:.3} deg; {} states no {letter} axis",
                machine.model
            )),
            Some(ax) => {
                if let Some((lo, hi)) = ax.travel
                    && (ang < lo - 1e-9 || ang > hi + 1e-9)
                {
                    out.push(format!("move {i} commands {letter} {ang:.3} deg, the travel is {lo:.3}..{hi:.3}"));
                }
            }
        }
    }
    out
}

/// The machine's one rotary, when it has exactly one and it states a
/// direction along X (within the post's own 0.998 of its length squared,
/// `tormach.cps` `aRotary`). A second rotary of any kind, or an unstated
/// direction, keeps the letter law: capability is stated, never assumed.
fn only_x_rotary(machine: &Machine) -> Option<&AxisLimits> {
    let mut rotaries = machine.axes.iter().filter(|a| a.rotary);
    let ax = rotaries.next()?;
    if rotaries.next().is_some() {
        return None;
    }
    let d = ax.axis?;
    let n2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    (d.iter().all(|v| v.is_finite()) && n2 > 0.0 && d[0] * d[0] >= 0.998 * n2).then_some(ax)
}

/// The tool angle about X the canonical letters `[a, b, c]` stand for,
/// or None when no rotation about X reaches the pose. The pose law
/// (`cl::pose`): axis = Rz(C)·Ry(B)·Rx(A)·Z; a turn `t` about X reaches
/// exactly (0, -sin t, cos t), the axes with no X component - so the face
/// underneath, canonical A0 B180, is t = 180. Read from the letters, the
/// way the PathPilot post reads its stream (`tormach.cps` `toolAngle`),
/// with the same tolerances, so the check and that post agree on every
/// pose. The G43.4 dialects (`fanuc.cps`, `doosan-dnm.cps`) write the
/// letters as they come, unnegated, so the check's pass is not their
/// word: each refuses by name the B of A0 B180 and a non-zero A on a
/// lone rotary stated along -X.
fn x_tool_angle([a, b, c]: [f64; 3]) -> Option<f64> {
    if !(a.is_finite() && b.is_finite() && c.is_finite()) || c.abs() > 1e-6 {
        return None;
    }
    let (sa, ca) = a.to_radians().sin_cos();
    let (sb, cb) = b.to_radians().sin_cos();
    let (ax, ay, az) = (ca * sb, -sa, ca * cb);
    if ax.abs() > 1e-9 {
        return None;
    }
    let mut t = (-ay).atan2(az).to_degrees();
    if t <= -180.0 + 1e-6 {
        t = 180.0; // one name for the face underneath
    }
    Some(if t.abs() < 1e-9 { 0.0 } else { t })
}

/// [`pose_problems`] on a machine whose one rotary `ax` turns about X:
/// the pose is judged as the angle that rotary must hold, signed by the
/// stated direction (a [-1,0,0] table commands the negated angle, as the
/// post writes it) and checked against its travel. A pose leaning along
/// X - any B or C a turn about X cannot make - is refused by name.
fn x_rotary_problem(machine: &Machine, ax: &AxisLimits, i: usize, letters: [f64; 3]) -> Option<String> {
    // Dust-free for the sentence (an asin of -0.0 reads "-0.000").
    let [da, db, dc] = letters.map(|v| if v.abs() < 1e-9 { 0.0 } else { v });
    let Some(t) = x_tool_angle(letters) else {
        return Some(format!(
            "move {i} commands A {da:.3} B {db:.3} C {dc:.3} deg; {}'s only rotary, {} about X, cannot reach that pose",
            machine.model, ax.name
        ));
    };
    let sign = ax.axis.map_or(1.0, |d| if d[0] < 0.0 { -1.0 } else { 1.0 });
    let word = sign * t;
    let (lo, hi) = ax.travel?;
    (word < lo - 1e-6 || word > hi + 1e-6).then(|| {
        format!(
            "move {i} commands {} {word:.3} deg (the pose A {da:.3} B {db:.3}), the travel is {lo:.3}..{hi:.3}",
            ax.name
        )
    })
}

/// The C letter gate (issue #33): a move stating spindle C angle `c`
/// needs a rotary named C on the machine, and a stated C travel must
/// hold the unwrapped angle. None when it is fine.
pub(super) fn c_letter_problem(machine: &Machine, i: usize, c: f64) -> Option<String> {
    let Some(ax) = machine.axes.iter().find(|x| x.rotary && x.name.eq_ignore_ascii_case("C")) else {
        return Some(format!("move {i} turns the spindle as C {c:.3} deg; {} states no C axis", machine.model));
    };
    let (lo, hi) = ax.travel?;
    (c < lo - 1e-9 || c > hi + 1e-9)
        .then(|| format!("move {i} commands C {c:.3} deg, the travel is {lo:.3}..{hi:.3}"))
}

/// The rigid-tap gate (#786): a path that carries a tap cycle, on a
/// machine whose file says its spindle cannot rigid-tap
/// (`spindle.rigid_tapping: false`), is refused by name - whatever post
/// is chosen, and never converted to another cycle. Unstated reads as
/// "not limited" (the unstated-limit law): nothing is said then.
pub(super) fn tap_gate(out: &mut MachineCheck, machine: &Machine, name: &str, path: &crate::cl::Toolpath) {
    if machine.rigid_tapping != Some(false) {
        return;
    }
    let taps = path.cycles.iter().filter(|c| c.kind == crate::cl::CycleKind::Tap).count();
    if taps > 0 {
        let m = &machine.model;
        say(out, format!("{name}: {taps} tap cycle(s) need rigid tapping, which {m} cannot do - use a tapping head or thread milling"));
    }
}

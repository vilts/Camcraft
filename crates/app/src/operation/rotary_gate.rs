//! The rotary kinds' axis-letter gate (M13.5 B): which machine
//! rotary turns the part a Rotary Contour/Parallel/Offset poses
//! about. The letter is the MACHINE's word, never the kind's: a mill
//! table's trunnion is A or B, a mill-turn's spindle is C, and the
//! fitted axis the generator turned about must be one the mounted
//! machine states - refused by name where it states none.
//!
//! The C half (issue #33): the spindle's C is admitted and answered
//! as 'C'; the caller then stamps the path with the stated C angle
//! (`cl::stamp_c`) - the post's pose law renders a tool AXIS as
//! [A, B, 0] and cannot carry C, so the angle rides beside it, and a
//! C-axis op never posts as A/B swings the mill-turn does not have.
//!
//! Provenance: our own wiring of the machine inventory to the pose
//! law; axis lettering per RS-274 / ISO 6983 convention. No vendor
//! material consulted.

use crate::machine::AxisLimits;
use crate::machining2d::GenError;

/// A cosine this close to one reads two unit directions as the same
/// line (either sign - a rotary turns both ways).
const SAME_LINE: f64 = 0.999;

/// The letter the mounted machine gives the rotary line `dir` (unit,
/// world frame) for `kind`. `None` machine, or a machine whose
/// rotaries state no axis vector, checks nothing (the unstated-limit
/// law) and answers `Ok(None)`. A stated inventory must hold a rotary
/// on that line.
pub(super) fn rotary_letter(
    machine: Option<&(String, Vec<AxisLimits>)>,
    dir: [f64; 3],
    kind: &str,
) -> Result<Option<char>, GenError> {
    let Some((model, rotaries)) = machine else {
        return Ok(None);
    };
    let stated: Vec<&AxisLimits> = rotaries.iter().filter(|a| a.axis.is_some()).collect();
    if stated.is_empty() {
        return Ok(None);
    }
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if !(len.is_finite() && len > 1e-12) {
        return Err(GenError::Invalid(format!("{kind}: the fitted rotary axis states no direction")));
    }
    let hit = stated.iter().find(|a| {
        a.axis.is_some_and(|v| {
            let vl = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            vl > 1e-12
                && ((v[0] * dir[0] + v[1] * dir[1] + v[2] * dir[2]) / (vl * len)).abs() >= SAME_LINE
        })
    });
    let Some(ax) = hit else {
        let names: Vec<&str> = stated.iter().map(|a| a.name.as_str()).collect();
        return Err(GenError::Invalid(format!(
            "{kind} turns the part about ({:.3}, {:.3}, {:.3}); {model} states no rotary on that line (its rotaries: {}) - a mill table's A/B or a mill-turn spindle's C must hold it",
            dir[0] / len,
            dir[1] / len,
            dir[2] / len,
            if names.is_empty() { "none".to_owned() } else { names.join(", ") }
        )));
    };
    Ok(ax.name.chars().next().map(|c| c.to_ascii_uppercase()))
}

/// Stamp a rotary kind's part-frame path with the spindle C (issue
/// #33) when the gate answered the machine's C for the fitted line
/// `c_axis`; any other letter returns the path untouched. The line
/// must stand along world Z - C turns about Z - and the stamp is split
/// to `tol` (the op's verify budget), refused by name where the law
/// cannot hold (`cl::stamp_c`).
///
/// A machine stating a C travel (`machine`'s rotary named C - the
/// one the machine check judges) gets the stamped path unwound by
/// whole turns at its clear links (`cl::unwind_c`, slice 2): the
/// rewinds ride as orbit rapids every later law judges, and a cut
/// that no rewind can fit is refused here by name. Only a kind whose
/// unwind is proven end to end asks for it (`unwinds`: Rotary Contour);
/// any other kind is refused by name where its C leaves the travel.
pub(super) fn stamp_spindle_c(
    path: crate::cl::Toolpath,
    c_axis: Option<crate::multiaxis::axis::Axis>,
    (machine, unwinds): (Option<&(String, Vec<AxisLimits>)>, bool),
    tol: f64,
    kind: &str,
) -> Result<crate::cl::Toolpath, GenError> {
    let Some(ax) = c_axis else {
        return Ok(path);
    };
    // #1015: the kernel's world path is in hand at every refusal below.
    stamp_c_about(&path, ax, machine, unwinds, tol, kind).map_err(|e| e.with_ghost(&path))
}

/// [`stamp_spindle_c`]'s body, over the path it may refuse.
fn stamp_c_about(
    path: &crate::cl::Toolpath,
    ax: crate::multiaxis::axis::Axis,
    machine: Option<&(String, Vec<AxisLimits>)>,
    unwinds: bool,
    tol: f64,
    kind: &str,
) -> Result<crate::cl::Toolpath, GenError> {
    let d = ax.dir;
    let len = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt();
    if !(len > 1e-12 && (d.z / len).abs() >= SAME_LINE) {
        return Err(GenError::Invalid(format!(
            "{kind}: the part turns about ({:.3}, {:.3}, {:.3}), not the spindle's Z line - C cannot hold it",
            d.x, d.y, d.z
        )));
    }
    // The line's XY where it crosses z = 0 (it stands within SAME_LINE
    // of vertical, so the crossing is finite).
    let t = -ax.origin.z / d.z;
    let center = [ax.origin.x + d.x * t, ax.origin.y + d.y * t];
    let tol = if tol.is_finite() && tol > 0.0 { tol } else { crate::machining2d::ARC_FIT_TOL };
    let stamped = crate::cl::stamp_c(path, center, tol).map_err(|e| GenError::Invalid(format!("{kind}: {e}")))?;
    let Some((lo, hi)) = c_travel(machine) else {
        return Ok(stamped);
    };
    if unwinds {
        let unwound = crate::cl::unwind_c(&stamped, center, (lo, hi), tol, false).map_err(|e| GenError::Invalid(format!("{kind}: {e}")))?;
        // #906: a rewind orbit is C-turning rapids, capped only by the
        // C rate the machine states. None stated: refuse the rewind by
        // name rather than post it at an uncapped feed. (Shifting the
        // opening stretch adds no move: the section's first G00 C
        // positions it, at no F.)
        if unwound.moves.len() != stamped.moves.len() && !c_rated(machine) {
            return Err(GenError::Invalid(format!(
                "{kind}: the path needs a C rewind, but the machine file states a C travel but no C rate - a rewind would post at an uncapped feed; state the C axis's feed or rapid rate"
            )));
        }
        return Ok(unwound);
    }
    // A kind whose links the unwind law has not been proven on end to
    // end posts only what fits the travel as stamped - exactly what the
    // machine check passed before slice 2 - and is refused here, by
    // name, where it would need a rewind.
    let (c_lo, c_hi) = stamped
        .moves
        .iter()
        .filter_map(|m| m.rotary)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), c| (l.min(c), h.max(c)));
    if c_lo < lo - 1e-9 || c_hi > hi + 1e-9 {
        return Err(GenError::Invalid(format!(
            "{kind}: the path turns C over {c_lo:.3}..{c_hi:.3} deg, past the machine's C travel {lo:.3}..{hi:.3}; unwinding C at links is proven for Rotary Contour only, so {kind} is refused where it would need a rewind"
        )));
    }
    Ok(stamped)
}

/// The C travel the mounted machine states for its rotary named C
/// (the same axis `machine::check`'s C letter gate reads), or None.
fn c_travel(machine: Option<&(String, Vec<AxisLimits>)>) -> Option<(f64, f64)> {
    machine?.1.iter().find(|a| a.rotary && a.name.eq_ignore_ascii_case("C"))?.travel
}

/// True when the mounted machine's rotary named C states a rate -
/// its feed or its rapid (deg/min), as the lathe post caps by (#906).
fn c_rated(machine: Option<&(String, Vec<AxisLimits>)>) -> bool {
    let rated = |r: Option<f64>| r.is_some_and(|v| v.is_finite() && v > 0.0);
    machine.is_some_and(|m| m.1.iter().any(|a| a.rotary && a.name.eq_ignore_ascii_case("C") && (rated(a.feed) || rated(a.rapid))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rot(name: &str, axis: [f64; 3]) -> AxisLimits {
        AxisLimits {
            name: name.into(),
            rotary: true,
            axis: Some(axis),
            ..Default::default()
        }
    }

    /// One and a half turns of a radial helix at r 10: C reads -540.
    fn helix() -> crate::cl::Toolpath {
        use crate::cl::{Move, MoveKind};
        use crate::geometry::Point3;
        let at = |k: usize| {
            let th = (k as f64 * 30.0).to_radians();
            Point3::new(10.0 * th.cos(), 10.0 * th.sin(), -(k as f64) / 6.0)
        };
        let moves = (0..18)
            .map(|k| Move::new(MoveKind::Feed, at(k), at(k + 1), Some(400.0)).with_axis([at(k + 1).x / 10.0, at(k + 1).y / 10.0, 0.0]))
            .collect();
        crate::cl::Toolpath::new(moves)
    }

    fn spindle() -> Option<crate::multiaxis::axis::Axis> {
        use crate::geometry::Point3;
        Some(crate::multiaxis::axis::Axis { origin: Point3::new(0.0, 0.0, 0.0), dir: Point3::new(0.0, 0.0, 1.0) })
    }

    fn c_travel_machine(lo: f64, hi: f64) -> (String, Vec<AxisLimits>) {
        let mut c = rot("C", [0.0, 0.0, 1.0]);
        c.travel = Some((lo, hi));
        ("MT".to_owned(), vec![c])
    }

    #[test]
    fn parallel_and_offset_are_refused_by_name_where_c_would_need_a_rewind() {
        let m = c_travel_machine(-360.0, 360.0);
        for kind in ["Rotary Parallel", "Rotary Offset"] {
            let e = stamp_spindle_c(helix(), spindle(), (Some(&m), false), 0.01, kind).err().map(|e| e.to_string()).unwrap_or_default();
            assert!(e.contains(kind) && e.contains("proven for Rotary Contour only"), "{e}");
            // Inside the travel as stamped: posts exactly as before slice 2.
            let wide = c_travel_machine(-720.0, 720.0);
            let t = stamp_spindle_c(helix(), spindle(), (Some(&wide), false), 0.01, kind);
            assert!(t.is_ok_and(|t| t.moves.iter().all(|m| m.rotary.is_some_and(|c| (-720.0..=0.0).contains(&c)))), "{kind}");
        }
        // The contour unwinds: its 540 deg opening stretch is shifted a
        // whole turn into the travel (the section opens there).
        let t = stamp_spindle_c(helix(), spindle(), (Some(&m), true), 0.01, "Rotary Contour");
        assert!(t.is_ok_and(|t| t.moves.iter().all(|m| m.rotary.is_some_and(|c| (-360.0..=360.0).contains(&c)))));
    }

    #[test]
    fn a_gate_refusal_hands_back_the_world_path_it_judged() {
        // #1015: the rotary kernel's path is world (the frame the post
        // reads), so the refused path rides as the ghost, move for move.
        let m = c_travel_machine(-360.0, 360.0);
        let e = stamp_spindle_c(helix(), spindle(), (Some(&m), false), 0.01, "Rotary Parallel").err();
        let ghost = e.as_ref().and_then(|e| e.ghost()).map(|g| g.moves().to_vec());
        assert_eq!(ghost, Some(helix().moves), "{e:?}");
        // A tilted line refused before any stamp: the same path rides.
        let leaning = crate::multiaxis::axis::Axis { dir: crate::geometry::Point3::new(1.0, 0.0, 0.0), ..spindle().expect("axis") };
        let e = stamp_spindle_c(helix(), Some(leaning), (Some(&m), false), 0.01, "Rotary Offset").err();
        assert_eq!(e.as_ref().and_then(|e| e.ghost()).map(|g| g.len()), Some(helix().moves.len()), "{e:?}");
    }

    /// Rotary Contour unwinds over the axial plane only: a path whose
    /// only clearance is a retract cylinder is refused by name through
    /// the gate, never rewound at a radius no shipped kind has proven.
    #[test]
    fn the_contour_takes_no_radial_link() {
        use crate::cl::{Move, MoveKind};
        use crate::geometry::Point3;
        let at = |k: usize, z: f64| {
            let th = (k as f64 * 30.0).to_radians();
            Point3::new(10.0 * th.cos(), 10.0 * th.sin(), z)
        };
        let radial = |p: Point3| {
            let r = p.x.hypot(p.y);
            [p.x / r, p.y / r, 0.0]
        };
        let mut moves: Vec<Move> = Vec::new();
        for ring in 0..3 {
            let z = -(ring as f64);
            if let Some(end) = moves.last().map(|m| m.to) {
                let (a, b) = (Point3::new(20.0, 0.0, end.z), Point3::new(20.0, 0.0, z));
                for (p, q) in [(end, a), (a, b), (b, at(0, z))] {
                    moves.push(Move::new(MoveKind::Rapid, p, q, None).with_axis(radial(q)));
                }
            }
            for k in 0..12 {
                let to = if k == 11 { at(0, z) } else { at(k + 1, z) };
                moves.push(Move::new(MoveKind::Feed, at(k, z), to, Some(400.0)).with_axis(radial(to)));
            }
        }
        let m = c_travel_machine(-360.0, 360.0);
        let path = crate::cl::Toolpath::new(moves);
        let e = stamp_spindle_c(path, spindle(), (Some(&m), true), 0.01, "Rotary Contour").err().map(|e| e.to_string()).unwrap_or_default();
        assert!(e.contains("Rotary Contour") && e.contains("radial links are not taken for this kind"), "{e}");
    }

    /// #906: two helices linked over the top need an orbit; with no C
    /// rate stated the rewind is refused by name, with one it unwinds.
    #[test]
    fn a_rewind_without_a_stated_c_rate_is_refused_by_name() {
        use crate::cl::{Move, MoveKind};
        use crate::geometry::Point3;
        let mut path = helix().moves;
        let lower = |mut m: Move| {
            m.from.z -= 4.0;
            m.to.z -= 4.0;
            m
        };
        let more: Vec<Move> = helix().moves.into_iter().map(lower).collect();
        let (Some(end), Some(start)) = (path.last().map(|m| m.to), more.first().map(|m| m.from)) else {
            panic!("the helix has moves")
        };
        // Up, round the spindle line through (0, 15) - never across it - and down.
        let (up, via, over) = (Point3::new(end.x, end.y, 5.0), Point3::new(0.0, 15.0, 5.0), Point3::new(start.x, start.y, 5.0));
        for (p, q) in [(end, up), (up, via), (via, over), (over, start)] {
            path.push(Move::new(MoveKind::Rapid, p, q, None).with_axis([q.x / q.x.hypot(q.y), q.y / q.x.hypot(q.y), 0.0]));
        }
        path.extend(more);
        let path = crate::cl::Toolpath::new(path);
        let m = c_travel_machine(-360.0, 360.0);
        let e = stamp_spindle_c(path.clone(), spindle(), (Some(&m), true), 0.01, "Rotary Contour").err().map(|e| e.to_string()).unwrap_or_default();
        assert!(e.contains("Rotary Contour") && e.contains("states a C travel but no C rate"), "{e}");
        let mut rated = m.clone();
        for a in &mut rated.1 {
            a.feed = Some(6000.0);
        }
        let t = stamp_spindle_c(path, spindle(), (Some(&rated), true), 0.01, "Rotary Contour");
        assert!(t.is_ok_and(|t| t.moves.iter().all(|m| m.rotary.is_some_and(|c| (-360.0..=360.0).contains(&c)))));
    }

    #[test]
    fn no_machine_checks_nothing() {
        assert_eq!(rotary_letter(None, [1.0, 0.0, 0.0], "Rotary Contour").ok(), Some(None));
    }

    #[test]
    fn a_table_trunnion_names_its_letter_either_way_round() {
        let m = ("Bench".to_owned(), vec![rot("A", [1.0, 0.0, 0.0])]);
        assert_eq!(rotary_letter(Some(&m), [1.0, 0.0, 0.0], "k").ok(), Some(Some('A')));
        assert_eq!(rotary_letter(Some(&m), [-1.0, 0.0, 0.0], "k").ok(), Some(Some('A')));
    }

    #[test]
    fn a_missing_line_refuses_by_name_and_the_spindle_answers_c() {
        let b = ("Bench".to_owned(), vec![rot("B", [0.0, 1.0, 0.0])]);
        let e = rotary_letter(Some(&b), [1.0, 0.0, 0.0], "Rotary Offset").err();
        assert!(e.is_some_and(|e| e.to_string().contains("states no rotary on that line")));
        let c = ("MT".to_owned(), vec![rot("C", [0.0, 0.0, 1.0])]);
        assert_eq!(rotary_letter(Some(&c), [0.0, 0.0, 1.0], "Rotary Contour").ok(), Some(Some('C')));
    }
}

// Provenance: clean-room implementation from public 5-axis XYZAC trunnion geometry and rotary chord bisection.

//! Path-level XYZAC solving, rotary chord subdivision, pole C-reindex splitting, and CL adapter.

use camcraft_cl::cl::{MoveKind, Toolpath};

use super::math::{
    cross, dot, ensure_finite_scalar, ensure_finite_vec, forward_machine_validated,
    forward_part_validated, lerp3, norm, normalize_finite, slerp_unit, sub,
};
use super::solve::AcSolver;
use super::types::{
    AcTrunnionConfig, KinematicsError, MachinePose, PartPose, PathMove, PathOptions,
    SectionBoundary, SolvedMove,
};

const MAX_SUBDIV_DEPTH: u8 = 14;
const POSE_MATCH_POS_TOL_MM: f64 = 1e-7;
const POSE_MATCH_AXIS_DOT: f64 = 1.0 - 1e-12;

/// Solves a sequence of part-space moves into machine-space coordinates with rotary chord subdivision.
pub fn solve_path(
    cfg: &AcTrunnionConfig,
    moves: &[PathMove],
    opts: &PathOptions,
) -> Result<Vec<SolvedMove>, KinematicsError> {
    let chord_tol_mm = ensure_finite_scalar(opts.chord_tol_mm, "chord_tol_mm")?;
    if chord_tol_mm <= 0.0 {
        return Err(KinematicsError::InvalidConfig {
            message: format!("chord_tol_mm must be positive, got {chord_tol_mm}"),
        });
    }
    if let Some(max_deg) = opts.max_angular_step_deg {
        let max_deg = ensure_finite_scalar(max_deg, "max_angular_step_deg")?;
        if max_deg <= 0.0 {
            return Err(KinematicsError::InvalidConfig {
                message: format!("max_angular_step_deg must be positive, got {max_deg}"),
            });
        }
    }

    let mut solver = AcSolver::new(cfg)?;
    let mut out = Vec::new();
    let mut prev_end: Option<(PartPose, MachinePose)> = None;

    for (source_index, mv) in moves.iter().enumerate() {
        let from_tip = ensure_finite_vec(mv.from_tip, "move from_tip")?;
        let to_tip = ensure_finite_vec(mv.to_tip, "move to_tip")?;
        let from_axis = normalize_finite(mv.from_axis, "move from_axis")?;
        let to_axis = normalize_finite(mv.to_axis, "move to_axis")?;
        if let Some(feed) = mv.feed {
            let _ = ensure_finite_scalar(feed, "move feed")?;
        }

        let p_from = PartPose {
            tip: from_tip,
            axis: from_axis,
        };
        let p_to = PartPose {
            tip: to_tip,
            axis: to_axis,
        };

        let m_from = match prev_end {
            Some((prev_part, prev_mach)) if same_part_pose(prev_part, p_from) => prev_mach,
            Some(_) => return Err(KinematicsError::DiscontinuousPath { source_index }),
            None => solver.solve_pose(p_from, SectionBoundary::Continuous)?,
        };

        // A clearance marker does not establish clearance before the lift finishes.
        // Keep the winding through the lift; refuse exhausted travel instead of
        // synthesizing an unchecked unwind, even after reaching clearance.
        let allow_subdivide =
            mv.safe_retract || mv.kind != MoveKind::Rapid || opts.subdivide_rapids;

        if !allow_subdivide {
            let m_to = solver.solve_pose(p_to, SectionBoundary::Continuous)?;
            out.push(SolvedMove {
                source_index,
                kind: mv.kind,
                feed: mv.feed,
                from: m_from,
                to: m_to,
                tip_from: p_from.tip,
                tip_to: p_to.tip,
                axis_from: p_from.axis,
                axis_to: p_to.axis,
            });
            prev_end = Some((p_to, m_to));
            continue;
        }

        let m_end = emit_continuous_segment(
            &mut solver,
            source_index,
            mv.kind,
            mv.feed,
            opts,
            p_from,
            m_from,
            p_to,
            0,
            &mut out,
        )?;
        prev_end = Some((p_to, m_end));
    }

    Ok(out)
}

/// Adapts a CL [`Toolpath`] into [`PathMove`] entries and solves it for an AC trunnion machine.
/// Returns [`KinematicsError::UnsupportedArc`] for arcs; callers must linearize them within tolerance.
pub fn solve_toolpath(
    cfg: &AcTrunnionConfig,
    toolpath: &Toolpath,
    opts: &PathOptions,
) -> Result<Vec<SolvedMove>, KinematicsError> {
    let mut path_moves = Vec::with_capacity(toolpath.moves.len());
    let mut prev_axis = [0.0, 0.0, 1.0];

    for (source_index, mv) in toolpath.moves.iter().enumerate() {
        // PathMove represents straight segments only. Never discard CL arc geometry.
        if mv.arc.is_some() {
            return Err(KinematicsError::UnsupportedArc { source_index });
        }
        let axis = mv.axis.unwrap_or([0.0, 0.0, 1.0]);
        let from_tip = [mv.from.x, mv.from.y, mv.from.z];
        let to_tip = [mv.to.x, mv.to.y, mv.to.z];
        let xy_delta = (to_tip[0] - from_tip[0]).hypot(to_tip[1] - from_tip[1]);
        let dz = to_tip[2] - from_tip[2];
        let same_axis = (axis[0] - prev_axis[0]).abs() <= 1e-9
            && (axis[1] - prev_axis[1]).abs() <= 1e-9
            && (axis[2] - prev_axis[2]).abs() <= 1e-9;
        let safe_retract =
            mv.kind == MoveKind::Rapid && dz > 1e-6 && xy_delta <= 1e-6 && same_axis;

        path_moves.push(PathMove {
            kind: mv.kind,
            from_tip,
            to_tip,
            from_axis: axis,
            to_axis: axis,
            feed: mv.feed,
            safe_retract,
        });
        prev_axis = axis;
    }

    solve_path(cfg, &path_moves, opts)
}

#[allow(clippy::too_many_arguments)]
fn emit_continuous_segment(
    solver: &mut AcSolver,
    source_index: usize,
    kind: MoveKind,
    feed: Option<f64>,
    opts: &PathOptions,
    p0: PartPose,
    m0: MachinePose,
    p1: PartPose,
    depth: u8,
    out: &mut Vec<SolvedMove>,
) -> Result<MachinePose, KinematicsError> {
    let mut trial = solver.clone();
    let m1 = trial.solve_pose(p1, SectionBoundary::Continuous)?;

    // When leaving the pole while A travel blocks the smooth -A meridian, re-index C first at fixed pose.
    let at_start_pole = norm(cross(solver.trunnion().c_unit, p0.axis)) <= solver.trunnion().pole_sin_tol;
    if at_start_pole && (m1.c_deg - m0.c_deg).abs() > 1.0 && !same_part_pose(p0, p1) {
        let mut cur_m = emit_pole_c_reorient(
            solver,
            source_index,
            kind,
            feed,
            opts,
            p0,
            m0,
            m1.c_deg,
            out,
        )?;
        cur_m = emit_continuous_segment(
            solver,
            source_index,
            kind,
            feed,
            opts,
            p0,
            cur_m,
            p1,
            depth,
            out,
        )?;
        return Ok(cur_m);
    }

    let needs_split = exceeds_angular_step(m0, m1, opts.max_angular_step_deg)
        || exceeds_chord_tol(solver, p0, m0, p1, m1, opts.chord_tol_mm)?;
    if needs_split && depth >= MAX_SUBDIV_DEPTH {
        return Err(KinematicsError::SubdivisionLimitExceeded {
            source_index,
            stage: "continuous segment",
        });
    }

    if !needs_split {
        let committed = solver.solve_pose(p1, SectionBoundary::Continuous)?;
        out.push(SolvedMove {
            source_index,
            kind,
            feed,
            from: m0,
            to: committed,
            tip_from: p0.tip,
            tip_to: p1.tip,
            axis_from: p0.axis,
            axis_to: p1.axis,
        });
        return Ok(committed);
    }

    let p_mid = PartPose {
        tip: lerp3(p0.tip, p1.tip, 0.5),
        axis: slerp_unit(p0.axis, p1.axis, 0.5),
    };
    let m_mid = emit_continuous_segment(
        solver,
        source_index,
        kind,
        feed,
        opts,
        p0,
        m0,
        p_mid,
        depth + 1,
        out,
    )?;
    emit_continuous_segment(
        solver,
        source_index,
        kind,
        feed,
        opts,
        p_mid,
        m_mid,
        p1,
        depth + 1,
        out,
    )
}

#[allow(clippy::too_many_arguments)]
fn emit_pole_c_reorient(
    solver: &mut AcSolver,
    source_index: usize,
    kind: MoveKind,
    feed: Option<f64>,
    opts: &PathOptions,
    p_pole: PartPose,
    m0: MachinePose,
    target_c_deg: f64,
    out: &mut Vec<SolvedMove>,
) -> Result<MachinePose, KinematicsError> {
    let total_dc = (target_c_deg - m0.c_deg).abs();
    let max_step = opts.max_angular_step_deg.unwrap_or(5.0).min(10.0);
    let mut steps = (total_dc / max_step).ceil().max(1.0) as usize;
    steps = steps.min(4096);

    // Refine step count until fixed-pole C-spin chords stay inside chord_tol_mm.
    loop {
        let dc = (target_c_deg - m0.c_deg) / (steps as f64);
        let mid_c0 = m0.c_deg;
        let mid_c1 = m0.c_deg + dc;
        let (seg0, _) = forward_machine_validated(solver.trunnion(), p_pole, m0.a_deg, mid_c0)?;
        let (seg1, _) = forward_machine_validated(solver.trunnion(), p_pole, m0.a_deg, mid_c1)?;
        let needs_split = exceeds_angular_step(seg0, seg1, opts.max_angular_step_deg)
            || exceeds_chord_tol(solver, p_pole, seg0, p_pole, seg1, opts.chord_tol_mm)?;
        if !needs_split {
            break;
        }
        if steps >= 4096 {
            return Err(KinematicsError::SubdivisionLimitExceeded {
                source_index,
                stage: "pole C reorientation",
            });
        }
        steps = (steps * 2).min(4096);
    }

    let mut cur_m = m0;
    for i in 1..=steps {
        let t = (i as f64) / (steps as f64);
        let c_next = m0.c_deg + (target_c_deg - m0.c_deg) * t;
        let (next_m, _) =
            forward_machine_validated(solver.trunnion(), p_pole, m0.a_deg, c_next)?;
        out.push(SolvedMove {
            source_index,
            kind,
            feed,
            from: cur_m,
            to: next_m,
            tip_from: p_pole.tip,
            tip_to: p_pole.tip,
            axis_from: p_pole.axis,
            axis_to: p_pole.axis,
        });
        cur_m = next_m;
    }

    solver.set_modal_angles(cur_m.a_deg, cur_m.c_deg);
    Ok(cur_m)
}

fn exceeds_angular_step(m0: MachinePose, m1: MachinePose, max_step: Option<f64>) -> bool {
    let Some(limit) = max_step else {
        return false;
    };
    (m1.a_deg - m0.a_deg).abs() > limit + 1e-9 || (m1.c_deg - m0.c_deg).abs() > limit + 1e-9
}

fn exceeds_chord_tol(
    solver: &AcSolver,
    p0: PartPose,
    m0: MachinePose,
    p1: PartPose,
    m1: MachinePose,
    chord_tol_mm: f64,
) -> Result<bool, KinematicsError> {
    let v = solver.trunnion();
    for s in [0.25, 0.5, 0.75] {
        let interp = MachinePose {
            xyz: lerp3(m0.xyz, m1.xyz, s),
            a_deg: m0.a_deg + (m1.a_deg - m0.a_deg) * s,
            c_deg: m0.c_deg + (m1.c_deg - m0.c_deg) * s,
        };
        let actual = forward_part_validated(v, interp)?;
        let desired_tip = lerp3(p0.tip, p1.tip, s);
        if norm(sub(actual.tip, desired_tip)) > chord_tol_mm {
            return Ok(true);
        }
    }
    Ok(false)
}

#[inline]
fn same_part_pose(a: PartPose, b: PartPose) -> bool {
    norm(sub(a.tip, b.tip)) <= POSE_MATCH_POS_TOL_MM && dot(a.axis, b.axis) >= POSE_MATCH_AXIS_DOT
}

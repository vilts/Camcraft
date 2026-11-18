// Provenance: clean-room RS-274D inverse-time feed (G93) solver from first-principles kinematics.

//! Shared `G93` inverse-time feed (`F = 1 / dt_min`, `min^-1`) solver.
//!
//! Used across multi-axis kinematics validation and post processors so linear,
//! rotary, and simultaneous 5-axis feed timing stays consistent with machine
//! `max_normal_speed` ceilings while leaving part-space cutting feed exact.

use crate::cl::MoveKind;
use crate::kinematics::SolvedMove;
use crate::machine::Machine;
use thiserror::Error;

/// Unit system in which caller-supplied [`G93SegmentInput`] linear quantities are expressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinearUnit {
    Mm,
    Inch,
}

impl LinearUnit {
    /// Conversion multiplier from this unit (or `/min`) into mm (or mm/min).
    #[must_use]
    pub const fn to_mm_scale(self) -> f64 {
        match self {
            Self::Mm => 1.0,
            Self::Inch => 25.4,
        }
    }
}

/// Machine normal-feed speed ceilings (`max_normal_speed`) in canonical units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AxisSpeedLimits {
    /// Maximum normal feed speed `[X, Y, Z]` in mm/min (`None` = unstated).
    pub linear_mm_per_min: [Option<f64>; 3],
    /// Maximum normal feed speed `[A, B, C]` in deg/min (`None` = unstated).
    pub rotary_deg_per_min: [Option<f64>; 3],
}

impl AxisSpeedLimits {
    /// Extract `[X, Y, Z]` linear (`mm/min`) and `[A, B, C]` rotary (`deg/min`)
    /// `max_normal_speed` limits from a parsed [`Machine`].
    #[must_use]
    pub fn from_machine(machine: &Machine) -> Self {
        let mut limits = Self::default();
        for axis in &machine.axes {
            let Some(feed) = axis.feed else {
                continue;
            };
            if axis.rotary {
                if axis.name.eq_ignore_ascii_case("A") {
                    limits.rotary_deg_per_min[0] = Some(feed);
                } else if axis.name.eq_ignore_ascii_case("B") {
                    limits.rotary_deg_per_min[1] = Some(feed);
                } else if axis.name.eq_ignore_ascii_case("C") {
                    limits.rotary_deg_per_min[2] = Some(feed);
                }
            } else if axis.name.eq_ignore_ascii_case("X") {
                limits.linear_mm_per_min[0] = Some(feed);
            } else if axis.name.eq_ignore_ascii_case("Y") {
                limits.linear_mm_per_min[1] = Some(feed);
            } else if axis.name.eq_ignore_ascii_case("Z") {
                limits.linear_mm_per_min[2] = Some(feed);
            }
        }
        limits
    }
}

/// Solver tolerances, clamp bounds, and pure-rotary fallback policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct G93Options {
    /// Lower finite `F` clamp (`min^-1`). Default `0.001`.
    pub f_min: f64,
    /// Upper finite `F` clamp (`min^-1`). Default `60_000.0`.
    pub f_max: f64,
    /// Linear zero-motion tolerance (mm). Default `1e-9`.
    pub pos_eps_mm: f64,
    /// Rotary zero-motion tolerance (deg). Default `1e-7`.
    pub ang_eps_deg: f64,
    /// Optional fallback speed (`deg/min`) for pure rotary reorientations (`ds_tip == 0`)
    /// when stated machine axis limits are absent.
    pub rotary_only_deg_per_min: Option<f64>,
}

impl Default for G93Options {
    fn default() -> Self {
        Self {
            f_min: 0.001,
            f_max: 60_000.0,
            pos_eps_mm: 1e-9,
            ang_eps_deg: 1e-7,
            rotary_only_deg_per_min: None,
        }
    }
}

/// One segment handed to [`solve_g93_segment`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct G93SegmentInput {
    pub kind: MoveKind,
    /// Part-space start tool tip (mm, or linear units when passed to [`solve_g93_segment_in_units`]).
    pub tip_from: [f64; 3],
    /// Part-space end tool tip.
    pub tip_to: [f64; 3],
    /// Machine-space linear start `[X, Y, Z]` (includes pivot/RTCP slide motion).
    pub machine_xyz_from: [f64; 3],
    /// Machine-space linear end `[X, Y, Z]`.
    pub machine_xyz_to: [f64; 3],
    /// Unwrapped commanded rotary start `[A, B, C]` (deg).
    pub rotary_deg_from: [f64; 3],
    /// Unwrapped commanded rotary end `[A, B, C]` (deg).
    pub rotary_deg_to: [f64; 3],
    /// Programmed cutting feed (`mm/min`, or linear units/min with [`solve_g93_segment_in_units`]).
    pub feed_mm_per_min: Option<f64>,
}

impl G93SegmentInput {
    /// Construct a [`G93SegmentInput`] from an IK [`SolvedMove`].
    #[must_use]
    pub fn from_solved_move(m: &SolvedMove) -> Self {
        Self {
            kind: m.kind,
            tip_from: m.tip_from,
            tip_to: m.tip_to,
            machine_xyz_from: m.from.xyz,
            machine_xyz_to: m.to.xyz,
            rotary_deg_from: [m.from.a_deg, 0.0, m.from.c_deg],
            rotary_deg_to: [m.to.a_deg, 0.0, m.to.c_deg],
            feed_mm_per_min: m.feed,
        }
    }
}

/// Solved `G93` inverse-time breakdown for one non-rapid, non-empty feed block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct G93Timing {
    /// Part-space tip chord length `||tip_1 - tip_0||` (mm).
    pub ds_tip_mm: f64,
    /// Minimum block time requested by programmed tip feed (minutes).
    pub dt_cut_min: f64,
    /// Minimum block time imposed by stated linear/rotary `max_normal_speed` limits (minutes).
    pub dt_lim_min: f64,
    /// Governing block duration `max(dt_cut_min, dt_lim_min)` (minutes).
    pub dt_min: f64,
    /// Final clamped inverse-time word `F = clamp(1 / dt_min, f_min, f_max)` (`min^-1`).
    pub f_inv_min: f64,
}

/// Result of solving one motion segment for `G93` posting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum G93Decision {
    /// Non-rapid segment carrying active motion and a finite inverse-time `F` word.
    Feed(G93Timing),
    /// Rapid move: executed in `G94` (`G0`), never emits a `G93` `F` word.
    Rapid,
    /// Zero-motion duplicate move within linear/rotary tolerances; caller should skip emitting it.
    Skip,
}

/// Refusal reasons when inputs or limits are invalid.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum G93Error {
    #[error("G93 requires finite coordinates and angles (invalid {field})")]
    NonFiniteCoordinate { field: &'static str },
    #[error("G93 feed move requires a finite positive FeedRate, got {0:?}")]
    InvalidFeedRate(Option<f64>),
    #[error("G93 axis {axis} max_normal_speed must be finite and > 0, got {limit}")]
    InvalidAxisLimit { axis: &'static str, limit: f64 },
    #[error("G93 options invalid: {0}")]
    InvalidOptions(&'static str),
    #[error("G93 rotary-only reorientation has no positive FeedRate or stated axis speed limit ({context})")]
    MissingFeedAndLimits { context: &'static str },
}

fn validate_vec3(v: [f64; 3], field: &'static str) -> Result<(), G93Error> {
    if v.iter().all(|x| x.is_finite()) {
        Ok(())
    } else {
        Err(G93Error::NonFiniteCoordinate { field })
    }
}

fn check_axes(names: [&'static str; 3], vals: [Option<f64>; 3]) -> Result<(), G93Error> {
    for (axis, lim) in names.into_iter().zip(vals) {
        if let Some(limit) = lim
            && (!limit.is_finite() || limit <= 0.0)
        {
            return Err(G93Error::InvalidAxisLimit { axis, limit });
        }
    }
    Ok(())
}

fn validate_limits(limits: &AxisSpeedLimits) -> Result<(), G93Error> {
    check_axes(["X", "Y", "Z"], limits.linear_mm_per_min)?;
    check_axes(["A", "B", "C"], limits.rotary_deg_per_min)
}

fn validate_options(opts: &G93Options) -> Result<(), G93Error> {
    if !opts.f_min.is_finite() || opts.f_min <= 0.0 {
        return Err(G93Error::InvalidOptions("f_min must be finite and > 0"));
    }
    if !opts.f_max.is_finite() || opts.f_max < opts.f_min {
        return Err(G93Error::InvalidOptions("f_max must be finite and >= f_min"));
    }
    if !opts.pos_eps_mm.is_finite() || opts.pos_eps_mm < 0.0 {
        return Err(G93Error::InvalidOptions("pos_eps_mm must be finite and >= 0"));
    }
    if !opts.ang_eps_deg.is_finite() || opts.ang_eps_deg < 0.0 {
        return Err(G93Error::InvalidOptions("ang_eps_deg must be finite and >= 0"));
    }
    if let Some(r) = opts.rotary_only_deg_per_min
        && (!r.is_finite() || r <= 0.0)
    {
        return Err(G93Error::InvalidOptions(
            "rotary_only_deg_per_min must be finite and > 0 when set",
        ));
    }
    Ok(())
}

/// Solve one segment in canonical mm / mm/min / deg / deg/min units.
pub fn solve_g93_segment(
    seg: &G93SegmentInput,
    limits: &AxisSpeedLimits,
    opts: &G93Options,
) -> Result<G93Decision, G93Error> {
    validate_options(opts)?;
    validate_limits(limits)?;
    validate_vec3(seg.tip_from, "tip_from")?;
    validate_vec3(seg.tip_to, "tip_to")?;
    validate_vec3(seg.machine_xyz_from, "machine_xyz_from")?;
    validate_vec3(seg.machine_xyz_to, "machine_xyz_to")?;
    validate_vec3(seg.rotary_deg_from, "rotary_deg_from")?;
    validate_vec3(seg.rotary_deg_to, "rotary_deg_to")?;

    if seg.kind == MoveKind::Rapid {
        return Ok(G93Decision::Rapid);
    }

    let dx_t = seg.tip_to[0] - seg.tip_from[0];
    let dy_t = seg.tip_to[1] - seg.tip_from[1];
    let dz_t = seg.tip_to[2] - seg.tip_from[2];
    let ds_tip = (dx_t * dx_t + dy_t * dy_t + dz_t * dz_t).sqrt();

    let d_xyz = [
        (seg.machine_xyz_to[0] - seg.machine_xyz_from[0]).abs(),
        (seg.machine_xyz_to[1] - seg.machine_xyz_from[1]).abs(),
        (seg.machine_xyz_to[2] - seg.machine_xyz_from[2]).abs(),
    ];
    let d_rot = [
        (seg.rotary_deg_to[0] - seg.rotary_deg_from[0]).abs(),
        (seg.rotary_deg_to[1] - seg.rotary_deg_from[1]).abs(),
        (seg.rotary_deg_to[2] - seg.rotary_deg_from[2]).abs(),
    ];
    let max_d_xyz = d_xyz.into_iter().fold(0.0_f64, f64::max);
    let max_d_rot = d_rot.into_iter().fold(0.0_f64, f64::max);

    if ds_tip <= opts.pos_eps_mm && max_d_xyz <= opts.pos_eps_mm && max_d_rot <= opts.ang_eps_deg {
        return Ok(G93Decision::Skip);
    }

    let mut dt_lim = 0.0_f64;
    for i in 0..3 {
        if d_xyz[i] > opts.pos_eps_mm
            && let Some(v_max) = limits.linear_mm_per_min[i]
        {
            dt_lim = dt_lim.max(d_xyz[i] / v_max);
        }
        if d_rot[i] > opts.ang_eps_deg
            && let Some(w_max) = limits.rotary_deg_per_min[i]
        {
            dt_lim = dt_lim.max(d_rot[i] / w_max);
        }
    }

    let dt_cut = if ds_tip > opts.pos_eps_mm {
        let Some(v_prog) = seg.feed_mm_per_min.filter(|v| v.is_finite() && *v > 0.0) else {
            return Err(G93Error::InvalidFeedRate(seg.feed_mm_per_min));
        };
        ds_tip / v_prog
    } else if max_d_rot > opts.ang_eps_deg {
        if let Some(rot_rate) = opts.rotary_only_deg_per_min {
            max_d_rot / rot_rate
        } else if dt_lim > 0.0 {
            0.0
        } else if let Some(v_prog) = seg.feed_mm_per_min.filter(|v| v.is_finite() && *v > 0.0) {
            max_d_rot / v_prog
        } else {
            return Err(G93Error::MissingFeedAndLimits {
                context: "pure rotary motion with no FeedRate or rotary max_normal_speed",
            });
        }
    } else if let Some(v_prog) = seg.feed_mm_per_min.filter(|v| v.is_finite() && *v > 0.0) {
        max_d_xyz / v_prog
    } else if dt_lim > 0.0 {
        0.0
    } else {
        return Err(G93Error::InvalidFeedRate(seg.feed_mm_per_min));
    };

    let dt_min = dt_cut.max(dt_lim);
    let f_raw = if dt_min > 0.0 { 1.0 / dt_min } else { opts.f_max };
    let f_inv_min = if f_raw.is_finite() {
        f_raw.clamp(opts.f_min, opts.f_max)
    } else {
        opts.f_max
    };

    Ok(G93Decision::Feed(G93Timing {
        ds_tip_mm: ds_tip,
        dt_cut_min: dt_cut,
        dt_lim_min: dt_lim,
        dt_min,
        f_inv_min,
    }))
}

/// Solve a segment expressed in `unit` (`Mm` for G21, `Inch` for G20), proving unit invariance.
pub fn solve_g93_segment_in_units(
    unit: LinearUnit,
    seg: &G93SegmentInput,
    limits: &AxisSpeedLimits,
    opts: &G93Options,
) -> Result<G93Decision, G93Error> {
    let scale = unit.to_mm_scale();
    let scaled_seg = G93SegmentInput {
        kind: seg.kind,
        tip_from: seg.tip_from.map(|v| v * scale),
        tip_to: seg.tip_to.map(|v| v * scale),
        machine_xyz_from: seg.machine_xyz_from.map(|v| v * scale),
        machine_xyz_to: seg.machine_xyz_to.map(|v| v * scale),
        rotary_deg_from: seg.rotary_deg_from,
        rotary_deg_to: seg.rotary_deg_to,
        feed_mm_per_min: seg.feed_mm_per_min.map(|v| v * scale),
    };
    let scaled_limits = AxisSpeedLimits {
        linear_mm_per_min: limits.linear_mm_per_min.map(|opt| opt.map(|v| v * scale)),
        rotary_deg_per_min: limits.rotary_deg_per_min,
    };
    solve_g93_segment(&scaled_seg, &scaled_limits, opts)
}

/// Solve an entire sequence of IK [`SolvedMove`]s into `(source_index, G93Decision)` pairs.
pub fn solve_g93_solved_moves(
    moves: &[SolvedMove],
    limits: &AxisSpeedLimits,
    opts: &G93Options,
) -> Result<Vec<(usize, G93Decision)>, G93Error> {
    let mut out = Vec::with_capacity(moves.len());
    for m in moves {
        let seg = G93SegmentInput::from_solved_move(m);
        let decision = solve_g93_segment(&seg, limits, opts)?;
        out.push((m.source_index, decision));
    }
    Ok(out)
}

/// Format an `F` inverse-time word by flooring to 4 decimal places (`min^-1`)
/// so rounding never shortens `dt_min` above a machine speed limit.
#[must_use]
pub fn format_g93_feed_word(f_inv_min: f64) -> String {
    if !f_inv_min.is_finite() || f_inv_min <= 0.0 {
        return "0".to_string();
    }
    let scaled = (f_inv_min * 10_000.0 + 1e-12).floor() / 10_000.0;
    let safe = if scaled <= 0.0 { 0.0001 } else { scaled };
    let mut s = format!("{safe:.4}");
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

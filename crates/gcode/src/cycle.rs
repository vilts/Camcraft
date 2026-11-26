//! Canned cycle modal state, sticky words and validation before expansion.
//! Provenance: canned cycle rules from RS-274/NGC (NISTIR 6556 §3.5.16) and the LinuxCNC G-code reference (common cycle info: sticky R/Z, clear Z, G98/G99, L repeats).
//!
//! State rule (#3657 review 6): a cycle stays active over contiguous cycle blocks, also
//! when the same or another cycle code is written again. Only a non-cycle motion
//! code, G80, M6 or a program end ends it. R and Z (stored as absolute levels) and
//! the initial level of G98 come from the first block of the series. Q and P carry
//! over between cycle codes that use them.

use camcraft_cl::cl::{ArcPlane, PathBuilder};
use camcraft_geometry::geometry::Point3;

use crate::block::DecodedBlock;
use crate::block_codes::check_unused_words;
use crate::cycle_expand::{expand_hole, moves_per_hole, peck_count, Hole};
use crate::domain::{check_len, check_point};
use crate::interp::Interpreter;
use crate::motion::dwell_to_seconds;
use crate::types::{CycleReturnMode, CycleType, DistanceMode, GcodeError};

/// Most moves that one cycle block (all holes of its `L` repeats) can emit.
pub const MAX_BLOCK_MOVES: usize = 100_000;

/// Sticky parameters for an active canned-cycle modal group.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActiveCycle {
    pub kind: CycleType,
    pub initial_z: f64,
    pub r_z: f64,
    pub bottom_z: f64,
    pub peck_mm: f64,
    pub dwell_s: f64,
}

/// Parameters parsed from one canned-cycle block (in mm and seconds, domain-checked).
#[derive(Clone, Copy, Debug)]
pub struct CycleBlockInput {
    pub line: usize,
    pub kind: CycleType,
    pub distance: DistanceMode,
    pub return_mode: CycleReturnMode,
    pub x_mm: Option<f64>,
    pub y_mm: Option<f64>,
    pub z_mm: Option<f64>,
    pub r_mm: Option<f64>,
    pub q_mm: Option<f64>,
    pub p_sec: Option<f64>,
    pub repeat: u32,
    pub feed_mm_min: f64,
    pub chip_break_retract_mm: f64,
}

fn invalid(line: usize, message: String) -> GcodeError {
    GcodeError::InvalidCycle { line, message }
}

fn is_peck(kind: CycleType) -> bool {
    matches!(kind, CycleType::ChipBreakPeck | CycleType::DeepPeck)
}

fn allows_dwell(kind: CycleType) -> bool {
    matches!(
        kind,
        CycleType::DwellDrill | CycleType::DeepPeck | CycleType::Bore | CycleType::BackBore
    )
}

/// Validate and expand one canned-cycle block (with optional `L` repeats) onto `builder`.
pub fn execute_cycle_block(
    input: CycleBlockInput,
    active: &mut Option<ActiveCycle>,
    cur: &mut Point3,
    builder: &mut PathBuilder,
    move_lines: &mut Vec<usize>,
) -> Result<(), GcodeError> {
    let line = input.line;
    let prev = *active;
    let initial_z = prev.map_or(cur.z, |a| a.initial_z);
    let r_z = match (input.r_mm, input.distance) {
        (Some(r), DistanceMode::Absolute) => r,
        (Some(r), DistanceMode::Incremental) => initial_z + r,
        (None, _) => prev
            .map(|a| a.r_z)
            .ok_or_else(|| invalid(line, format!("canned cycle {:?} requires R retract plane", input.kind)))?,
    };
    let r_z = check_len(line, "cycle R level", r_z)?;
    let bottom_z = match (input.z_mm, input.distance) {
        (Some(z), DistanceMode::Absolute) => z,
        (Some(z), DistanceMode::Incremental) => r_z + z,
        (None, _) => prev
            .map(|a| a.bottom_z)
            .ok_or_else(|| invalid(line, format!("canned cycle {:?} requires Z depth", input.kind)))?,
    };
    let bottom_z = check_len(line, "cycle Z depth", bottom_z)?;
    if bottom_z >= r_z {
        return Err(invalid(line, format!("bottom Z ({bottom_z:.4}) must be below R plane ({r_z:.4})")));
    }

    let peck_mm = resolve_peck(input, prev)?;
    let dwell_s = resolve_dwell(input, prev)?;
    let pecks = if is_peck(input.kind) {
        peck_count(r_z, bottom_z, peck_mm).ok_or_else(|| {
            invalid(line, format!("peck increment Q ({peck_mm}) needs more than 4096 pecks per hole"))
        })?
    } else {
        0
    };
    // Bound the whole expansion of the block before any move is emitted.
    let total = moves_per_hole(input.kind, pecks).saturating_mul(input.repeat as usize);
    if total > MAX_BLOCK_MOVES {
        return Err(invalid(
            line,
            format!("cycle block expands to {total} moves; the limit is {MAX_BLOCK_MOVES}"),
        ));
    }

    *active = Some(ActiveCycle { kind: input.kind, initial_z, r_z, bottom_z, peck_mm, dwell_s });

    let clear_z = match input.return_mode {
        CycleReturnMode::InitialZ => initial_z.max(r_z),
        CycleReturnMode::RPlane => r_z,
    };
    for _ in 0..input.repeat {
        let (hx, hy) = match input.distance {
            DistanceMode::Absolute => (input.x_mm.unwrap_or(cur.x), input.y_mm.unwrap_or(cur.y)),
            DistanceMode::Incremental => (cur.x + input.x_mm.unwrap_or(0.0), cur.y + input.y_mm.unwrap_or(0.0)),
        };
        check_point(line, "hole position", Point3::new(hx, hy, r_z))?;
        let hole = Hole {
            line,
            kind: input.kind,
            hx,
            hy,
            r_z,
            bottom_z,
            clear_z,
            peck_mm,
            dwell_s,
            feed: input.feed_mm_min,
            chip_break_retract_mm: input.chip_break_retract_mm,
        };
        expand_hole(&hole, cur, builder, move_lines)?;
    }
    Ok(())
}

fn resolve_peck(input: CycleBlockInput, prev: Option<ActiveCycle>) -> Result<f64, GcodeError> {
    let line = input.line;
    if !is_peck(input.kind) {
        if input.q_mm.is_some() {
            return Err(invalid(line, format!("Q peck depth is not valid for {:?}", input.kind)));
        }
        return Ok(0.0);
    }
    let q = match input.q_mm {
        Some(v) => v,
        None => prev
            .filter(|a| is_peck(a.kind))
            .map(|a| a.peck_mm)
            .ok_or_else(|| invalid(line, format!("peck cycle {:?} requires positive Q increment", input.kind)))?,
    };
    if !q.is_finite() || q <= 0.0 {
        return Err(invalid(line, format!("peck increment Q ({q}) must be positive")));
    }
    Ok(q)
}

fn resolve_dwell(input: CycleBlockInput, prev: Option<ActiveCycle>) -> Result<f64, GcodeError> {
    let line = input.line;
    if !allows_dwell(input.kind) {
        if input.p_sec.is_some() {
            return Err(invalid(line, format!("P dwell is not valid for {:?}", input.kind)));
        }
        return Ok(0.0);
    }
    let sticky = prev.filter(|a| allows_dwell(a.kind)).map(|a| a.dwell_s);
    match input.p_sec {
        Some(v) => Ok(v),
        None if input.kind == CycleType::DwellDrill => {
            sticky.ok_or_else(|| invalid(line, "G82 dwell cycle requires P dwell word".to_string()))
        }
        None => Ok(sticky.unwrap_or(0.0)),
    }
}

impl<'a> Interpreter<'a> {
    pub(crate) fn execute_cycle(
        &mut self,
        b: &DecodedBlock,
        cycle_type: CycleType,
        allow_h: bool,
    ) -> Result<(), GcodeError> {
        let line = b.line;
        check_unused_words(b, false, true, true, true, allow_h, true)?;
        // Cycle axes are X/Y locations and Z depths: only G17 maps them (#3657 review 4).
        if self.modal.plane != ArcPlane::Xy {
            return Err(GcodeError::UnsupportedCode {
                line,
                code: format!("{cycle_type:?} cycle in G18/G19"),
                reason: "canned cycles run in the G17 (XY) plane only".to_string(),
            });
        }
        let unit = self.require_unit(line)?;
        let feed = self.require_feed(line, "canned cycle")?;
        let repeat = b.l.unwrap_or(1);
        if repeat > 10_000 {
            return Err(invalid(line, format!("cycle repeat count L{repeat} exceeds 10000")));
        }
        let p_sec = match b.p {
            Some(p) => Some(dwell_to_seconds(line, p, b.p_has_point, self.options.dialect.cycle_dwell_unit)?),
            None => None,
        };
        let input = CycleBlockInput {
            line,
            kind: cycle_type,
            distance: self.modal.distance,
            return_mode: self.modal.cycle_return,
            x_mm: self.len_word(line, 'X', b.x, unit)?,
            y_mm: self.len_word(line, 'Y', b.y, unit)?,
            z_mm: self.len_word(line, 'Z', b.z, unit)?,
            r_mm: self.len_word(line, 'R', b.r, unit)?,
            q_mm: self.len_word(line, 'Q', b.q, unit)?,
            p_sec,
            repeat,
            feed_mm_min: feed,
            chip_break_retract_mm: self.options.dialect.chip_break_retract_mm,
        };
        execute_cycle_block(
            input,
            &mut self.active_cycle,
            &mut self.pos,
            &mut self.current.builder,
            &mut self.current.move_lines,
        )
    }
}

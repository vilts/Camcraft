//! Block-level representation and word-conflict validation for parsed NC lines.
//! Provenance: block modal-group container and duplicate-word checks from RS-274/NGC (NISTIR 6556 §3.6).

use camcraft_cl::cl::ArcPlane;
use camcraft_geometry::geometry::units::Unit;

use crate::block_codes::{apply_g, apply_m};
use crate::lexer::{ParsedLine, Word};
use crate::types::{
    ArcCenterMode, CycleReturnMode, CycleType, DistanceMode, FeedMode, GcodeError, SpindleDir,
    StopKind, WorkOffset,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonModalG {
    Dwell,
    HomeG28,
    SecondaryG30,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group1G {
    Rapid,
    Linear,
    ArcCw,
    ArcCcw,
    Cancel,
    Cycle(CycleType),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthOp {
    Apply,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathControlG {
    ExactStop,
    Continuous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopOrEnd {
    Stop(StopKind),
    EndM2,
    EndM30,
}

#[derive(Clone, Debug, Default)]
pub struct DecodedBlock {
    pub line: usize,
    pub is_percent: bool,
    pub program_number: Option<u32>,
    pub non_modal: Option<NonModalG>,
    pub g53: bool,
    pub group1: Option<Group1G>,
    pub plane: Option<ArcPlane>,
    pub distance: Option<DistanceMode>,
    pub arc_center: Option<ArcCenterMode>,
    pub feed_mode: Option<FeedMode>,
    pub units: Option<Unit>,
    pub comp_off: bool,
    pub length_op: Option<LengthOp>,
    pub work_offset: Option<WorkOffset>,
    pub cycle_return: Option<CycleReturnMode>,
    pub path_control: Option<PathControlG>,
    pub spindle_dir: Option<SpindleDir>,
    pub tool_change: bool,
    pub stop_or_end: Option<StopOrEnd>,
    pub mist_on: bool,
    pub flood_on: bool,
    pub coolant_off: bool,
    pub tsc: Option<bool>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
    pub i: Option<f64>,
    pub j: Option<f64>,
    pub k: Option<f64>,
    pub r: Option<f64>,
    pub p: Option<f64>,
    /// The `P` word was written with a decimal point (`P10.`): some controllers
    /// (Haas) read it as seconds, and without the point as milliseconds.
    pub p_has_point: bool,
    pub q: Option<f64>,
    pub f: Option<f64>,
    pub s: Option<f64>,
    pub t: Option<u32>,
    pub h: Option<u32>,
    pub l: Option<u32>,
}

impl DecodedBlock {
    #[must_use]
    pub fn has_executable_words(&self) -> bool {
        self.non_modal.is_some()
            || self.g53
            || self.group1.is_some()
            || self.plane.is_some()
            || self.distance.is_some()
            || self.arc_center.is_some()
            || self.feed_mode.is_some()
            || self.units.is_some()
            || self.comp_off
            || self.length_op.is_some()
            || self.work_offset.is_some()
            || self.cycle_return.is_some()
            || self.path_control.is_some()
            || self.spindle_dir.is_some()
            || self.tool_change
            || self.stop_or_end.is_some()
            || self.mist_on
            || self.flood_on
            || self.coolant_off
            || self.tsc.is_some()
            || self.has_any_param_or_axis()
    }

    #[must_use]
    pub fn has_any_param_or_axis(&self) -> bool {
        self.x.is_some()
            || self.y.is_some()
            || self.z.is_some()
            || self.i.is_some()
            || self.j.is_some()
            || self.k.is_some()
            || self.r.is_some()
            || self.p.is_some()
            || self.q.is_some()
            || self.f.is_some()
            || self.s.is_some()
            || self.t.is_some()
            || self.h.is_some()
            || self.l.is_some()
    }
}

pub fn decode_block(parsed: &ParsedLine) -> Result<DecodedBlock, GcodeError> {
    let line = parsed.line;
    let mut b = DecodedBlock {
        line,
        is_percent: parsed.is_percent,
        program_number: parsed.program_number,
        ..DecodedBlock::default()
    };

    for w in &parsed.words {
        match w.letter {
            'G' => apply_g(line, &mut b, w)?,
            'M' => apply_m(line, &mut b, w)?,
            'X' => set_unique(line, 'X', &mut b.x, w.value)?,
            'Y' => set_unique(line, 'Y', &mut b.y, w.value)?,
            'Z' => set_unique(line, 'Z', &mut b.z, w.value)?,
            'I' => set_unique(line, 'I', &mut b.i, w.value)?,
            'J' => set_unique(line, 'J', &mut b.j, w.value)?,
            'K' => set_unique(line, 'K', &mut b.k, w.value)?,
            'R' => set_unique(line, 'R', &mut b.r, w.value)?,
            'P' => {
                set_unique(line, 'P', &mut b.p, w.value)?;
                b.p_has_point = w.raw.contains('.');
            }
            'Q' => set_unique(line, 'Q', &mut b.q, w.value)?,
            'F' => set_unique(line, 'F', &mut b.f, w.value)?,
            'S' => set_unique(line, 'S', &mut b.s, w.value)?,
            'T' => set_unique_int(line, 'T', &mut b.t, w)?,
            'H' => set_unique_int(line, 'H', &mut b.h, w)?,
            'L' => set_unique_int(line, 'L', &mut b.l, w)?,
            'A' | 'B' | 'C' => {
                return Err(GcodeError::UnsupportedCode {
                    line,
                    code: w.raw.clone(),
                    reason: format!(
                        "rotary axis word '{}' is not supported in 3-axis milling mode",
                        w.letter
                    ),
                });
            }
            'D' => {
                return Err(GcodeError::UnsupportedCode {
                    line,
                    code: w.raw.clone(),
                    reason: "cutter compensation register word 'D' is not supported".to_string(),
                });
            }
            _ => {
                return Err(GcodeError::UnknownWord {
                    line,
                    word: w.raw.clone(),
                });
            }
        }
    }

    if b.g53 && b.non_modal.is_some() {
        return Err(GcodeError::InvalidModal {
            line,
            message: "G53 cannot appear on the same block as G4, G28, or G30".to_string(),
        });
    }
    if b.non_modal == Some(NonModalG::Dwell) && b.group1.is_some() {
        return Err(GcodeError::InvalidModal {
            line,
            message: "G4 dwell cannot share a block with a Group 1 motion code".to_string(),
        });
    }
    if b.path_control == Some(PathControlG::Continuous)
        && (b.p.is_some() || b.q.is_some())
        && (b.non_modal == Some(NonModalG::Dwell)
            || matches!(b.group1, Some(Group1G::Cycle(_))))
    {
        return Err(GcodeError::InvalidModal {
            line,
            message: "G64 P/Q cannot share a block with G4 or a canned cycle".to_string(),
        });
    }

    Ok(b)
}

fn set_unique(
    line: usize,
    letter: char,
    slot: &mut Option<f64>,
    val: f64,
) -> Result<(), GcodeError> {
    if slot.is_some() {
        return Err(GcodeError::Syntax {
            line,
            message: format!("duplicate '{letter}' word on block"),
        });
    }
    *slot = Some(val);
    Ok(())
}

fn set_unique_int(
    line: usize,
    letter: char,
    slot: &mut Option<u32>,
    w: &Word,
) -> Result<(), GcodeError> {
    if slot.is_some() {
        return Err(GcodeError::Syntax {
            line,
            message: format!("duplicate '{letter}' word on block"),
        });
    }
    if w.value < 0.0 || w.value.fract() != 0.0 || w.value > u32::MAX as f64 {
        return Err(GcodeError::Syntax {
            line,
            message: format!("word '{letter}' requires a non-negative integer, got '{}'", w.raw),
        });
    }
    *slot = Some(w.value as u32);
    Ok(())
}

pub fn set_group<T>(
    line: usize,
    group_name: &str,
    slot: &mut Option<T>,
    val: T,
) -> Result<(), GcodeError> {
    if slot.is_some() {
        return Err(GcodeError::InvalidModal {
            line,
            message: format!("multiple G-codes from {group_name} on one block"),
        });
    }
    *slot = Some(val);
    Ok(())
}

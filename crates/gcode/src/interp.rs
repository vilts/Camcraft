//! Modal state interpreter that executes decoded RS-274/ISO 6983 blocks into `cl::Toolpath` sections.
//! Provenance: block execution order and modal group semantics from RS-274/NGC (NISTIR 6556 §3.8) and ISO 6983.

use camcraft_geometry::geometry::Point3;
use camcraft_geometry::geometry::units::{from_display, Unit, MM_PER_INCH};

use crate::block::{decode_block, DecodedBlock, PathControlG, StopOrEnd};
use crate::cycle::ActiveCycle;
use crate::domain::{check_feed, check_len, check_options};
use crate::lexer::tokenize;
use crate::section::SectionBuilder;
use crate::types::{
    DwellEvent, GcodeError, LineMap, ModalState, MotionMode, MoveLine,
    ParseOptions, ParsedProgram, ParsedSection, PathControlMode, ProgramEnd, StopEvent,
};

pub(crate) struct Interpreter<'a> {
    pub(crate) options: &'a ParseOptions,
    pub(crate) modal: ModalState,
    pub(crate) pos: Point3,
    pub(crate) active_cycle: Option<ActiveCycle>,
    program_number: Option<u32>,
    pub(crate) sections: Vec<ParsedSection>,
    pub(crate) current: SectionBuilder,
    pub(crate) dwells: Vec<DwellEvent>,
    stops: Vec<StopEvent>,
    ended: Option<ProgramEnd>,
    pub(crate) tools: Vec<u32>,
}

impl<'a> Interpreter<'a> {
    fn new(options: &'a ParseOptions) -> Self {
        let modal = ModalState::from_dialect(&options.dialect);
        let pos = options.dialect.initial_position;
        let current = SectionBuilder::new(0, pos, &modal);
        Self {
            options,
            modal,
            pos,
            active_cycle: None,
            program_number: None,
            sections: Vec::new(),
            current,
            dwells: Vec::new(),
            stops: Vec::new(),
            ended: None,
            tools: Vec::new(),
        }
    }

    fn step_block(&mut self, b: &DecodedBlock) -> Result<(), GcodeError> {
        let line = b.line;
        if self.ended.is_some() {
            if b.is_percent || !b.has_executable_words() {
                return Ok(());
            }
            return Err(GcodeError::Syntax {
                line,
                message: "executable words after program end (M2/M30)".to_string(),
            });
        }
        if b.is_percent {
            return Ok(());
        }
        if let Some(n) = b.program_number {
            if self.program_number.is_none() {
                self.program_number = Some(n);
            }
            return Ok(());
        }
        if !b.has_executable_words() {
            return Ok(());
        }

        self.apply_modal_setup(b)?;
        self.apply_tool_and_aux(b)?;
        self.sync_section();
        self.execute_motion(b)?;

        if let Some(se) = b.stop_or_end {
            let after_move = self.current.builder.moves_len().checked_sub(1);
            match se {
                StopOrEnd::Stop(kind) => self.stops.push(StopEvent {
                    line,
                    kind,
                    section: self.current.index,
                    after_move,
                }),
                StopOrEnd::EndM2 => {
                    self.active_cycle = None;
                    self.modal.motion = MotionMode::None;
                    self.ended = Some(ProgramEnd::M2);
                }
                StopOrEnd::EndM30 => {
                    self.active_cycle = None;
                    self.modal.motion = MotionMode::None;
                    self.ended = Some(ProgramEnd::M30);
                }
            }
        }
        Ok(())
    }

    fn apply_modal_setup(&mut self, b: &DecodedBlock) -> Result<(), GcodeError> {
        let line = b.line;
        if let Some(u) = b.units {
            self.modal.units = Some(u);
        }
        if let Some(fm) = b.feed_mode {
            self.modal.feed_mode = fm;
        }
        if let Some(pl) = b.plane {
            self.guard_cycle_plane(b, pl)?;
            self.modal.plane = pl;
        }
        if let Some(d) = b.distance {
            self.modal.distance = d;
        }
        if let Some(ac) = b.arc_center {
            self.modal.arc_center = ac;
        }
        if let Some(cr) = b.cycle_return {
            self.modal.cycle_return = cr;
        }
        if let Some(w) = b.work_offset {
            self.change_work_offset(line, w)?;
        }
        if let Some(pc) = b.path_control {
            self.modal.path_control = match pc {
                PathControlG::ExactStop => PathControlMode::ExactStop,
                PathControlG::Continuous => {
                    let tol = match b.p {
                        Some(p) => {
                            if !p.is_finite() || p < 0.0 {
                                return Err(GcodeError::InvalidModal {
                                    line,
                                    message: format!("G64 P tolerance ({p}) must be non-negative"),
                                });
                            }
                            let u = self.require_unit(line)?;
                            Some(check_len(line, "G64 P tolerance", from_display(p, u))?)
                        }
                        None => None,
                    };
                    if let Some(q) = b.q {
                        if !q.is_finite() || q < 0.0 {
                            return Err(GcodeError::InvalidModal {
                                line,
                                message: format!("G64 Q tolerance ({q}) must be non-negative"),
                            });
                        }
                        let _ = self.require_unit(line)?;
                    }
                    PathControlMode::Continuous { tolerance_mm: tol }
                }
            };
        }
        if let Some(f) = b.f {
            if !f.is_finite() || f <= 0.0 {
                return Err(GcodeError::InvalidModal {
                    line,
                    message: format!("feed rate F ({f}) must be positive"),
                });
            }
            let u = self.require_unit(line)?;
            self.modal.feed_rate_mm_min = Some(check_feed(line, scale_rate(f, u))?);
        }
        if let Some(s) = b.s {
            if !s.is_finite() || s < 0.0 {
                return Err(GcodeError::InvalidModal {
                    line,
                    message: format!("spindle speed S ({s}) must be non-negative"),
                });
            }
            self.modal.spindle_rpm = Some(s);
        }
        Ok(())
    }

    pub(crate) fn require_unit(&self, line: usize) -> Result<Unit, GcodeError> {
        self.modal.units.ok_or(GcodeError::MissingUnitMode { line })
    }

    pub(crate) fn require_feed(&self, line: usize, what: &str) -> Result<f64, GcodeError> {
        self.modal
            .feed_rate_mm_min
            .filter(|f| f.is_finite() && *f > 0.0)
            .ok_or_else(|| GcodeError::InvalidModal {
                line,
                message: format!("{what} requires a positive active feed rate F"),
            })
    }

    /// True once any move was emitted in the program.
    pub(crate) fn has_moved(&self) -> bool {
        !self.sections.is_empty() || self.current.has_moves()
    }

    fn finish(mut self) -> ParsedProgram {
        // An empty last section is kept only when a dwell or stop names it.
        let idx = self.current.index;
        let named = self.dwells.iter().any(|d| d.section == idx)
            || self.stops.iter().any(|s| s.section == idx);
        if self.current.has_moves() || named {
            self.sections.push(self.current.finish());
        }
        let mut entries = Vec::new();
        for (sec_idx, sec) in self.sections.iter().enumerate() {
            for (move_idx, &line) in sec.move_lines.iter().enumerate() {
                entries.push(MoveLine {
                    section: sec_idx,
                    move_index: move_idx,
                    line,
                });
            }
        }
        ParsedProgram {
            program_number: self.program_number,
            sections: self.sections,
            line_map: LineMap { entries },
            dwells: self.dwells,
            stops: self.stops,
            ended: self.ended,
            tools: self.tools,
            final_modal: self.modal,
        }
    }
}

fn scale_rate(v: f64, unit: Unit) -> f64 {
    match unit {
        Unit::Millimeters => v,
        Unit::Inches => v * MM_PER_INCH,
    }
}

pub fn interpret(program: &str, options: &ParseOptions) -> Result<ParsedProgram, GcodeError> {
    check_options(options)?;
    let lines = tokenize(program)?;
    let mut interp = Interpreter::new(options);
    for parsed in &lines {
        let block = decode_block(parsed)?;
        interp.step_block(&block)?;
    }
    Ok(interp.finish())
}

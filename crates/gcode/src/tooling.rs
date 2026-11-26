//! Tool change, length offset, spindle and coolant words, and the section split at a state change.
//! Provenance: M6/T, G43/G49, M3/M4/M5 and M7/M8/M9 semantics from RS-274/NGC (NISTIR 6556 §3.6.2, §3.7) and ISO 6983.
//!
//! A section is one run of moves with one tool, work offset, length offset, spindle
//! and coolant state (#3657 review 5). A change of any of these after a move of the
//! section closes it at that move boundary, and the next move starts a new section
//! with the new state. Thus each move has the state that was active when it ran.

use camcraft_cl::cl::ArcPlane;

use crate::block::{DecodedBlock, Group1G, LengthOp};
use crate::interp::Interpreter;
use crate::section::SectionBuilder;
use crate::types::{GcodeError, MotionMode};

impl<'a> Interpreter<'a> {
    pub(crate) fn apply_tool_and_aux(&mut self, b: &DecodedBlock) -> Result<(), GcodeError> {
        let line = b.line;
        if let Some(t) = b.t {
            self.modal.pending_tool = Some(t);
        }
        if b.tool_change {
            let new_tool = self.modal.pending_tool.take().ok_or_else(|| GcodeError::InvalidModal {
                line,
                message: "M6 tool change requires a T tool number".to_string(),
            })?;
            if !self.tools.contains(&new_tool) {
                self.tools.push(new_tool);
            }
            self.active_cycle = None;
            if matches!(self.modal.motion, MotionMode::Cycle(_)) {
                self.modal.motion = MotionMode::None;
            }
            self.modal.active_tool = Some(new_tool);
        }
        if let Some(lo) = b.length_op {
            match lo {
                LengthOp::Apply => {
                    let h = b.h.ok_or_else(|| GcodeError::InvalidModal {
                        line,
                        message: "G43 requires an H length-offset number".to_string(),
                    })?;
                    self.modal.length_offset = Some(h);
                }
                LengthOp::Cancel => {
                    if b.h.is_some() {
                        return Err(GcodeError::InvalidModal {
                            line,
                            message: "H word is invalid with G49".to_string(),
                        });
                    }
                    self.modal.length_offset = None;
                }
            }
        } else if b.h.is_some() {
            return Err(GcodeError::InvalidModal {
                line,
                message: "word 'H' requires G43 on the same block".to_string(),
            });
        }
        if let Some(dir) = b.spindle_dir {
            self.modal.spindle_dir = dir;
        }
        if b.mist_on {
            self.modal.coolant.mist = true;
        }
        if b.flood_on {
            self.modal.coolant.flood = true;
        }
        if b.coolant_off {
            self.modal.coolant.flood = false;
            self.modal.coolant.mist = false;
        }
        if let Some(tsc) = b.tsc {
            self.modal.coolant.through_spindle = tsc;
        }
        Ok(())
    }

    /// Bring the open section in line with the modal state before the block's motion.
    /// An empty section takes the state (and the cursor) as its start. A section with
    /// moves and a different state is closed, and a new one starts at the cursor.
    pub(crate) fn sync_section(&mut self) {
        if !self.current.has_moves() {
            self.current = SectionBuilder::new(self.current.index, self.pos, &self.modal);
            return;
        }
        if self.current.state_matches(&self.modal) {
            return;
        }
        let next_idx = self.sections.len() + 1;
        let prev = std::mem::replace(
            &mut self.current,
            SectionBuilder::new(next_idx, self.pos, &self.modal),
        );
        self.sections.push(prev.finish());
    }

    /// Canned cycles run only in G17 (#3657 review 4). Refuse a G18/G19 change while a
    /// cycle stays active after this block, and a cycle code in the same block.
    pub(crate) fn guard_cycle_plane(&self, b: &DecodedBlock, plane: ArcPlane) -> Result<(), GcodeError> {
        if plane == ArcPlane::Xy {
            return Ok(());
        }
        let cycle_after = match b.group1 {
            Some(Group1G::Cycle(_)) => true,
            Some(_) => false,
            None => matches!(self.modal.motion, MotionMode::Cycle(_)),
        };
        if cycle_after {
            return Err(GcodeError::UnsupportedCode {
                line: b.line,
                code: if plane == ArcPlane::Xz { "G18" } else { "G19" }.to_string(),
                reason: "a plane change while a canned cycle is active is not supported (cycles run in G17 only)"
                    .to_string(),
            });
        }
        Ok(())
    }
}

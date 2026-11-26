//! Coordinate frames: the work-offset change, `G28`/`G30` reference returns, `G53` machine moves and target points.
//! Provenance: work and machine coordinate systems from RS-274/NGC (NISTIR 6556 §3.2.2, §3.5.8) and the LinuxCNC G-code reference (G28, G30, G53, G54-G59).
//!
//! The frame rule (#3657 review 3): the cursor `pos` is always in the frame of the
//! active work offset. A machine point `m` is `m - origin(wcs)` in that frame. A
//! work-offset change after the first move moves the cursor into the new frame
//! (`pos + origin(old) - origin(new)`), and both origins must be known for it. Before
//! the first move, the cursor is the caller's start point, stated in the frame of
//! the first motion, so a change there keeps it.

use camcraft_cl::cl::MoveKind;
use camcraft_geometry::geometry::Point3;
use camcraft_geometry::geometry::units::{from_display, Unit};

use crate::block::DecodedBlock;
use crate::domain::{check_len, check_point};
use crate::interp::Interpreter;
use crate::types::{DistanceMode, GcodeError, MotionMode, WorkOffset};

impl<'a> Interpreter<'a> {
    /// Convert one length word to mm and refuse it outside the numeric domain.
    pub(crate) fn len_word(
        &self,
        line: usize,
        letter: char,
        v: Option<f64>,
        unit: Unit,
    ) -> Result<Option<f64>, GcodeError> {
        v.map(|v| check_len(line, &format!("word {letter}"), from_display(v, unit)))
            .transpose()
    }

    /// The end point of a linear block in the active work frame.
    pub(crate) fn target_point(&self, b: &DecodedBlock, unit: Unit) -> Result<Point3, GcodeError> {
        let line = b.line;
        let x = self.len_word(line, 'X', b.x, unit)?;
        let y = self.len_word(line, 'Y', b.y, unit)?;
        let z = self.len_word(line, 'Z', b.z, unit)?;
        let p = match self.modal.distance {
            DistanceMode::Absolute => Point3::new(
                x.unwrap_or(self.pos.x),
                y.unwrap_or(self.pos.y),
                z.unwrap_or(self.pos.z),
            ),
            DistanceMode::Incremental => Point3::new(
                self.pos.x + x.unwrap_or(0.0),
                self.pos.y + y.unwrap_or(0.0),
                self.pos.z + z.unwrap_or(0.0),
            ),
        };
        check_point(line, "target point", p)
    }

    /// Apply a work-offset word: move the cursor into the new frame by the frame rule.
    pub(crate) fn change_work_offset(&mut self, line: usize, next: WorkOffset) -> Result<(), GcodeError> {
        let prev = self.modal.work_offset;
        if next == prev {
            return Ok(());
        }
        if self.has_moved() {
            let refs = &self.options.references;
            let (Some(o_prev), Some(o_next)) = (refs.wcs_origin(prev), refs.wcs_origin(next)) else {
                return Err(GcodeError::MissingReferencePosition {
                    line,
                    code: format!("G{} after motion in G{}", next.g_code(), prev.g_code()),
                });
            };
            let moved = Point3::new(
                self.pos.x + o_prev.x - o_next.x,
                self.pos.y + o_prev.y - o_next.y,
                self.pos.z + o_prev.z - o_next.z,
            );
            self.pos = check_point(line, "cursor in the new work offset", moved)?;
        }
        self.modal.work_offset = next;
        Ok(())
    }

    /// Machine origin of the active work offset, or a refusal naming `code`.
    fn active_origin(&self, line: usize, code: &str) -> Result<Point3, GcodeError> {
        self.options
            .references
            .wcs_origin(self.modal.work_offset)
            .ok_or_else(|| GcodeError::MissingReferencePosition {
                line,
                code: format!("{code} in G{}", self.modal.work_offset.g_code()),
            })
    }

    pub(crate) fn execute_reference_return(&mut self, b: &DecodedBlock, is_g28: bool) -> Result<(), GcodeError> {
        let line = b.line;
        let code = if is_g28 { "G28" } else { "G30" };
        let unit = self.require_unit(line)?;
        let refs = &self.options.references;
        let ref_pt = if is_g28 { refs.g28 } else { refs.g30 }.ok_or_else(|| {
            GcodeError::MissingReferencePosition {
                line,
                code: code.to_string(),
            }
        })?;
        // The reference point is in machine coordinates: its work-frame point needs
        // the machine origin of the active offset, never an assumed zero.
        let org = self.active_origin(line, code)?;
        let ref_wcs = check_point(
            line,
            "reference point in the work offset",
            Point3::new(ref_pt.x - org.x, ref_pt.y - org.y, ref_pt.z - org.z),
        )?;
        if b.x.is_some() || b.y.is_some() || b.z.is_some() {
            let inter = self.target_point(b, unit)?;
            self.current.push_move(line, MoveKind::Rapid, inter, None);
            self.pos = inter;
            let final_pt = Point3::new(
                if b.x.is_some() { ref_wcs.x } else { self.pos.x },
                if b.y.is_some() { ref_wcs.y } else { self.pos.y },
                if b.z.is_some() { ref_wcs.z } else { self.pos.z },
            );
            self.current.push_move(line, MoveKind::Rapid, final_pt, None);
            self.pos = final_pt;
        } else {
            self.current.push_move(line, MoveKind::Rapid, ref_wcs, None);
            self.pos = ref_wcs;
        }
        Ok(())
    }

    pub(crate) fn execute_g53(&mut self, b: &DecodedBlock) -> Result<(), GcodeError> {
        let line = b.line;
        let unit = self.require_unit(line)?;
        if self.modal.distance != DistanceMode::Absolute {
            return Err(GcodeError::InvalidModal {
                line,
                message: "G53 machine-coordinate move requires G90 absolute distance mode".to_string(),
            });
        }
        let org = self.active_origin(line, "G53")?;
        let (kind, feed) = match self.modal.motion {
            MotionMode::Rapid => (MoveKind::Rapid, None),
            MotionMode::Linear => (MoveKind::Feed, Some(self.require_feed(line, "G53 G1")?)),
            _ => {
                return Err(GcodeError::InvalidModal {
                    line,
                    message: "G53 is valid only with G0 or G1 motion mode".to_string(),
                });
            }
        };
        let x = self.len_word(line, 'X', b.x, unit)?;
        let y = self.len_word(line, 'Y', b.y, unit)?;
        let z = self.len_word(line, 'Z', b.z, unit)?;
        let to = check_point(
            line,
            "G53 target in the work offset",
            Point3::new(
                x.map_or(self.pos.x, |v| v - org.x),
                y.map_or(self.pos.y, |v| v - org.y),
                z.map_or(self.pos.z, |v| v - org.z),
            ),
        )?;
        self.current.push_move(line, kind, to, feed);
        self.pos = to;
        Ok(())
    }
}

//! Toolpath section accumulation and aligned 1-based move-to-line tracking.
//! Provenance: binds RS-274 tool-change and modal-state section splitting to CamCraft's `cl::PathBuilder` and `cl::Toolpath` contract.

use camcraft_cl::cl::{ArcPlane, MoveKind, PathBuilder};
use camcraft_geometry::geo2d::Point2;
use camcraft_geometry::geometry::Point3;

use crate::types::{CoolantState, ModalState, ParsedSection, SpindleDir, WorkOffset};

pub struct SectionBuilder {
    pub index: usize,
    pub tool_number: Option<u32>,
    pub work_offset: WorkOffset,
    pub length_offset: Option<u32>,
    pub spindle_rpm: Option<f64>,
    pub spindle_dir: SpindleDir,
    pub coolant: CoolantState,
    pub builder: PathBuilder,
    pub move_lines: Vec<usize>,
}

impl SectionBuilder {
    pub fn new(index: usize, start: Point3, modal: &ModalState) -> Self {
        Self {
            index,
            tool_number: modal.active_tool,
            work_offset: modal.work_offset,
            length_offset: modal.length_offset,
            spindle_rpm: modal.spindle_rpm,
            spindle_dir: modal.spindle_dir,
            coolant: modal.coolant,
            builder: PathBuilder::new(start),
            move_lines: Vec::new(),
        }
    }

    pub fn has_moves(&self) -> bool {
        self.builder.moves_len() > 0
    }

    /// True when the section's tool, offsets, spindle and coolant equal `modal`.
    pub fn state_matches(&self, modal: &ModalState) -> bool {
        self.tool_number == modal.active_tool
            && self.work_offset == modal.work_offset
            && self.length_offset == modal.length_offset
            && self.spindle_rpm == modal.spindle_rpm
            && self.spindle_dir == modal.spindle_dir
            && self.coolant == modal.coolant
    }

    pub fn push_move(&mut self, line: usize, kind: MoveKind, to: Point3, feed: Option<f64>) {
        let before = self.builder.moves_len();
        self.builder.push(kind, to, feed);
        let after = self.builder.moves_len();
        for _ in before..after {
            self.move_lines.push(line);
        }
    }

    pub fn push_arc(
        &mut self,
        line: usize,
        to: Point3,
        feed: f64,
        center: Point2,
        ccw: bool,
        plane: ArcPlane,
    ) {
        let before = self.builder.moves_len();
        self.builder
            .push_arc_in(MoveKind::Feed, to, Some(feed), center, ccw, plane);
        let after = self.builder.moves_len();
        for _ in before..after {
            self.move_lines.push(line);
        }
    }

    pub fn finish(self) -> ParsedSection {
        ParsedSection {
            index: self.index,
            tool_number: self.tool_number,
            work_offset: self.work_offset,
            length_offset: self.length_offset,
            spindle_rpm: self.spindle_rpm,
            spindle_dir: self.spindle_dir,
            coolant: self.coolant,
            toolpath: self.builder.finish(),
            move_lines: self.move_lines,
        }
    }
}

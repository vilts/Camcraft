//! G-code and M-code modal-group decoding and parameter-word checks.
//! Provenance: G/M code tables and unused parameter checks from RS-274/NGC (NISTIR 6556 Tables 4-7) and ISO 6983.

use camcraft_cl::cl::ArcPlane;
use camcraft_geometry::geometry::units::Unit;

use crate::block::{set_group, DecodedBlock, Group1G, LengthOp, NonModalG, PathControlG, StopOrEnd};
use crate::lexer::Word;
use crate::types::{
    ArcCenterMode, CycleReturnMode, CycleType, DistanceMode, FeedMode, GcodeError, SpindleDir,
    StopKind, WorkOffset,
};

pub fn apply_g(line: usize, b: &mut DecodedBlock, w: &Word) -> Result<(), GcodeError> {
    let Some(c) = &w.code else {
        return Err(GcodeError::UnknownWord {
            line,
            word: w.raw.clone(),
        });
    };
    let name = c.label('G');
    match (c.major, c.minor) {
        (0, None) => set_group(line, "Group 1 (motion)", &mut b.group1, Group1G::Rapid),
        (1, None) => set_group(line, "Group 1 (motion)", &mut b.group1, Group1G::Linear),
        (2, None) => set_group(line, "Group 1 (motion)", &mut b.group1, Group1G::ArcCw),
        (3, None) => set_group(line, "Group 1 (motion)", &mut b.group1, Group1G::ArcCcw),
        (4, None) => set_group(line, "Group 0 (non-modal)", &mut b.non_modal, NonModalG::Dwell),
        (17, None) => set_group(line, "Group 2 (plane)", &mut b.plane, ArcPlane::Xy),
        (18, None) => set_group(line, "Group 2 (plane)", &mut b.plane, ArcPlane::Xz),
        (19, None) => set_group(line, "Group 2 (plane)", &mut b.plane, ArcPlane::Yz),
        (20, None) => set_group(line, "Group 6 (units)", &mut b.units, Unit::Inches),
        (21, None) => set_group(line, "Group 6 (units)", &mut b.units, Unit::Millimeters),
        (28, None) => set_group(line, "Group 0 (non-modal)", &mut b.non_modal, NonModalG::HomeG28),
        (30, None) => set_group(line, "Group 0 (non-modal)", &mut b.non_modal, NonModalG::SecondaryG30),
        (40, None) => {
            if b.comp_off {
                return Err(GcodeError::InvalidModal {
                    line,
                    message: "duplicate G40 on block".to_string(),
                });
            }
            b.comp_off = true;
            Ok(())
        }
        (43, None) => set_group(line, "Group 8 (length offset)", &mut b.length_op, LengthOp::Apply),
        (49, None) => set_group(line, "Group 8 (length offset)", &mut b.length_op, LengthOp::Cancel),
        (53, None) => {
            if b.g53 {
                return Err(GcodeError::InvalidModal {
                    line,
                    message: "duplicate G53 on block".to_string(),
                });
            }
            b.g53 = true;
            Ok(())
        }
        (54, None) => set_group(line, "Group 12 (work offset)", &mut b.work_offset, WorkOffset::G54),
        (55, None) => set_group(line, "Group 12 (work offset)", &mut b.work_offset, WorkOffset::G55),
        (56, None) => set_group(line, "Group 12 (work offset)", &mut b.work_offset, WorkOffset::G56),
        (57, None) => set_group(line, "Group 12 (work offset)", &mut b.work_offset, WorkOffset::G57),
        (58, None) => set_group(line, "Group 12 (work offset)", &mut b.work_offset, WorkOffset::G58),
        (59, None) => set_group(line, "Group 12 (work offset)", &mut b.work_offset, WorkOffset::G59),
        (61, None) => set_group(line, "Group 13 (path control)", &mut b.path_control, PathControlG::ExactStop),
        (64, None) => set_group(line, "Group 13 (path control)", &mut b.path_control, PathControlG::Continuous),
        (73, None) => set_group(line, "Group 1 (motion)", &mut b.group1, Group1G::Cycle(CycleType::ChipBreakPeck)),
        (80, None) => set_group(line, "Group 1 (motion)", &mut b.group1, Group1G::Cancel),
        (81, None) => set_group(line, "Group 1 (motion)", &mut b.group1, Group1G::Cycle(CycleType::Drill)),
        (82, None) => set_group(line, "Group 1 (motion)", &mut b.group1, Group1G::Cycle(CycleType::DwellDrill)),
        (83, None) => set_group(line, "Group 1 (motion)", &mut b.group1, Group1G::Cycle(CycleType::DeepPeck)),
        (84, None) => set_group(line, "Group 1 (motion)", &mut b.group1, Group1G::Cycle(CycleType::Tap)),
        (85, None) => set_group(line, "Group 1 (motion)", &mut b.group1, Group1G::Cycle(CycleType::Ream)),
        (86, None) => set_group(line, "Group 1 (motion)", &mut b.group1, Group1G::Cycle(CycleType::Bore)),
        (89, None) => set_group(line, "Group 1 (motion)", &mut b.group1, Group1G::Cycle(CycleType::BackBore)),
        (90, None) => set_group(line, "Group 3 (distance)", &mut b.distance, DistanceMode::Absolute),
        (91, None) => set_group(line, "Group 3 (distance)", &mut b.distance, DistanceMode::Incremental),
        (90, Some(1)) => set_group(line, "arc center mode", &mut b.arc_center, ArcCenterMode::Absolute),
        (91, Some(1)) => set_group(line, "arc center mode", &mut b.arc_center, ArcCenterMode::Incremental),
        (94, None) => set_group(line, "Group 5 (feed mode)", &mut b.feed_mode, FeedMode::PerMinute),
        (98, None) => set_group(line, "Group 10 (cycle return)", &mut b.cycle_return, CycleReturnMode::InitialZ),
        (99, None) => set_group(line, "Group 10 (cycle return)", &mut b.cycle_return, CycleReturnMode::RPlane),
        (41 | 42, None) | (41 | 42, Some(1)) => Err(GcodeError::UnsupportedCode {
            line,
            code: name,
            reason: "cutter radius compensation (G41/G42) is not supported".to_string(),
        }),
        (65..=67, None) | (66, Some(1)) => Err(GcodeError::UnsupportedCode {
            line,
            code: name,
            reason: "macro calls (G65/G66/G67) are not supported".to_string(),
        }),
        (31, None) | (38, Some(2..=5)) => Err(GcodeError::UnsupportedCode {
            line,
            code: name,
            reason: "probing moves (G31/G38.2) are not supported".to_string(),
        }),
        (43, Some(4 | 5)) => Err(GcodeError::UnsupportedCode {
            line,
            code: name,
            reason: "RTCP (G43.4) is not supported in 3-axis milling mode".to_string(),
        }),
        (93 | 95, None) => Err(GcodeError::UnsupportedCode {
            line,
            code: name,
            reason: "only G94 feed-per-minute mode is supported".to_string(),
        }),
        _ => Err(GcodeError::UnsupportedCode {
            line,
            code: name,
            reason: "unsupported G-code in 3-axis ISO mode".to_string(),
        }),
    }
}

pub fn apply_m(line: usize, b: &mut DecodedBlock, w: &Word) -> Result<(), GcodeError> {
    let Some(c) = &w.code else {
        return Err(GcodeError::UnknownWord {
            line,
            word: w.raw.clone(),
        });
    };
    let name = c.label('M');
    match c.major {
        0 => set_group(line, "program stop/end", &mut b.stop_or_end, StopOrEnd::Stop(StopKind::ProgramStop)),
        1 => set_group(line, "program stop/end", &mut b.stop_or_end, StopOrEnd::Stop(StopKind::OptionalStop)),
        2 => set_group(line, "program stop/end", &mut b.stop_or_end, StopOrEnd::EndM2),
        30 => set_group(line, "program stop/end", &mut b.stop_or_end, StopOrEnd::EndM30),
        3 => set_group(line, "spindle", &mut b.spindle_dir, SpindleDir::Cw),
        4 => set_group(line, "spindle", &mut b.spindle_dir, SpindleDir::Ccw),
        5 => set_group(line, "spindle", &mut b.spindle_dir, SpindleDir::Off),
        6 => {
            if b.tool_change {
                return Err(GcodeError::InvalidModal {
                    line,
                    message: "duplicate M6 on block".to_string(),
                });
            }
            b.tool_change = true;
            Ok(())
        }
        7 => {
            if b.coolant_off {
                return Err(GcodeError::InvalidModal {
                    line,
                    message: "M7 cannot share a block with M9".to_string(),
                });
            }
            b.mist_on = true;
            Ok(())
        }
        8 => {
            if b.coolant_off {
                return Err(GcodeError::InvalidModal {
                    line,
                    message: "M8 cannot share a block with M9".to_string(),
                });
            }
            b.flood_on = true;
            Ok(())
        }
        9 => {
            if b.mist_on || b.flood_on {
                return Err(GcodeError::InvalidModal {
                    line,
                    message: "M9 cannot share a block with M7 or M8".to_string(),
                });
            }
            b.coolant_off = true;
            Ok(())
        }
        88 => set_group(line, "through-spindle coolant", &mut b.tsc, true)
            .map_err(|_| GcodeError::InvalidModal { line, message: "conflicting M88/M89 on block".into() }),
        89 => set_group(line, "through-spindle coolant", &mut b.tsc, false)
            .map_err(|_| GcodeError::InvalidModal { line, message: "conflicting M88/M89 on block".into() }),
        98 | 99 => Err(GcodeError::UnsupportedCode {
            line,
            code: name,
            reason: "subprogram calls (M98/M99) are not supported".to_string(),
        }),
        _ => Err(GcodeError::UnsupportedCode {
            line,
            code: name,
            reason: "unsupported M-code in 3-axis ISO mode".to_string(),
        }),
    }
}

pub fn check_unused_words(
    b: &DecodedBlock,
    allow_ijk: bool,
    allow_r: bool,
    allow_p: bool,
    allow_q: bool,
    allow_h: bool,
    allow_l: bool,
) -> Result<(), GcodeError> {
    let line = b.line;
    if !allow_ijk {
        for (ch, v) in [('I', b.i), ('J', b.j), ('K', b.k)] {
            if v.is_some() {
                return Err(GcodeError::InvalidModal {
                    line,
                    message: format!("word '{ch}' is only valid in G2/G3 arc blocks"),
                });
            }
        }
    }
    if !allow_r && b.r.is_some() {
        return Err(GcodeError::InvalidModal {
            line,
            message: "word 'R' is only valid in G2/G3 arcs or canned cycles".to_string(),
        });
    }
    if !allow_p && b.p.is_some() {
        return Err(GcodeError::InvalidModal {
            line,
            message: "word 'P' is only valid with G4, G64, or canned cycles".to_string(),
        });
    }
    if !allow_q && b.q.is_some() {
        return Err(GcodeError::InvalidModal {
            line,
            message: "word 'Q' is only valid with G73, G83, or G64".to_string(),
        });
    }
    if !allow_h && b.h.is_some() {
        return Err(GcodeError::InvalidModal {
            line,
            message: "word 'H' requires G43 on the same block".to_string(),
        });
    }
    if !allow_l && b.l.is_some() {
        return Err(GcodeError::InvalidModal {
            line,
            message: "word 'L' is only valid in canned cycle blocks".to_string(),
        });
    }
    Ok(())
}

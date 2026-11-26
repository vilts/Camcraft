//! Section state at the move boundary (#3657 review 5): spindle, coolant, length- and work-offset changes after a move start a new section of the same tool.
//! Provenance: M3/M4/M5, M7/M8/M9, G43/G49 and G54-G59 from RS-274/NGC (NISTIR 6556 §3.6, §3.7).

use camcraft_gcode::{CoolantState, ParseOptions, ReferencePositions, SpindleDir, WorkOffset};
use camcraft_geometry::geometry::Point3;

use crate::parse_in_time;

/// The review input: `S1000 M3` after the first move of tool 1.
#[test]
fn spindle_start_after_the_first_move_starts_a_section() {
    let nc = "G21\nT1 M6\nG0 X1.\nS1000 M3\nG1 X2. F100\n";
    let p = parse_in_time(nc, &ParseOptions::default()).expect("spindle change");
    assert_eq!(p.sections.len(), 2);
    let (a, b) = (&p.sections[0], &p.sections[1]);
    assert_eq!((a.tool_number, a.spindle_dir, a.spindle_rpm), (Some(1), SpindleDir::Off, None));
    assert_eq!((b.tool_number, b.spindle_dir, b.spindle_rpm), (Some(1), SpindleDir::Cw, Some(1000.0)));
    assert_eq!(a.move_lines, vec![3]);
    assert_eq!(b.move_lines, vec![5]);
    assert_eq!(b.toolpath.moves[0].from, a.toolpath.moves[0].to);
}

/// Changes before, between and after the moves of one tool.
#[test]
fn every_state_change_between_moves_is_kept() {
    let mut references = ReferencePositions::default();
    references.wcs_origins[WorkOffset::G54.index()] = Some(Point3::ZERO);
    references.wcs_origins[WorkOffset::G55.index()] = Some(Point3::new(10.0, 0.0, 0.0));
    let opts = ParseOptions { references, ..Default::default() };
    let nc = "\
G21 G90
T1 M6
S2000 M3
G0 X1.
M8
G0 X2.
G43 H1
G0 X3.
G55
G0 X4.
M5
G0 X5.
M9
M5
M30";
    let p = parse_in_time(nc, &opts).expect("state changes");
    // Before the first move: one section has S2000 M3 from its start.
    // Between moves: M8, G43 H1, G55 and M5 each start a section.
    // After the last move: M9 and M5 start no empty section.
    assert_eq!(p.sections.len(), 5);
    assert!(p.sections.iter().all(|s| s.tool_number == Some(1)));
    let flood = CoolantState { flood: true, ..CoolantState::default() };
    let s = &p.sections;
    assert_eq!((s[0].spindle_dir, s[0].spindle_rpm, s[0].coolant), (SpindleDir::Cw, Some(2000.0), CoolantState::default()));
    assert_eq!((s[1].coolant, s[1].length_offset), (flood, None));
    assert_eq!((s[2].length_offset, s[2].work_offset), (Some(1), WorkOffset::G54));
    assert_eq!((s[3].work_offset, s[3].spindle_dir), (WorkOffset::G55, SpindleDir::Cw));
    assert_eq!((s[4].spindle_dir, s[4].coolant, s[4].work_offset), (SpindleDir::Off, flood, WorkOffset::G55));
    let lines: Vec<Vec<usize>> = s.iter().map(|x| x.move_lines.clone()).collect();
    assert_eq!(lines, vec![vec![4], vec![6], vec![8], vec![10], vec![12]]);
    assert_eq!(p.final_modal.spindle_dir, SpindleDir::Off);
    assert_eq!(p.final_modal.coolant, CoolantState::default());
    assert_eq!(p.line_map.len(), 5);
}

#[test]
fn a_repeated_equal_state_word_keeps_one_section() {
    let nc = "G21 G90\nS1000 M3\nG0 X1.\nS1000 M3\nG1 X2. F100 S1000\n";
    let p = parse_in_time(nc, &ParseOptions::default()).expect("same state");
    assert_eq!(p.sections.len(), 1);
    assert_eq!(p.sections[0].toolpath.moves.len(), 2);
}

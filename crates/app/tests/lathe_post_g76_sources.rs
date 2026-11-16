//! The G76 readings that two published sources settle (issue #1842).
//! The Fanuc lathe post states the kernel's laid passes as the two-block
//! G76 cycle; these tests pin the three readings of that cycle which the
//! FANUC Series 0i-F lathe operator's manual (B-64604EN-1, section 4.2.7,
//! Fig. 4.2.7 (a) and (b)) and the Siemens SINUMERIK ISO Turning
//! programming manual (A5E52211627B, "Multiple-thread cutting cycle
//! (G76)", Fig. 4-25 and 4-26) state the same way:
//! 1. the cycle starts at the start point A, and with a tool angle each
//!    deeper pass starts further along the direction of travel, on the
//!    flank line from the crest point B (single-edge infeed);
//! 2. every pass ends at the programmed X/Z end point (D);
//! 3. Q(dmin) clamps the cut of ONE pass - its depth below the pass
//!    before it - never the sqrt(n) depth itself.
//!
//! The fourth reading (how the control rounds dd*sqrt(n) inside the
//! cycle) has no published statement and stays open on #1842. The
//! sources and what was dropped: docs/decisions/1842-g76-schedule-sources.md.
//!
//! Gate map (M4.4): covers camcraft_motion::turning (thread_cycle's
//! record), app::posting + the lathe dialect's G76 words.
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use camcraft_app::turning::{DepthSchedule, InfeedMethod, ThreadInfeed};

use common::g76::{control_depths, g76_words, g76_world, laid, laid_ends, post, same_ladder};

/// A 55-degree flank thread whose minimum depth engages the clamp.
fn flank_area() -> ThreadInfeed {
    ThreadInfeed {
        method: InfeedMethod::Flank,
        flank_angle: 27.5,
        schedule: DepthSchedule::ConstantArea,
        min_depth: 0.1,
        finish_allowance: 0.05,
        spring_passes: 1,
        repeat_each_depth: false,
    }
}

/// The Z word of the first line of `text` that starts with `head` and
/// contains `marker`.
fn z_of(text: &str, head: &str, marker: &str) -> f64 {
    let line = text.lines().find(|l| l.starts_with(head) && l.contains(marker)).expect("the block");
    line.split_whitespace()
        .find_map(|w| w.strip_prefix('Z'))
        .and_then(|z| z.parse::<f64>().ok())
        .unwrap_or_else(|| panic!("Z in {line}"))
}

/// The positioning Z right before the cycle: the start point A.
fn start_z(text: &str) -> f64 {
    let lines: Vec<&str> = text.lines().collect();
    let at = lines.iter().position(|l| l.starts_with("G76")).expect("a G76 block");
    let before = lines.get(at.wrapping_sub(1)).copied().unwrap_or_default();
    assert!(before.starts_with("G00"), "the cycle is not entered from a G00: {before}\n{text}");
    z_of(before, "G00", "Z")
}

#[test]
fn each_pass_starts_on_the_crest_flank_line_along_the_travel() {
    let world = g76_world(0.4, flank_area());
    let (text, refused) = post(&world);
    assert!(refused.is_empty(), "{refused:?}");
    let (_, a, _, _, k, _) = g76_words(&text);
    let tan = (a * 0.5).to_radians().tan();
    let (passes, z0) = laid(&world);
    // A: the posted start point is the record's start, the station where
    // a zero-depth pass on the flank line would start (point B's Z).
    let a_z = start_z(&text);
    assert!((a_z - z0).abs() < 1e-3, "G00 Z{a_z} vs the record start {z0}\n{text}");
    // The travel runs from A toward the posted end Z (-Z here).
    let end_z = z_of(&text, "G76", "F");
    assert!(end_z < a_z, "the cycle runs toward -Z: A {a_z}, end {end_z}");
    // Each deeper pass starts further along that travel, depth x tan(a/2)
    // from A: the leading flank cuts (single-edge infeed).
    for w in passes.windows(2) {
        let ((z1, d1), (z2, d2)) = (w[0], w[1]);
        if d2 > d1 + 1e-9 {
            assert!(z2 < z1, "a deeper pass starts back toward A: {passes:?}");
        }
    }
    let (z_root, _) = passes.last().copied().expect("passes");
    assert!((z_root - (z0 - k * tan)).abs() < 1e-3, "the root pass starts k*tan(a/2) from A: {passes:?}");
    same_ladder(&world, &text);
}

#[test]
fn every_pass_runs_to_the_posted_end_z_inside_the_root_pass() {
    let world = g76_world(0.4, flank_area());
    let (text, refused) = post(&world);
    assert!(refused.is_empty(), "{refused:?}");
    let (_, a, _, _, k, _) = g76_words(&text);
    let tan = (a * 0.5).to_radians().tan();
    let end_z = z_of(&text, "G76", "F");
    let (ends, record_end) = laid_ends(&world);
    let (passes, _) = laid(&world);
    assert_eq!(ends.len(), passes.len(), "one end for each pass: {ends:?}");
    // The posted end point D is the record's end: the root pass's end.
    assert!((end_z - record_end).abs() < 1e-3, "G76 Z{end_z} vs the record end {record_end}");
    let last = ends.last().copied().expect("ends");
    assert!((last - end_z).abs() < 1e-3, "the root pass ends at D: {ends:?}");
    // The control runs EVERY pass to D. The kernel lays a shallower pass
    // short of D by (k - depth) x tan(a/2): the control's extra length is
    // that much, at that depth, inside the groove the root pass cuts to
    // D. No pass ends past D toward the chuck. Kept on purpose (#1896):
    // docs/decisions/1896-thread-pass-ends.md.
    for (end, (_, depth)) in ends.iter().zip(&passes) {
        let short = (k - depth) * tan;
        assert!((end - (end_z + short)).abs() < 1e-3, "pass at {depth}: ends {end}, D {end_z}, short {short}");
        assert!(*end >= end_z - 1e-3, "a pass ends past D: {ends:?}");
    }
}

#[test]
fn the_minimum_depth_clamps_the_cut_of_one_pass() {
    let world = g76_world(0.4, flank_area());
    let (text, refused) = post(&world);
    assert!(refused.is_empty(), "{refused:?}");
    let (m, _, dmin, d, k, dd) = g76_words(&text);
    let (got, _) = laid(&world);
    let depths: Vec<f64> = got.iter().map(|&(_, w)| w).collect();
    let rough = depths.len() - m;
    // The clamp engages on this ladder: a sqrt(n) rung less than dmin
    // below the pass before it.
    let engaged = (2..=rough).any(|n| {
        let prev = depths[n - 2];
        dd * (n as f64).sqrt() < prev + dmin - 1e-9
    });
    assert!(engaged, "the clamp never engages: {depths:?}");
    // Every roughing cut below the pass before it is dmin or more (the
    // last one may stop at k - d).
    for n in 1..rough {
        let cut = depths[n] - depths[n - 1];
        assert!(cut >= dmin - 1e-3 || (depths[n] - (k - d)).abs() < 1e-3, "pass {}: cut {cut} < {dmin}: {depths:?}", n + 1);
    }
    // The other reading - clamp against the UNCLAMPED rung before,
    // dd*sqrt(n-1) + dmin - gives a different ladder, so this pin tells
    // the two apart.
    let mut other = Vec::new();
    for n in 1.. {
        let nf = n as f64;
        let next = (dd * nf.sqrt()).max(if n == 1 { 0.0 } else { dd * (nf - 1.0).sqrt() + dmin });
        if next >= k - d - 1e-9 {
            other.push(k - d);
            break;
        }
        other.push(next);
    }
    let differs = other.len() != rough || other.iter().zip(&depths).any(|(o, w)| (o - w).abs() > 1e-3);
    assert!(differs, "the two readings agree here, the pin is blind: {other:?}");
    let (want, _) = control_depths(&text);
    assert_eq!(want.len(), depths.len(), "{want:?} vs {depths:?}");
}

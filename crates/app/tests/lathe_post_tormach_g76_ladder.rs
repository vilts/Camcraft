//! The Tormach PathPilot (LinuxCNC) LATHE post's G76 pass count (#1619):
//! under R1 (LinuxCNC G76: "R1.0 selects constant depth") the control
//! cuts J, 2J, ... with the last pass clamped at K, so ceil(K/J) passes.
//! The posted grid words must make that count the record's own rung
//! count - the passes the stream laid and the verdict judged - in mm
//! and in inch, or the record is refused by name.
//!
//! Gate map (M4.4): covers app::posting (stream_records, post_setup on
//! the Tormach lathe post), post::post (engine on the PathPilot lathe
//! dialect: the G76 grid words).
//! Lane shape (M4.4): all tests here run in seconds.

mod tormach_lathe_support;

use camcraft_app::post::{cnc, cnc_thread};
use camcraft_app::posting;
use camcraft_app::schema::PostOptions;
use tormach_lathe_support::{g33s, g76, g76_world, g76_words, post, program_with};

fn with_rec(records: &[cnc::Record], f: impl Fn(&mut cnc_thread::ThreadRec)) -> Vec<cnc::Record> {
    records
        .iter()
        .cloned()
        .map(|r| match r {
            cnc::Record::Thread(Some(mut t)) => {
                f(&mut t);
                cnc::Record::Thread(Some(t))
            }
            other => other,
        })
        .collect()
}

/// A length word in whole grid steps (0.0001 in or 0.001 mm).
fn to_steps(v: f64, inch: bool) -> i64 {
    let step = if inch { 0.0001 } else { 0.001 };
    (v / step).round() as i64
}

/// A G76 word as a whole count of the control's grid steps.
fn steps(text: &str, c: char, inch: bool) -> i64 {
    let v = g76_words(text).iter().find(|(l, _)| *l == c).map_or(f64::NAN, |(_, v)| *v);
    to_steps(v, inch)
}

/// The drive-line X word immediately before the G76 block, in grid steps.
fn drive_line_x_steps(text: &str, inch: bool) -> i64 {
    let lines: Vec<&str> = text.lines().collect();
    let at = lines.iter().position(|l| l.starts_with("G76 ")).expect("G76");
    let x = lines[..at]
        .iter()
        .rev()
        .find_map(|l| l.split(' ').find_map(|w| w.strip_prefix('X')))
        .and_then(|v| v.parse::<f64>().ok())
        .expect("the drive-line rapid");
    to_steps(x, inch)
}

/// LinuxCNC's R1 ladder: J, 2J, ... clamped at K - ceil(K/J) passes.
fn r1_passes(text: &str, inch: bool) -> i64 {
    let (j, k) = (steps(text, 'J', inch), steps(text, 'K', inch));
    assert!(j >= 1 && k >= j, "J {j} K {k}: {text}");
    (k + j - 1) / j
}

/// Each R1 pass's `(bottom_x, depth)` on the control's grid (`X + I - min(m*J, K)`).
fn r1_pass_schedule(text: &str, inch: bool) -> Vec<(i64, i64)> {
    let r = g76_words(text).iter().find(|(l, _)| *l == 'R').map_or(f64::NAN, |(_, v)| *v);
    assert!((r - 1.0).abs() < 1e-9, "constant-depth R1 expected: {text}");
    let peak = drive_line_x_steps(text, inch) + steps(text, 'I', inch);
    let (j, k, n) = (steps(text, 'J', inch), steps(text, 'K', inch), r1_passes(text, inch));
    (1..=n)
        .map(|pass| {
            let depth = (pass * j).min(k);
            (peak - depth, depth)
        })
        .collect()
}

#[test]
fn g76_words_cut_exactly_the_records_passes_in_mm_and_inch() {
    for inch in [false, true] {
        let (world, _history) = g76_world(inch);
        // The record's rungs: the passes the stream lays (one G33 each
        // without the cycle) and ceil(height / first depth).
        let laid = g33s(&program_with(&world, PostOptions::default())).len() as i64;
        let records = posting::stream_records(&world).expect("generated");
        let t = records
            .iter()
            .find_map(|r| match r {
                cnc::Record::Thread(Some(t)) => Some(t.clone()),
                _ => None,
            })
            .expect("a thread record");
        let rungs = (t.height / t.first_depth - 1e-9).ceil() as i64;
        assert_eq!((laid, rungs), (3, 3), "the fixture's ladder: 0.4, 0.8, 1.2");
        let text = program_with(&world, g76());
        assert_eq!(r1_passes(&text, inch), rungs, "inch {inch}: the control's ladder is not the record's: {text}");
        // The first cut is never a full grid step heavier than the
        // record's: inch J 0.0315 against the record's 0.031496.
        let j_mm = steps(&text, 'J', inch) as f64 * if inch { 0.00254 } else { 0.001 };
        let step = if inch { 0.00254 } else { 0.001 };
        assert!(j_mm - 2.0 * t.first_depth < step, "J {j_mm} mm a full step over 2 x {}: {text}", t.first_depth);
    }
}

#[test]
fn g76_refuses_by_name_when_no_grid_j_cuts_the_records_passes() {
    let why = "no first depth J on the control's grid";
    // mm: a crest 2 um over the height law (inside the lattice
    // allowance) makes K 2.402; J 0.8 would cut a fourth 0.002 pass,
    // J 0.801 is a full step over the record's first cut. Refused.
    let (world, _history) = g76_world(false);
    let records = posting::stream_records(&world).expect("generated");
    let (text, refused) = post(&world, &with_rec(&records, |t| t.major_diameter += 0.002), g76());
    assert!(refused.iter().any(|w| w.contains("G76") && w.contains(why)), "mm: {refused:?}");
    assert!(!text.contains("G76 P"), "mm: a refused cycle was written: {text}");
    // inch: 0.15 deep in rungs of 0.012 is 13 passes; K is 118 steps
    // and no whole J has 12J < 118 <= 13J. Refused.
    let (world, _history) = g76_world(true);
    let records = posting::stream_records(&world).expect("generated");
    let fine = with_rec(&records, |t| {
        t.height = 0.15;
        t.first_depth = 0.012;
        t.min_depth = 0.012;
        // The thirteenth pass takes the remaining 0.006, so the
        // allowance check must pass before the grid refusal is tested.
        t.finish_allowance = 0.006;
        t.major_diameter = t.minor_diameter + 0.3;
    });
    let (text, refused) = post(&world, &fine, g76());
    assert!(refused.iter().any(|w| w.contains("G76") && w.contains(why)), "inch: {refused:?}");
    assert!(!text.contains("G76 P"), "inch: a refused cycle was written: {text}");
}

#[test]
fn inch_g76_pass_bottoms_match_pathpilot_hub_8l_readings() {
    // PathPilot Hub 8L SIM (PathPilot v2.14.4, 2026-11-18 12:42 PDT)
    // DRO readings at each pass bottom of thread_g76_inch.nc:
    // pass 1: X (DIA) 0.4409, Z 0.5512 -> 0.3150 (depth 0.0315)
    // pass 2: X (DIA) 0.4094, Z 0.5512 -> 0.3150 (depth 0.0630)
    // pass 3: X (DIA) 0.3780, Z 0.5512 -> 0.3150 (depth 0.0944)
    // #2952 moved the start from Z0.5512 (inside the bar) to Z0.6299 in
    // front of the free end face. The pass bottoms (X and depth) come
    // from the X I J K words only, which did not change, so the readings
    // still pin them; the start Z of the readings is the old one.
    let want = [(4409, 315), (4094, 630), (3780, 944)];
    let (world, _history) = g76_world(true);
    let posted = program_with(&world, g76());
    let golden = include_str!("golden/nc/tormach-lathe/thread_g76_inch.nc").replace("\r\n", "\n");
    assert_eq!(r1_pass_schedule(&posted, true), want, "posted R1 schedule must match Hub 8L DRO pass bottoms: {posted}");
    assert_eq!(r1_pass_schedule(&golden, true), want, "golden R1 schedule must match Hub 8L DRO pass bottoms: {golden}");
    assert_eq!(posted.replace("\r\n", "\n"), golden);
    assert!(golden.contains("G0 X0.4764\nG0 Z0.6299\nG76 P0.07874 Z0.315 I-0.004 J0.0315 K0.0944 R1 Q0 H0\nG0 X0.4764 Z0.9449\n"));
}

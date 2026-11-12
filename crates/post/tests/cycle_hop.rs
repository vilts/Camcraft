//! The canned-cycle hop law (#994): a post that writes drilling cycles
//! moves between holes no lower than the stream's hop and leaves the last
//! hole at the stream's retract - never at the R plane the cycle drills
//! from. One generic checker reads the posted program back as a control
//! would (RS-274 / ISO 6983 canned cycles: position over the hole at the
//! current level, rapid to R, drill, return to the initial level under
//! G98 or to R under G99) and is run on every shipped post that writes
//! cycle words, on the recorded `drill_retract` stream (R 1, hop 3,
//! clearance 10) and on the same stream with its cycles anchored on the
//! rapid down to R (a legacy producer's layout, which the posts seat).
//! A program that states no return mode is read the conservative way:
//! the lower of the two levels.
//!
//! #1028 adds the two streams no generator writes but a legacy or foreign
//! one may: two cycle records back to back (no rapid leaves the first
//! hole) and a cycle read straight after probing in its section. Both
//! must seat at the section's hop, and the first is refused by name when
//! the section states no hop. The checker reads probe moves (G31, G38.n,
//! MEAS) as the straight moves they are and G91 back-offs incrementally.
//!
//! Provenance: our own test; canned-cycle semantics from the public
//! RS-274 / ISO 6983 vocabulary and general machine-tool handbook
//! practice, no vendor material.
//!
//! Gate map (M4.4): covers assets/posts/fanuc.cps, haas.cps, mazak.cps,
//! sinumerik.cps, tormach.cps, doosan-dnm.cps (their cycle groups),
//! camcraft_post::post::engine, the post binary.

mod recorded;

use std::process::Command;

/// The posts that write canned-cycle words (Heidenhain and Carvera post
/// the expanded chain, which is the stream's own path).
const CYCLE_POSTS: [&str; 6] = ["fanuc.cps", "haas.cps", "mazak.cps", "sinumerik.cps", "tormach.cps", "doosan-dnm.cps"];

const EPS: f64 = 1e-6;

#[derive(Default)]
struct Read {
    /// Each fault the checker found, in program order.
    faults: Vec<String>,
    /// Holes the program drilled.
    holes: usize,
}

fn word(tok: &str) -> Option<(char, f64)> {
    let mut c = tok.chars();
    let l = c.next()?.to_ascii_uppercase();
    if !l.is_ascii_alphabetic() {
        return None;
    }
    c.as_str().parse::<f64>().ok().map(|v| (l, v))
}

/// Read `program` back and check every XY traverse made after the first
/// hole runs at Z >= `hop`, and that the tool stands at Z >= `exit` when
/// the program leaves the cut (its first home move after the last hole,
/// or its end). The positions are the work frame's; homing (G28 / G30 /
/// G53) puts the tool at the top of Z travel and forgets X Y.
fn check(program: &str, hop: f64, exit: f64) -> Read {
    check_from(program, hop, exit, false)
}

/// [`check`], and with `from_start` every XY traverse made once the
/// tool's position is known (after its first positioned block), not only
/// after the first hole - for a stream whose first hole follows other
/// low motion in its section (a probe, #1028).
fn check_from(program: &str, hop: f64, exit: f64, from_start: bool) -> Read {
    let mut r = Read::default();
    let mut inc = false; // G91 in force
    let (mut x, mut y, mut z): (Option<f64>, Option<f64>, f64) = (None, None, f64::INFINITY);
    let mut motion = 0u32; // 0/1/2/3, or the cycle's number (73, 81..89)
    let mut ret: Option<u32> = None; // 98 / 99, None while unstated
    let mut initial = f64::INFINITY; // the level the open cycle group was entered at
    let (mut cyc_z, mut cyc_r) = (f64::NAN, f64::NAN);
    let mut left_at: Option<f64> = None; // Z when the cut was left
    for (n, raw) in program.lines().enumerate() {
        let line = raw.split(';').next().unwrap_or("");
        let mut text = String::new();
        let mut depth = 0;
        for ch in line.chars() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ if depth == 0 => text.push(ch),
                _ => {}
            }
        }
        let words: Vec<(char, f64)> = text.split_whitespace().filter_map(word).collect();
        let gs: Vec<u32> = words.iter().filter(|(l, _)| *l == 'G').map(|(_, v)| (v * 10.0).round() as u32).collect();
        let get = |l: char| words.iter().find(|(w, _)| *w == l).map(|(_, v)| *v);
        let mut new_cycle = false;
        for g in &gs {
            match *g {
                0 | 10 | 20 | 30 => motion = *g / 10,
                // A skip / straight-probe move travels in a straight line.
                310 | 382..=385 => motion = 1,
                900 => inc = false,
                910 => inc = true,
                800 => motion = 0,
                980 => ret = Some(98),
                990 => ret = Some(99),
                730 | 810..=890 if g % 10 == 0 => {
                    motion = *g / 10;
                    new_cycle = true;
                }
                _ => {}
            }
        }
        if gs.iter().any(|g| matches!(*g, 280 | 300 | 530)) {
            if r.holes > 0 && left_at.is_none() {
                left_at = Some(z);
            }
            x = None;
            y = None;
            z = f64::INFINITY;
            continue;
        }
        if gs.contains(&800) {
            continue; // G80 cancels the group and moves nothing
        }
        let (nx, ny, nz) = if inc {
            let step = |l: char, at: Option<f64>| at.map(|v| v + get(l).unwrap_or(0.0));
            (step('X', x), step('Y', y), get('Z').map(|d| z + d))
        } else {
            (get('X').or(x), get('Y').or(y), get('Z'))
        };
        let watching = r.holes > 0 || (from_start && x.is_some() && y.is_some() && z.is_finite());
        let moves_xy = |a: Option<f64>, b: Option<f64>| match (a, b) {
            (Some(a), Some(b)) => (a - b).abs() > EPS,
            _ => true,
        };
        let traverse = moves_xy(x, nx) || moves_xy(y, ny);
        if motion >= 73 {
            if get('X').is_none() && get('Y').is_none() {
                continue;
            }
            if new_cycle {
                initial = z;
                cyc_z = nz.unwrap_or(cyc_z);
                cyc_r = get('R').unwrap_or(cyc_r);
            }
            // The control positions over the hole at the level it stands at.
            if traverse && watching && z < hop - EPS {
                r.faults.push(format!("line {}: `{}` moves to the next hole at Z{z}, below the hop Z{hop}", n + 1, raw.trim()));
            }
            if !(cyc_r.is_finite() && cyc_z.is_finite()) {
                r.faults.push(format!("line {}: `{}` is a cycle with no R or Z", n + 1, raw.trim()));
                continue;
            }
            z = match ret {
                Some(98) => initial,
                Some(_) => cyc_r,
                None => initial.min(cyc_r), // no stated mode: the lower of the two
            };
            x = nx;
            y = ny;
            r.holes += 1;
            continue;
        }
        if get('X').is_none() && get('Y').is_none() && nz.is_none() {
            continue;
        }
        let to_z = nz.unwrap_or(z);
        // A rapid may dogleg (each axis at its own rate): during the XY
        // travel Z can already be at either end, so the lower one counts.
        if traverse && watching && z.min(to_z) < hop - EPS {
            r.faults.push(format!(
                "line {}: `{}` traverses at Z{}, below the hop Z{hop}",
                n + 1,
                raw.trim(),
                z.min(to_z)
            ));
        }
        x = nx;
        y = ny;
        z = to_z;
        if r.holes > 0 {
            left_at = None; // still at the cut
        }
    }
    let final_z = left_at.unwrap_or(z);
    if r.holes > 0 && final_z < exit - EPS {
        r.faults.push(format!("the program leaves its last hole at Z{final_z}, below the retract Z{exit}"));
    }
    r
}

/// Post `stream` (a `.cnc` text) through `script` with the binary, no
/// machine: the program, or the binary's refusal as an error.
fn post(script: &str, stream: &str, tag: &str) -> Result<String, String> {
    let dir = std::env::temp_dir();
    let stem = format!("camcraft_cycle_hop_{tag}_{}_{}", script.trim_end_matches(".cps"), std::process::id());
    let input = dir.join(format!("{stem}.cnc"));
    let out = dir.join(format!("{stem}.nc"));
    let log = dir.join(format!("{stem}.log"));
    std::fs::write(&input, stream).map_err(|e| e.to_string())?;
    let cps = recorded::repo().join("assets/posts").join(script);
    let r = Command::new(env!("CARGO_BIN_EXE_post"))
        .arg("--log")
        .arg(&log)
        .arg(&cps)
        .arg(&input)
        .arg(&out)
        .output()
        .map_err(|e| e.to_string())?;
    let program = std::fs::read_to_string(&out).ok();
    let warnings = std::fs::read_to_string(&log).unwrap_or_default();
    for f in [&input, &out, &log] {
        let _ = std::fs::remove_file(f);
    }
    if !r.status.success() {
        return Err(String::from_utf8_lossy(&r.stderr).into_owned());
    }
    if !warnings.trim().is_empty() {
        return Err(format!("warnings: {warnings}"));
    }
    program.ok_or_else(|| "no program written".into())
}

fn param(stream: &str, name: &str) -> f64 {
    let key = format!("PARAM {name} ");
    stream
        .lines()
        .find_map(|l| l.strip_prefix(&key))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or_else(|| panic!("the stream states no {name}"))
}

fn assert_hop_law(stream: &str, tag: &str) {
    assert_hop_law_from(stream, tag, false);
}

fn assert_hop_law_from(stream: &str, tag: &str, from_start: bool) {
    let hop = param(stream, "RetractPlane");
    let exit = param(stream, "ClearancePlane");
    for script in CYCLE_POSTS {
        let program = post(script, stream, tag).unwrap_or_else(|e| panic!("{script} {tag}: must post: {e}"));
        let r = check_from(&program, hop, exit, from_start);
        assert_eq!(r.holes, 2, "{script} {tag}: two holes drilled by cycle words:\n{program}");
        assert!(r.faults.is_empty(), "{script} {tag}: {:#?}\n{program}", r.faults);
    }
}

fn drill_retract() -> String {
    let p = recorded::stream_path("drill_retract");
    // A Windows checkout with core.autocrlf=true reads the stream with CRLF;
    // normalise, as the recorded runner does, so the anchors below match.
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
}

#[test]
fn every_cycle_post_traverses_between_holes_at_the_hop() {
    assert_hop_law(&drill_retract(), "recorded");
}

#[test]
fn a_cycle_anchored_on_the_rapid_to_r_is_seated_first() {
    // AT 1 / AT 16 are the first plunges; one move earlier is the rapid
    // down to R, which then follows the cycle record instead of leading it.
    let s = drill_retract();
    assert!(s.contains(" AT 1\n") && s.contains(" AT 16\n"), "the recorded anchors moved: update this variant");
    let moved = s.replace(" AT 1\n", " AT 0\n").replace(" AT 16\n", " AT 15\n");
    assert_hop_law(&moved, "anchored");
}

#[test]
fn the_checker_catches_the_r_plane_traverse() {
    // The program #994 found: G98 read with the tool already on R, so
    // the traverse to the second hole and the exit both stay at Z1.
    let bad = "G00 X10 Y10 Z1\nG98\nG73 X10 Y10 Z-20 R1 Q4 F60\nG80\nG73 X30 Y10 Z-15 R1 Q4 F60\nG80\nM5\nG91 G28 Z0\n";
    let r = check(bad, 3.0, 10.0);
    assert_eq!(r.holes, 2);
    assert_eq!(r.faults.len(), 2, "{:#?}", r.faults);
    assert!(r.faults[0].contains("below the hop Z3"), "{:#?}", r.faults);
    assert!(r.faults[1].contains("leaves its last hole at Z1"), "{:#?}", r.faults);
    // A program with no return mode and a modal continuation hole: the
    // lower level counts, so a cycle entered at R traverses at R.
    let unstated = "G0 X10 Y10 Z1\nG73 X10 Y10 Z-20 R1 Q4 F60\nX30 Y10\nG80\nG0 Z10\n";
    let r = check(unstated, 3.0, 10.0);
    assert_eq!((r.holes, r.faults.len()), (2, 1), "{:#?}", r.faults);
    // And the shape the fix posts passes.
    let good = "G00 X10 Y10 Z1\nG99 G73 X10 Y10 Z-20 R1 Q4 F60\nG80\nG00 X10 Y10 Z3\nG00 X30 Y10 Z3\nG00 X30 Y10 Z1\n\
                G99 G73 X30 Y10 Z-15 R1 Q4 F60\nG80\nG00 X30 Y10 Z3\nG00 X30 Y10 Z10\nM5\nG91 G28 Z0\n";
    let r = check(good, 3.0, 10.0);
    assert_eq!((r.holes, r.faults.len()), (2, 0), "{:#?}", r.faults);
}

/// #1028: the recorded stream with the first hole's hop, traverse and
/// descent cut out, so the second cycle record follows the first hole's
/// chain with no rapid leaving it - the tool stands on R over hole 1.
fn back_to_back() -> String {
    let s = drill_retract();
    let leave = "Rapid X 10000 Y 10000 Z 3000\nRapid X 30000 Y 10000 Z 3000\nRapid X 30000 Y 10000 Z 1000\n";
    assert!(s.contains(leave) && s.contains(" AT 16\n"), "the recorded hop moved: update this variant");
    // Three moves fewer before hole 2's first plunge.
    s.replace(leave, "").replace(" AT 16\n", " AT 13\n")
}

#[test]
fn back_to_back_cycle_records_traverse_at_the_hop() {
    assert_hop_law(&back_to_back(), "back_to_back");
}

#[test]
fn back_to_back_cycle_records_without_a_hop_are_refused_by_name() {
    let full = back_to_back();
    let s = full.replace("PARAM RetractPlane 3.000000\n", "");
    assert_ne!(s, full, "the recorded stream states no RetractPlane: update this variant");
    for script in CYCLE_POSTS {
        match post(script, &s, "no_hop") {
            Ok(program) => panic!("{script}: a cycle away from its hole with no hop must be refused:\n{program}"),
            Err(e) => assert!(e.contains("no RetractPlane to traverse at"), "{script}: refused, but not by name: {e}"),
        }
    }
}

/// #1028: a section whose first motion is probing - a protected descent
/// to Z5, a touch on the stock top at X10 Y10, a back-off to Z1 (below the
/// hop Z3) - and whose first cycle, at X30, is read straight after it with
/// no rapid to the hole; then a second hole by the stream's hop. With no
/// rapid in the section before the cycle, only the probe moves tell the
/// post where the tool stands.
const PROBE_THEN_CYCLE: &str = "; HSMW-CNC v1
PARAM StartX 10.000000
PARAM StartY 10.000000
PARAM StartZ 10.000000
PARAM OperationName ProbeThenPeck
PARAM ToolDiameter 6.000000
PARAM SpindleSpeed 1500.000000
PARAM WorkOffset G54
PARAM OriginX 0.000000
PARAM OriginY 0.000000
PARAM OriginZ 0.000000
PARAM ClearancePlane 10.000000
PARAM RetractPlane 3.000000
PARAM TopOfStock 0.000000
TOOL 3 DRILL-6
SECTION
GUARD 10000 10000 5000 F 300000
TOUCH 10000 10000 -5000 EXPECT 10000 10000 0 F 30000 M -
RETREAT 0 0 1000 EXPECT 10000 10000 1000 F 300000
CYCLE PECK X 30000 Y 10000 Z -8000 R 1000 ENTRY 0 Q 4000 P 0 F 60000 AT 0
Plunge X 30000 Y 10000 Z -3000 F 60000
Rapid X 30000 Y 10000 Z -2000
Plunge X 30000 Y 10000 Z -7000 F 60000
Rapid X 30000 Y 10000 Z -6000
Plunge X 30000 Y 10000 Z -8000 F 60000
Rapid X 30000 Y 10000 Z 1000
Rapid X 30000 Y 10000 Z 3000
Rapid X 50000 Y 10000 Z 3000
Rapid X 50000 Y 10000 Z 1000
CYCLE PECK X 50000 Y 10000 Z -8000 R 1000 ENTRY 0 Q 4000 P 0 F 60000 AT 9
Plunge X 50000 Y 10000 Z -3000 F 60000
Rapid X 50000 Y 10000 Z -2000
Plunge X 50000 Y 10000 Z -7000 F 60000
Rapid X 50000 Y 10000 Z -6000
Plunge X 50000 Y 10000 Z -8000 F 60000
Rapid X 50000 Y 10000 Z 1000
Rapid X 50000 Y 10000 Z 10000
ENDSECTION
";

#[test]
fn a_cycle_read_after_probing_is_seated_at_the_hop() {
    assert_hop_law_from(PROBE_THEN_CYCLE, "probe_then_cycle", true);
}

#[test]
fn the_checker_reads_probe_moves_and_incremental_back_offs() {
    // A G31 touch to Z0, a G91 back-off of 1 to Z1, then a cycle read
    // away from the touch: the control positions at Z1, below the hop.
    let bad = "G00 X10 Y10 Z10\nG31 X10 Y10 Z0 F30\nG91 G01 X0 Y0 Z1 F300\nG90\nG99 G73 X30 Y10 Z-8 R1 Q4 F60\nG80\nG00 Z10\n";
    let r = check_from(bad, 3.0, 10.0, true);
    assert_eq!((r.holes, r.faults.len()), (1, 1), "{:#?}", r.faults);
    assert!(r.faults[0].contains("at Z1, below the hop Z3"), "{:#?}", r.faults);
    // Without `from_start` the first hole's approach is not watched.
    assert!(check(bad, 3.0, 10.0).faults.is_empty());
    // The seated shape passes.
    let good = "G00 X10 Y10 Z10\nG38.2 X10 Y10 Z0 F30\nG91 G1 Z1 F300\nG90\nG0 Z3\nG0 X30 Y10\nG0 Z1\n\
                G99 G73 X30 Y10 Z-8 R1 Q4 F60\nG80\nG0 Z10\n";
    let r = check_from(good, 3.0, 10.0, true);
    assert_eq!((r.holes, r.faults.len()), (1, 0), "{:#?}", r.faults);
}

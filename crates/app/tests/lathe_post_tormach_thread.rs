//! Threading on the Tormach PathPilot (LinuxCNC) LATHE post (#612):
//! `assets/posts/tormach-lathe.cps` posts the threading stream as G33
//! passes (one block per pass, K the record's lead) or, with the post
//! option, as one LinuxCNC G76 block in PathPilot's conversational word
//! order (P Z I J K R Q H); the hand rides the spindle (#629: M3 for a
//! right-hand thread toward the chuck, M4 for a left-hand one, a
//! half-turn against the spindle refused); the G76 diameters land on
//! the control's grid with the root never deeper than the minor
//! diameter. Every program passes the LinuxCNC syntax check.
//!
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test lathe_post_tormach_thread`
//!
//! Gate map (M4.4): covers app::posting (stream_records, post_setup on
//! the Tormach lathe post), post::post (engine on the PathPilot lathe
//! dialect: G33, G76, the hand), app::commands (SetPostOptions).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;
mod tormach_lathe_support;

use camcraft_app::cl::{MoveKind, ThreadHand};
use camcraft_app::commands::SetPostOptions;
use camcraft_app::post::{cnc, cnc_thread, engine};
use camcraft_app::posting;
use camcraft_app::schema::PostOptions;
use serde_json::json;
use tormach_lathe_support::{
    g33s, g76, g76_words, gated, lathe_mch, linuxcnc_sane, post, program_with, thread_world, thread_world_with, CPS, DEPTH,
    PITCH,
};

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

fn word(ws: &[(char, f64)], c: char) -> f64 {
    ws.iter().find(|(l, _)| *l == c).map_or(f64::NAN, |(_, v)| *v)
}

// ------------------------------------------------------------ goldens

#[test]
fn a_thread_posts_as_g33_passes_by_default_in_mm_and_inch() {
    for (inch, file) in [(false, "thread_g33_mm"), (true, "thread_g33_inch")] {
        let (world, _history) = thread_world(inch);
        let text = program_with(&world, PostOptions::default());
        assert!(!text.contains("G76"), "{text}");
        // #629: a right-hand thread toward the chuck, under M3, every
        // pass from the top (z 14) down to z 8.
        assert!(text.contains("G97 S800 M3\n") && !text.contains("M4"), "{text}");
        let (z_end, k) = if inch { ("0.315", "0.07874") } else { ("8", "2") };
        let blocks = g33s(&text);
        // One synchronized block per pass: three rungs of 0.4 to 1.2.
        assert_eq!(blocks.len(), 3, "one G33 per pass, half-turns merged: {text}");
        for (z, kk) in &blocks {
            assert_eq!((z.as_str(), kk.as_str()), (z_end, k), "toward the chuck, K = the lead: {text}");
        }
        common::write_golden(&format!("nc/tormach-lathe/{file}.nc"), &text);
    }
}

#[test]
fn a_thread_posts_as_one_g76_block_with_the_option_in_mm_and_inch() {
    for (inch, file) in [(false, "thread_g76_mm"), (true, "thread_g76_inch")] {
        let (world, mut history) = thread_world(inch);
        history.execute(&world, SetPostOptions::new(&world, g76()).expect("G76 chosen"));
        let text = program_with(&world, g76());
        assert_eq!(text.matches("G76").count(), 1, "one block, LinuxCNC's form: {text}");
        assert!(!text.contains("G33"), "the passes are the control's now: {text}");
        let line = text.lines().find(|l| l.starts_with("G76")).expect("G76");
        if !inch {
            // Drive line Ø12.1 at the thread's top; peak Ø12, so I -0.1;
            // J 0.8 = the first 0.4 rung on the diameter; K 2.4 = 1.2
            // deep; R1 equal rungs, Q0 radial, H0 no spring pass.
            assert!(text.contains("G0 X12.1 Z14\n"), "{text}");
            assert_eq!(line, "G76 P2 Z8 I-0.1 J0.8 K2.4 R1 Q0 H0");
        } else {
            // The grid law: Ø12.1 is 0.4764, Ø12 0.4724 (I -0.004, not a
            // second rounding of 0.1 mm); the root rounds OUT to 0.378
            // (Ø9.6 is 0.37795 in), so K 0.0944 - never deeper.
            assert!(text.contains("G0 X0.4764 Z0.5512\n"), "{text}");
            assert_eq!(line, "G76 P0.07874 Z0.315 I-0.004 J0.0314 K0.0944 R1 Q0 H0");
        }
        assert_eq!(gated(&world, file), text, "the in-app post follows the document's option");
        common::write_golden(&format!("nc/tormach-lathe/{file}.nc"), &text);
    }
}

// ------------------------------------------------------------ the hand

#[test]
fn a_left_hand_thread_posts_toward_the_chuck_under_m4() {
    // #629: the passes still run toward the chuck; the helix turns the
    // other way, which the tool does about the part only under M4 -
    // stated before the section opens and voiced with an operator note.
    let (world, _history) = thread_world_with(false, ThreadHand::Left, PITCH, DEPTH);
    let text = program_with(&world, PostOptions::default());
    assert!(text.contains("G97 S800 M4\n") && !text.contains(" M3\n"), "{text}");
    assert!(text.contains("MOUNT THE INSERT FOR M4"), "{text}");
    assert!(g33s(&text).iter().all(|(z, k)| z == "8" && k == "2"), "{text}");
    let g = program_with(&world, g76());
    assert!(g.contains("M4\n") && g.contains("G76 P2 Z8 "), "{g}");
    assert_eq!(gated(&world, "lh"), text, "the gated in-app post agrees");
}

#[test]
fn a_thread_against_the_spindle_is_refused_by_name() {
    let (world, _history) = thread_world(false);
    let records = posting::stream_records(&world).expect("generated");
    // A G76 record claiming the other hand than the section's M3.
    let (text, refused) = post(&world, &with_rec(&records, |t| t.hand = ThreadHand::Left), g76());
    assert!(
        refused.iter().any(|w| w.contains("G76 threading cycle refused") && w.contains("left-hand thread cut toward the chuck needs M4")),
        "{refused:?}"
    );
    assert!(!text.contains("G76 P"), "{text}");
    // Left-hand passes posted under M3 (the reverse PARAM lost): every
    // half-turn refused, no G33 written.
    let (lw, _h) = thread_world_with(false, ThreadHand::Left, PITCH, DEPTH);
    let lost: Vec<cnc::Record> = posting::stream_records(&lw)
        .expect("generated")
        .into_iter()
        .filter(|r| !matches!(r, cnc::Record::Param { name, .. } if name == "SpindleDirection"))
        .collect();
    let (text, refused) = post(&lw, &lost, PostOptions::default());
    assert!(refused.iter().any(|w| w.contains("would cut the other hand")), "{refused:?}");
    assert!(!text.contains("G33"), "{text}");
}

// ------------------------------------------------------------ G33 K

#[test]
fn g33_states_one_k_per_thread_on_a_lead_off_the_lattice() {
    // 12 TPI: a half lead of 1.058333 mm is no lattice number, so the
    // half-turns' rounded Z travel differs block to block. K is the
    // record's lead, once per pass, in both units - never re-derived.
    let lead = 25.4 / 12.0;
    for (inch, k) in [(false, "2.116667"), (true, "0.083333")] {
        let (world, _history) = thread_world_with(inch, ThreadHand::Right, lead, DEPTH);
        let text = program_with(&world, PostOptions::default());
        let blocks = g33s(&text);
        assert_eq!(blocks.len(), 3, "three passes of 0.4: {text}");
        assert!(blocks.iter().all(|(_, kk)| kk == k), "one K: {blocks:?}");
    }
    // A stream with no THREAD record: K is the pass's own Z travel over
    // its turns, still one block per pass.
    let lead = 25.4 / 24.0;
    let h = lead / 2.0;
    let mv = |x: f64, z: f64, center: Option<(f64, f64)>, kind: MoveKind| {
        cnc::Record::Move(cnc::MoveRec {
            kind, x, y: 0.0, z: (z * 1000.0).round() / 1000.0, feed: Some(800.0), center, ccw: false,
            plane: Default::default(), abc: None,
        })
    };
    let mut r = vec![
        cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(800.0) },
        cnc::Record::Tool { number: 1, name: "TD".into() },
        cnc::Record::Section { begin: true },
        mv(5.0, 10.0, None, MoveKind::Rapid),
    ];
    for i in 0..6 {
        let x = if i % 2 == 0 { -5.0 } else { 5.0 };
        r.push(mv(x, 10.0 - h * f64::from(i + 1), Some((0.0, 0.0)), MoveKind::Feed));
    }
    r.push(cnc::Record::Section { begin: false });
    let out = engine::run(CPS, &lathe_mch(false), &cnc::serialize(&r)).expect("runs");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(g33s(&out.program), vec![("6.825".to_string(), "1.058333".to_string())], "{}", out.program);
    linuxcnc_sane(&out.program);
}

// ------------------------------------------------------------ G76

#[test]
fn g76_rounds_to_the_grid_and_never_deeper_than_the_minor() {
    // The record's numbers each ride the half-micron lattice on their
    // own: a minor diameter 1.5 um off the height law and the end
    // point is still one thread, posted - and the root rounds OUT.
    let (world, _history) = thread_world(false);
    let records = posting::stream_records(&world).expect("generated");
    let bent = with_rec(&records, |t| t.minor_diameter += 0.0015);
    let (text, refused) = post(&world, &bent, g76());
    assert!(refused.is_empty(), "a lattice disagreement refused a legal cycle: {refused:?}");
    let w = g76_words(&text);
    // Peak Ø12, minor Ø9.6015 -> root Ø9.602 (outward), K 2.398.
    assert_eq!((word(&w, 'I'), word(&w, 'K')), (-0.1, 2.398), "{text}");
    // In both units the root the control computes (X + I - K) is never
    // inside the record's minor, and J never exceeds K.
    for inch in [false, true] {
        let (world, _history) = thread_world(inch);
        let text = program_with(&world, g76());
        // The drive line: the X of the rapid just before the G76 block.
        let lines: Vec<&str> = text.lines().collect();
        let at = lines.iter().position(|l| l.starts_with("G76 ")).expect("G76");
        let x: f64 = lines[at - 1]
            .split(' ')
            .find_map(|w| w.strip_prefix('X'))
            .and_then(|v| v.parse().ok())
            .expect("the drive-line rapid");
        let w = g76_words(&text);
        let unit = if inch { 25.4 } else { 1.0 };
        let root = (x + word(&w, 'I') - word(&w, 'K')) * unit;
        assert!(root >= 9.6 - 1e-9, "root Ø{root} inside the minor Ø9.6: {text}");
        assert!(root - 9.6 < 0.003, "root Ø{root} more than a grid step shallow: {text}");
        assert!(word(&w, 'J') <= word(&w, 'K'), "{text}");
        // The PathPilot conversational word order: P Z I J K R Q H.
        let order: String = w.iter().map(|(c, _)| *c).collect();
        assert_eq!(order, "PZIJKRQH", "{text}");
    }
}

#[test]
fn a_g76_record_the_cycle_cannot_state_exactly_is_refused_by_name() {
    let (world, _history) = thread_world(false);
    let records = posting::stream_records(&world).expect("generated");
    let bare: Vec<cnc::Record> = records.iter().filter(|r| !matches!(r, cnc::Record::Thread(_))).cloned().collect();
    type Bend = fn(&mut cnc_thread::ThreadRec);
    let cases: [(&str, Vec<cnc::Record>); 7] = [
        ("no thread cycle record", bare),
        ("infeed angle", with_rec(&records, (|t| t.infeed_angle = 29.5) as Bend)),
        ("finish allowance", with_rec(&records, (|t| t.finish_allowance = 0.05) as Bend)),
        ("pull-out", with_rec(&records, (|t| t.pull_out = 1.0) as Bend)),
        ("not equal rungs", with_rec(&records, (|t| t.min_depth = 0.1) as Bend)),
        ("not an external thread", with_rec(&records, (|t| t.minor_diameter = 13.0) as Bend)),
        // Past the lattice allowance: a real disagreement, not rounding.
        ("diameters and the thread height disagree", with_rec(&records, (|t| t.minor_diameter += 0.01) as Bend)),
    ];
    for (why, recs) in cases {
        let (text, refused) = post(&world, &recs, g76());
        assert!(refused.iter().any(|w| w.contains("G76") && w.contains(why)), "{why}: {refused:?}");
        assert!(!text.contains("G76 P"), "{why}: a refused cycle was written: {text}");
    }
}

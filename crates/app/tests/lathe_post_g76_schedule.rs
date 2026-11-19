//! The G76 words state the verified passes (issue #417): with the
//! document's "Threading: G76 cycle" option the control cuts the
//! passes itself, so the words posted must describe exactly the passes
//! the kernel laid and the verdict judged. The control's schedule for
//! the words `G76 P(m)(r)(a) Q(dmin) R(d)` / `G76 X Z P(k) Q(dd) F` is
//! modelled here as the public G76 references define it (GSK988TA
//! manual G76, Fanuc-compatible; behaviour only): pass n reaches
//! max(dd*sqrt(n), previous + dmin) up to k - d, the first finishing
//! pass cuts d to k and m - 1 more repeat k, and with a non-zero tool
//! angle each pass starts tan(a/2) x its depth along Z from the start
//! point (the fixture and the control model: tests/common/g76.rs; the
//! published sources: lathe_post_g76_sources.rs, #1842). Pinned: a
//! constant-area flank thread's golden whose words
//! reproduce its laid depths and stations; the default thread's words
//! reproducing its three passes; a refusal by name for each plan the
//! cycle cannot state - the repeat of each roughing depth (issue #418)
//! among them, which posts as G32 passes.
//!
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test lathe_post_g76_schedule`
//!
//! Gate map (M4.4): covers camcraft_motion::turning (thread_cycle's
//! record), app::posting + the lathe dialect's G76 words and refusals.
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use camcraft_app::commands::{History, SetPostOptions};
use camcraft_app::posting;
use camcraft_app::schema::{PostOptions, ThreadingOutput};
use camcraft_app::turning::{DepthSchedule, InfeedMethod, ThreadInfeed};
use camcraft_app::world::new_world;

use common::g76::{control_depths, g76_world, laid, post, post_stream, same_ladder};

#[test]
fn a_constant_area_flank_g76_states_its_verified_passes() {
    let plan = ThreadInfeed {
        method: InfeedMethod::Flank,
        flank_angle: 27.5,
        schedule: DepthSchedule::ConstantArea,
        min_depth: 0.1,
        finish_allowance: 0.05,
        spring_passes: 1,
        repeat_each_depth: false,
        dips: Default::default(),
    };
    let world = g76_world(0.4, plan);
    let (text, refused) = post(&world);
    assert!(refused.is_empty(), "{refused:?}");
    assert!(text.contains("G76 P020055 Q100 R0.05\n"), "{text}");
    assert!(text.contains(" P1200 Q400 F2.\n"), "{text}");
    same_ladder(&world, &text);
    common::write_golden("nc/fanuc-lathe/thread_g76_area.nc", &text);
}

#[test]
fn the_default_thread_g76_states_its_three_passes() {
    let world = g76_world(0.4, ThreadInfeed::default());
    let (text, refused) = post(&world);
    assert!(refused.is_empty(), "{refused:?}");
    same_ladder(&world, &text);
    assert!(text.contains("G76 P010000 Q400 R0.4\n"), "{text}");
    // A remainder rung: 0.5, 1.0 and the root 1.2 - R is the last 0.2,
    // not the rung.
    let world = g76_world(0.5, ThreadInfeed::default());
    let (text, refused) = post(&world);
    assert!(refused.is_empty(), "{refused:?}");
    assert!(text.contains("G76 P010000 Q500 R0.2\n"), "{text}");
    same_ladder(&world, &text);
}

#[test]
fn the_default_thread_g76_states_its_three_passes_in_inch() {
    // PR #517 review: each inch word rounds on its own (0.4 mm = Q157,
    // 1.2 mm = P472), so R0.0157 left k - d = 0.0315 one ten-thousandth
    // past the second rung 0.0314 and the control cut a fourth pass
    // nobody verified. R comes from the rounded words' own ladder.
    let world = g76_world(0.4, ThreadInfeed::default());
    let stream = camcraft_app::post::cnc::serialize(&posting::stream_records(&world).expect("generated"));
    let (text, refused) = post_stream(&world, &stream, "inch");
    assert!(refused.is_empty(), "{refused:?}");
    assert!(text.contains("G76 P010000 Q157 R0.0158\n"), "{text}");
    assert!(text.contains(" P472 Q157 F0.07874\n"), "{text}");
    let (want, _) = control_depths(&text);
    assert_eq!(want.len(), 3, "the control cuts {want:?} from\n{text}");
    same_ladder(&world, &text);
}

/// `stream` with the THREAD record's `key` numbers (whole microns) replaced.
fn with_thread_words(stream: &str, words: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for line in stream.lines() {
        if line.starts_with("THREAD X") {
            let mut w: Vec<String> = line.split_whitespace().map(str::to_string).collect();
            for (key, value) in words {
                let at = w.iter().position(|x| x == key).expect("the THREAD word");
                w[at + 1] = (*value).to_string();
            }
            out.push_str(&w.join(" "));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

#[test]
fn a_ladder_the_rounded_words_cannot_state_is_refused_by_name() {
    // Inch ladders whose rounded first depth drifts the control's rungs
    // off the record's: 0.1 mm = Q39 (0.0039 in, 0.9906 mm) over 13
    // sqrt rungs leaves the allowance two increments from the record's
    // R0.033 (0.839 mm) - one past what the words may round by.
    let world = g76_world(0.4, ThreadInfeed::default());
    let stream = camcraft_app::post::cnc::serialize(&posting::stream_records(&world).expect("generated"));
    let cases: [(&str, &str, &str, &str); 2] = [
        ("100", "10", "839", "Q39 (first depth), Q4 (minimum depth) and P472 (thread height) make the control cut 13 rough passes"),
        ("146", "10", "288", "make the control cut 39 rough passes"),
    ];
    for (first, min, finish, why) in cases {
        let s = with_thread_words(&stream, &[("FIRST", first), ("MIN", min), ("FINISH", finish)]);
        let (text, refused) = post_stream(&world, &s, "inch");
        assert!(
            refused.iter().any(|w| w.contains("G76 threading cycle refused - the rounded words cannot state the verified passes") && w.contains(why)),
            "{first}/{finish}: not refused by name: {refused:?}\n{text}"
        );
        assert!(!text.contains("G76 X"), "{first}: a refused cycle was written: {text}");
    }
    // The same record in mm: the micron words state 13 rungs, R one
    // micron over the record (rung 13 is 0.36056, not 0.361).
    let s = with_thread_words(&stream, &[("FIRST", "100"), ("MIN", "10"), ("FINISH", "839")]);
    let (text, refused) = post_stream(&world, &s, "mm");
    assert!(refused.is_empty() && text.contains("G76 P010000 Q10 R0.84\n"), "{refused:?}\n{text}");
}

#[test]
fn a_plan_the_cycle_cannot_state_is_refused_by_name() {
    let alt = ThreadInfeed { method: InfeedMethod::Alternating, flank_angle: 30.0, ..ThreadInfeed::default() };
    let modified = ThreadInfeed { method: InfeedMethod::Flank, flank_angle: 29.5, ..ThreadInfeed::default() };
    let repeat = ThreadInfeed { repeat_each_depth: true, ..ThreadInfeed::default() };
    let cases: [(f64, ThreadInfeed, &str); 4] = [
        (0.4, alt, "alternating flank infeed"),
        (0.4, modified, "not a G76 tool angle"),
        (1.5, ThreadInfeed::default(), "one-pass thread"),
        (0.4, repeat, "repeat each roughing depth"),
    ];
    for (infeed, plan, why) in cases {
        let world = g76_world(infeed, plan);
        let (text, refused) = post(&world);
        assert!(
            refused.iter().any(|w| w.contains("G76 threading cycle refused") && w.contains(why)),
            "{why}: not refused by name: {refused:?}"
        );
        assert!(!text.contains("G76 X"), "{why}: a refused cycle was written: {text}");
    }
}

#[test]
fn the_alternating_flag_rides_the_stream_and_the_file() {
    let alt = ThreadInfeed { method: InfeedMethod::Alternating, flank_angle: 30.0, ..ThreadInfeed::default() };
    let world = g76_world(0.4, alt);
    let records = posting::stream_records(&world).expect("generated");
    let text = camcraft_app::post::cnc::serialize(&records);
    assert!(text.lines().any(|l| l.starts_with("THREAD X") && l.ends_with(" ALT 1")), "{text}");
    let back = camcraft_app::post::cnc::parse(&text).expect("parses");
    assert_eq!(camcraft_app::post::cnc::serialize(&back), text, "write-parse-write is a fixed point");
    let file = std::env::temp_dir().join(format!("camcraft-g76-alt-{}.json", std::process::id()));
    camcraft_app::project::save(&world, &file).expect("saved");
    let other = new_world();
    camcraft_app::project::load(&other, &file).expect("loaded");
    let _ = std::fs::remove_file(&file);
    let e = camcraft_app::model::find_by_name(&other, "TH1").expect("op");
    let t = camcraft_app::model::toolpath_of(&other, &e).expect("cache").path.threads;
    assert!(t.first().is_some_and(|t| t.alternating), "{t:?}");
    // A single-flank record writes no ALT word: every earlier stream's bytes.
    let world = g76_world(0.4, ThreadInfeed::default());
    let text = camcraft_app::post::cnc::serialize(&posting::stream_records(&world).expect("generated"));
    assert!(!text.contains(" ALT "), "{text}");
}

#[test]
fn a_repeated_depth_posts_as_g32_and_its_flag_rides_the_stream() {
    let repeat = ThreadInfeed { repeat_each_depth: true, ..ThreadInfeed::default() };
    let world = g76_world(0.4, repeat);
    let records = posting::stream_records(&world).expect("generated");
    let text = camcraft_app::post::cnc::serialize(&records);
    assert!(text.lines().any(|l| l.starts_with("THREAD X") && l.ends_with(" REP 1")), "{text}");
    let back = camcraft_app::post::cnc::parse(&text).expect("parses");
    assert_eq!(camcraft_app::post::cnc::serialize(&back), text, "write-parse-write is a fixed point");
    // With G32 output the same passes post: five plunges, each a G32 run.
    let mut history = History::default();
    let g32 = PostOptions { threading: ThreadingOutput::G32Passes, ..PostOptions::default() };
    history.execute(&world, SetPostOptions::new(&world, g32).expect("G32"));
    let (nc, refused) = post(&world);
    assert!(refused.is_empty(), "{refused:?}");
    assert!(nc.contains("G32") && !nc.contains("G76"), "{nc}");
    let (laid, _) = laid(&world);
    let depths: Vec<f64> = laid.iter().map(|&(_, d)| d).collect();
    assert_eq!(depths.len(), 5, "{depths:?}");
    for (d, w) in depths.iter().zip([0.4, 0.4, 0.8, 0.8, 1.2]) {
        assert!((d - w).abs() < 1e-9, "{depths:?}");
    }
    // Off writes no REP word: every earlier stream's bytes.
    let world = g76_world(0.4, ThreadInfeed::default());
    let text = camcraft_app::post::cnc::serialize(&posting::stream_records(&world).expect("generated"));
    assert!(!text.contains(" REP "), "{text}");
}

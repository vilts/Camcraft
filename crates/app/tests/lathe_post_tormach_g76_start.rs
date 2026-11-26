//! The Tormach PathPilot (LinuxCNC) LATHE post's G76 start (#2952, the
//! law of #2878): with no entry taper, LinuxCNC's G76 moves the insert
//! to each pass's cut depth at rapid at the start Z. So the start point
//! must stand in free air - in front of the end face, or in a relief
//! groove wider than the insert. When the record says that it does not
//! (`start_in_air` false, or an older stream without the `AIR` word),
//! the post refuses the cycle by name and the in-app gate writes no
//! file. G33 passes (each one synchronized from the drive line) still
//! post on the same solid bar.
//!
//! Provenance: our own fixtures; the G76 behaviour from the public
//! LinuxCNC G-code reference (G76 section; behaviour only).
//!
//! Gate map (M4.4): covers app::operation (generate, g76_start: the
//! record's start_in_air), app::posting (stream_records, post_setup on
//! the Tormach lathe post), post::post (cnc_thread's AIR word, engine on
//! the PathPilot lathe dialect: the G76 start refusal).
//! Lane shape (M4.4): all tests here run in seconds.

mod tormach_lathe_support;

use camcraft_app::commands::SetPostOptions;
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::schema::PostOptions;
use tormach_lathe_support::{g33s, g76, g76_world, post, program_with, thread_world, CPS};

const WHY: &str = "the start point is above material";

/// The stream's thread record says that its start stands in free air.
fn start_in_air(records: &[cnc::Record]) -> bool {
    records
        .iter()
        .find_map(|r| match r {
            cnc::Record::Thread(Some(t)) => Some(t.start_in_air),
            _ => None,
        })
        .expect("a thread record")
}

#[test]
fn tormach_g76_refuses_by_name_when_the_start_point_is_inside_a_solid_bar() {
    for inch in [false, true] {
        // The bar runs on at Ø12 to z 40; the cycle starts at z 14, above
        // material. Each pass would rapid into the bar.
        let (world, mut history) = thread_world(inch);
        let records = posting::stream_records(&world).expect("generated");
        assert!(!start_in_air(&records), "inch {inch}: the solid bar's start is not in free air");
        let (text, refused) = post(&world, &records, g76());
        assert!(
            refused.iter().any(|w| w.contains("G76 threading cycle refused - ")
                && w.contains(WHY)
                && w.contains("in front of the end face")
                && w.contains("relief groove wider than the insert")
                && w.contains("regenerate the operation")
                && w.contains("G33 passes")),
            "inch {inch}: {refused:?}"
        );
        assert!(!text.contains("G76 P"), "inch {inch}: a refused cycle was written: {text}");
        // The in-app gate: refused by name, no file written.
        history.execute(&world, SetPostOptions::new(&world, g76()).expect("G76 chosen"));
        let path = std::env::temp_dir().join(format!("camcraft-2952-solid-{inch}-{}.nc", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let err = posting::post_setup(&world, &path).expect_err("the gate blocks a G76 that starts above material");
        assert!(err.contains("G76") && err.contains(WHY), "inch {inch}: unexpected gate error: {err}");
        assert!(!path.exists(), "inch {inch}: a refused post must write no file");
    }
}

#[test]
fn tormach_g76_posts_when_the_start_point_is_in_free_air_in_front_of_the_end_face() {
    for (inch, start) in [(false, "G0 X12.1\nG0 Z16\nG76 P2 Z8 I-0.1 J0.8 K2.4 R1 Q0 H0\n"),
        (true, "G0 X0.4764\nG0 Z0.6299\nG76 P0.07874 Z0.315 I-0.004 J0.0315 K0.0944 R1 Q0 H0\n")]
    {
        // The bar ends at its free end face z 14.693; the cycle starts at
        // z 16, so the insert's band [15.134, 16.866] is clear of it.
        let (world, mut history) = g76_world(inch);
        let records = posting::stream_records(&world).expect("generated");
        assert!(start_in_air(&records), "inch {inch}: the start in front of the end face is in free air");
        let text = program_with(&world, g76());
        assert!(text.contains(start), "inch {inch}: {text}");
        history.execute(&world, SetPostOptions::new(&world, g76()).expect("G76 chosen"));
        let path = std::env::temp_dir().join(format!("camcraft-2952-air-{inch}-{}.nc", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let posted = posting::post_setup(&world, &path).expect("a free-air G76 posts through the gate");
        let _ = std::fs::remove_file(&path);
        assert!(posted.program.contains(start), "inch {inch}: {}", posted.program);
    }
}

#[test]
fn tormach_g33_passes_still_post_when_the_start_point_is_inside_a_solid_bar() {
    // The flag gates the cycle only: G33 passes on the solid bar post
    // with no refusal (program_with asserts that), one block per pass.
    let (world, _history) = thread_world(false);
    let records = posting::stream_records(&world).expect("generated");
    assert!(!start_in_air(&records));
    let text = program_with(&world, PostOptions::default());
    assert!(!text.contains("G76"), "{text}");
    assert_eq!(g33s(&text).len(), 3, "{text}");
}

#[test]
fn tormach_g76_refuses_a_legacy_stream_without_the_air_word() {
    // An older stream (or a cached record from before #2878) has no AIR
    // word: it reads false, so the cycle is refused until regenerated.
    let (world, _history) = g76_world(false);
    let records = posting::stream_records(&world).expect("generated");
    let stream = cnc::serialize(&records);
    assert!(stream.contains(" AIR 1\n"), "the free-air stream carries AIR 1:\n{stream}");
    let legacy = stream.replace(" AIR 1\n", "\n");
    let out = engine::run_with(CPS, &posting::machine_value(&world), &legacy, &posting::options_value(g76())).expect("runs");
    assert!(
        out.warnings.iter().any(|w| w.contains("G76 threading cycle refused") && w.contains(WHY)),
        "a legacy stream without AIR 1 must refuse G76: {:?}",
        out.warnings
    );
    assert!(!out.program.contains("G76 P"), "{}", out.program);
}

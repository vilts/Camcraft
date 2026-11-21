//! Pins the lane record and the collected verdict (#2908): a record reads back as written, the verdict
//! names each lane with its runner, time and result and each red step, and a lane that is missing,
//! double, void or cut short makes the verdict red.
//! Provenance: issue #2908; our own gate record format.

use crate::lane_split::Lane;
use crate::verdict::{self, Record, Row};

fn row(ok: bool, name: &str, note: &str) -> Row {
    Row { ok, secs: 12, name: name.into(), note: note.into() }
}

fn rec(index: usize, of: usize, runner: &str, rows: Vec<Row>) -> Record {
    let result = if rows.iter().all(|r| r.ok) { "green" } else { "red" };
    Record {
        lane: Lane { index, of },
        runner: runner.into(),
        result: result.into(),
        wall: 754,
        planned: rows.len(),
        rows,
        from: format!("lane-{index}/gate-logs/batch-x/lane-result.tsv"),
    }
}

fn green(index: usize, of: usize, runner: &str) -> Record {
    rec(index, of, runner, vec![row(true, "debug build: app (--no-run)", ""), row(true, "app suites: cmds", "")])
}

#[test]
fn a_record_reads_back_as_written() {
    let r = rec(2, 3, "uma", vec![row(true, "app suites: lib", ""), row(false, "app suites: cmds", "failed: cmds [tests: undo_a,\tundo_b]")]);
    let text = verdict::render(&r);
    assert!(text.starts_with("lane\t2/3\nrunner\tuma\nresult\tred\nwall\t754\nplanned\t2\nstep\tok\t12\tapp suites: lib\t\n"), "{text}");
    let mut want = r.clone();
    want.rows[1].note = "failed: cmds [tests: undo_a, undo_b]".into(); // a tab in a note becomes a space
    assert_eq!(verdict::parse(&text, &r.from).expect("parse"), want);
}

#[test]
fn a_damaged_record_is_an_error() {
    assert!(verdict::parse("lane\t1/2\nrunner\twopr\n", "f").expect_err("no result").contains("no lane, result or planned line"));
    assert!(verdict::parse("lane\t1/2\nresult\tfine\nplanned\t1\n", "f").expect_err("bad result").contains("line not of a lane record"));
    assert!(verdict::parse("lane\t3/2\nresult\tgreen\nplanned\t1\n", "f").expect_err("bad lane").contains("--lane needs N/M"));
}

#[test]
fn all_lanes_green_is_a_green_verdict_that_names_each_lane() {
    let (text, ok) = verdict::verdict(&[green(2, 3, "uma"), green(1, 3, "wopr"), green(3, 3, "m3")], &[]);
    assert!(ok, "{text}");
    assert!(text.starts_with("## Batch gate verdict: GREEN (3 of 3 lane(s) reported)"), "{text}");
    for line in [
        "| 1/3 | wopr | 12.5 min | green | 2 of 2 | `lane-1/gate-logs/batch-x/lane-result.tsv` |",
        "| 2/3 | uma | 12.5 min | green | 2 of 2 |",
        "| 3/3 | m3 | 12.5 min | green | 2 of 2 |",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
    assert!(!text.contains("Red steps") && !text.contains("Lane faults"), "{text}");
}

#[test]
fn a_red_step_is_named_with_its_lane_runner_and_note() {
    let red = rec(2, 2, "uma", vec![row(true, "app suites: lib", ""), row(false, "app suites: cmds", "failed: cmds [tests: undo_a]")]);
    let (text, ok) = verdict::verdict(&[green(1, 2, "wopr"), red], &[]);
    assert!(!ok, "{text}");
    assert!(text.starts_with("## Batch gate verdict: RED (2 of 2 lane(s) reported)"), "{text}");
    assert!(text.contains("| 2/2 | uma | 12.5 min | red | 2 of 2 |"), "{text}");
    assert!(text.contains("\nRed steps:\n- lane 2/2 (uma): app suites: cmds - failed: cmds [tests: undo_a]\n"), "{text}");
}

#[test]
fn a_missing_a_double_a_void_or_a_short_lane_is_red() {
    // Missing: lane 2 of 3 wrote no record (its job was cancelled or its runner was lost).
    let (text, ok) = verdict::verdict(&[green(1, 3, "wopr"), green(3, 3, "uma")], &[]);
    assert!(!ok && text.contains("RED (2 of 3 lane(s) reported)") && text.contains("| 2/3 | - | - | no record | - | - |"), "{text}");
    assert!(text.contains("- lane 2/3 has no record: it must have exactly one"), "{text}");
    // Double: two records for one lane.
    let (text, ok) = verdict::verdict(&[green(1, 2, "wopr"), green(2, 2, "uma"), green(2, 2, "m3")], &[]);
    assert!(!ok && text.contains("- lane 2/2 has 2 records: it must have exactly one"), "{text}");
    // Void: the checkout moved (#1201).
    let mut void = green(2, 2, "uma");
    void.result = "void".into();
    let (text, ok) = verdict::verdict(&[green(1, 2, "wopr"), void], &[]);
    assert!(!ok && text.contains("- lane 2/2 is void: the checkout moved during the run"), "{text}");
    // Short: a failed build stopped the lane before its other steps.
    let mut short = rec(2, 2, "uma", vec![row(false, "debug build: app (--no-run)", "compile error")]);
    short.planned = 40;
    let (text, ok) = verdict::verdict(&[green(1, 2, "wopr"), short], &[]);
    assert!(!ok && text.contains("| 2/2 | uma | 12.5 min | red | 1 of 40 |"), "{text}");
    assert!(text.contains("- lane 2/2 ran 1 of 40 step(s): the other steps did not run"), "{text}");
    // A different split, an unreadable record, and no record at all.
    let (text, ok) = verdict::verdict(&[green(1, 2, "wopr"), green(2, 2, "uma"), green(1, 1, "m3")], &[]);
    assert!(!ok && text.contains("is of a different split than the 2 lanes of this verdict"), "{text}");
    let (text, ok) = verdict::verdict(&[green(1, 1, "wopr")], &["x/lane-result.tsv: line not of a lane record: 'junk'".into()]);
    assert!(!ok && text.contains("\nLane faults:\n- x/lane-result.tsv: line not of a lane record: 'junk'\n"), "{text}");
    let (text, ok) = verdict::verdict(&[], &[]);
    assert!(!ok && text.contains("- no lane record was found"), "{text}");
}

#[test]
fn the_runner_name_comes_from_the_environment() {
    let env = |pairs: &'static [(&'static str, &'static str)]| move |k: &str| pairs.iter().find(|p| p.0 == k).map(|p| p.1.to_string());
    assert_eq!(verdict::runner_name(env(&[("RUNNER_NAME", "wopr"), ("HOSTNAME", "box")])), "wopr");
    assert_eq!(verdict::runner_name(env(&[("RUNNER_NAME", " "), ("HOSTNAME", "box")])), "box");
    assert_eq!(verdict::runner_name(env(&[])), "unknown");
}

#[test]
fn the_collector_finds_the_records_of_downloaded_artifacts() {
    let root = std::env::temp_dir().join(format!("xtask-verdict-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (n, r) in [(1, green(1, 2, "wopr")), (2, green(2, 2, "uma"))] {
        let dir = root.join(format!("gate-logs-lane-{n}")).join("gate-logs").join("batch-20261120-101010");
        std::fs::create_dir_all(&dir).expect("artifact dir");
        std::fs::write(dir.join(verdict::RECORD), verdict::render(&r)).expect("record");
        std::fs::write(dir.join("01-app_suites_lib.log"), "test result: ok.\n").expect("log");
    }
    let mut files = Vec::new();
    verdict::find(&root, 6, &mut files);
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(files[0].to_string_lossy().contains("gate-logs-lane-1"), "sorted: {files:?}");
    assert_eq!(verdict::run(&[root.to_string_lossy().into_owned()]), Ok(0));
    std::fs::remove_file(&files[1]).expect("drop lane 2");
    assert_eq!(verdict::run(&[root.to_string_lossy().into_owned()]), Ok(1), "a missing lane is red");
    assert!(verdict::run(&[]).is_err() && verdict::run(&["no-such-dir-2908".into()]).is_err());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_zero_step_lane_record_is_incomplete() {
    let empty = rec(1, 1, "wopr", Vec::new());
    let (text, ok) = verdict::verdict(&[empty], &[]);
    assert!(!ok, "{text}");
    assert!(text.contains("RED (1 of 1 lane(s) reported)"), "{text}");
    assert!(text.contains("empty lane is incomplete"), "{text}");
}

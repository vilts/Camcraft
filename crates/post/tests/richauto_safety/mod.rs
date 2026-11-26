//! Provenance: original safety and formatting pins for RichAuto A11 issue #1914.
use super::*;

#[test]
fn an_unstated_feed_keeps_a_known_modal_feed_and_refuses_an_unknown_one() {
    let out = run(
        &program("Rapid X 0 Y 0 Z 5000\nFeed X 10000 Y 0 Z 5000 F 300000\nFeed X 20000 Y 0 Z 5000\n"),
        &Value::Null,
    );
    clean(&out);
    let b = bodies(&out.program);
    let first = b.iter().position(|l| *l == "G1X10.000F300.0").expect("known feed");
    assert_eq!(b[first + 1], "G1X20.000", "{}", out.program);
    refused(&program("Rapid X 0 Y 0 Z 5000\nFeed X 10000 Y 0 Z 5000\n"), &["feed"]);
}

#[test]
fn g89_and_ream_boring_expand_to_feed_in_and_feed_out() {
    for (kind, dwell) in [("BACKBORE", 0), ("BACKBORE", 500), ("REAM", 0)] {
        let stream = hole(kind, dwell, " F 100000", true).replace(
            "Rapid X 10000 Y 20000 Z 2000\nRapid X 10000 Y 20000 Z 10000\n",
            "Feed X 10000 Y 20000 Z 2000 F 100000\nRapid X 10000 Y 20000 Z 10000\n",
        );
        let out = run(&stream, &Value::Null);
        clean(&out);
        for word in ["G85", "G87", "G89"] {
            assert!(!out.program.contains(word), "{}", out.program);
        }
        let b = bodies(&out.program);
        let floor = b.iter().position(|l| *l == "G1Z-6.000F100.0").expect("feed in");
        let retract = floor + b[floor..].iter().position(|l| *l == "G1Z2.000").expect("feed out");
        assert!(!b[floor..retract].contains(&"M05"), "{}", out.program);
        if dwell > 0 {
            assert!(b[floor..retract].contains(&"G04P500"), "{}", out.program);
        }
    }
}

#[test]
fn zero_radius_arc_is_refused_by_name() {
    refused(
        &program("Rapid X 0 Y 0 Z 5000\nArc CCW X 10000 Y 0 Z 5000 F 300000 I 0 J 0\n"),
        &["radius"],
    );
}

#[test]
fn invalid_tool_and_coolant_are_refused_by_name() {
    refused(&program(CUT).replace("TOOL 1", "TOOL 0"), &["tool number"]);
    let script = read("assets/posts/richauto.cps");
    for (ask, coolants, word) in [
        ("mist", json!([]), "coolant mist is unsupported"),
        ("flood", json!([{"name": "Mist", "mCode": "M7"}]), "flood"),
        ("flood", json!([{"name": "Flood", "mCode": "M7"}]), "m8"),
    ] {
        let stream = program(CUT).replace("Coolant off", &format!("Coolant {ask}"));
        let out = engine::run(
            &script,
            &json!({"spindleMax": 24000, "coolants": coolants}),
            &stream,
        )
        .expect("post");
        assert!(
            out.warnings.join(" ").to_ascii_lowercase().contains(word),
            "{:?}",
            out.warnings
        );
    }
}

#[test]
fn coolant_refusals_name_the_operation() {
    let script = read("assets/posts/richauto.cps");
    for (ask, coolants, word) in [
        ("mist", json!([]), "coolant mist is unsupported"),
        ("flood", json!([{"name": "Mist", "mCode": "M7"}]), "no flood entry"),
        ("flood", json!([{"name": "Flood", "mCode": "M7"}]), "requires m8"),
        ("flood", json!([{"name": "Flood", "mCode": ""}]), "empty"),
    ] {
        let stream = program(CUT)
            .replace("OperationName A11", "OperationName ReviewCoolantOp")
            .replace("Coolant off", &format!("Coolant {ask}"));
        let out = engine::run(
            &script,
            &json!({"spindleMax": 24000, "coolants": coolants}),
            &stream,
        )
        .expect("post");
        assert!(
            out.warnings
                .iter()
                .any(|w| w.contains("operation ReviewCoolantOp:") && w.to_ascii_lowercase().contains(word)),
            "{ask} {coolants}: {:?}",
            out.warnings
        );
    }

    // Default coolant ask (Coolant param omitted) also names the operation.
    let unstated = program(CUT)
        .replace("OperationName A11", "OperationName UnstatedFloodOp")
        .replace("PARAM Coolant off\n", "");
    let out = engine::run(
        &script,
        &json!({"spindleMax": 24000, "coolants": [{"name": "Flood", "mCode": "M7"}]}),
        &unstated,
    )
    .expect("post");
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("operation UnstatedFloodOp:") && w.contains("requires M8, got M7")),
        "{:?}",
        out.warnings
    );

    // A prior dry section's OperationName does not leak into the next section.
    let dry = program(CUT).replace("OperationName A11", "OperationName DryPassOp");
    let wet = program(CUT)
        .replace("OperationName A11", "OperationName ReviewCoolantOp")
        .replace("Coolant off", "Coolant flood");
    let out = engine::run(
        &script,
        &json!({"spindleMax": 24000, "coolants": [{"name": "Flood", "mCode": "M7"}]}),
        &format!("{dry}{wet}"),
    )
    .expect("post");
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("operation ReviewCoolantOp:") && !w.contains("DryPassOp")),
        "{:?}",
        out.warnings
    );

    let nameless_wet = program(CUT)
        .replace("PARAM OperationName A11\n", "")
        .replace("Coolant off", "Coolant flood");
    let out = engine::run(
        &script,
        &json!({"spindleMax": 24000, "coolants": [{"name": "Flood", "mCode": "M7"}]}),
        &format!("{dry}{nameless_wet}"),
    )
    .expect("post");
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("requires M8, got M7") && !w.contains("DryPassOp")),
        "{:?}",
        out.warnings
    );
}

#[test]
fn a_bore_without_rapid_retract_is_refused_by_name() {
    let stream = hole("BORE", 0, " F 100000", true).replace(
        "Rapid X 10000 Y 20000 Z 2000\nRapid X 10000 Y 20000 Z 10000\n",
        "",
    );
    refused(&stream, &["rapid retract"]);
}

#[test]
fn nonpositive_cut_feed_is_refused_by_name() {
    refused(&program(CUT).replace("F 300000", "F 0"), &["feed"]);
}

#[test]
fn missing_tool_and_excessive_floor_dwell_are_refused_by_name() {
    refused(&program(CUT).replace("TOOL 1 ROUTER\n", ""), &["no tool"]);
    refused(&hole("DRILL", 2147483648, " F 100000", true), &["dwell"]);
}

#[test]
fn feeding_before_bore_restart_is_refused_by_name() {
    let stream = hole("BORE", 0, " F 100000", true).replace(
        "Rapid X 10000 Y 20000 Z 2000\nRapid X 10000 Y 20000 Z 10000\n",
        "Feed X 10000 Y 20000 Z 2000 F 100000\n",
    );
    refused(&stream, &["spindle stopped"]);
}

#[test]
fn vertical_planes_use_incremental_centres_restore_g17_and_full_circle_splits() {
    let out = run(
        &read("crates/post/tests/recorded/streams/arcs.cnc"),
        &Value::Null,
    );
    clean(&out);
    for words in ["G18", "G19", "K0.000I10.000", "J10.000K0.000"] {
        assert!(out.program.contains(words), "missing {words}: {}", out.program);
    }
    // Verify G17 is restored if a section ends in G18 or G19.
    let g19_end = program(
        "Rapid X 0 Y 20000 Z 0\nArc CCW X 0 Y 30000 Z -10000 F 600000 J 30000 K -10000\nRapid X 0 Y 30000 Z 10000\n",
    );
    let out_g19 = run(&g19_end, &Value::Null);
    clean(&out_g19);
    let b = bodies(&out_g19.program);
    let g19_pos = b.iter().rposition(|l| l.starts_with("G19")).expect("G19");
    let g17_restore = g19_pos + b[g19_pos..].iter().position(|l| *l == "G17").expect("G17 restored");
    let m5_pos = g19_pos + b[g19_pos..].iter().position(|l| *l == "M05").expect("M05");
    assert!(g17_restore < m5_pos, "{}", out_g19.program);

    let circle = program(
        "Rapid X 10000 Y 0 Z 5000\nPlunge X 10000 Y 0 Z 0 F 300000\nArc CCW X 10000 Y 0 Z 0 F 800000 I 0 J 0\n",
    );
    let out = run(&circle, &Value::Null);
    clean(&out);
    let arcs: Vec<&str> = bodies(&out.program).into_iter().filter(|l| starts_word(l, "G3")).collect();
    assert_eq!(arcs, ["G3X-10.000Y0.000Z0.000I-10.000J0.000F800.0", "G3X10.000Y0.000Z0.000I10.000J0.000"], "{}", out.program);
}

#[test]
fn rotary_moves_emit_one_warning_per_program() {
    let out = run(
        &read("crates/post/tests/recorded/streams/rotary.cnc"),
        &Value::Null,
    );
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    assert!(out.warnings[0].to_ascii_lowercase().contains("rotary"));
}

#[test]
fn comments_are_ascii_parentheses_and_active_flood_stops() {
    let stream = program(CUT)
        .replace("OperationName A11", "OperationName A11 (x);% café")
        .replace("Coolant off", "Coolant flood");
    let out = engine::run(
        &read("assets/posts/richauto.cps"),
        &json!({"spindleMax": 24000, "coolants": [{"name": "Flood", "mCode": "M8"}]}),
        &stream,
    )
    .expect("post");
    clean(&out);
    assert!(out.program.is_ascii(), "{}", out.program);
    let comment = out
        .program
        .lines()
        .find(|line| line.contains("OPERATION:"))
        .expect("operation comment");
    assert!(
        comment.starts_with('(') && comment.ends_with(')'),
        "{comment}"
    );
    assert_eq!(comment.matches('(').count(), 1, "{comment}");
    assert_eq!(comment.matches(')').count(), 1, "{comment}");
    assert!(
        !comment.contains(';') && !comment.contains('%'),
        "{comment}"
    );
    let b = bodies(&out.program);
    assert!(b.contains(&"M8"), "{}", out.program);
    let end = b.iter().position(|l| *l == "M05").expect("M05");
    assert_eq!(b[end..], ["M05", "M9", "M30", "%"], "{}", out.program);
}

/// #3442 / #3474 (review of PR #3294): an arc under the three-decimal
/// output step is no arc block. The A11 reads a block whose written end is
/// its written start as a full circle. Two flat arcs post no block, the
/// helix posts its straight Z move, the XZ arc posts no block and no G18,
/// and the real quarter arc stays. A short arc whose written ends are on
/// one ray from the written centre posts as its straight move.
#[test]
fn an_arc_under_the_output_step_is_no_arc_block() {
    let f = "F 180000";
    let out = run(
        &program(&format!(
            "Rapid X 18123 Y 5432 Z 5500\nPlunge X 18123 Y 5432 Z 5500 F 300000\n\
             Arc CCW X 18123 Y 5432.03 Z 5500 {f} I 10123 J 5432\n\
             Arc CCW X 18123 Y 5432.04 Z 5500 {f} I 10123 J 5432\n\
             Arc CW X 18123 Y 5432.02 Z 5400 {f} I 10123 J 5432\n\
             Arc CCW X 18123 Y 5432.02 Z 5399.97 {f} I 10123 K 5400\n\
             Arc CCW X 10123 Y 13432 Z 5400 {f} I 10123 J 5432\n\
             Rapid X 10123 Y 13432 Z 10000\n"
        )),
        &Value::Null,
    );
    clean(&out);
    let b = bodies(&out.program);
    let arcs: Vec<&str> = b.iter().copied().filter(|l| starts_word(l, "G2") || starts_word(l, "G3")).collect();
    assert_eq!(arcs, ["G3X10.123Y13.432Z5.400I-8.000J0.000"], "{}", out.program);
    assert!(b.contains(&"G1Z5.400F180.0"), "the helix posts its Z move:\n{}", out.program);
    assert!(!b.iter().any(|l| l.contains("G18")), "{}", out.program);

    let out = run(
        &program(
            "Rapid X 10010.51 Y 5000 Z 5500\nPlunge X 10010.51 Y 5000 Z 5500 F 300000\n\
             Arc CCW X 10010.498393 Y 5000.493788 Z 5500 F 180000 I 10000 J 5000\n\
             Rapid X 10010.498393 Y 5000.493788 Z 10000\n",
        ),
        &Value::Null,
    );
    clean(&out);
    let b = bodies(&out.program);
    assert!(!b.iter().any(|l| starts_word(l, "G2") || starts_word(l, "G3")), "{}", out.program);
    assert!(b.contains(&"G1X10.010F180.0"), "the same-angle arc posts its straight move:\n{}", out.program);
}

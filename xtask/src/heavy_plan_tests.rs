//! Pins the heavy tier (#2568): the diff-to-heavy-binary mapping, also through the crate dependencies.
//! Provenance: our own build law (AGENTS.md "Heavy lanes at review"); cargo's target selectors.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::plan::{self, Plan, Step};
use crate::{heavy, parse_gate};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn heavy_plan(changed: &[&str]) -> Plan {
    let changed: Vec<String> = changed.iter().map(|s| (*s).to_string()).collect();
    heavy::plan(&root(), &changed).expect("heavy plan")
}

fn names(p: &Plan) -> Vec<&str> {
    p.steps.iter().map(|s| s.name.as_str()).collect()
}

/// The `-p` packages of the release build step (the first step of a plan that has steps).
fn built(p: &Plan) -> Vec<&str> {
    let build = p.steps.first().expect("build step");
    assert_eq!(build.name, "heavy build (release --no-run)");
    build.args.windows(2).filter(|w| w[0] == "-p").map(|w| w[1].as_str()).collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn gate_heavy_is_a_tier_with_a_base() {
    let a = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
    let g = parse_gate(&a("heavy --base origin/main --dry-run")).expect("heavy is a gate tier");
    assert_eq!((g.tier.as_str(), g.base.as_str(), g.dry), ("heavy", "origin/main", true));
    assert!(parse_gate(&a("heavy --no-snapshots")).is_err(), "--no-snapshots stays release-only");
}

#[test]
fn reach_follows_the_crate_dependencies() {
    let reach = heavy::reach(&root()).expect("manifests");
    assert_eq!(reach["app"], set(&["app", "ui"]));
    assert_eq!(reach["motion"], set(&["app", "motion", "ui"]));
    assert_eq!(reach["geometry"], set(&["app", "cl", "gcode", "geometry", "motion", "post", "sim", "tools", "ui"]));
    // gpu (#2620) has no camcraft-* dependency: the kernel reaches every crate but it.
    let all_but_gpu = plan::ALL_CRATES.iter().filter(|d| **d != "gpu").map(|d| (*d).to_string());
    assert_eq!(reach["kernel"], all_but_gpu.collect::<BTreeSet<_>>());
    assert_eq!(reach["gpu"], set(&["gpu", "ui"]));
}

#[test]
fn a_motion_source_change_reaches_the_motion_and_app_heavy_lanes() {
    let p = heavy_plan(&["crates/motion/src/finish3d.rs"]);
    assert_eq!(built(&p), ["camcraft-motion", "camcraft-app"]);
    // Exactly the release tier's steps of the two crates: same names, same commands, none of kernel.
    let want: Vec<Step> = plan::heavy_steps(&root())
        .into_iter()
        .skip(1)
        .filter(|s| !s.args.contains(&"camcraft-kernel".to_string()))
        .collect();
    assert_eq!(p.steps[1..], want[..]);
    let n = names(&p);
    for step in ["motion heavy: lib", "motion heavy: finish3d", "app heavy: lib", "app heavy: pocket3d_hsm", "app heavy: doc"] {
        assert!(n.contains(&step), "{step} missing in {n:?}");
    }
    assert!(p.notes.iter().any(|l| l.contains("app heavy: every target - depends on motion")), "{:#?}", p.notes);
}

#[test]
fn a_changed_suite_reaches_only_its_own_heavy_binary() {
    let p = heavy_plan(&["crates/app/tests/lathe_field_sim.rs"]);
    assert_eq!(names(&p), ["heavy build (release --no-run)", "app heavy: lathe_field_sim"]);
    assert_eq!(built(&p), ["camcraft-app"]);
    let run = &p.steps[1].args;
    assert!(run.contains(&"--release".to_string()) && run.last().map(String::as_str) == Some("--ignored"), "{run:?}");
    assert!(run.windows(2).any(|w| w == ["--test", "lathe_field_sim"]), "{run:?}");
}

#[test]
fn shared_test_code_reaches_the_heavy_lanes_of_its_own_crate_only() {
    let p = heavy_plan(&["crates/motion/tests/common/shape_mask.rs"]);
    assert_eq!(built(&p), ["camcraft-motion"]);
    assert!(names(&p).iter().skip(1).all(|n| n.starts_with("motion heavy: ")), "{:?}", names(&p));
    assert!(names(&p).contains(&"motion heavy: finish3d"));
}

#[test]
fn a_kernel_or_workspace_change_reaches_every_heavy_lane() {
    let all = plan::heavy_steps(&root());
    for f in ["crates/kernel/src/lib.rs", "Cargo.lock", "Cargo.toml", ".cargo/config.toml", "rust-toolchain.toml"] {
        assert_eq!(heavy_plan(&[f]).steps, all, "{f}");
    }
    // geometry sits above kernel: it reaches motion and app, not the kernel lanes.
    assert_eq!(built(&heavy_plan(&["crates/geometry/src/lib.rs"])), ["camcraft-motion", "camcraft-app"]);
}

#[test]
fn a_diff_outside_the_heavy_crates_reaches_no_heavy_lane() {
    let p = heavy_plan(&[
        "crates/ui/src/egui_app/mod.rs",
        "crates/ui/tests/gpu_render.rs",
        "crates/app/README.md",
        "docs/decisions/README.md",
        "xtask/src/heavy.rs",
        "AGENTS.md",
    ]);
    assert!(p.steps.is_empty(), "{:?}", names(&p));
    assert!(p.notes.iter().any(|l| l.contains("reaches no heavy lane")), "{:#?}", p.notes);
}

#[test]
fn a_catalog_case_reaches_the_binary_that_reads_it() {
    let changed = vec!["examples/catalog/2d/pocket/case.toml".to_string()];
    let r = heavy::reached(&root(), &changed).expect("reached");
    assert!(r.crates.is_empty(), "{r:?}");
    assert_eq!(r.targets.keys().map(String::as_str).collect::<Vec<_>>(), ["app:example_catalog"]);
    assert!(r.keeps("app", "example_catalog") && !r.keeps("app", "lib") && !r.keeps("motion", "lib"));
}

#[test]
fn an_unreadable_manifest_is_an_error_not_a_smaller_plan() {
    let empty = std::env::temp_dir().join(format!("xtask-heavy-no-manifest-{}", std::process::id()));
    let e = heavy::reach(&empty).expect_err("no manifest");
    assert!(e.contains("gate heavy: cannot read") && e.contains("Cargo.toml"), "{e}");
}

#[test]
fn review_requires_targeted_heavy_tests_on_both_paths() {
    // #3556: a review runs only targeted tests, not the full heavy lanes. The
    // old demand must not come back, and "not run" stays a blocker. A runner
    // verdict still feeds step 5, where a `P1` targeted step runs.
    let p = heavy_plan(&["crates/motion/src/finish3d.rs"]);
    for lane in ["motion heavy: finish3d", "app heavy: pocket3d_hsm"] {
        assert!(names(&p).contains(&lane), "{lane} must stay in the reach map");
    }
    let policy = include_str!("../../.claude/agents/PR-reviewer.md");
    let runner = policy.split("3. **Worktree + trial merge**").next().expect("runner path");
    assert!(
        runner.contains("and 4, then continue at step 5.")
            && runner.contains("The step 5 check is required on both paths."),
        "a runner verdict must still reach the common heavy check"
    );
    let common = policy
        .split("5. **Heavy check on both paths**")
        .nth(1)
        .expect("heavy check outside skipped local steps")
        .split("6. **Law check against AGENTS.md**")
        .next()
        .expect("common heavy check before law check");
    for required in [
        "This step is required after runner results and after local tests",
        "Do not\n   run the full heavy lanes for a PR (#3556)",
        "no PR is \"owed heavy\"",
        "**Owed heavy result** is retired",
        "Example catalog: if the diff changes a post or a toolpath",
        "Light lane: run `cargo xtask gate light` once",
        "`P1` under the safety floor (wrong output): one targeted heavy step",
        "cargo xtask gate heavy --base <main ref> --only \"<crate> heavy: <target>\"",
        "Never run the whole lane.",
        "report \"targeted\n     heavy step not run\" as a blocker, never a pass",
        "A red targeted step is\n     a blocker.",
        "`P2` or lower: no heavy step.",
        "Its absence is not a finding",
    ] {
        assert!(common.contains(required), "common heavy check must require: {required}");
    }
    // The report keeps "not run" out of the pass column.
    assert!(policy.contains("\"Not run\" is never a pass\n     for a required step"), "a required step never passes as \"not run\"");
    // The old full-lane demand does not return.
    for retired in [
        "report \"heavy lanes not run\" as a blocker",
        "cargo xtask gate heavy --base <main ref>` locally",
        "head and all reached lanes match",
        "If the author result is absent, stale or incomplete, run",
        "A red reached\n   heavy test is a blocker",
    ] {
        assert!(!common.contains(retired), "the retired full-lane demand must not return: {retired}");
    }
}

#[test]
fn review_report_always_has_a_light_lane_result_row() {
    // #3846: mentioning the light lane in prose did not keep it in reports.
    let policy = include_str!("../../.claude/agents/PR-reviewer.md");
    let report = policy
        .split("7. **Post the review**")
        .nth(1)
        .and_then(|s| s.split("8. **Label**").next())
        .expect("review report template");
    for required in [
        "| Step | Result | Head | Evidence |",
        "| Light lane (`cargo xtask gate light`) | <PASS / FAIL / Not run> | `<headRefOid>` | <where; counts and log/run URL, or reason not run> |",
        "Include this fixed row in every review comment, also when not built.",
        "never leave the result blank",
        "A red light run stays a note, not a blocker by itself.",
    ] {
        assert!(report.contains(required), "review report must keep: {required}");
    }
}

#[test]
fn an_owed_heavy_result_is_allowed_for_p3_and_lower_only() {
    // #3467 allowed a P3-or-lower review to defer the heavy lanes; #3556
    // retired that rule. No review defers a heavy result and no merge waits
    // for one. The old deferral must not come back.
    let policy = include_str!("../../.claude/agents/PR-reviewer.md");
    let common = policy
        .split("5. **Heavy check on both paths**")
        .nth(1)
        .and_then(|s| s.split("6. **Law check against AGENTS.md**").next())
        .expect("step 5");
    assert!(common.contains("no PR is \"owed heavy\""), "step 5 retires the owed rule");
    assert!(common.contains("**Owed heavy result** is retired"), "step 5 names the retired rule");
    for retired in [
        "**Owed heavy result**, #3467",
        "`P3` or lower:",
        "\"not run - owed to the wave run before merge\"",
        "owed to the wave run",
        "The PR can get `merge-ready`",
    ] {
        assert!(!common.contains(retired), "the retired owed rule must not return: {retired}");
    }

    let law = include_str!("../../AGENTS.md");
    let rule = law.split("**Owed heavy result** (#3467)").nth(1).expect("AGENTS.md retirement");
    let rule = rule.split("\n- **").next().expect("rule end");
    for required in [
        "is retired (#3556)",
        "A review never writes\n    \"owed to the wave run\"",
        "no merge waits for a heavy record",
    ] {
        assert!(rule.contains(required), "AGENTS.md owed rule must say: {required}");
    }

    let merge = include_str!("../../.claude/skills/merge-ready/SKILL.md");
    assert!(merge.contains("no merge waits for a heavy lane result\n(#3556)"), "a merge does not wait for a heavy result");
    assert!(
        merge.contains("A PR is never \"owed heavy\"\n(**Owed heavy result** is retired)"),
        "the merge step retires the owed rule"
    );
    for retired in ["owed to the wave run before merge", "never merge a PR whose review says \"not"] {
        assert!(!merge.contains(retired), "the retired owed refusal must not return: {retired}");
    }
}

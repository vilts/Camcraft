//! Pins the light tier (#2569): its plan, the twin contract, the first light twin and the red-run note.
//! Provenance: our own build law (AGENTS.md "Three gate tiers", "The light lane"); libtest's `--ignored` and name filters.

use std::path::{Path, PathBuf};

use crate::plan::Step;
use crate::{light, parse_gate, scan, select};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

const SLOT_FILE: &str = "crates/motion/src/machining2d/adaptive/tests.rs";
const SLOT_FULL: &str = "the_slot_keeps_its_verdict_under_sub_micron_perturbations";

fn a(s: &str) -> Vec<String> {
    s.split_whitespace().map(String::from).collect()
}

#[test]
fn gate_light_is_a_tier_of_its_own() {
    let g = parse_gate(&a("light --dry-run")).expect("light");
    assert_eq!((g.tier.as_str(), g.dry), ("light", true));
    // The snapshot flag stays a release flag; the step selection works as for each tier.
    assert!(parse_gate(&a("light --no-snapshots")).is_err());
    assert_eq!(parse_gate(&a("light --only motion")).expect("only").only, ["motion"]);
    let heavy = parse_gate(&a("heavy --base upstream/main --dry-run")).expect("heavy");
    assert_eq!((heavy.tier.as_str(), heavy.base.as_str(), heavy.dry), ("heavy", "upstream/main", true));
    for tier in ["heavy", "light"] {
        assert!(crate::USAGE.contains(&format!("cargo xtask gate {tier}")), "help names {tier}");
    }
    // The refresh keeps main's batch line (#2727) next to the light line (#2569).
    let batch = crate::USAGE.lines().find(|l| l.contains("cargo xtask gate batch")).expect("batch line");
    assert!(batch.contains("per-crate debug prebuilds") && batch.contains("#2727"), "{batch}");
    let selection = parse_gate(&a("test pin --only motion")).expect_err("test has no step selection");
    assert!(selection.contains("quick, heavy, batch, light or release"), "{selection}");
    let unknown = parse_gate(&a("nightly")).expect_err("unknown tier");
    assert!(unknown.contains("quick | heavy | batch | light | release | test"), "{unknown}");
}

#[test]
fn light_plan_is_one_release_build_then_one_step_for_each_target_with_a_twin() {
    let plan = light::plan(&root()).expect("light plan");
    let twins = light::discover(&root()).expect("twins");
    assert!(!twins.is_empty());
    let build = &plan.steps[0];
    assert_eq!(build.name, light::BUILD);
    assert!(build.args.contains(&"--release".to_string()) && build.args.contains(&"--no-run".to_string()));
    // Only the crates that hold a twin are built: the light tier stays at a few minutes.
    let dirs: std::collections::BTreeSet<&str> = twins.iter().map(|t| t.dir.as_str()).collect();
    let built: Vec<&str> = build.args.windows(2).filter(|w| w[0] == "-p").map(|w| w[1].as_str()).collect();
    assert_eq!(built.len(), dirs.len());
    for d in &dirs {
        assert!(built.contains(&format!("camcraft-{d}").as_str()), "{d} not built: {built:?}");
    }
    let step: &Step = plan.steps.iter().find(|s| s.name == "motion light: lib").expect("motion lib step");
    let slot_light = format!("{SLOT_FULL}_light");
    let mut want = vec!["test", "--release", "-j", crate::plan::J, "-p", "camcraft-motion", "--no-fail-fast", "--lib", "--", crate::plan::T, "--ignored"];
    let names: Vec<&str> = twins.iter().filter(|t| t.dir == "motion" && t.target == "lib").map(|t| t.test.as_str()).collect();
    assert!(names.contains(&slot_light.as_str()), "{names:?}");
    want.extend(names);
    assert_eq!(step.args, want);
    // Each step after the build runs ignored tests in release, and names at least one twin.
    for s in &plan.steps[1..] {
        assert!(s.name.contains(" light: "), "{}", s.name);
        let tail = s.args.iter().position(|x| x == "--ignored").map(|i| &s.args[i + 1..]).unwrap_or_default();
        assert!(!tail.is_empty() && tail.iter().all(|n| n.ends_with(light::SUFFIX)), "{}: {:?}", s.name, s.args);
    }
    // `--only` selects a light step as it selects a step of each other tier.
    let one = select::apply(plan, &["motion light: lib".to_string()], None).expect("only");
    assert_eq!(one.steps.len(), 1);
}

#[test]
fn the_slot_sweep_keeps_its_full_test_and_has_a_light_twin() {
    let fns = scan::test_fns(&scan::read(&root().join(SLOT_FILE))).expect("scan");
    let full = fns.iter().find(|t| t.name == SLOT_FULL).expect("the full sweep stays");
    assert!(full.ignored, "the full sweep stays in the heavy lane");
    let twin = fns.iter().find(|t| t.name == format!("{SLOT_FULL}_light")).expect("light twin");
    assert!(twin.ignored, "the light twin runs in release only");
    let twins = light::discover(&root()).expect("twins");
    let found = twins.iter().find(|t| t.test == twin.name).expect("discovered");
    assert_eq!((found.dir.as_str(), found.target.as_str(), found.file.as_str()), ("motion", "lib", SLOT_FILE));
    // The heavy lane still holds the full test: the twin does not take its place.
    let text = scan::read(&root().join(SLOT_FILE));
    let attrs = text.split_once(&format!("fn {SLOT_FULL}()")).expect("full fn").0;
    let attrs = attrs.rsplit("#[test]").next().expect("attributes");
    assert!(attrs.contains("#[ignore = \"heavy lane"), "{attrs}");
}

const GOOD: &str = "#[test]\n#[ignore = \"heavy lane: big\"]\nfn sweep() {\n}\n#[test]\n#[ignore = \"light lane: two rows of sweep\"]\nfn sweep_light() {\n}\n";

#[test]
fn a_light_twin_needs_its_name_and_its_full_test_next_to_it() {
    let got = light::parse(GOOD, "motion", "lib", "f.rs").expect("good");
    assert_eq!(got.len(), 1);
    assert_eq!((got[0].test.as_str(), got[0].target.as_str()), ("sweep_light", "lib"));
    // No full test in the file: the twin would stand in for a test that is not there.
    let alone = GOOD.replace("fn sweep()", "fn other()");
    let e = light::parse(&alone, "motion", "lib", "f.rs").expect_err("no full test");
    assert!(e.contains("sweep_light") && e.contains("f.rs") && e.contains("sweep"), "{e}");
    // The full test left the ignored set: its golden asserts must stay there.
    let unignored = GOOD.replace("#[ignore = \"heavy lane: big\"]\n", "");
    assert!(light::parse(&unignored, "motion", "lib", "f.rs").is_err());
    // A twin of a twin is not a full test.
    let chained = GOOD.replace("heavy lane: big", "light lane: small");
    assert!(light::parse(&chained, "motion", "lib", "f.rs").is_err());
    // The name is the contract of the libtest filter.
    let named = GOOD.replace("fn sweep_light()", "fn sweep_small()");
    let e = light::parse(&named, "motion", "lib", "f.rs").expect_err("name");
    assert!(e.contains("sweep_small") && e.contains("_light"), "{e}");
    // A file without the marker has no twin.
    assert!(light::parse("#[test]\nfn plain() {\n}\n", "motion", "lib", "f.rs").expect("none").is_empty());
}

#[test]
fn a_red_light_run_names_the_full_heavy_rerun_of_each_red_step() {
    assert!(light::rerun_note(&[]).is_empty(), "a green run has no note");
    let note = light::rerun_note(&["motion light: lib".to_string(), "app light: pocket3d_hsm".to_string()]).join("\n");
    assert!(note.contains("cargo xtask gate release --only \"motion heavy: lib\""), "{note}");
    assert!(note.contains("cargo xtask gate release --only \"app heavy: pocket3d_hsm\""), "{note}");
    assert!(note.contains("does not replace the cut gate"), "{note}");
    // The named steps exist in the release plan: the printed command selects exactly one step.
    let one = select::apply(crate::plan::release(&root()), &["motion heavy: lib".to_string()], None).expect("heavy step");
    assert_eq!(one.steps.len(), 1);
    // A red build has no heavy twin step: the note names the heavy build.
    let build = light::rerun_note(&[light::BUILD.to_string()]).join("\n");
    assert!(build.contains("--only \"heavy build\""), "{build}");
}

#[test]
fn the_law_names_the_light_lane_and_the_local_full_rerun() {
    let law = scan::read(&root().join("AGENTS.md"));
    assert!(law.contains("**The light lane**") && law.contains("`cargo xtask gate light`"), "AGENTS.md names the light tier");
    assert!(law.contains("**Heavy lanes at review**") && law.contains("`cargo xtask gate heavy [--base main]`"), "AGENTS.md keeps the heavy tier");
    assert!(law.contains("**Local full rerun**") && law.contains("--failed gate-logs/<run>"), "AGENTS.md names the local full rerun");
    assert!(law.contains("A local run never replaces\n  the cut gate") || law.contains("A local run never\n  replaces the cut gate"), "a local run is not the cut gate");
}

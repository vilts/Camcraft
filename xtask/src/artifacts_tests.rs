//! Provenance: issue #1951 - reading cargo's JSON artifact list and choosing the steps that run a test
//! executable directly. The message texts are in the form that cargo 1.100 prints (Windows paths included).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::artifacts::{self, Built, Package, TestExe};
use crate::direct;
use crate::json::{self, Json};
use crate::plan::{self, Step};

#[test]
fn json_reads_cargo_lines_and_refuses_bad_text() {
    let v = json::parse(r#" {"a":"C:\\x\\y.exe","b":[true,null,-1.5e1],"c":{"d":"q\"\u00e9\ud83d\ude00\n"}} "#).expect("valid");
    assert_eq!(v.text("a"), Some(r"C:\x\y.exe"));
    assert_eq!(v.get("b").map(Json::items), Some(&[Json::Bool(true), Json::Null, Json::Num(-15.0)][..]));
    assert_eq!(v.get("c").and_then(|c| c.text("d")), Some("q\"\u{e9}\u{1F600}\n"));
    assert_eq!(v.text("b"), None, "an array is not a string");
    assert_eq!(json::parse("[]"), Some(Json::Arr(Vec::new())));
    for bad in ["", "{", "{\"a\":}", "[1,]", "\"open", "{\"a\":1} x", "\"\\q\"", "nul", "\"\\u12\"", "--", "{\"a\" 1}"] {
        assert_eq!(json::parse(bad), None, "{bad:?} is not JSON");
    }
    let deep = format!("{}{}", "[".repeat(5000), "]".repeat(5000));
    assert_eq!(json::parse(&deep), None, "deep nesting is refused, not a stack overflow");
}

const META: &str = r#"{"packages":[
 {"name":"camcraft-cl","version":"0.1.0","id":"path+file:///w/crates/cl#camcraft-cl@0.1.0","manifest_path":"/w/crates/cl/Cargo.toml","authors":[]},
 {"name":"camcraft-app","version":"0.24.1-rc.2+b7","id":"path+file:///w/crates/app#camcraft-app@0.24.1-rc.2","license":"MIT","license_file":null,
  "description":"the app","manifest_path":"/w/crates/app/Cargo.toml","authors":["A","B"],"readme":"README.md","repository":null,"homepage":null,"rust_version":"1.90"}],
 "target_directory":"/w/target","workspace_root":"/w"}"#;

fn artifact(manifest: &str, kind: &str, name: &str, src: &str, test: bool, exe: Option<&str>) -> String {
    let exe = exe.map_or("null".to_string(), |e| format!("\"{e}\""));
    format!(
        r#"{{"reason":"compiler-artifact","package_id":"x","manifest_path":"{manifest}","target":{{"kind":["{kind}"],"name":"{name}","src_path":"{src}"}},"profile":{{"test":{test}}},"filenames":[],"executable":{exe},"fresh":true}}"#
    )
}

fn messages(finished: &str) -> String {
    let app = "/w/crates/app/Cargo.toml";
    [
        "   Compiling camcraft-app v0.24.1 (not JSON: skipped)".to_string(),
        artifact("/w/crates/cl/Cargo.toml", "lib", "camcraft_cl", "/w/crates/cl/src/lib.rs", true, Some("/w/target/debug/deps/camcraft_cl-1")),
        artifact(app, "custom-build", "build-script-build", "/w/crates/app/build.rs", false, None),
        r#"{"reason":"build-script-executed","package_id":"other","linked_paths":["native=/w/target/debug/build/flecs/out","/elsewhere/lib"],"env":[["NOT_OURS","1"]],"out_dir":"/w/target/o"}"#.to_string(),
        r#"{"reason":"build-script-executed","package_id":"path+file:///w/crates/app#camcraft-app@0.24.1-rc.2","linked_paths":[],"env":[["APP_BUILT","yes"]],"out_dir":"/w/target/debug/build/app/out"}"#.to_string(),
        artifact(app, "lib", "camcraft_app", "/w/crates/app/src/lib.rs", false, None),
        artifact(app, "bin", "camcraft-cli", "/w/crates/app/src/bin/cli.rs", false, Some("/w/target/debug/camcraft-cli")),
        artifact(app, "lib", "camcraft_app", "/w/crates/app/src/lib.rs", true, Some("/w/target/debug/deps/camcraft_app-2")),
        artifact(app, "bin", "camcraft-cli", "/w/crates/app/src/bin/cli.rs", true, Some("/w/target/debug/deps/camcraft_cli-3")),
        artifact(app, "test", "smoke", "/w/crates/app/tests/smoke.rs", true, Some("/w/target/debug/deps/smoke-4")),
        artifact(app, "example", "demo", "/w/crates/app/examples/demo.rs", true, Some("/w/target/debug/examples/demo-5")),
        finished.to_string(),
    ]
    .join("\n")
}

const OK: &str = r#"{"reason":"build-finished","success":true}"#;

fn app() -> Package {
    artifacts::package("camcraft-app", META, &messages(OK)).expect("recorded")
}

#[test]
fn package_takes_only_its_own_test_executables_from_the_json() {
    let p = app();
    assert_eq!(p.dir, PathBuf::from("/w/crates/app"));
    let exe = |p: &TestExe| p.exe.to_string_lossy().into_owned();
    assert_eq!(p.lib.iter().map(exe).collect::<Vec<_>>(), ["/w/target/debug/deps/camcraft_app-2"]);
    assert_eq!(p.bins.iter().map(exe).collect::<Vec<_>>(), ["/w/target/debug/deps/camcraft_cli-3"], "the test build of the bin, not the app itself");
    assert_eq!(p.tests.len(), 1);
    assert_eq!((p.tests[0].0.as_str(), exe(&p.tests[0].1)), ("smoke", "/w/target/debug/deps/smoke-4".to_string()));
    assert_eq!(p.count(), 3, "no example, no build script, no other package, no plain binary");
    // The labels are cargo's `Running` texts.
    assert_eq!(p.lib[0].label, "unittests src/lib.rs");
    assert_eq!(p.bins[0].label, "unittests src/bin/cli.rs");
    assert_eq!(p.tests[0].1.label, "tests/smoke.rs");
}

#[test]
fn package_gives_the_variables_of_a_cargo_test_process() {
    let p = app();
    let var = |k: &str| p.env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    for (k, want) in [
        ("CARGO_MANIFEST_DIR", "/w/crates/app"),
        ("CARGO_MANIFEST_PATH", "/w/crates/app/Cargo.toml"),
        ("CARGO_PKG_NAME", "camcraft-app"),
        ("CARGO_PKG_VERSION", "0.24.1-rc.2+b7"),
        ("CARGO_PKG_VERSION_MAJOR", "0"),
        ("CARGO_PKG_VERSION_MINOR", "24"),
        ("CARGO_PKG_VERSION_PATCH", "1"),
        ("CARGO_PKG_VERSION_PRE", "rc.2"),
        ("CARGO_PKG_AUTHORS", "A:B"),
        ("CARGO_PKG_DESCRIPTION", "the app"),
        ("CARGO_PKG_HOMEPAGE", ""),
        ("CARGO_PKG_REPOSITORY", ""),
        ("CARGO_PKG_LICENSE", "MIT"),
        ("CARGO_PKG_LICENSE_FILE", ""),
        ("CARGO_PKG_RUST_VERSION", "1.90"),
        ("CARGO_PKG_README", "README.md"),
        ("CARGO_BIN_EXE_camcraft-cli", "/w/target/debug/camcraft-cli"),
        ("OUT_DIR", "/w/target/debug/build/app/out"),
        ("APP_BUILT", "yes"),
    ] {
        assert_eq!(var(k), Some(want), "{k}");
    }
    assert_eq!(var("NOT_OURS"), None, "the build script variables of another package stay out");
    let want = ["/w/target/debug/build/flecs/out", "/w/target/debug", "/w/target/debug/deps"];
    assert_eq!(p.lib_paths, want.map(PathBuf::from), "linked paths in the target dir (not /elsewhere), then the profile dirs");
}

#[test]
fn package_refuses_a_build_that_did_not_finish_or_a_package_it_does_not_know() {
    let red = artifacts::package("camcraft-app", META, &messages(r#"{"reason":"build-finished","success":false}"#));
    assert_eq!(red, Err("the build did not finish with success".into()));
    assert_eq!(artifacts::package("camcraft-app", META, &messages("")), Err("the build printed no build-finished message".into()));
    assert_eq!(artifacts::package("camcraft-ui", META, &messages(OK)), Err("cargo metadata has no package camcraft-ui".into()));
    assert_eq!(artifacts::package("camcraft-app", "not json", &messages(OK)), Err("cargo metadata did not print JSON".into()));
}

fn built() -> Built {
    Built::from([("camcraft-app".to_string(), app())])
}

fn suite(sel: &[&str]) -> Step {
    let mut args = vec!["test", "-j", "2", "-p", "camcraft-app", "--no-fail-fast"];
    args.extend(sel);
    args.extend(["--", "--test-threads=2"]);
    Step::new("app suites: x", &args, "app")
}

#[test]
fn only_a_single_crate_debug_build_with_json_records() {
    assert_eq!(direct::records(&plan::debug_build_crate("app")), Some("camcraft-app".into()));
    assert_eq!(direct::records(&plan::debug_build(&["cl", "app"])), None, "the release tier's one build does not ask for JSON");
    let two = Step::new("b", &["test", "-p", "camcraft-cl", "-p", "camcraft-app", "--no-run", direct::JSON], "");
    assert_eq!(direct::records(&two), None, "a build of several crates unifies features: its executables are other ones");
    assert_eq!(direct::records(&Step::new("b", &["test", "--no-run", direct::JSON], "")), None, "no package named");
    assert_eq!(direct::records(&Step::new("b", &["test", "-p", "camcraft-app", "--no-run"], "")), None, "no JSON, no record");
    assert_eq!(direct::records(&Step::new("b", &["test", "--release", "-p", "camcraft-app", "--no-run", direct::JSON], "")), None);
    assert_eq!(direct::records(&suite(&["--lib"])), None);
}

#[test]
fn a_debug_target_step_of_a_recorded_package_runs_its_executable() {
    let b = built();
    let of = |s: &Step| direct::of(s, &s.args, &b, "the-cargo");
    let d = of(&suite(&["--test", "smoke"])).expect("direct");
    assert_eq!(d.exes, [b["camcraft-app"].tests[0].1.clone()]);
    assert_eq!(d.cwd, b["camcraft-app"].dir, "cargo starts a test in the package directory");
    assert_eq!(d.args, ["--test-threads=2"], "the libtest args after `--` are kept");
    assert_eq!(d.env[0], ("CARGO".to_string(), OsString::from("the-cargo")));
    assert!(d.env.iter().any(|(k, v)| k == "CARGO_PKG_NAME" && v == "camcraft-app"));
    assert!(d.env.iter().any(|(k, _)| k == direct::LIB_PATH_VAR));
    assert_eq!(of(&suite(&["--lib"])).map(|d| d.exes), Some(b["camcraft-app"].lib.clone()));
    assert_eq!(of(&suite(&["--bins"])).map(|d| d.exes), Some(b["camcraft-app"].bins.clone()));
    // Cargo keeps every step that is not exactly one recorded debug target.
    assert_eq!(of(&suite(&["--doc"])), None, "doctests have no executable");
    assert_eq!(of(&suite(&["--test", "not_built"])), None, "no executable in the list: never a guess at a path");
    assert_eq!(of(&suite(&["--test", "smoke", "--release"])), None);
    assert_eq!(of(&suite(&["--test", "smoke", "--features", "x"])), None, "an arg the gate does not know stays with cargo");
    assert_eq!(of(&suite(&["--lib", "--test", "smoke"])), None, "two targets are one cargo step");
    assert_eq!(of(&Step::new("s", &["test", "-j", "2", "-p", "camcraft-app", "--test", "smoke", "--", "--test-threads=2"], "")), None, "fail-fast stays with cargo");
    assert_eq!(of(&Step::new("s", &["test", "-j", "2", "-p", "camcraft-cl", "--no-fail-fast", "--lib"], "")), None, "no build of this package was recorded");
    assert_eq!(of(&Step::new("c", &["clippy", "-p", "camcraft-app"], "")), None);
    let mut known = suite(&["--test", "smoke"]);
    known.expect_fail = Some(crate::known_fail::KnownFail { issue: 1, dir: "app".into(), target: "smoke".into(), test: "t".into(), file: "f".into(), line: 1, reason: "r".into() });
    assert_eq!(of(&known), None, "a known-fail check stays as it is");
    assert_eq!(direct::of(&suite(&["--lib"]), &suite(&["--lib"]).args, &Built::new(), "c"), None);
}

#[test]
fn the_batch_plan_records_each_crate_and_keeps_its_steps() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
    let p = plan::batch(root);
    let builds: Vec<&Step> = p.steps.iter().filter(|s| crate::lanes::is_build(s)).collect();
    assert!(!builds.is_empty() && builds.iter().all(|s| direct::records(s).is_some()), "each batch prebuild records its artifacts");
    // With every target in the list, each suite step but a `--doc` one is a direct run; nothing else is.
    let mut all = Built::new();
    for s in &p.steps {
        let Some(key) = crate::lanes::target_key(s) else { continue };
        let (dir, target) = key.split_once(':').expect("dir:target");
        let pkg = all.entry(plan::pkg(dir)).or_default();
        let exe = TestExe { exe: PathBuf::from(format!("/t/{dir}-{target}")), label: target.to_string() };
        match target {
            "lib" => pkg.lib.push(exe),
            "bins" => pkg.bins.push(exe),
            "doc" => {}
            _ => pkg.tests.push((target.to_string(), exe)),
        }
    }
    let mut direct_steps = 0;
    for s in &p.steps {
        let d = direct::of(s, &s.args, &all, "cargo");
        let suite = s.name.contains(" suites: ") && !s.name.ends_with(": doc");
        assert_eq!(d.is_some(), suite, "{}", s.name);
        direct_steps += usize::from(d.is_some());
    }
    assert!(direct_steps > 100, "the batch suites are several hundred steps: {direct_steps}");
    let release = plan::release(root);
    assert!(release.steps.iter().all(|s| direct::records(s).is_none()), "the release tier is not changed: its one debug build unifies features");
}

#[test]
fn lib_path_puts_the_build_dirs_first_and_keeps_the_current_value() {
    let first = [PathBuf::from("/t/debug"), PathBuf::from("/t/debug/deps")];
    let cur = std::env::join_paths(["/tool/lib", "/usr/lib"]).expect("join");
    let got = direct::lib_path(&first, Some(cur), None).expect("joined");
    let dirs: Vec<PathBuf> = std::env::split_paths(&got).collect();
    assert_eq!(dirs, ["/t/debug", "/t/debug/deps", "/tool/lib", "/usr/lib"].map(PathBuf::from));
    let unset: Vec<PathBuf> = std::env::split_paths(&direct::lib_path(&first, None, Some("/home/u".into())).expect("joined")).collect();
    assert_eq!(&unset[..2], &first[..]);
    assert_eq!(unset.len() > 2, cfg!(target_os = "macos"), "only macOS gets cargo's default fallback dirs");
}

//! Starting the process of a gate step: cargo, or - for one debug test target of a crate whose build
//! step recorded its artifacts - the test executable itself, as `cargo test` starts it: in the package
//! directory, with cargo's variables, its args after `--` (#1951). One cargo start costs about 0.5 s
//! on Windows, and batch has several hundred such steps.
//! Provenance: our own build law (AGENTS.md "Three gate tiers"); the Cargo Book (`cargo test`, "JSON
//! messages", "Environment variables"); libtest's CLI.

use std::ffi::OsString;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::artifacts::{self, Built, Package, TestExe};
use crate::plan::Step;

/// The cargo flag of a build step whose test executables the gate records.
pub const JSON: &str = "--message-format=json-render-diagnostics";

/// The package of a build step that records: a debug `--no-run` build of exactly one package with
/// `JSON`. A build of several packages unifies their features, so its executables are not the ones
/// that `cargo test -p <one crate>` runs (#2727); it never records.
pub fn records(s: &Step) -> Option<String> {
    let a = &s.args;
    let flag = |f: &str| a.iter().any(|x| x == f);
    if !(flag("--no-run") && flag(JSON)) || flag("--release") {
        return None;
    }
    let mut pkgs = a.windows(2).filter(|w| w[0] == "-p").map(|w| w[1].clone());
    let one = pkgs.next()?;
    pkgs.next().is_none().then_some(one)
}

/// `CAMCRAFT_GATE_NO_DIRECT=1` makes each step a cargo step again (a comparison run, or a platform where
/// a direct run does not behave as `cargo test`).
pub const OPT_OUT: &str = "CAMCRAFT_GATE_NO_DIRECT";

pub fn opted_out() -> bool {
    std::env::var_os(OPT_OUT).is_some_and(|v| v == "1")
}

/// `of`, unless the environment opts out.
pub fn chosen(s: &Step, args: &[String], built: &Built, cargo: &str) -> Option<Direct> {
    if opted_out() { None } else { of(s, args, built, cargo) }
}

/// A step as a direct run: the executables (in cargo's order), their working directory, args and variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Direct {
    pub exes: Vec<TestExe>,
    pub cwd: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, OsString)>,
}

/// The direct run of `s` (`args` are its scaled args), or `None` when cargo must run it: a step that is
/// not exactly `test -j N -p <pkg> --no-fail-fast (--lib | --bins | --test <name>) [-- <libtest args>]`
/// (so `--release`, `--doc`, a quick or known-fail step), a package without a recorded build in this run
/// (`--only` / `--failed` left its build step out), or a target without an executable in the list.
pub fn of(s: &Step, args: &[String], built: &Built, cargo: &str) -> Option<Direct> {
    if s.expect_fail.is_some() {
        return None;
    }
    let split = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let mut it = args[..split].iter().map(String::as_str);
    if it.next() != Some("test") {
        return None;
    }
    let (mut pkg, mut target, mut keep_going) = (None, None, false);
    while let Some(a) = it.next() {
        match a {
            "-j" => drop(it.next()?),
            "-p" if pkg.is_none() => pkg = Some(it.next()?),
            "--no-fail-fast" => keep_going = true,
            "--lib" | "--bins" if target.is_none() => target = Some((a, "")),
            "--test" if target.is_none() => target = Some((a, it.next()?)),
            _ => return None,
        }
    }
    let p: &Package = built.get(pkg?)?;
    let exes: Vec<TestExe> = match target? {
        ("--lib", _) => p.lib.clone(),
        ("--bins", _) => p.bins.clone(),
        (_, name) => p.tests.iter().filter(|(n, _)| n == name).map(|(_, e)| e.clone()).collect(),
    };
    if exes.is_empty() || !keep_going {
        return None;
    }
    let mut env: Vec<(String, OsString)> = vec![("CARGO".into(), cargo.into())];
    env.extend(p.env.iter().map(|(k, v)| (k.clone(), OsString::from(v))));
    if let Some(v) = lib_path(&p.lib_paths, std::env::var_os(LIB_PATH_VAR), std::env::var_os("HOME")) {
        env.push((LIB_PATH_VAR.into(), v));
    }
    Some(Direct { exes, cwd: p.dir.clone(), args: args.get(split + 1..).unwrap_or(&[]).to_vec(), env })
}

/// The variable of the dynamic library search path, as cargo chooses it.
pub const LIB_PATH_VAR: &str = if cfg!(windows) {
    "PATH"
} else if cfg!(target_os = "macos") {
    "DYLD_FALLBACK_LIBRARY_PATH"
} else {
    "LD_LIBRARY_PATH"
};

/// `first` before the current value of the variable. Under `cargo xtask` that value already has the
/// toolchain's library directories. On macOS an unset variable gets the system's default fallback, as
/// cargo adds it. `None` when a path cannot be joined: the variable then stays as it is.
pub fn lib_path(first: &[PathBuf], current: Option<OsString>, home: Option<OsString>) -> Option<OsString> {
    let mut all: Vec<PathBuf> = first.to_vec();
    match current {
        Some(v) => all.extend(std::env::split_paths(&v)),
        None if cfg!(target_os = "macos") => {
            all.extend(home.map(|h| PathBuf::from(h).join("lib")));
            all.extend([PathBuf::from("/usr/local/lib"), PathBuf::from("/usr/lib")]);
        }
        None => {}
    }
    std::env::join_paths(all).ok()
}

/// The line the gate prints for a direct step.
pub fn describe(d: &Direct, root: &Path) -> String {
    let exes: Vec<String> = d.exes.iter().map(|e| shown(&e.exe, root)).collect();
    format!("{} {} (direct, from the build's cargo JSON artifacts)", exes.join(" + "), d.args.join(" "))
}

fn shown(p: &Path, root: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

/// Runs each executable of `d` with its output in `log`, after a `Running` line in cargo's form. All of
/// them run (the step has `--no-fail-fast`). The exit code of the first one that failed, else 0. An
/// executable that cannot start is an `error:` line in the log and code -1: a red step, never a pass.
pub fn run(d: &Direct, root: &Path, mut log: File) -> Result<i32, String> {
    let io = |e: std::io::Error| format!("gate log: {e}");
    let mut code = 0;
    for t in &d.exes {
        writeln!(log, "     Running {} ({})", t.label, shown(&t.exe, root)).map_err(io)?;
        let status = Command::new(&t.exe)
            .args(&d.args)
            .envs(d.env.iter().map(|(k, v)| (k, v)))
            .current_dir(&d.cwd)
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(io)?)
            .stderr(log.try_clone().map_err(io)?)
            .status();
        let one = match status {
            Ok(st) => st.code().unwrap_or(-1),
            Err(e) => {
                writeln!(log, "error: could not start the test executable {}: {e}", t.exe.display()).map_err(io)?;
                -1
            }
        };
        if code == 0 {
            code = one;
        }
    }
    Ok(code)
}

/// Runs `cargo <args>` in `dir` with its output in `log`. With `capture`, stdout (the JSON messages of
/// a recording build) comes back as text and only stderr (the build's readable output) goes to the log.
pub fn cargo(cargo: &str, env: Vec<(&'static str, String)>, args: &[String], dir: &Path, log: File, capture: bool) -> Result<(i32, Option<String>), String> {
    let fail = |e: std::io::Error| format!("cargo {}: {e}", args.join(" "));
    let mut cmd = Command::new(cargo);
    cmd.envs(env).args(args).current_dir(dir).stdin(Stdio::null());
    if capture {
        let out = cmd.stdout(Stdio::piped()).stderr(log).output().map_err(fail)?;
        return Ok((out.status.code().unwrap_or(-1), Some(String::from_utf8_lossy(&out.stdout).into_owned())));
    }
    let err = log.try_clone().map_err(fail)?;
    let status = cmd.stdout(log).stderr(err).status().map_err(fail)?;
    Ok((status.code().unwrap_or(-1), None))
}

/// The package of a green recording build, from its JSON `messages` and one `cargo metadata --no-deps`
/// call in `root` (the package fields of cargo's variables). `Err` says why nothing was recorded.
pub fn record(root: &Path, cargo: &str, name: &str, messages: &str) -> Result<Package, String> {
    let out = Command::new(cargo)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!("cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    artifacts::package(name, &String::from_utf8_lossy(&out.stdout), messages)
}

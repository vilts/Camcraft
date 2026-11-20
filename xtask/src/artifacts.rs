//! The test executables of one crate, read from the JSON messages of its `cargo test --no-run` build,
//! and the variables that cargo gives a test process of that crate (#1951). A test executable is only
//! ever taken from this list: the gate never searches `target/` for one (a glob can start the GUI).
//! Provenance: the Cargo Book ("JSON messages", "cargo metadata", "Environment variables Cargo sets for
//! crates"); the run-time variables were read from a test process under cargo 1.100.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::json::{self, Json};

/// One test executable and the text of cargo's `Running` line for it (`unittests src/lib.rs`,
/// `tests/smoke.rs`), which the gate writes into the step log so the notes read as before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestExe {
    pub exe: PathBuf,
    pub label: String,
}

/// What one build recorded for one package.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Package {
    /// The manifest directory: cargo starts each test process of the package there.
    pub dir: PathBuf,
    pub lib: Vec<TestExe>,
    pub bins: Vec<TestExe>,
    /// The integration binaries by target name (`--test <name>`).
    pub tests: Vec<(String, TestExe)>,
    /// The variables cargo sets for a test process, the `CARGO` path excluded.
    pub env: Vec<(String, String)>,
    /// The directories cargo puts first on the dynamic library path.
    pub lib_paths: Vec<PathBuf>,
}

impl Package {
    pub fn count(&self) -> usize {
        self.lib.len() + self.bins.len() + self.tests.len()
    }
}

/// The recorded builds of one gate run, by package name (`camcraft-app`).
pub type Built = HashMap<String, Package>;

const LIB_KINDS: &[&str] = &["lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"];

fn kinds(target: &Json) -> Vec<&str> {
    target.get("kind").map(Json::items).unwrap_or(&[]).iter().filter_map(Json::str).collect()
}

/// `1.2.3-rc.1+build` as the `CARGO_PKG_VERSION_*` parts (major, minor, patch, pre).
fn version_parts(version: &str) -> [String; 4] {
    let core = version.split('+').next().unwrap_or("");
    let (nums, pre) = core.split_once('-').unwrap_or((core, ""));
    let mut n = nums.split('.');
    let mut part = || n.next().unwrap_or("").to_string();
    [part(), part(), part(), pre.to_string()]
}

/// The package `name` from the text of `cargo metadata --no-deps` and the JSON `messages` of its build.
/// `Err` names what is missing; the caller then leaves the steps of the package with cargo.
pub fn package(name: &str, metadata: &str, messages: &str) -> Result<Package, String> {
    let meta = json::parse(metadata.trim()).ok_or("cargo metadata did not print JSON")?;
    let pkg = meta
        .get("packages")
        .map(Json::items)
        .unwrap_or(&[])
        .iter()
        .find(|p| p.text("name") == Some(name))
        .ok_or_else(|| format!("cargo metadata has no package {name}"))?;
    let manifest = pkg.text("manifest_path").ok_or("cargo metadata gives no manifest_path")?;
    let id = pkg.text("id").unwrap_or("");
    let dir = Path::new(manifest).parent().ok_or("the manifest has no directory")?.to_path_buf();
    let version = pkg.text("version").unwrap_or("");
    let [major, minor, patch, pre] = version_parts(version);
    let authors: Vec<&str> = pkg.get("authors").map(Json::items).unwrap_or(&[]).iter().filter_map(Json::str).collect();
    let field = |k: &str| pkg.text(k).unwrap_or("").to_string();
    let mut env: Vec<(String, String)> = vec![
        ("CARGO_MANIFEST_DIR".into(), dir.to_string_lossy().into_owned()),
        ("CARGO_MANIFEST_PATH".into(), manifest.to_string()),
        ("CARGO_PKG_NAME".into(), name.to_string()),
        ("CARGO_PKG_VERSION".into(), version.to_string()),
        ("CARGO_PKG_VERSION_MAJOR".into(), major),
        ("CARGO_PKG_VERSION_MINOR".into(), minor),
        ("CARGO_PKG_VERSION_PATCH".into(), patch),
        ("CARGO_PKG_VERSION_PRE".into(), pre),
        ("CARGO_PKG_AUTHORS".into(), authors.join(":")),
        ("CARGO_PKG_DESCRIPTION".into(), field("description")),
        ("CARGO_PKG_HOMEPAGE".into(), field("homepage")),
        ("CARGO_PKG_REPOSITORY".into(), field("repository")),
        ("CARGO_PKG_LICENSE".into(), field("license")),
        ("CARGO_PKG_LICENSE_FILE".into(), field("license_file")),
        ("CARGO_PKG_RUST_VERSION".into(), field("rust_version")),
        ("CARGO_PKG_README".into(), field("readme")),
    ];
    let target_dir = meta.text("target_directory").map(PathBuf::from);
    let mut out = Package { dir: dir.clone(), ..Package::default() };
    let mut finished = false;
    for line in messages.lines().filter(|l| l.starts_with('{')) {
        let Some(m) = json::parse(line) else { continue };
        match m.text("reason") {
            Some("compiler-artifact") if m.text("manifest_path") == Some(manifest) => {
                let (Some(target), Some(exe)) = (m.get("target"), m.text("executable")) else { continue };
                let (kind, tname) = (kinds(target), target.text("name").unwrap_or(""));
                if !m.get("profile").and_then(|p| p.get("test")).is_some_and(Json::is_true) {
                    // The plain binary of the package: cargo names it to the test process.
                    if kind.contains(&"bin") {
                        env.push((format!("CARGO_BIN_EXE_{tname}"), exe.to_string()));
                    }
                    continue;
                }
                let src = Path::new(target.text("src_path").unwrap_or(""));
                let rel = src.strip_prefix(&dir).unwrap_or(src).to_string_lossy().replace('\\', "/");
                let unit = TestExe { exe: PathBuf::from(exe), label: format!("unittests {rel}") };
                if kind.contains(&"test") {
                    out.tests.push((tname.to_string(), TestExe { label: rel, ..unit }));
                } else if kind.contains(&"bin") {
                    out.bins.push(unit);
                } else if kind.iter().any(|k| LIB_KINDS.contains(k)) {
                    out.lib.push(unit);
                }
            }
            Some("build-script-executed") => {
                for p in m.get("linked_paths").map(Json::items).unwrap_or(&[]).iter().filter_map(Json::str) {
                    // `native=/path`, `dependency=/path`, ... or a bare path; cargo keeps those in the target dir.
                    let path = PathBuf::from(p.split_once('=').map_or(p, |(_, v)| v));
                    if target_dir.as_ref().is_some_and(|t| path.starts_with(t)) && !out.lib_paths.contains(&path) {
                        out.lib_paths.push(path);
                    }
                }
                if m.text("package_id") == Some(id) {
                    if let Some(dir) = m.text("out_dir") {
                        env.push(("OUT_DIR".into(), dir.to_string()));
                    }
                    for pair in m.get("env").map(Json::items).unwrap_or(&[]) {
                        if let [k, v] = pair.items()
                            && let (Some(k), Some(v)) = (k.str(), v.str())
                        {
                            env.push((k.to_string(), v.to_string()));
                        }
                    }
                }
            }
            Some("build-finished") => {
                if !m.get("success").is_some_and(Json::is_true) {
                    return Err("the build did not finish with success".into());
                }
                finished = true;
            }
            _ => {}
        }
    }
    if !finished {
        return Err("the build printed no build-finished message".into());
    }
    if let Some(t) = target_dir {
        // The profile directory (and `deps` in the older layout), where cargo puts the workspace's dylibs.
        out.lib_paths.push(t.join("debug"));
        out.lib_paths.push(t.join("debug").join("deps"));
    }
    out.env = env;
    Ok(out)
}

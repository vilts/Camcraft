use chrono::{DateTime, Utc};
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_owned())
}

fn valid_sha(value: &str) -> bool {
    (value.len() == 40 || value.len() == 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-env-changed=CAMCRAFT_SOURCE_SHA");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    embed_windows_icon();

    // A source archive has no .git directory. Its packager can provide the
    // exact source commit; an ordinary checkout reads its own HEAD.
    let sha = std::env::var("CAMCRAFT_SOURCE_SHA")
        .ok()
        .filter(|s| valid_sha(s))
        .or_else(|| {
            git(&["rev-parse", "HEAD"])
                .filter(|s| valid_sha(s))
                .map(|sha| {
                    if git(&["status", "--porcelain"]).is_some_and(|s| !s.is_empty()) {
                        format!("{sha}-dirty")
                    } else {
                        sha
                    }
                })
        })
        .unwrap_or_else(|| "Custom Build".to_owned());
    println!("cargo:rustc-env=CAMCRAFT_SOURCE_SHA={sha}");

    // SOURCE_DATE_EPOCH makes reproducible package builds possible; otherwise
    // this records when the UI crate was actually compiled, in UTC.
    let built = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .and_then(|secs| DateTime::<Utc>::from_timestamp(secs, 0))
        .unwrap_or_else(Utc::now);
    println!("cargo:rustc-env=CAMCRAFT_BUILD_UTC={}", built.format("%Y-%m-%d %H:%M UTC"));

    // A branch tip can change while its source files do not. Watch Git's
    // HEAD and current ref so the embedded ID follows the checked-out tree.
    if let Some(head) = git(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={head}");
    }
    if let Some(reference) = git(&["symbolic-ref", "HEAD"])
        && let Some(path) = git(&["rev-parse", "--git-path", &reference])
    {
        println!("cargo:rerun-if-changed={path}");
    }
}

/// Embeds the logotype `.ico` (assets/logo/icons, #1459) as the exe's icon. Provenance: Win32 resource files
/// (winresource wraps rc.exe / windres). A missing resource compiler warns instead of failing the build.
#[cfg(windows)]
fn embed_windows_icon() {
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/logo/icons/camcraft.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let Some(icon) = icon.to_str() else {
        println!("cargo:warning=exe icon not embedded: the icon path is not UTF-8");
        return;
    };
    let mut res = winresource::WindowsResource::new();
    res.set_icon(icon);
    if let Err(err) = res.compile() {
        println!("cargo:warning=exe icon not embedded: {err}");
    }
}

#[cfg(not(windows))]
fn embed_windows_icon() {}

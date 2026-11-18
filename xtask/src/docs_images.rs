//! `cargo xtask docs-images` (#1902): render the User Guide screenshots
//! off-screen via CamCraft's headless GPU + `egui_kittest` UI harness from
//! committed demo projects into `site/static/images/guide/`, or verify the
//! committed PNGs and markdown references (`--check`).
//!
//! Provenance: our own build and documentation tooling over CamCraft's UI
//! harness and demo projects; no third-party or vendor images.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

const PNG_SIG: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const MIN_WIDTH: u32 = 1000;
const MAX_WIDTH: u32 = 1400;
const MIN_HEIGHT: u32 = 400;
const MIN_BYTES: u64 = 5_000;
const MAX_BYTES: u64 = 650_000;

const USAGE: &str = "\
usage:
  cargo xtask docs-images            render all guide PNGs off-screen and verify them
  cargo xtask docs-images --check    verify committed demo projects, PNGs, sizes, and alt text";

pub const DEMO_PROJECTS: &[&str] = &[
    "first-part.camproj",
    "guide-workholding.camproj",
    "guide-3d.camproj",
    "guide-multiaxis.camproj",
];

/// Every rendered guide PNG and the markdown page that embeds it (#1902).
pub const MANIFEST: &[(&str, &str)] = &[
    ("site/content/guide/set-up/setups.md", "setup-overview.png"),
    ("site/content/guide/set-up/workholding.md", "workholding-vise-toolpath.png"),
    ("site/content/guide/operations/operations-2d.md", "op-2d-pocket-params.png"),
    ("site/content/guide/operations/operations-2d.md", "op-2d-stepover-before.png"),
    ("site/content/guide/operations/operations-2d.md", "op-2d-stepover-after.png"),
    ("site/content/guide/operations/operations-2d.md", "op-2d-entry-before.png"),
    ("site/content/guide/operations/operations-2d.md", "op-2d-entry-after.png"),
    ("site/content/guide/operations/operations-2d.md", "op-2d-lead-before.png"),
    ("site/content/guide/operations/operations-2d.md", "op-2d-lead-after.png"),
    ("site/content/guide/operations/operations-3d.md", "op-3d-pocket-roughing.png"),
    ("site/content/guide/operations/operations-3d.md", "op-3d-finish-parallel.png"),
    ("site/content/guide/multi-axis/operations-multiaxis.md", "op-multiaxis-swarf.png"),
    ("site/content/guide/multi-axis/indexed.md", "op-indexed-3plus2.png"),
];

pub fn run(root: &Path, args: &[String]) -> Result<i32, String> {
    let mut check_only = false;
    for arg in args {
        match arg.as_str() {
            "--check" => check_only = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(0);
            }
            other => return Err(format!("unknown docs-images option '{other}'\n{USAGE}")),
        }
    }
    if !check_only {
        render_offscreen(root)?;
    }
    check(root)?;
    Ok(0)
}

fn render_offscreen(root: &Path) -> Result<(), String> {
    let out_dir = root.join("site/static/images/guide");
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("create {}: {e}", out_dir.display()))?;
    let examples_dir = root.join("examples");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(root.join("crates/ui"))
        .env("CAMCRAFT_GUIDE_IMAGES_OUT", &out_dir)
        .env("CAMCRAFT_GUIDE_PROJECTS_OUT", &examples_dir)
        .args([
            "test",
            "-p",
            "camcraft-ui",
            "--lib",
            "--",
            "egui_app::tests::guide_images::render_all_guide_images",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .status()
        .map_err(|e| format!("failed to run headless UI guide renderer: {e}"))?;
    if !status.success() {
        return Err(format!(
            "headless UI guide renderer exited with status {status}"
        ));
    }
    Ok(())
}

pub fn check(root: &Path) -> Result<(), String> {
    for demo in DEMO_PROJECTS {
        let path = root.join("examples").join(demo);
        if !path.is_file() {
            return Err(format!("missing committed demo project: {}", path.display()));
        }
        let len = std::fs::metadata(&path)
            .map_err(|e| format!("stat {}: {e}", path.display()))?
            .len();
        if len < 200 {
            return Err(format!("demo project {} is unexpectedly small ({len} B)", path.display()));
        }
    }

    let images_dir = root.join("site/static/images/guide");
    if !images_dir.is_dir() {
        return Err(format!(
            "missing rendered guide image directory: {}",
            images_dir.display()
        ));
    }

    let mut expected_files = BTreeSet::new();
    for &(page_rel, file) in MANIFEST {
        expected_files.insert(file.to_string());
        let png_path = images_dir.join(file);
        verify_png(&png_path)?;
        let page_path = root.join(page_rel);
        let text = std::fs::read_to_string(&page_path)
            .map_err(|e| format!("read {}: {e}", page_path.display()))?;
        verify_markdown_ref(&text, page_rel, file)?;
    }

    let ops_2d = std::fs::read_to_string(root.join("site/content/guide/operations/operations-2d.md"))
        .map_err(|e| format!("read operations-2d.md: {e}"))?;
    for kind in ["stepover", "entry", "lead"] {
        for phase in ["before", "after"] {
            let needle = format!("{kind}-{phase}.png");
            if !ops_2d.contains(&needle) {
                return Err(format!(
                    "site/content/guide/operations/operations-2d.md is missing before/after {needle}"
                ));
            }
        }
    }

    let entries = std::fs::read_dir(&images_dir)
        .map_err(|e| format!("read {}: {e}", images_dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("dir entry: {e}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().is_file() && name.ends_with(".png") && !expected_files.contains(&name) {
            return Err(format!(
                "unreferenced PNG {} in {}",
                name,
                images_dir.display()
            ));
        }
    }

    println!(
        "docs-images: {} guide PNGs ok (~1200 px wide, optimised)",
        MANIFEST.len()
    );
    Ok(())
}

fn verify_png(path: &Path) -> Result<(), String> {
    let data = std::fs::read(path).map_err(|e| format!("missing PNG {}: {e}", path.display()))?;
    if data.len() < 24 || &data[..8] != PNG_SIG || &data[12..16] != b"IHDR" {
        return Err(format!("invalid PNG header in {}", path.display()));
    }
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    if !(MIN_WIDTH..=MAX_WIDTH).contains(&width) {
        return Err(format!(
            "{} width {width} px outside {MIN_WIDTH}..={MAX_WIDTH}",
            path.display()
        ));
    }
    if height <= MIN_HEIGHT {
        return Err(format!("{} height {height} px too small", path.display()));
    }
    let bytes = data.len() as u64;
    if !(MIN_BYTES..=MAX_BYTES).contains(&bytes) {
        return Err(format!(
            "{} size {bytes} B outside {MIN_BYTES}..={MAX_BYTES}",
            path.display()
        ));
    }
    Ok(())
}

fn verify_markdown_ref(markdown: &str, page: &str, file: &str) -> Result<(), String> {
    let needle = format!("images/guide/{file})");
    let Some(end) = markdown.find(&needle) else {
        return Err(format!("{page} does not embed images/guide/{file}"));
    };
    let prefix = &markdown[..end];
    let Some(open) = prefix.rfind("![") else {
        return Err(format!("{page} reference to {file} is missing ![alt](...) syntax"));
    };
    let Some(close) = prefix[open + 2..].find("](") else {
        return Err(format!("{page} reference to {file} has malformed markdown image"));
    };
    let alt = &prefix[open + 2..open + 2 + close];
    if alt.split_whitespace().count() <= 4 {
        return Err(format!(
            "{page} image {file} needs descriptive alt text (> 4 words), got '{alt}'"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_alt_text_is_refused() {
        let md = "![Setup view](../../../images/guide/setup-overview.png)";
        let err = verify_markdown_ref(md, "setups.md", "setup-overview.png")
            .expect_err("2-word alt text must be refused");
        assert!(err.contains("setup-overview.png"), "got: {err}");
    }

    #[test]
    fn narrow_png_width_is_refused() {
        let path = std::env::temp_dir().join("camcraft-test-narrow.png");
        let mut bytes = vec![0u8; 6000];
        bytes[..8].copy_from_slice(PNG_SIG);
        bytes[12..16].copy_from_slice(b"IHDR");
        bytes[16..20].copy_from_slice(&800u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&600u32.to_be_bytes());
        std::fs::write(&path, &bytes).expect("write temp png");
        let err = verify_png(&path).expect_err("800px PNG must be refused");
        let _ = std::fs::remove_file(&path);
        assert!(err.contains("800 px"), "got: {err}");
    }

    #[test]
    fn corrupt_png_header_is_refused() {
        let path = std::env::temp_dir().join("camcraft-test-bad.png");
        std::fs::write(&path, vec![0u8; 6000]).expect("write temp file");
        let err = verify_png(&path).expect_err("corrupt PNG must be refused");
        let _ = std::fs::remove_file(&path);
        assert!(err.contains("invalid PNG header"), "got: {err}");
    }
}

//! Optional per-case PDF build (`pandoc` + headless Chrome) for
//! `cargo xtask catalog --pdf` (#2392, #2393).
//!
//! Provenance: our own build tooling over `pandoc` and headless Chrome,
//! following the repository's markdown-to-PDF workflow.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::catalog::{copy_dir_all, CatalogCase};
use crate::catalog_media::{collect_pngs, find_on_path, resolve_approx, MediaArgs};

const CHROME_CANDIDATES: &[&str] = &[
    "google-chrome",
    "chromium",
    "chromium-browser",
    "chrome",
];

pub fn run_pdf(
    root: &Path,
    cases: &[CatalogCase],
    media: &MediaArgs,
    rendered_dir: Option<&Path>,
) -> Result<(), String> {
    let pandoc = find_on_path(&["pandoc"]).ok_or_else(|| {
        "catalog --pdf requires `pandoc` on PATH (and headless Chrome/Chromium)".to_owned()
    })?;
    let chrome = find_chrome().ok_or_else(|| {
        "catalog --pdf requires headless Chrome/Chromium on PATH (google-chrome, chromium, chromium-browser) or CAMCRAFT_CHROME"
            .to_owned()
    })?;
    let out_root = media
        .pdf_out
        .clone()
        .unwrap_or_else(|| root.join("target").join("catalog-pdf"));
    let out_canon = resolve_approx(&out_root)?;
    let examples_canon = resolve_approx(&root.join("examples"))?;
    if out_canon.starts_with(&examples_canon) {
        return Err("--pdf-out must not write inside examples/".into());
    }
    let stage = std::env::temp_dir().join(format!("xtask-catalog-pdf-{}", std::process::id()));
    let _ = fs::remove_dir_all(&stage);

    let res = (|| -> Result<(), String> {
        for c in cases {
            let case_out = out_root.join(&c.family);
            fs::create_dir_all(&case_out)
                .map_err(|e| format!("create {}: {e}", case_out.display()))?;
            let work = stage.join(&c.family).join(&c.name);
            copy_dir_all(&c.dir, &work)?;
            let mut md = fs::read_to_string(work.join("README.md"))
                .map_err(|e| format!("{}: read README.md: {e}", c.id()))?;
            if let Some(shots) = rendered_dir {
                for (_, png) in collect_pngs(shots, std::slice::from_ref(c), false)? {
                    let name = png
                        .file_name()
                        .and_then(|name| name.to_str())
                        .ok_or_else(|| format!("{}: invalid screenshot filename", c.id()))?;
                    fs::copy(&png, work.join(name))
                        .map_err(|e| format!("{}: stage screenshot: {e}", c.id()))?;
                    md.push_str(&format!("\n\n![{}: {name}](<{name}>)\n", c.name));
                }
            }
            let md_path = work.join("catalog-case.md");
            let html_path = work.join("catalog-case.html");
            let pdf_path = case_out.join(format!("{}.pdf", c.name));
            fs::write(&md_path, md)
                .map_err(|e| format!("{}: write temp markdown: {e}", c.id()))?;

            let p_args = pdf_pandoc_args(&md_path, &html_path, &c.name);
            let p_status = Command::new(&pandoc)
                .current_dir(&work)
                .args(&p_args)
                .status()
                .map_err(|e| format!("{}: run pandoc: {e}", c.id()))?;
            if !p_status.success() {
                return Err(format!("{}: pandoc exited with {p_status}", c.id()));
            }
            let c_args = pdf_chrome_args(&html_path, &pdf_path);
            let c_status = Command::new(&chrome)
                .args(&c_args)
                .status()
                .map_err(|e| format!("{}: run headless Chrome: {e}", c.id()))?;
            if !c_status.success() || !pdf_path.is_file() {
                return Err(format!("{}: headless Chrome failed ({c_status})", c.id()));
            }
            println!("built PDF {} -> {}", c.id(), pdf_path.display());
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&stage);
    res
}

pub fn pdf_pandoc_args(md_path: &Path, html_path: &Path, title: &str) -> Vec<String> {
    vec![
        md_path.to_string_lossy().into_owned(),
        "--standalone".into(),
        "--embed-resources".into(),
        "--metadata".into(),
        format!("title={title}"),
        "-o".into(),
        html_path.to_string_lossy().into_owned(),
    ]
}

pub fn pdf_chrome_args(html_path: &Path, pdf_path: &Path) -> Vec<String> {
    vec![
        "--headless".into(),
        "--disable-gpu".into(),
        "--no-sandbox".into(),
        format!("--print-to-pdf={}", pdf_path.display()),
        html_path.to_string_lossy().into_owned(),
    ]
}

fn find_chrome() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("CAMCRAFT_CHROME") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    find_on_path(CHROME_CANDIDATES)
}

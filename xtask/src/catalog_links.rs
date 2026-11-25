//! Provenance: our own catalog metadata to operation-guide link generator (#2855).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use regex::Regex;

use super::CatalogCase;

#[path = "catalog_metadata.rs"]
pub(crate) mod metadata;

#[cfg(test)]
#[path = "catalog_links_tests.rs"]
mod tests;

const START: &str = "<!-- catalog:examples:start -->";
const END: &str = "<!-- catalog:examples:end -->";
const URL: &str = "https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/";
pub const PAGES: [&str; 7] = [
    "operations/operations-2d.md",
    "operations/operations-3d.md",
    "operations/operations-turning.md",
    "operations/operations-edm.md",
    "operations/operations-plasma.md",
    "operations/operations-probing.md",
    "multi-axis/operations-multiaxis.md",
];

pub fn sync(root: &Path, cases: &[CatalogCase], check: bool) -> Result<(), String> {
    let kinds = metadata::registry(root)?;
    let mut by_kind: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let ids: BTreeSet<_> = cases.iter().map(CatalogCase::id).collect();
    for case in cases {
        let path = case.dir.join("case.toml");
        let text =
            fs::read_to_string(&path).map_err(|e| format!("{}: read case.toml: {e}", case.id()))?;
        let fields =
            metadata::parse(&text).map_err(|e| format!("{}: case.toml: {e}", case.id()))?;
        for kind in fields.get("kinds").ok_or("case.toml: missing kinds")? {
            if !kinds.contains(kind) {
                return Err(format!("{}: case.toml: unknown kind '{kind}'", case.id()));
            }
            by_kind.entry(kind.clone()).or_default().push(case.id());
        }
    }
    let mut pages = Vec::new();
    let links = Regex::new(r"examples/catalog/([A-Za-z0-9_-]+/[A-Za-z0-9_-]+)")
        .map_err(|e| e.to_string())?;
    for page in PAGES {
        let path = root.join("site/content/guide").join(page);
        let text =
            fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        // Scan before regeneration, including links outside generated blocks.
        if check {
            for cap in links.captures_iter(&text) {
                if !ids.contains(&cap[1]) {
                    return Err(format!(
                        "{page}: example link names unknown case '{}'",
                        &cap[1]
                    ));
                }
            }
        }
        pages.push((page, path, text));
    }
    let mut covered = BTreeSet::new();
    let mut updates = Vec::new();
    for (page, path, text) in pages {
        let generated = render(page, &text, &kinds, &by_kind, &mut covered)?;
        if generated != text {
            if check {
                return Err(format!(
                    "{page}: example links missing or stale; run `cargo xtask catalog`"
                ));
            }
            updates.push((path, generated));
        }
    }
    for kind in by_kind.keys() {
        if !covered.contains(kind) {
            return Err(format!(
                "kind '{kind}': no operation guide heading for example links"
            ));
        }
    }
    for (path, text) in updates {
        fs::write(&path, text).map_err(|e| format!("write {}: {e}", path.display()))?;
    }
    Ok(())
}

fn heading_kind<'a>(page: &str, heading: &'a str) -> &'a str {
    if page.ends_with("operations-turning.md") {
        match heading {
            "Part" => "Parting",
            "Chamfer" => "Chamfering",
            "Taper" => "Taper Turning",
            "Multi-Pass" => "Multi-Pass Turning",
            "Form" => "Form Turning",
            "Bore" => "Boring",
            _ => heading,
        }
    } else {
        heading
    }
}

fn render(
    page: &str,
    text: &str,
    kinds: &BTreeSet<String>,
    by_kind: &BTreeMap<String, Vec<String>>,
    covered: &mut BTreeSet<String>,
) -> Result<String, String> {
    let mut out = String::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        // Existing generated blocks must be well formed; never eat guide prose.
        if line == START || line == END {
            return Err(format!("{page}: misplaced catalog example marker"));
        }
        out.push_str(line);
        out.push('\n');
        let Some(heading) = line.strip_prefix("## ") else {
            continue;
        };
        let kind = heading_kind(page, heading);
        if !kinds.contains(kind) {
            continue;
        }
        covered.insert(kind.to_string());
        // Blocks live directly below headings; preserve all other bytes/paragraphs.
        if lines.peek() == Some(&"") {
            lines.next();
        }
        if lines.peek() == Some(&START) {
            lines.next();
            let mut closed = false;
            for line in lines.by_ref() {
                if line == END {
                    closed = true;
                    break;
                }
                if line.starts_with("## ") || line == START {
                    return Err(format!("{page}: unclosed catalog example block"));
                }
            }
            if !closed {
                return Err(format!("{page}: unclosed catalog example block"));
            }
            if lines.peek() == Some(&"") {
                lines.next();
            }
        }
        out.push_str(&format!("\n{START}\n**Example cases**\n\n"));
        if let Some(ids) = by_kind.get(kind) {
            for id in ids {
                out.push_str(&format!("- [{id}]({URL}{id})\n"));
            }
        } else {
            out.push_str("No catalog case yet.\n");
        }
        out.push_str(&format!("{END}\n\n"));
    }
    Ok(out)
}

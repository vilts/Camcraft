//! Cargo integration-suite roots and their Rust module paths (#927).
//! Provenance: Cargo's package layout and the Rust Reference's module-file rules.

use std::path::{Path, PathBuf};

use crate::braces::{self, Lex};
use crate::scan::{self, TestFn, re};

pub fn entry(root: &Path, dir: &str, binary: &str) -> PathBuf {
    let tests = root.join("crates").join(dir).join("tests");
    let flat = tests.join(format!("{binary}.rs"));
    if flat.is_file() {
        flat
    } else {
        tests.join(binary).join("main.rs")
    }
}

#[derive(Debug)]
pub struct Source {
    pub path: PathBuf,
    pub module: String,
    pub precise: bool,
}

struct Module {
    name: String,
    parents: Vec<String>,
    path: Option<String>,
}

/// Inline module prefixes for each line, out-of-line module declarations, and `true` when the code holds
/// a `mod` token the line scan did not read as a declaration (`mod a; mod b;` on one line,
/// `#[cfg(test)] mod b;`, a declaration split over lines). A caller that must not miss a module file
/// (`sources_and_missing`, #3706) then counts the file as unknown.
fn modules(text: &str) -> Result<(Vec<String>, Vec<Module>, bool), String> {
    let declaration = re(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*([;{])")?;
    let token = re(r"\bmod\b")?;
    let mut unsure = false;
    let path_attr = re(r#"^\s*#\[path\s*=\s*"([^"]+)"\s*\]"#)?;
    let mut stack: Vec<(String, i64)> = Vec::new();
    let (mut depth, mut lex) = (0, Lex::default());
    let mut prefixes = Vec::new();
    let mut external = Vec::new();
    let mut path = None;
    for line in text.lines() {
        let parents: Vec<String> = stack.iter().map(|(n, _)| n.clone()).collect();
        prefixes.push(parents.join("::"));
        let mut read = false;
        if lex == Lex::Code {
            if let Some(c) = path_attr.captures(line) {
                path = Some(c[1].to_string());
            } else if let Some(c) = declaration.captures(line) {
                read = true;
                let name = c[1].to_string();
                if &c[2] == ";" {
                    external.push(Module {
                        name,
                        parents,
                        path: path.take(),
                    });
                } else {
                    stack.push((name, depth + 1));
                    path = None;
                }
            } else if !line.trim().is_empty()
                && !line.trim_start().starts_with('#')
                && !line.trim_start().starts_with("//")
                && !line.trim_start().starts_with("/*")
            {
                path = None;
            }
        }
        let code = braces::code(line, &mut lex);
        if token.find_iter(&code).count() != usize::from(read) {
            unsure = true;
        }
        let open = code.chars().filter(|&c| c == '{').count() as i64;
        let close = code.chars().filter(|&c| c == '}').count() as i64;
        depth += open - close;
        while stack.last().is_some_and(|(_, level)| *level > depth) {
            stack.pop();
        }
    }
    Ok((prefixes, external, unsure))
}

/// Actual declared module files, not every .rs helper lying next to the suite.
pub fn sources(entry: &Path) -> Result<Vec<Source>, String> {
    Ok(sources_and_missing(entry)?.0)
}

/// [`sources`], with the count of declared `mod x;` children whose file is absent (a cfg-gated module
/// can be), plus one for each file whose `mod` tokens the scan could not all read (`modules`). A caller that must not drop a suite by accident (`plan::may_run_ignored`, #3186) treats a
/// missing child as unknown text.
pub fn sources_and_missing(entry: &Path) -> Result<(Vec<Source>, usize), String> {
    let mut out = Vec::new();
    let mut missing = 0;
    visit(entry, "", true, &mut Vec::new(), &mut out, &mut missing)?;
    Ok((out, missing))
}

fn visit(
    file: &Path,
    prefix: &str,
    inherited_precision: bool,
    active: &mut Vec<PathBuf>,
    out: &mut Vec<Source>,
    missing: &mut usize,
) -> Result<(), String> {
    if active.len() >= 64 || active.iter().any(|p| p == file) {
        return Err(format!(
            "cyclic or excessively nested suite module: {}",
            file.display()
        ));
    }
    let text = std::fs::read_to_string(file)
        .map_err(|e| format!("suite module {}: {e}", file.display()))?;
    let (_, modules, unsure) = modules(&text)?;
    if unsure {
        *missing = missing.saturating_add(1);
    }
    // Do not guess target/features for cfg-gated modules or functions. Quick can run
    // the suite whole; named selection refuses an ambiguous exact path.
    let cfg = re(r"#\s*!?\s*\[\s*cfg[^\]]*\]")?;
    let precise = inherited_precision
        && cfg.find_iter(&text).all(|m| {
            let attribute: String = m.as_str().split_whitespace().collect();
            ["#[cfg(test)]", "#![cfg(test)]"].contains(&attribute.as_str())
        });
    out.push(Source {
        path: file.to_path_buf(),
        module: prefix.to_string(),
        precise,
    });
    active.push(file.to_path_buf());
    let parent = file.parent().unwrap_or_else(|| Path::new(""));
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let base = if prefix.is_empty() || stem == "mod" {
        parent.to_path_buf()
    } else {
        parent.join(stem)
    };
    for m in modules {
        let mut dir = base.clone();
        for p in &m.parents {
            dir.push(p);
        }
        let child = match m.path {
            Some(p) => {
                // In a non-mod.rs module, #[path] is relative to its containing directory.
                let mut relative = if m.parents.is_empty() {
                    parent.to_path_buf()
                } else {
                    base.clone()
                };
                for p in &m.parents {
                    relative.push(p);
                }
                relative.join(p)
            }
            None => {
                let flat = dir.join(format!("{}.rs", m.name));
                if flat.is_file() {
                    flat
                } else {
                    dir.join(&m.name).join("mod.rs")
                }
            }
        };
        // cfg-gated modules may legitimately be absent on this checkout. The owning suite
        // still runs whole for structural edits; missing files are never invented test hits.
        if child.is_file() {
            let name = [prefix.to_string(), m.parents.join("::"), m.name]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("::");
            visit(&child, &name, precise, active, out, missing)?;
        } else {
            *missing = missing.saturating_add(1);
        }
    }
    active.pop();
    Ok(())
}

pub fn test_fns(source: &Source) -> Result<Vec<TestFn>, String> {
    let text = scan::read(&source.path);
    let (prefixes, _, _) = modules(&text)?;
    let mut tests = scan::test_fns(&text)?;
    for test in &mut tests {
        let inline = prefixes
            .get(test.start.saturating_sub(1))
            .cloned()
            .unwrap_or_default();
        test.name = [source.module.clone(), inline, test.name.clone()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("::");
    }
    Ok(tests)
}

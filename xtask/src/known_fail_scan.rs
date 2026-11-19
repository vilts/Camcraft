//! Provenance: our own known-fail marker contract (#2567), using Rust ignore reasons and libtest names.

use std::path::Path;

use crate::{known_fail::KnownFail, plan, scan};

/// `#[ignore = "known-fail #ISSUE | TARGET | FULL_TEST_NAME | PANIC_LINE_PREFIX"]`.
/// The prefix is literal text, without Rust escapes. Spaces in the prefix are significant.
pub fn discover(root: &Path) -> Result<Vec<KnownFail>, String> {
    let mut out = Vec::new();
    for dir in plan::HEAVY_CRATES {
        for tree in ["src", "tests"] {
            for file in scan::rs_files_recursive(&root.join("crates").join(dir).join(tree)) {
                let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
                if !text.contains("known-fail") {
                    continue;
                }
                let rel = file.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
                let entries = parse(&text, dir, &rel)?;
                for k in entries {
                    let valid_target = if tree == "src" {
                        k.target == "lib"
                    } else {
                        scan::binaries(root, dir).contains(&k.target)
                    };
                    if !valid_target {
                        return Err(format!("known-fail marker in {rel}: unknown target {}", k.target));
                    }
                    if out.iter().any(|old: &KnownFail| old.dir == k.dir && old.target == k.target && old.test == k.test) {
                        return Err(format!("duplicate known-fail marker: {}", k.test));
                    }
                    out.push(k);
                }
            }
        }
    }
    Ok(out)
}

pub fn parse(text: &str, dir: &str, file: &str) -> Result<Vec<KnownFail>, String> {
    let lines: Vec<_> = text.lines().collect();
    let mut out = Vec::new();
    for t in scan::test_fns(text)? {
        for line in &lines[t.start - 1..t.end.min(lines.len())] {
            let line = line.trim();
            if !line.starts_with("#[ignore") || !line.contains("known-fail") {
                continue;
            }
            let bad = || format!("invalid known-fail marker in {file} for {}", t.name);
            let value = line.strip_prefix("#[ignore = \"known-fail #").and_then(|s| s.strip_suffix("\"]")).ok_or_else(bad)?;
            let fields: Vec<_> = value.splitn(4, " | ").collect();
            if fields.len() != 4 || fields.iter().any(|s| s.trim().is_empty() || s.contains(['"', '\\'])) {
                return Err(bad());
            }
            let issue = fields[0].parse::<u32>().map_err(|_| bad())?;
            if issue == 0 || fields[2].rsplit("::").next() != Some(t.name.as_str()) {
                return Err(bad());
            }
            let assertions: Vec<_> = (t.start - 1..t.end.min(lines.len()))
                .filter(|&i| lines[i].trim() == "// known-fail-assert")
                .collect();
            if assertions.len() != 1 {
                return Err(format!("{}: expected one // known-fail-assert", bad()));
            }
            let assertion = assertions[0] + 1;
            if !lines.get(assertion).is_some_and(|l| {
                let l = l.trim();
                l.starts_with("assert!(") || l.starts_with("assert_eq!(") || l.starts_with("assert_ne!(")
            }) {
                return Err(format!("{}: known-fail-assert must precede an assertion", bad()));
            }
            out.push(KnownFail {
                dir: dir.into(), target: fields[1].into(), test: fields[2].into(),
                issue, file: file.into(), line: assertion + 1, reason: fields[3].into(),
            });
        }
    }
    let markers = lines.iter().filter(|l| l.trim().starts_with("#[ignore") && l.contains("known-fail")).count();
    if markers != out.len() {
        return Err(format!("known-fail marker in {file} must belong to a #[test] function"));
    }
    Ok(out)
}

//! #1253: every message the troubleshooting page quotes still exists in the
//! sources, so a reworded refusal cannot leave the page describing a message
//! nobody sees. Provenance: our own docs convention (see the page's source).
//!
//! Gate map (#1253): covers site/content/guide/reference/troubleshooting.md vs crates/*/src.

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rs_files(dir: &Path, out: &mut String) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs")
            && let Ok(t) = fs::read_to_string(&p)
        {
            {
                // join string-literal line continuations ("...\<newline>   ...")
                let mut joined = String::new();
                let mut it = t.split("\\\n");
                if let Some(first) = it.next() {
                    joined.push_str(first);
                }
                for part in it {
                    joined.push_str(part.trim_start());
                }
                out.push_str(&joined);
            }
        }
    }
}

/// Backticked spans of `### ` headings, split at `<...>` and `...`
/// placeholders into literal fragments of 8+ characters.
fn stems(page: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in page.lines().filter(|l| l.starts_with("### ")) {
        for (i, span) in line.split('`').enumerate() {
            if i % 2 == 0 {
                continue;
            }
            let mut frag = String::new();
            let mut depth = 0;
            let flush = |frag: &mut String, out: &mut Vec<String>| {
                let t = frag.trim_matches(|c: char| !c.is_alphanumeric()).to_owned();
                if t.len() >= 8 {
                    out.push(t);
                }
                frag.clear();
            };
            let chars: Vec<char> = span.chars().collect();
            let mut k = 0;
            while k < chars.len() {
                let c = chars[k];
                if c == '<' {
                    depth += 1;
                    flush(&mut frag, &mut out);
                } else if c == '>' && depth > 0 {
                    depth -= 1;
                } else if depth == 0 && chars[k..].starts_with(&['.', '.', '.']) {
                    flush(&mut frag, &mut out);
                    k += 2;
                } else if depth == 0 {
                    frag.push(c);
                }
                k += 1;
            }
            flush(&mut frag, &mut out);
        }
    }
    out
}

#[test]
fn troubleshooting_messages_still_exist_in_the_sources() {
    // #1908 grouped the guide by job stage: the page is under reference/.
    let page = fs::read_to_string(root().join("site/content/guide/reference/troubleshooting.md"))
        .expect("troubleshooting page is readable");
    let mut src = String::new();
    for entry in fs::read_dir(root().join("crates"))
        .expect("crates dir")
        .flatten()
    {
        rs_files(&entry.path().join("src"), &mut src);
    }
    let found = stems(&page);
    assert!(
        found.len() > 20,
        "the stem extractor found only {} stems",
        found.len()
    );
    let stale: Vec<&String> = found.iter().filter(|s| !src.contains(s.as_str())).collect();
    assert!(
        stale.is_empty(),
        "troubleshooting.md quotes message stems no longer in crates/*/src (reworded or removed?): {stale:#?}"
    );
}

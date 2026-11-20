//! Provenance: issue #2740 and our gate law: changed fixtures and importers select their readers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use crate::plan::ALL_CRATES;
use crate::{scan, suites};

fn fixture_component(path: &Path) -> bool {
    path.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("testdata" | "fixture" | "fixtures" | "golden" | "goldens" | "snapshots")
        )
    })
}

// Lexical resolution also works for deleted inputs; canonicalize would lose their readers.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn map(
    root: &Path,
    changed: &[String],
    libs: &mut BTreeSet<String>,
    whole: &mut BTreeSet<String>,
    why: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    if changed.iter().any(|p| {
        p.starts_with("crates/app/src/geometry/import/")
            || p.starts_with("crates/kernel/src/solidcraft/")
            || matches!(
                p.as_str(),
                "crates/kernel/src/solidcore_step.rs" | "crates/kernel/src/solidcore_import.rs"
            )
    }) {
        for (dir, binary) in [
            ("app", "feature_edge_g1_join"),
            ("kernel", "step_agreement"),
            ("kernel", "machining_mesh"),
        ] {
            if suites::entry(root, dir, binary).is_file() {
                let key = format!("{dir}:{binary}");
                whole.insert(key.clone());
                why.insert(key, "STEP/mesh importer changed".into());
            }
        }
    }

    let inputs: Vec<_> = changed
        .iter()
        .filter_map(|file| {
            let path = Path::new(file);
            let parent = path.parent()?;
            fixture_component(parent).then(|| (file, normalize(&root.join(parent))))
        })
        .collect();
    if inputs.is_empty() {
        return Ok(());
    }
    // A plain text path scan is conservative. It does not try to interpret test I/O.
    let quoted = scan::re(r#""([^"\r\n]+)""#)?;
    // A manifest-directory concat suffix starts with `/`, but remains crate-relative.
    let manifest_prefix = scan::re(r#"concat!\s*\(\s*env!\s*\(\s*"CARGO_MANIFEST_DIR"\s*\)\s*,\s*"[/\\]"#)?;
    for dir in ALL_CRATES {
        let crate_root = root.join("crates").join(dir);
        let mut targets: Vec<_> = scan::binaries(root, dir)
            .into_iter()
            .map(|binary| {
                let entry = suites::entry(root, dir, &binary);
                (Some(binary), entry)
            })
            .collect();
        let lib = crate_root.join("src/lib.rs");
        if lib.is_file() {
            targets.push((None, lib));
        }
        for (binary, entry) in targets {
            let mut hit = None;
            for source in suites::sources(&entry)? {
                let text = std::fs::read_to_string(&source.path)
                    .map_err(|e| format!("fixture reader {}: {e}", source.path.display()))?;
                let text = manifest_prefix.replace_all(&text, "\"");
                for literal in quoted.captures_iter(&text) {
                    let value = literal[1].replace('\\', "/");
                    // A formatted suffix can name any child of its static directory prefix.
                    let prefix = value.split('{').next().unwrap_or(&value);
                    let path = Path::new(prefix);
                    // Bare "..", "tests" or an error message must not select every input.
                    if !fixture_component(path) {
                        continue;
                    }
                    for base in [
                        root,
                        crate_root.as_path(),
                        source.path.parent().unwrap_or(root),
                    ] {
                        let reference = normalize(&base.join(path));
                        if let Some((file, _)) = inputs.iter().find(|(_, input)| {
                            reference.starts_with(input) || input.starts_with(&reference)
                        }) {
                            hit = Some((*file).clone());
                            break;
                        }
                    }
                    if hit.is_some() {
                        break;
                    }
                }
                if hit.is_some() {
                    break;
                }
            }
            if let Some(file) = hit {
                let key = if let Some(binary) = binary {
                    let key = format!("{dir}:{binary}");
                    whole.insert(key.clone());
                    key
                } else {
                    libs.insert((*dir).to_string());
                    format!("{dir}:lib")
                };
                why.entry(key)
                    .or_insert_with(|| format!("fixture directory read by suite: {file}"));
            }
        }
    }
    Ok(())
}

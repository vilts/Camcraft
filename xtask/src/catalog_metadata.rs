//! Provenance: our own restricted, flat catalog metadata format (#2855).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use regex::Regex;

/// Flat TOML: quoted strings and arrays of quoted strings; no tables or escapes.
/// Reject unsupported syntax rather than silently misreading it.
pub fn parse(text: &str) -> Result<BTreeMap<String, Vec<String>>, String> {
    let token = Regex::new(r#"\A([A-Za-z_]+|[=\[\],]|"[^"\\\x00-\x1f]*"|'[^'\x00-\x1f]*')"#)
        .map_err(|e| e.to_string())?;
    let whitespace = Regex::new(r"\A(?:\s+|#[^\n]*(?:\n|$))*").map_err(|e| e.to_string())?;
    let mut rest = text;
    let mut tokens = Vec::new();
    while !rest.is_empty() {
        if let Some(space) = whitespace.find(rest) {
            if space.as_str().contains('\n') {
                tokens.push("\n");
            }
            rest = &rest[space.end()..];
        }
        if rest.is_empty() {
            break;
        }
        let caps = token
            .captures(rest)
            .ok_or("unsupported case.toml syntax (use quoted strings without escapes)")?;
        let matched = caps.get(0).ok_or("missing metadata token")?;
        let value = caps.get(1).ok_or("missing metadata value")?;
        tokens.push(value.as_str());
        rest = &rest[matched.end()..];
    }
    let mut input = tokens.into_iter().peekable();
    let mut fields = BTreeMap::new();
    while let Some(key) = input.next() {
        if key == "\n" {
            continue;
        }
        if !["kinds", "machine", "expect"].contains(&key) {
            return Err(format!("unknown field '{key}'"));
        }
        if input.next() != Some("=") {
            return Err(format!("{key}: expected '='"));
        }
        let mut values = Vec::new();
        if key == "kinds" {
            if input.next() != Some("[") {
                return Err("kinds: expected an array of kind names".into());
            }
            while input.peek() != Some(&"]") {
                while input.peek() == Some(&"\n") {
                    input.next();
                }
                if input.peek() == Some(&"]") {
                    break;
                }
                values.push(string(input.next())?);
                while input.peek() == Some(&"\n") {
                    input.next();
                }
                match input.peek() {
                    Some(&",") => {
                        input.next();
                    }
                    Some(&"]") => {}
                    _ => return Err("kinds: expected ',' or ']'".into()),
                }
            }
            input.next();
        } else {
            values.push(string(input.next())?);
        }
        if !matches!(input.peek(), None | Some(&"\n")) {
            return Err(format!("{key}: expected a newline after the value"));
        }
        if values.is_empty() || values.iter().any(|v| v.trim().is_empty()) {
            return Err(format!("{key}: must not be empty"));
        }
        if values.iter().collect::<BTreeSet<_>>().len() != values.len() {
            return Err(format!("{key}: duplicate value"));
        }
        if fields.insert(key.to_string(), values).is_some() {
            return Err(format!("duplicate field '{key}'"));
        }
    }
    for key in ["kinds", "machine", "expect"] {
        if !fields.contains_key(key) {
            return Err(format!("missing {key}"));
        }
    }
    let expect = fields.get("expect").and_then(|v| v.first()).map(String::as_str);
    if !matches!(expect, Some("post" | "refusal")) {
        return Err(format!(
            "expect: must be \"post\" or \"refusal\", got '{}'",
            expect.unwrap_or_default()
        ));
    }
    Ok(fields)
}

/// What a case's script ends with (#2856): a posted program (`<case>.nc`)
/// or a refused post (`refusal.txt`, the refusal lines of the run).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    Post,
    Refusal,
}

/// Read the `expect` field of `<dir>/case.toml`.
pub fn expect_of(dir: &Path) -> Result<Expect, String> {
    let path = dir.join("case.toml");
    let text = fs::read_to_string(&path).map_err(|e| format!("read case.toml: {e}"))?;
    let fields = parse(&text).map_err(|e| format!("case.toml: {e}"))?;
    match fields.get("expect").and_then(|v| v.first()).map(String::as_str) {
        Some("refusal") => Ok(Expect::Refusal),
        _ => Ok(Expect::Post),
    }
}

fn string(value: Option<&str>) -> Result<String, String> {
    let value = value.ok_or("expected quoted string")?;
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        return Ok(value[1..value.len() - 1].to_string());
    }
    Err(format!("expected quoted string, got '{value}'"))
}

pub fn registry(root: &Path) -> Result<BTreeSet<String>, String> {
    let path = root.join("crates/app/src/operation/kinds.rs");
    let source = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let rows = source
        .split_once("pub const ALL:")
        .and_then(|(_, tail)| tail.split_once("\n    ];"))
        .map(|(rows, _)| rows)
        .ok_or("cannot read KindInfo::ALL registry")?;
    let labels = Regex::new(r#"(?m)^\s*label:\s*"([^"]+)""#).map_err(|e| e.to_string())?;
    let out: BTreeSet<_> = labels
        .captures_iter(rows)
        .map(|c| c[1].to_string())
        .collect();
    if out.is_empty() {
        return Err("KindInfo::ALL has no kind labels".into());
    }
    Ok(out)
}

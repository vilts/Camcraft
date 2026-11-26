//! The CSS of a `<style>` element, checked without selector matching: each declaration of
//! each rule must be on the allow-list of the inline styles, so no rule of a sheet can hide,
//! move or resize a shape that the import cuts.
//! Provenance: CSS Syntax Level 3 (qualified rules, at-rules and blocks; the tokens come from
//! `css.rs`) and CSS Nesting 1 (a rule inside a declaration block); original implementation.
//!
//! The reader does not know which element a selector matches. Thus `display` and
//! `visibility` are refused by name with any value, and text that is not a whole rule is
//! refused: a part that this scan cannot read is never skipped.

use super::css::{Lexer, Piece};
use super::facts::{draws_marker, refused};
use super::style::inline_declarations;
use super::{SvgError, invalid};

/// At-rules whose block cannot change a shape: the block is skipped.
const INERT_BLOCKS: &[&str] = &["font-face"];
/// At-rules of one statement that cannot change a shape.
const INERT_STATEMENTS: &[&str] = &["charset", "namespace"];

fn short(text: &str) -> String {
    text.chars().take(40).collect()
}

/// The refusal of a bad `url(` token: a browser drops its declaration and reads the next one.
fn bad_url() -> SvgError {
    invalid("style sheet has a bad url( token: it is not read in part")
}

/// The index of the first byte of `stops` at or after `from` that is outside a string, a
/// comment, an escape and a `url(` token.
fn scan(text: &str, from: usize, stops: &[u8]) -> Result<Option<usize>, SvgError> {
    for (at, piece) in Lexer::new(text, from) {
        match piece {
            Piece::BadUrl => return Err(bad_url()),
            Piece::Char(ch) if u8::try_from(ch).is_ok_and(|byte| stops.contains(&byte)) => return Ok(Some(at)),
            _ => {}
        }
    }
    Ok(None)
}

/// `text` without the leading white space, comments and the `<!--` / `-->` that CSS permits.
fn lead(text: &str) -> &str {
    let mut rest = text;
    loop {
        let trimmed = rest.trim_start();
        rest = if let Some(after) = trimmed.strip_prefix("/*") {
            after.find("*/").map_or("", |end| after.get(end + 2..).unwrap_or(""))
        } else if let Some(after) = trimmed.strip_prefix("<!--").or_else(|| trimmed.strip_prefix("-->")) {
            after
        } else {
            return trimmed;
        };
    }
}

/// Check one declaration block. `notes` gets the kinds that the summary counts.
fn declarations(body: &str, notes: &mut Vec<&'static str>) -> Result<(), SvgError> {
    for decl in inline_declarations(body) {
        if decl.bad_url {
            return Err(bad_url());
        }
        // A name that is empty or has a space in it is not a CSS property: a browser drops
        // that declaration, and so does this reader.
        if decl.property.is_empty() || decl.property.contains(' ') {
            continue;
        }
        let name = decl.property.to_ascii_lowercase();
        match name.as_str() {
            "display" | "visibility" => {
                return Err(invalid(format!(
                    "style sheet {name} is not supported: selectors are not matched, \
                     and a rule could hide a shape that the import cuts"
                )));
            }
            "clip-path" => notes.push("clip-path"),
            "mask" => notes.push("mask"),
            _ if draws_marker(&name, &decl.value) => notes.push("marker"),
            _ => {}
        }
        if let Some(refusal) = refused("style sheet", &name, &decl.value) {
            return Err(refusal);
        }
    }
    Ok(())
}

/// Check the text of one `<style>` element. A sheet of paint and text rules passes.
pub(super) fn check(sheet: &str, notes: &mut Vec<&'static str>) -> Result<(), SvgError> {
    let malformed = |what: &str| invalid(format!("style sheet has {what}: it is not read in part"));
    let unsupported = |rule: &str| invalid(format!("style sheet rule @{} is not supported", short(rule)));
    let mut at = 0;
    loop {
        let Some(stop) = scan(sheet, at, b"{};")? else {
            return match lead(sheet.get(at..).unwrap_or("")) {
                "" => Ok(()),
                _ => Err(malformed("text after its last rule")),
            };
        };
        let prelude = lead(sheet.get(at..stop).unwrap_or(""));
        let at_rule = prelude.strip_prefix('@').map(|rest| {
            let len = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).unwrap_or(rest.len());
            rest.get(..len).unwrap_or("").to_ascii_lowercase()
        });
        match (sheet.as_bytes().get(stop), at_rule) {
            (Some(b';'), Some(rule)) if INERT_STATEMENTS.contains(&rule.as_str()) => at = stop + 1,
            (Some(b';'), Some(rule)) => return Err(unsupported(&rule)),
            (Some(b';'), None) => return Err(malformed("a ';' outside a rule")),
            (Some(b'{'), rule) => {
                if let Some(rule) = rule.as_deref().filter(|rule| !INERT_BLOCKS.contains(rule)) {
                    return Err(unsupported(rule));
                }
                // The end of the file closes an open block, as in a browser.
                let end = scan(sheet, stop + 1, b"{}")?.unwrap_or(sheet.len());
                if sheet.as_bytes().get(end) == Some(&b'{') {
                    return Err(malformed("a rule inside a rule"));
                }
                if rule.is_none() {
                    declarations(sheet.get(stop + 1..end).unwrap_or(""), notes)?;
                }
                at = end + 1;
            }
            _ => return Err(malformed("a '}' outside a rule")),
        }
    }
}

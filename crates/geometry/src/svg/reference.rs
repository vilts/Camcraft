//! XML references (`&name;`, `&#10;`, `&#xA;`) in character data and attribute values: one
//! function finds a reference, decodes it and refuses what it cannot decode, so no reference
//! that a browser replaces with a value goes through as text.
//! Provenance: the W3C XML 1.0 Recommendation (4.1 character and entity references, 4.6
//! predefined entities, 2.2 the `Char` production); original implementation.

use std::borrow::Cow;

use super::{SvgError, invalid};

/// The start of a name for a message: a hostile name can be megabytes long.
fn short(name: &str) -> String {
    name.chars().take(40).collect()
}

/// The character of a code point that XML permits in a document (the `Char` production).
fn xml_char(code: u32) -> Option<char> {
    let legal = matches!(code, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x1_0000..=0x10_FFFF);
    char::from_u32(code).filter(|_| legal)
}

/// The character of the reference `&name;`: one of the five predefined entities, or a
/// character reference with any count of leading zeros.
fn character(name: &str) -> Option<char> {
    match name {
        "lt" => return Some('<'),
        "gt" => return Some('>'),
        "amp" => return Some('&'),
        "quot" => return Some('"'),
        "apos" => return Some('\''),
        _ => {}
    }
    let (digits, radix) = match name.strip_prefix("#x") {
        Some(hex) => (hex, 16),
        None => (name.strip_prefix('#')?, 10),
    };
    // `from_str_radix` reads a sign; a reference has digits only.
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    let significant = digits.trim_start_matches('0');
    if significant.len() > 8 {
        return None;
    }
    xml_char(u32::from_str_radix(if significant.is_empty() { "0" } else { significant }, radix).ok()?)
}

/// `raw` with its references replaced by their characters. A run after an `&` that ends at a
/// `;` is a reference. A reference that is not a character (an entity of the DOCTYPE, a name
/// that is not an XML name, a code point that is not a character) is refused by name: a
/// browser puts a value there, or does not draw the file. `place` gives the words for where
/// `raw` is; it is called for a refusal only.
pub(super) fn expand(raw: &str, place: impl Fn() -> String) -> Result<Cow<'_, str>, SvgError> {
    if !raw.contains('&') {
        return Ok(Cow::Borrowed(raw));
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    // One pass: each scan stops at the end of a run, and the next search starts there.
    while let Some(amp) = rest.find('&') {
        out.push_str(rest.get(..amp).unwrap_or(""));
        let after = rest.get(amp + 1..).unwrap_or("");
        // XML whitespace only (2.3, `S`): a name can hold other spaces, such as U+1680, and
        // such a run is a reference that must decode or be refused.
        let len = after
            .find([' ', '\t', '\r', '\n', ';', '&', '<', '"', '\''])
            .unwrap_or(after.len());
        let (name, tail) = after.split_at(len);
        match tail.strip_prefix(';') {
            Some(tail) if !name.is_empty() => match character(name) {
                Some(ch) => {
                    out.push(ch);
                    rest = tail;
                }
                None if name.starts_with('#') => {
                    return Err(invalid(format!(
                        "character reference &{}; in {} is not a character",
                        short(name),
                        place()
                    )));
                }
                None => {
                    return Err(invalid(format!(
                        "entity reference &{}; in {}: an entity of the DOCTYPE is not expanded",
                        short(name),
                        place()
                    )));
                }
            },
            // A plain `&` is text.
            _ => {
                out.push('&');
                out.push_str(name);
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    Ok(Cow::Owned(out))
}

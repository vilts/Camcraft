//! STEP syntax-error locator (#912): when `ruststep` rejects a file it fails
//! the whole DATA section and its position is not the entity that broke, so
//! we re-scan ourselves to name the first bad entity's `#id` and line.
//!
//! Provenance: record framing (`;` terminators, `'..'` strings with `''`
//! escapes, `/* .. */` comments, `DATA;` .. `ENDSEC;` sections) from
//! ISO 10303-21; each record is checked with ruststep's own entity parser.
//! One linear pass: every byte is visited once and every record is parsed
//! once, so a huge file costs no more than the parse that already failed.

use ruststep::parser::exchange::entity_instance;

/// What the re-scan found.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    /// A DATA entity that does not parse (`id` is its `#n`, when readable).
    Entity { id: Option<String>, line: usize, text: String },
    /// A `'` string or `/*` comment that never closes.
    Unterminated { what: &'static str, line: usize },
    /// No `DATA;` section anywhere in the file.
    NoData,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Entity { id, line, text } => {
                let who = id.as_deref().map_or("an entity".to_string(), |i| format!("entity {i}"));
                write!(f, "syntax error in DATA section: {who} at line {line} cannot be parsed: {text}")
            }
            Failure::Unterminated { what, line } => {
                write!(f, "syntax error: {what} starting at line {line} is never closed")
            }
            Failure::NoData => write!(f, "syntax error: no DATA section found"),
        }
    }
}

/// Skip whitespace and comments at the start of `s`; returns the offset.
fn skip_lead(s: &str) -> usize {
    let mut i = 0;
    loop {
        let rest = &s[i..];
        let t = rest.trim_start();
        i += rest.len() - t.len();
        if let Some(body) = t.strip_prefix("/*") {
            match body.find("*/") {
                Some(e) => i += e + 4,
                None => return s.len(),
            }
        } else {
            return i;
        }
    }
}

fn keyword_is(head: &str, kw: &str) -> bool {
    let h = head.trim_end();
    h.len() >= kw.len()
        && h.is_char_boundary(kw.len())
        && h[..kw.len()].eq_ignore_ascii_case(kw)
        && h[kw.len()..].chars().next().is_none_or(|c| c == '(' || c.is_whitespace())
}

/// Check one record (text before its `;`) of a DATA section.
fn check_record(rec: &str, line: usize) -> Option<Failure> {
    let lead = skip_lead(rec);
    let head = &rec[lead..];
    if head.is_empty() {
        return None;
    }
    let line = line + rec[..lead].matches('\n').count();
    let mut probe = String::with_capacity(head.len() + 1);
    probe.push_str(head);
    probe.push(';');
    if matches!(entity_instance(&probe), Ok((rest, _)) if rest.trim().is_empty()) {
        return None;
    }
    let id = head
        .strip_prefix('#')
        .map(|r| r.chars().take_while(char::is_ascii_digit).collect::<String>())
        .filter(|d| !d.is_empty())
        .map(|d| format!("#{d}"));
    let text: String = head.chars().take(80).collect();
    let text = if head.chars().count() > 80 { format!("{text}...") } else { text };
    Some(Failure::Entity { id, line, text: text.replace('\n', " ") })
}

/// Find the first thing wrong with `text`, or `None` when every DATA entity
/// parses on its own (the failure is elsewhere - the caller keeps the
/// parser's own message).
pub fn locate(text: &str) -> Option<Failure> {
    let b = text.as_bytes();
    let (mut i, mut line) = (0usize, 1usize);
    let (mut start, mut start_line) = (0usize, 1usize);
    let (mut in_data, mut saw_data) = (false, false);
    while i < b.len() {
        match b[i] {
            b'\n' => line += 1,
            b'\'' => {
                let open = line;
                i += 1;
                loop {
                    match b.get(i) {
                        None => return Some(unterminated("string", open, in_data, text, start, start_line)),
                        Some(b'\'') if b.get(i + 1) == Some(&b'\'') => i += 1,
                        Some(b'\'') => break,
                        Some(b'\n') => line += 1,
                        _ => {}
                    }
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let open = line;
                i += 2;
                loop {
                    match b.get(i) {
                        None => return Some(unterminated("comment", open, in_data, text, start, start_line)),
                        Some(b'*') if b.get(i + 1) == Some(&b'/') => break,
                        Some(b'\n') => line += 1,
                        _ => {}
                    }
                    i += 1;
                }
                i += 1;
            }
            b';' => {
                let rec = &text[start..i];
                let head = &rec[skip_lead(rec)..];
                if keyword_is(head, "ENDSEC") {
                    in_data = false;
                } else if in_data {
                    if let Some(f) = check_record(rec, start_line) {
                        return Some(f);
                    }
                } else if keyword_is(head, "DATA") {
                    in_data = true;
                    saw_data = true;
                }
                start = i + 1;
                start_line = line;
            }
            _ => {}
        }
        i += 1;
    }
    if saw_data { None } else { Some(Failure::NoData) }
}

fn unterminated(
    what: &'static str,
    line: usize,
    in_data: bool,
    text: &str,
    start: usize,
    start_line: usize,
) -> Failure {
    // The open string/comment swallowed the rest; if an entity was being
    // read, name it: its record starts at `start`.
    if in_data {
        let rec = text.get(start..).unwrap_or("");
        let lead = skip_lead(rec);
        if let Some(r) = rec.get(lead..).filter(|r| r.starts_with('#')) {
            let id: String = r[1..].chars().take_while(char::is_ascii_digit).collect();
            if !id.is_empty() {
                let l = start_line + rec[..lead].matches('\n').count();
                return Failure::Entity {
                    id: Some(format!("#{id}")),
                    line: l,
                    text: format!("{what} starting at line {line} is never closed"),
                };
            }
        }
    }
    Failure::Unterminated { what, line }
}

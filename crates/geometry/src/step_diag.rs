//! STEP syntax-error locator (#912): before the kernel reads a file
//! (#917), a re-scan names the first bad entity's `#id` and line, a string
//! or comment that never closes, and a file with no DATA section.
//!
//! Provenance: record framing (`;` terminators, `'..'` strings with `''`
//! escapes, `/* .. */` comments, `DATA;` .. `ENDSEC;` sections) from
//! ISO 10303-21; each record is checked with solidcore's Part 21 parser,
//! the one the kernel reads the file with (#2789; it was ruststep's entity
//! parser). One linear pass: every byte is visited once and every record is
//! parsed once, so a huge file costs no more than the parse that already
//! failed. Moved from the app (#2789): it reads text, not the document.

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
    if record_parses(head) {
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
            // A resource reference `<...>` (ISO 10303-21 ed. 3) is text: a
            // `/*` or a `'` in it opens nothing (as `step_depth` reads it).
            // Since #917 this scan runs on every file, not only on one the
            // parser refused.
            b'<' => {
                while let Some(&c) = b.get(i + 1) {
                    i += 1;
                    match c {
                        b'>' => break,
                        b'\n' => line += 1,
                        _ => {}
                    }
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

/// Whether one DATA record (its text before the `;`) parses on its own, as
/// solidcore's Part 21 parser reads it in a DATA section: one entity
/// instance and nothing else. `head` holds no `;` outside a string or a
/// comment ([`locate`] cut it there), so the frame around it cannot be
/// closed early from inside.
///
/// The parser holds a text's counts to a density of that text's own length
/// (solidcore fa67b3f5, #3767): a dense but legal record, alone in this
/// short frame, can pass that density. That is no syntax fault of the
/// record; the parse of the whole file judges the counts.
#[cfg(not(target_arch = "wasm32"))]
fn record_parses(head: &str) -> bool {
    use camcraft_kernel::p21::{Error, parse};
    const OPEN: &str = "ISO-10303-21;\nDATA;\n";
    const CLOSE: &str = ";\nENDSEC;\nEND-ISO-10303-21;\n";
    let mut probe = String::with_capacity(OPEN.len() + head.len() + CLOSE.len());
    probe.push_str(OPEN);
    probe.push_str(head);
    probe.push_str(CLOSE);
    match parse(&probe) {
        Ok(ex) => ex.entities.len() == 1 && ex.duplicates.is_empty(),
        Err(Error::TooManyParams { .. } | Error::TooManyRecords { .. } | Error::TooManyEntities { .. }) => true,
        Err(_) => false,
    }
}

/// The browser build has no solidcore: every record passes here, and the
/// STEP reader refuses the file by name after this scan.
#[cfg(target_arch = "wasm32")]
fn record_parses(_head: &str) -> bool {
    true
}

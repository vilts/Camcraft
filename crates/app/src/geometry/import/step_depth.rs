//! STEP nesting guard: one linear byte scan that refuses a record whose
//! parentheses nest past [`MAX_NESTING`] before the recursive-descent
//! parser can overflow its stack on it (#1075).
//!
//! Provenance: lexical rules per ISO 10303-21, mirrored from the tokenizer
//! of the pinned ruststep (rev 90fdc93: `parser/token.rs`,
//! `parser/combinator.rs`, `parser/exchange/*.rs`); the cap is our own
//! measurement, recorded in docs/decisions/1075-step-nesting-cap.md.
//!
//! The scan mirrors a third-party tokenizer, so it must be re-audited
//! whenever the ruststep pin in the root `Cargo.toml` moves. ruststep
//! recurses only on `(` (`list`, `typed_parameter`, `subsuper_record`,
//! `anchor_item_list`), so the scan has to know exactly where a `(` is
//! plain text. Where ruststep's treatment is unclear the scan COUNTS the
//! parenthesis: a wrong refusal of a weird file is acceptable, a missed
//! deep record is not. ruststep parses front to back and stops at its
//! first syntax error, so after a construct it cannot parse it recurses
//! no further; text after one can only be over-counted here.
//!
//! | construct | ruststep | this scan |
//! |---|---|---|
//! | string `'...'` | `'` then `''` or any char but `'`, then `'`; `(` `)` `/*` `<` `"` are text; no backslash escape (`\'` ends it) | `Str`: same rule, `''` taken greedily as ruststep does |
//! | comment `/* ... */` | only between tokens; no nesting; first `*/` closes; `/*/` does not | `Comment`: same; `/*` in `Code` opens one |
//! | resource `<...>` (REFERENCE), anchor name `<...>` (ANCHOR) | `<`, any chars but `>`, `>`; `'` `/*` `(` `"` are text | `Uri`: `<` opens, first `>` closes |
//! | binary `"..."` | not implemented (`FIXME binary`): parse error at `"` | `Binary`: `'` `/*` `<` are text, `(` still COUNTED, `)` not subtracted |
//! | enumeration `.X.`, keywords, `!USER` keywords, `#n` / `@n` names, numbers, `$`, `*` | upper, digit, sign, `.` only - no state | no state; `#n` at depth 0 names the record |
//! | anchor tag `{name:item}` | braces are tokens; the item may be a `(` list | no state; `(` counted |
//! | HEADER records, `DATA(...)` parameters, ANCHOR item lists | the same `(` recursion as DATA records | counted like DATA records (reported without an entity id) |
//! | SIGNATURE (after `END-ISO-10303-21;`) | BASE64 `[0-9A-Za-z+/=]`, no `(`; parsed after every DATA section | scanned as code; whatever it opens only skips text ruststep never recurses into |
//! | unterminated string / comment / `<` / `"` | parse error at the opener | skipped to end of file (nothing after it is parsed) |
//! | unbalanced `)` | parse error | depth saturates at 0 |

use super::ImportError;

/// Deepest parenthesis nesting a STEP record may have. Real files nest a
/// handful deep (our fixtures reach 3); ruststep's parser, in a debug
/// build, dies at about 400 levels on a 1 MiB stack (the Windows main
/// thread, where an inline import runs) and about 820 on 2 MiB (release:
/// about 530 and 1080).
pub const MAX_NESTING: usize = 64;

/// Refuse `data` by name if any record nests deeper than [`MAX_NESTING`].
pub fn check(data: &[u8]) -> Result<(), ImportError> {
    check_with_cap(data, MAX_NESTING)
}

/// The deepest parenthesis nesting anywhere in `data` (strings,
/// comments and `<...>` skipped). For tests and diagnostics.
#[must_use]
pub fn max_depth(data: &[u8]) -> usize {
    let mut deepest = 0;
    scan(data, |depth, _, _| {
        deepest = deepest.max(depth);
        true
    });
    deepest
}

fn check_with_cap(data: &[u8], cap: usize) -> Result<(), ImportError> {
    let mut over: Option<(Option<u64>, usize)> = None;
    scan(data, |depth, entity, line| {
        if depth > cap {
            over = Some((entity, line));
            return false;
        }
        true
    });
    match over {
        None => Ok(()),
        Some((entity, line)) => {
            let at = entity.map_or_else(
                || "a record without an entity id".to_string(),
                |id| format!("entity #{id}"),
            );
            Err(ImportError::Step(format!(
                "record nests deeper than {cap} levels of parentheses at {at}, line {line}"
            )))
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Code,
    Str,
    Comment,
    /// `<...>`: a REFERENCE resource or an ANCHOR name.
    Uri,
    /// `"..."`: a binary literal ruststep cannot parse; `(` still counted.
    Binary,
}

/// Walk `data` once, calling `on_open(depth, entity, line)` after every
/// counted `(` (the module table says which); stop when it returns `false`. `entity` is
/// the `#id` of the record being read (the id that opened it at depth 0),
/// `line` is 1-based and counts `\n` only, so CRLF and LF agree.
fn scan(data: &[u8], mut on_open: impl FnMut(usize, Option<u64>, usize) -> bool) {
    let mut state = State::Code;
    let mut depth: usize = 0;
    let mut line: usize = 1;
    let mut entity: Option<u64> = None;
    let mut i = 0;
    while let Some(&b) = data.get(i) {
        let next = data.get(i + 1).copied();
        if b == b'\n' {
            line = line.saturating_add(1);
        }
        match state {
            State::Str => {
                if b == b'\'' {
                    if next == Some(b'\'') {
                        i += 1; // escaped quote, still inside the string
                    } else {
                        state = State::Code;
                    }
                }
            }
            State::Comment => {
                if b == b'*' && next == Some(b'/') {
                    state = State::Code;
                    i += 1;
                }
            }
            State::Uri => {
                if b == b'>' {
                    state = State::Code;
                }
            }
            State::Binary => match b {
                b'"' => state = State::Code,
                b'(' => {
                    depth = depth.saturating_add(1);
                    if !on_open(depth, entity, line) {
                        return;
                    }
                }
                _ => {}
            },
            State::Code => match b {
                b'\'' => state = State::Str,
                b'<' => state = State::Uri,
                b'"' => state = State::Binary,
                b'/' if next == Some(b'*') => {
                    state = State::Comment;
                    i += 1;
                }
                b'(' => {
                    depth = depth.saturating_add(1);
                    if !on_open(depth, entity, line) {
                        return;
                    }
                }
                b')' => depth = depth.saturating_sub(1),
                b';' if depth == 0 => entity = None,
                b'#' if depth == 0 => {
                    // The record's own id: `#` then digits, at depth 0.
                    let mut id: u64 = 0;
                    let mut j = i + 1;
                    let mut any = false;
                    while let Some(d) = data.get(j).filter(|d| d.is_ascii_digit()) {
                        id = id.saturating_mul(10).saturating_add(u64::from(d - b'0'));
                        any = true;
                        j += 1;
                    }
                    if any {
                        entity = Some(id);
                        i = j - 1;
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested(depth: usize) -> String {
        format!(
            "#7 = THING({}1{});\n",
            "(".repeat(depth - 1),
            ")".repeat(depth - 1)
        )
    }

    #[test]
    fn depth_counts_parentheses_of_a_record() {
        assert_eq!(max_depth(nested(5).as_bytes()), 5);
        assert_eq!(max_depth(b"#1 = A(1.0, (2, 3), $);"), 2);
    }

    #[test]
    fn strings_and_comments_do_not_count() {
        assert_eq!(max_depth(b"#1 = A('(((((', 'it''s ((( ');"), 1);
        assert_eq!(max_depth(b"/* ((((( */ #1 = A(1);"), 1);
        // A `''` escape must not end the string early.
        assert_eq!(max_depth(b"#1 = A('a''(((b');"), 1);
    }

    #[test]
    fn uri_spans_are_text_and_binary_parens_still_count() {
        assert_eq!(max_depth(b"#1 = <x((((/*'>;\n#2 = A(1);"), 1);
        assert_eq!(max_depth(b"<a'b> = 1;\n#2 = A((1));"), 2);
        // ruststep cannot parse a binary; err toward counting inside it.
        assert_eq!(max_depth(b"#1 = A(\"((('/*\");"), 4);
        assert_eq!(max_depth(b"#1 = A(\"'\", (2));"), 2);
    }

    #[test]
    fn refusal_names_cap_entity_and_line() {
        let text = format!("DATA;\n#1 = A(1);\n{}ENDSEC;\n", nested(10));
        let err = check_with_cap(text.as_bytes(), 8)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err.contains("deeper than 8"), "{err}");
        assert!(err.contains("entity #7"), "{err}");
        assert!(err.contains("line 3"), "{err}");
    }

    #[test]
    fn at_the_cap_passes() {
        assert!(check_with_cap(nested(8).as_bytes(), 8).is_ok());
        assert!(check_with_cap(nested(9).as_bytes(), 8).is_err());
    }

    #[test]
    fn crlf_and_lf_give_the_same_line() {
        let lf = format!("A;\n\n#1 = B(1);\n{}", nested(10));
        let crlf = lf.replace('\n', "\r\n");
        let a = check_with_cap(lf.as_bytes(), 8)
            .err()
            .map(|e| e.to_string());
        let b = check_with_cap(crlf.as_bytes(), 8)
            .err()
            .map(|e| e.to_string());
        assert!(a.as_deref().is_some_and(|m| m.contains("line 4")), "{a:?}");
        assert_eq!(a, b);
    }

    #[test]
    fn unbalanced_and_non_utf8_are_left_to_the_parser() {
        assert!(check(b"#1 = A(1)));)))(((;").is_ok());
        assert!(check(&[0xff, 0xfe, b'(', 0x80, b')']).is_ok());
        assert!(check(b"#1 = A('unterminated (((((").is_ok());
    }
}

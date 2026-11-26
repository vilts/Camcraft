//! One CSS scanner for the inline `style` and the `<style>` sheets: comments, strings, escapes
//! and `url(` tokens end where a browser ends them, so the two readers cannot disagree.
//! Provenance: CSS Syntax Level 3 (4.3.4 ident-like tokens, 4.3.5 strings, 4.3.6 url tokens,
//! 4.3.7 escapes, 4.3.14 the remnants of a bad url); original implementation.

/// One piece of CSS text.
pub(super) enum Piece<'a> {
    /// White space or a comment.
    Space,
    /// A string with its quotes; the escapes are decoded.
    Str(String),
    /// An escaped character outside a string.
    Escaped(char),
    /// An unquoted `url(` token, from its `(` to its `)`, as written. A comment, a `;` or a
    /// brace in it is part of the address.
    Url(&'a str),
    /// A bad `url(` token (a quote, a `(` or a gap in an unquoted address), to its first `)`.
    BadUrl,
    Char(char),
}

/// The identifier that ends at the scan position, as far as `url` needs it.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Word {
    #[default]
    None,
    U,
    Ur,
    Url,
    Other,
}

impl Word {
    /// The state after `ch`. An escaped character is a part of an identifier (`\75rl` is `url`).
    fn then(self, ch: char, escaped: bool) -> Word {
        let ident = escaped || ch.is_alphanumeric() || matches!(ch, '-' | '_') || !ch.is_ascii();
        match (self, ch.to_ascii_lowercase()) {
            (Word::None, 'u') => Word::U,
            (Word::U, 'r') => Word::Ur,
            (Word::Ur, 'l') => Word::Url,
            // `#url(` and `@url(` are a hash and an at-keyword before a block, not a function.
            _ if ident || matches!(ch, '#' | '@') => Word::Other,
            _ => Word::None,
        }
    }
}

fn is_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\x0c')
}

/// The pieces of `text` from a byte index, each with the index of its first byte.
pub(super) struct Lexer<'a> {
    text: &'a str,
    at: usize,
    word: Word,
}

impl<'a> Lexer<'a> {
    pub fn new(text: &'a str, from: usize) -> Self {
        Lexer { text, at: from, word: Word::None }
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = (usize, Piece<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.at;
        let rest = self.text.get(start..)?;
        let ch = rest.chars().next()?;
        let word = std::mem::take(&mut self.word);
        let after = rest.get(ch.len_utf8()..).unwrap_or("");
        let (piece, len) = if rest.starts_with("/*") {
            (Piece::Space, after.get(1..).and_then(|body| body.find("*/")).map_or(rest.len(), |end| end + 4))
        } else if is_space(ch) {
            (Piece::Space, 1)
        } else {
            match ch {
                '"' | '\'' => {
                    let mut out = String::new();
                    let end = consume_css_string(self.text, start, ch, &mut out);
                    (Piece::Str(out), end.saturating_sub(start))
                }
                '\\' => match decode_css_escape(after) {
                    Some((decoded, n)) => {
                        self.word = word.then(decoded, true);
                        (Piece::Escaped(decoded), 1 + n)
                    }
                    None => (Piece::Char('\\'), 1),
                },
                // `url(` before a quote is a function with a string argument: the usual rules.
                '(' if word == Word::Url && !after.trim_start_matches(is_space).starts_with(['"', '\'']) => {
                    let (good, n) = url_token(after);
                    let piece = if good { Piece::Url(rest.get(..1 + n).unwrap_or("")) } else { Piece::BadUrl };
                    (piece, 1 + n)
                }
                _ => {
                    self.word = word.then(ch, false);
                    (Piece::Char(ch), ch.len_utf8())
                }
            }
        };
        self.at = start.saturating_add(len.max(1));
        Some((start, piece))
    }
}

/// The unquoted address after `url(`: whether it is a good token, and its length with the `)`.
/// A token ends at the first `)` that is not escaped, or at the end of the text. Inside it no
/// quote starts a string, no `(` adds a level and no `/*` starts a comment.
fn url_token(body: &str) -> (bool, usize) {
    let mut good = true;
    let mut started = false;
    let mut gap = false;
    let mut chars = body.char_indices();
    while let Some((i, ch)) = chars.next() {
        match ch {
            ')' => return (good, i + 1),
            '\\' => match chars.clone().next() {
                // `\` before a line end is not an escape.
                Some((_, '\n' | '\r' | '\x0c')) => good = false,
                _ => {
                    chars.next();
                    good &= !gap;
                    started = true;
                }
            },
            ch if is_space(ch) => gap |= started,
            '"' | '\'' | '(' | '\u{0}'..='\u{8}' | '\u{b}' | '\u{e}'..='\u{1f}' | '\u{7f}' => good = false,
            _ => {
                // White space is permitted only before the `)`.
                good &= !gap;
                started = true;
            }
        }
    }
    (good, body.len())
}

/// Decode one CSS backslash escape starting immediately after `\`.
pub(super) fn decode_css_escape(rest: &str) -> Option<(char, usize)> {
    let mut chars = rest.char_indices();
    let (_, first) = chars.next()?;
    if matches!(first, '\n' | '\r' | '\x0c') {
        return None;
    }
    if first.is_ascii_hexdigit() {
        let mut end = first.len_utf8();
        for (idx, ch) in chars.take(5) {
            if !ch.is_ascii_hexdigit() {
                break;
            }
            end = idx + ch.len_utf8();
        }
        let code = u32::from_str_radix(&rest[..end], 16).unwrap_or(0);
        let decoded = match code {
            0 => '\u{FFFD}',
            n => char::from_u32(n).unwrap_or('\u{FFFD}'),
        };
        // Per CSS2.1 §4.1.3, one trailing whitespace (or CRLF) ends a hex escape.
        let tail = &rest[end..];
        if tail.starts_with("\r\n") {
            end += 2;
        } else if tail
            .as_bytes()
            .first()
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c'))
        {
            end += 1;
        }
        return Some((decoded, end));
    }
    Some((first, first.len_utf8()))
}

/// Read the string that starts at byte `i` into `out`, with its quotes; the index after it.
fn consume_css_string(style: &str, mut i: usize, quote: char, out: &mut String) -> usize {
    out.push(quote);
    i += quote.len_utf8();
    while let Some(rest) = style.get(i..) {
        let Some(ch) = rest.chars().next() else { break };
        if matches!(ch, '\n' | '\r' | '\x0c') {
            break;
        }
        if ch == quote {
            out.push(quote);
            i += quote.len_utf8();
            break;
        }
        if ch == '\\' {
            i += 1;
            let after = style.get(i..).unwrap_or("");
            if let Some((decoded, n)) = decode_css_escape(after) {
                out.push(decoded);
                i += n;
            } else if after.starts_with("\r\n") {
                i += 2;
            } else if after.chars().next().is_some_and(|c| matches!(c, '\n' | '\r' | '\x0c')) {
                i += 1;
            } else {
                break;
            }
            continue;
        }
        out.push(ch);
        i += ch.len_utf8();
    }
    i
}

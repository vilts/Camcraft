//! G-code syntax colouring for the post viewer (#256): a line tokenizer
//! plus an egui `LayoutJob` builder. Specced from the public RS-274 /
//! ISO 6983 word format (a letter followed by a number; `(..)` and `;`
//! comments; `N` block numbers); no HSMWorks material involved.
//! Display only: the job's text is byte-for-byte the posted line.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Ui};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

/// What a span of a G-code line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Plain,
    Comment,
    BlockNo,
    GWord,
    MWord,
    ToolWord,
    FeedSpeed,
    Axis,
    OtherWord,
    Number,
}

/// Split one line into contiguous `(byte range, kind)` spans covering
/// it exactly. Never panics: unknown characters are `Plain`, an
/// unterminated `(` comment runs to the end of the line.
pub(crate) fn tokenize(line: &str) -> Vec<(Range<usize>, Kind)> {
    let mut out: Vec<(Range<usize>, Kind)> = Vec::new();
    let mut push = |r: Range<usize>, k: Kind| {
        if r.is_empty() {
            return;
        }
        if let Some((last, lk)) = out.last_mut()
            && *lk == k
            && last.end == r.start
            && matches!(k, Kind::Plain | Kind::Comment)
        {
            last.end = r.end;
            return;
        }
        out.push((r, k));
    };
    let mut it = line.char_indices().peekable();
    let mut first_word = true;
    while let Some((i, c)) = it.next() {
        match c {
            ';' => {
                push(i..line.len(), Kind::Comment);
                break;
            }
            '(' => {
                let mut e = line.len();
                for (j, d) in it.by_ref() {
                    if d == ')' {
                        e = j + 1;
                        break;
                    }
                }
                push(i..e, Kind::Comment);
            }
            c if c.is_ascii_alphabetic() => {
                let kind = match c.to_ascii_uppercase() {
                    'N' if first_word => Kind::BlockNo,
                    'G' => Kind::GWord,
                    'M' => Kind::MWord,
                    'T' => Kind::ToolWord,
                    'F' | 'S' => Kind::FeedSpeed,
                    'X' | 'Y' | 'Z' | 'A' | 'B' | 'C' | 'U' | 'V' | 'W' | 'I' | 'J' | 'K' => {
                        Kind::Axis
                    }
                    _ => Kind::OtherWord,
                };
                first_word = false;
                push(i..i + c.len_utf8(), kind);
                let mut e = i + c.len_utf8();
                while let Some(&(j, d)) = it.peek() {
                    if d.is_ascii_digit() || matches!(d, '.' | '-' | '+') {
                        e = j + d.len_utf8();
                        it.next();
                    } else {
                        break;
                    }
                }
                push(i + c.len_utf8()..e, Kind::Number);
            }
            c if c.is_ascii_digit() || matches!(c, '.' | '-' | '+') => {
                let mut e = i + c.len_utf8();
                while let Some(&(j, d)) = it.peek() {
                    if d.is_ascii_digit() || d == '.' {
                        e = j + d.len_utf8();
                        it.next();
                    } else {
                        break;
                    }
                }
                push(i..e, Kind::Number);
            }
            c => {
                push(i..i + c.len_utf8(), Kind::Plain);
            }
        }
    }
    out
}

/// The span colours for the current theme, as `(dark?, kind)` -> ink.
pub(crate) fn ink(dark: bool, kind: Kind) -> Option<Color32> {
    let (d, l) = match kind {
        Kind::Plain => return None,
        Kind::Comment => ((0x6A, 0x99, 0x55), (0x2E, 0x7D, 0x32)),
        Kind::BlockNo => ((0x80, 0x88, 0x90), (0x70, 0x78, 0x80)),
        Kind::GWord => ((0x56, 0x9C, 0xD6), (0x00, 0x4F, 0xB0)),
        Kind::MWord => ((0xC5, 0x86, 0xC0), (0x8E, 0x24, 0xAA)),
        Kind::ToolWord => ((0xE5, 0xC0, 0x7B), (0xB0, 0x5A, 0x00)),
        Kind::FeedSpeed => ((0x4E, 0xC9, 0xB0), (0x00, 0x7A, 0x6A)),
        Kind::Axis => ((0xD7, 0x8A, 0x5A), (0xB0, 0x3A, 0x00)),
        Kind::OtherWord => ((0x9C, 0xDC, 0xFE), (0x2A, 0x5D, 0x8F)),
        Kind::Number => ((0xB5, 0xCE, 0xA8), (0x1B, 0x5E, 0x20)),
    };
    let (r, g, b) = if dark { d } else { l };
    Some(Color32::from_rgb(r, g, b))
}

/// Cached tokenization of a whole program: one span list per line.
pub(crate) type Spans = Arc<Vec<Vec<(Range<usize>, Kind)>>>;

pub(crate) fn program_key(program: &str) -> u64 {
    // Length plus a bounded sample of both ends: O(1) per frame. A
    // collision can only mis-colour a listing, never change its text.
    let mut h = DefaultHasher::new();
    program.len().hash(&mut h);
    let b = program.as_bytes();
    b[..b.len().min(256)].hash(&mut h);
    b[b.len().saturating_sub(256)..].hash(&mut h);
    h.finish()
}

/// Tokenize the program once per program (not per frame), cached in
/// the egui context's temp memory.
pub(crate) fn spans_for(ctx: &egui::Context, program: &str) -> Spans {
    let id = egui::Id::new("gcode-highlight-cache");
    let key = program_key(program);
    if let Some((k, s)) = ctx.data(|d| d.get_temp::<(u64, Spans)>(id))
        && k == key
    {
        return s;
    }
    let s: Spans = Arc::new(program.lines().map(tokenize).collect());
    ctx.data_mut(|d| d.insert_temp(id, (key, s.clone())));
    s
}

/// The gutter text of a listing row: the right-aligned line number and
/// two spaces. Its own non-selectable label (#1089), so selecting and
/// copying the program takes the G-code only, never the numbers.
pub(crate) fn gutter_text(number: usize) -> String {
    format!("{number:>4}  ")
}

/// The listing row's code: the coloured line, byte-for-byte as posted.
/// `dim` rests everything weak (trace pane outside its block).
pub(crate) fn row_job(
    ui: &Ui,
    line: &str,
    spans: &[(Range<usize>, Kind)],
    dim: bool,
) -> LayoutJob {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let dark = ui.visuals().dark_mode;
    let base = if dim {
        ui.visuals().weak_text_color()
    } else {
        ui.visuals().text_color()
    };
    let fmt = |c: Color32| TextFormat::simple(font.clone(), c);
    let mut job = LayoutJob::default();
    let mut at = 0;
    for (r, k) in spans {
        // Spans are built from this very line; guard anyway.
        let Some(text) = line.get(r.clone()) else {
            continue;
        };
        if r.start != at {
            continue;
        }
        at = r.end;
        let c = if dim { base } else { ink(dark, *k).unwrap_or(base) };
        job.append(text, 0.0, fmt(c));
    }
    if let Some(rest) = line.get(at..)
        && !rest.is_empty()
    {
        job.append(rest, 0.0, fmt(base));
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str) -> Vec<(&str, Kind)> {
        tokenize(line).into_iter().map(|(r, k)| (&line[r], k)).collect()
    }

    #[test]
    fn spans_cover_the_line_exactly() {
        for l in [
            "N10 G01 X-12.5 Y.5 F300. (cut) ; tail",
            "G0 X1 (unterminated",
            "  ",
            "%",
            "Gé Ω X1.2.3 ((",
            "",
        ] {
            let joined: String = tokenize(l).into_iter().map(|(r, _)| &l[r]).collect();
            assert_eq!(joined, l);
        }
    }

    #[test]
    fn words_numbers_and_comments_get_distinct_kinds() {
        let k = kinds("N10 G01 X-12.5 M3 T2 S8000 (hi) ;x");
        assert!(k.contains(&("N", Kind::BlockNo)));
        assert!(k.contains(&("G", Kind::GWord)));
        assert!(k.contains(&("-12.5", Kind::Number)));
        assert!(k.contains(&("X", Kind::Axis)));
        assert!(k.contains(&("M", Kind::MWord)));
        assert!(k.contains(&("T", Kind::ToolWord)));
        assert!(k.contains(&("S", Kind::FeedSpeed)));
        assert!(k.contains(&("(hi)", Kind::Comment)));
        assert!(k.contains(&(";x", Kind::Comment)));
    }

    #[test]
    fn n_after_the_first_word_is_not_a_block_number() {
        assert!(kinds("G1 N5").contains(&("N", Kind::OtherWord)));
    }

    #[test]
    fn malformed_lines_do_not_panic() {
        for l in ["((((", ")))", "G", "X..--++", "\u{1F600}G1", "(é", ";"] {
            let _ = tokenize(l);
        }
    }

    #[test]
    fn theme_inks_differ_and_plain_has_none() {
        assert!(ink(true, Kind::Plain).is_none());
        assert_ne!(ink(true, Kind::GWord), ink(false, Kind::GWord));
        assert_ne!(ink(true, Kind::GWord), ink(true, Kind::MWord));
    }
}

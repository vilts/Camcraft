//! CSS declaration lists (the inline `style`, and the block of a rule of a sheet) split into
//! declarations. The pieces come from the one scanner in `css.rs`.
//! Provenance: the W3C CSS 2.1 syntax rules (declaration syntax and `!important` rules) and
//! CSS Syntax Level 3 (a declaration with a bad url token is dropped); original implementation.

use super::css::{Lexer, Piece};

pub(super) struct StyleDecl {
    pub property: String,
    pub value: String,
    pub has_bang: bool,
    pub importance: String,
    /// The declaration has a bad `url(` token. A browser drops such a declaration and reads
    /// the next one; this reader refuses the style, because it is not read in part.
    pub bad_url: bool,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum DeclPart {
    #[default]
    Property,
    Value,
    Importance,
}

#[derive(Default)]
struct DeclBuilder {
    property: String,
    value: String,
    importance: String,
    part: DeclPart,
    has_colon: bool,
    has_bang: bool,
    pending_ws: bool,
    depth: usize,
    bad_url: bool,
}

impl DeclBuilder {
    fn target(&mut self) -> &mut String {
        match self.part {
            DeclPart::Property => &mut self.property,
            DeclPart::Value => &mut self.value,
            DeclPart::Importance => &mut self.importance,
        }
    }

    fn push_char(&mut self, ch: char) {
        let pending = self.pending_ws;
        self.pending_ws = false;
        let out = self.target();
        if pending && !out.is_empty() {
            out.push(' ');
        }
        out.push(ch);
    }

    fn finish(&mut self, out: &mut Vec<StyleDecl>) {
        if self.has_colon || self.bad_url {
            out.push(StyleDecl {
                property: std::mem::take(&mut self.property),
                value: std::mem::take(&mut self.value),
                has_bang: self.has_bang,
                importance: std::mem::take(&mut self.importance),
                bad_url: self.bad_url,
            });
        } else {
            self.property.clear();
            self.value.clear();
            self.importance.clear();
        }
        self.part = DeclPart::Property;
        self.has_colon = false;
        self.has_bang = false;
        self.pending_ws = false;
        self.depth = 0;
        self.bad_url = false;
    }
}

/// Split a CSS declaration list into declarations, stripping comments and decoding escapes.
pub(super) fn inline_declarations(style: &str) -> Vec<StyleDecl> {
    let mut decls = Vec::new();
    let mut b = DeclBuilder::default();
    for (_, piece) in Lexer::new(style, 0) {
        match piece {
            Piece::Space => b.pending_ws = true,
            Piece::Str(text) => {
                if b.pending_ws && !b.target().is_empty() {
                    b.target().push(' ');
                }
                b.pending_ws = false;
                b.target().push_str(&text);
            }
            Piece::Escaped(ch) => b.push_char(ch),
            // The address is one token: a `;`, a brace or a `(` in it changes nothing.
            Piece::Url(raw) => raw.chars().for_each(|ch| b.push_char(ch)),
            // The token ended at its first `)`, as in a browser: the next `;` splits.
            Piece::BadUrl => b.bad_url = true,
            Piece::Char(';') if b.depth == 0 => b.finish(&mut decls),
            Piece::Char(':') if b.part == DeclPart::Property && b.depth == 0 => {
                b.has_colon = true;
                b.part = DeclPart::Value;
                b.pending_ws = false;
            }
            Piece::Char('!') if b.part == DeclPart::Value && b.depth == 0 => {
                b.has_bang = true;
                b.part = DeclPart::Importance;
                b.pending_ws = false;
            }
            Piece::Char(ch @ ('(' | '[' | '{')) => {
                b.depth += 1;
                b.push_char(ch);
            }
            Piece::Char(ch @ (')' | ']' | '}')) => {
                b.depth = b.depth.saturating_sub(1);
                b.push_char(ch);
            }
            Piece::Char(ch) => b.push_char(ch),
        }
    }
    b.finish(&mut decls);
    decls
}

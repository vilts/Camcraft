//! SVG number, flag, length and list scanning.
//! Provenance: the W3C SVG 1.1 path-data and basic-data-type grammars (numbers that run
//! together, single-character arc flags, CSS absolute length units at 96 px to the inch);
//! original implementation.

use super::{SvgError, invalid};

/// A scanner over a list of numbers separated by white space and at most one comma.
pub(super) struct Nums<'a> {
    s: &'a [u8],
    pub at: usize,
}

impl<'a> Nums<'a> {
    pub fn new(text: &'a str) -> Self {
        Nums { s: text.as_bytes(), at: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.at).copied()
    }

    fn space(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    /// Skip white space and one optional comma.
    pub fn sep(&mut self) {
        self.space();
        if self.peek() == Some(b',') {
            self.at += 1;
            self.space();
        }
    }

    /// True at the end, after white space.
    pub fn done(&mut self) -> bool {
        self.space();
        self.at >= self.s.len()
    }

    /// The next byte, if it is a letter (a path command).
    pub fn letter(&mut self) -> Option<u8> {
        self.space();
        self.peek().filter(u8::is_ascii_alphabetic)
    }

    pub fn bump(&mut self) {
        self.at += 1;
    }

    fn digits(&mut self) -> usize {
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.at += 1;
        }
        self.at - start
    }

    /// One number, or `None` (cursor unmoved) when none starts here.
    pub fn number(&mut self) -> Option<f64> {
        self.space();
        let start = self.at;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.at += 1;
        }
        let whole = self.digits();
        let mut frac = 0;
        if self.peek() == Some(b'.') {
            self.at += 1;
            frac = self.digits();
        }
        if whole + frac == 0 {
            self.at = start;
            return None;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let mark = self.at;
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if self.digits() == 0 {
                self.at = mark; // "1em": the e is a unit, not an exponent
            }
        }
        let text = std::str::from_utf8(self.s.get(start..self.at)?).ok()?;
        text.parse().ok()
    }

    /// One arc flag: a single `0` or `1`.
    pub fn flag(&mut self) -> Option<bool> {
        self.space();
        let flag = match self.peek()? {
            b'0' => false,
            b'1' => true,
            _ => return None,
        };
        self.at += 1;
        Some(flag)
    }

    /// The text after the cursor (a unit suffix).
    pub fn tail(&self) -> &'a str {
        std::str::from_utf8(self.s.get(self.at..).unwrap_or(&[])).unwrap_or("").trim()
    }
}

/// A finite number, or a named refusal.
pub(super) fn finite(v: f64, what: &str) -> Result<f64, SvgError> {
    if v.is_finite() { Ok(v) } else { Err(invalid(format!("{what} is not a finite number"))) }
}

/// Every number of a list (`points`, `viewBox`), each finite.
pub(super) fn list(text: &str, what: &str) -> Result<Vec<f64>, SvgError> {
    let mut nums = Nums::new(text);
    let mut out = Vec::new();
    while !nums.done() {
        let v = nums.number().ok_or_else(|| invalid(format!("{what} is not a list of numbers")))?;
        out.push(finite(v, what)?);
        nums.sep();
    }
    Ok(out)
}

/// The unit of a length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum LenUnit {
    None,
    Px,
    Mm,
    Cm,
    In,
    Pt,
    Pc,
    /// `%`, `em` or `ex`: relative to something this reader does not know.
    Relative,
}

impl LenUnit {
    /// Millimetres per unit; `None` for a unitless or relative length.
    pub fn mm(self) -> Option<f64> {
        match self {
            LenUnit::Px => Some(25.4 / 96.0),
            LenUnit::Mm => Some(1.0),
            LenUnit::Cm => Some(10.0),
            LenUnit::In => Some(25.4),
            LenUnit::Pt => Some(25.4 / 72.0),
            LenUnit::Pc => Some(25.4 / 6.0),
            LenUnit::None | LenUnit::Relative => None,
        }
    }
}

/// A number with an optional unit.
pub(super) fn length(text: &str, what: &str) -> Result<(f64, LenUnit), SvgError> {
    let mut nums = Nums::new(text);
    let v = nums.number().ok_or_else(|| invalid(format!("{what} \"{text}\" is not a length")))?;
    let v = finite(v, what)?;
    let unit = match nums.tail() {
        "" => LenUnit::None,
        "px" => LenUnit::Px,
        "mm" => LenUnit::Mm,
        "cm" => LenUnit::Cm,
        "in" => LenUnit::In,
        "pt" => LenUnit::Pt,
        "pc" => LenUnit::Pc,
        "%" | "em" | "ex" => LenUnit::Relative,
        other => return Err(invalid(format!("{what} has the unknown unit \"{other}\""))),
    };
    Ok((v, unit))
}

/// A length inside the drawing, in user units: an absolute unit converts at 96 px to the inch.
/// A relative length (`%`, `em`, `ex`) is refused by name.
pub(super) fn user_length(text: &str, what: &str) -> Result<f64, SvgError> {
    let (v, unit) = length(text, what)?;
    match unit {
        LenUnit::None | LenUnit::Px => Ok(v),
        LenUnit::Relative => Err(invalid(format!("{what} \"{text}\" is a percent or font-relative length, which is not supported"))),
        absolute => Ok(v * absolute.mm().unwrap_or(1.0) * 96.0 / 25.4),
    }
}

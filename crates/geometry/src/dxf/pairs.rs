//! Provenance: Autodesk DXF ASCII code/value pairs and record delimiters.
use super::types::{DxfError, error};

#[derive(Clone, Copy)]
pub(super) struct Pair<'a> {
    pub code: u16,
    pub value: &'a str,
}

#[derive(Clone, Copy)]
pub(super) struct Pairs<'a> {
    pub rest: &'a str,
}

impl<'a> Pairs<'a> {
    fn line(&mut self) -> Option<&'a str> {
        if self.rest.is_empty() {
            return None;
        }
        let (line, rest) = self.rest.split_once('\n').unwrap_or((self.rest, ""));
        self.rest = rest;
        Some(line.trim_end_matches('\r'))
    }

    pub fn next(&mut self) -> Result<Option<Pair<'a>>, DxfError> {
        loop {
            let Some(code) = self.line() else {
                return Ok(None);
            };
            let value = self
                .line()
                .ok_or_else(|| error("stream", "truncated group pair"))?;
            let code = code
                .trim()
                .parse::<u16>()
                .map_err(|_| error("stream", "malformed group code"))?;
            if code > 1071 {
                return Err(error("stream", "group code outside DXF range"));
            }
            if code != 999 {
                return Ok(Some(Pair { code, value }));
            }
        }
    }

    /// Borrow a record body without collecting its group pairs.
    pub fn record(&mut self) -> Result<Option<Record<'a>>, DxfError> {
        let Some(first) = self.next()? else {
            return Ok(None);
        };
        if first.code != 0 {
            return Err(error("stream", "expected group 0 record"));
        }
        let start = self.rest;
        let mut scan = *self;
        loop {
            let before = scan.rest;
            match scan.next()? {
                Some(pair) if pair.code == 0 => {
                    self.rest = before;
                    break;
                }
                None => {
                    self.rest = "";
                    break;
                }
                _ => {}
            }
        }
        let body = &start[..start.len() - self.rest.len()];
        Ok(Some(Record {
            kind: first.value.trim(),
            body,
        }))
    }
}

#[derive(Clone, Copy)]
pub(super) struct Record<'a> {
    pub kind: &'a str,
    pub body: &'a str,
}

impl<'a> Record<'a> {
    pub fn pairs(self) -> Pairs<'a> {
        Pairs { rest: self.body }
    }

    pub fn field(self, code: u16) -> Result<Option<&'a str>, DxfError> {
        let mut result = None;
        let mut pairs = self.pairs();
        while let Some(pair) = pairs.next()? {
            if pair.code == code {
                if result.is_some() {
                    return Err(error(self.kind, &format!("duplicate group {code}")));
                }
                result = Some(pair.value);
            }
        }
        Ok(result)
    }

    pub fn required(self, code: u16) -> Result<&'a str, DxfError> {
        self.field(code)?
            .ok_or_else(|| error(self.kind, &format!("missing group {code}")))
    }

    pub fn number(self, code: u16, default: Option<f64>) -> Result<f64, DxfError> {
        match self.field(code)? {
            Some(value) => number(value, self.kind),
            None => default.ok_or_else(|| error(self.kind, &format!("missing group {code}"))),
        }
    }

    pub fn integer(self, code: u16, default: i32) -> Result<i32, DxfError> {
        match self.field(code)? {
            Some(value) => integer(value, self.kind),
            None => Ok(default),
        }
    }

    pub fn empty(self) -> Result<(), DxfError> {
        if self.pairs().next()?.is_some() {
            return Err(error(self.kind, "unexpected fields"));
        }
        Ok(())
    }

    /// Reject application data that can shadow geometry fields. Do not interpret it as geometry.
    pub fn validate(self) -> Result<(), DxfError> {
        let mut pairs = self.pairs();
        while let Some(pair) = pairs.next()? {
            if pair.code == 102 {
                return Err(error(
                    self.kind,
                    "application-defined group 102 is unsupported",
                ));
            }
            if matches!(pair.code, 10..=59 | 110..=149 | 210..=239 | 460..=469 | 1010..=1059) {
                number(pair.value, self.kind).map_err(|e| {
                    if matches!(pair.code, 210 | 220 | 230) {
                        error(self.kind, &format!("invalid extrusion: {e}"))
                    } else {
                        e
                    }
                })?;
            }
        }
        Ok(())
    }
}

pub(super) fn number(value: &str, kind: &str) -> Result<f64, DxfError> {
    let value = value
        .trim()
        .parse::<f64>()
        .map_err(|_| error(kind, "invalid numeric value"))?;
    if !value.is_finite() {
        return Err(error(kind, "non-finite numeric value"));
    }
    Ok(value)
}

pub(super) fn integer(value: &str, kind: &str) -> Result<i32, DxfError> {
    value
        .trim()
        .parse::<i32>()
        .map_err(|_| error(kind, "invalid integer value"))
}

/// Handles are unsigned hexadecimal strings of at most sixteen digits. Zero is the null pointer.
pub(super) fn handle(value: &str, kind: &str) -> Result<u64, DxfError> {
    let digits = value.trim();
    if digits.len() > 16 || !digits.bytes().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(error(kind, "invalid hexadecimal handle"));
    }
    u64::from_str_radix(digits, 16).map_err(|_| error(kind, "invalid hexadecimal handle"))
}

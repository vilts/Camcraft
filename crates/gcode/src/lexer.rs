//! Line-oriented RS-274/NGC and ISO 6983 block lexer.
//! Provenance: block syntax, parenthesized/semicolon comments, and word addressing from NISTIR 6556 (§3.3) and ISO 6983; no external parser code consulted.

use crate::types::GcodeError;

/// Parsed G-code or M-code number (integer major plus an optional one-digit decimal minor:
/// `G90.1` has minor 1; a code with two or more significant decimal digits is refused).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeNum {
    pub major: u32,
    pub minor: Option<u32>,
    pub text: String,
}

impl CodeNum {
    /// Formatted code name with letter prefix, e.g. `"G90.1"` or `"M30"`.
    #[must_use]
    pub fn label(&self, letter: char) -> String {
        format!("{letter}{}", self.text)
    }
}

/// One address-value word on a block (`G1`, `X10.5`, `F500`).
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub letter: char,
    pub value: f64,
    pub code: Option<CodeNum>,
    pub raw: String,
}

/// Tokenized source line (1-based line number).
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedLine {
    pub line: usize,
    pub is_percent: bool,
    pub program_number: Option<u32>,
    pub words: Vec<Word>,
}

const MACRO_KEYWORDS: &[&str] = &[
    "IF",
    "THEN",
    "ELSE",
    "ENDIF",
    "WHILE",
    "ENDWHILE",
    "DO",
    "END",
    "GOTO",
    "SUB",
    "ENDSUB",
    "CALL",
    "REPEAT",
    "ENDREPEAT",
    "RETURN",
];

/// Tokenize an entire NC program into lines, preserving 1-based line numbers.
pub fn tokenize(program: &str) -> Result<Vec<ParsedLine>, GcodeError> {
    program
        .lines()
        .enumerate()
        .map(|(idx, raw)| tokenize_line(idx + 1, raw))
        .collect()
}

pub fn tokenize_line(line: usize, raw: &str) -> Result<ParsedLine, GcodeError> {
    let bytes = raw.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut is_percent = false;
    let mut program_number: Option<u32> = None;
    let mut words: Vec<Word> = Vec::new();

    while i < len {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b == b';' {
            break;
        }
        if b == b'(' {
            i = skip_paren_comment(line, raw, i)?;
            continue;
        }
        if b == b'%' {
            if !words.is_empty() || program_number.is_some() || is_percent {
                return Err(GcodeError::Syntax {
                    line,
                    message: "unexpected '%' delimiter on block with words".to_string(),
                });
            }
            is_percent = true;
            i += 1;
            continue;
        }
        if is_percent {
            return Err(GcodeError::Syntax {
                line,
                message: "unexpected token after '%' delimiter".to_string(),
            });
        }
        if b == b'#' {
            return Err(GcodeError::UnsupportedCode {
                line,
                code: "#".to_string(),
                reason: "macro variables and '#' expressions are not supported".to_string(),
            });
        }
        if matches!(b, b'[' | b']' | b'=') {
            return Err(GcodeError::UnsupportedCode {
                line,
                code: (b as char).to_string(),
                reason: "macro expressions are not supported".to_string(),
            });
        }
        if b == b'/' {
            return Err(GcodeError::UnsupportedCode {
                line,
                code: "/".to_string(),
                reason: "optional block delete '/' is not supported".to_string(),
            });
        }
        if !b.is_ascii_alphabetic() {
            let ch = raw[i..].chars().next().unwrap_or('?');
            return Err(GcodeError::Syntax {
                line,
                message: format!("unexpected character '{ch}'"),
            });
        }

        // Reject multi-letter alphabetic identifiers (e.g. IF, GOTO, SUB, MOTON, CYCLE832).
        let alpha_end = scan_alpha(bytes, i);
        if alpha_end - i > 1 {
            let ident = raw[i..alpha_end].to_ascii_uppercase();
            if MACRO_KEYWORDS.contains(&ident.as_str()) {
                return Err(GcodeError::UnsupportedCode {
                    line,
                    code: ident,
                    reason: "macro/subprogram control flow is not supported".to_string(),
                });
            }
            return Err(GcodeError::UnknownWord { line, word: ident });
        }

        let letter = (b as char).to_ascii_uppercase();
        i += 1;
        while i < len && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i < len && bytes[i] == b'#' {
            return Err(GcodeError::UnsupportedCode {
                line,
                code: "#".to_string(),
                reason: "macro variables and '#' expressions are not supported".to_string(),
            });
        }
        if i < len && bytes[i] == b'[' {
            return Err(GcodeError::UnsupportedCode {
                line,
                code: "[".to_string(),
                reason: "macro expressions are not supported".to_string(),
            });
        }
        if letter == 'O' && i < len && bytes[i] == b'<' {
            return Err(GcodeError::UnsupportedCode {
                line,
                code: "O<...>".to_string(),
                reason: "named O-word subprograms are not supported".to_string(),
            });
        }

        let (num_str, next_i) = scan_number(line, letter, raw, i)?;
        i = next_i;

        let value: f64 = num_str.parse().map_err(|_| GcodeError::Syntax {
            line,
            message: format!("invalid number '{num_str}' on word '{letter}'"),
        })?;
        if !value.is_finite() {
            return Err(GcodeError::Syntax {
                line,
                message: format!("non-finite number '{num_str}' on word '{letter}'"),
            });
        }

        if letter == 'O' {
            let prog = parse_u32_token(line, letter, num_str)?;
            if program_number.is_some() {
                return Err(GcodeError::Syntax {
                    line,
                    message: "multiple O program numbers on one block".to_string(),
                });
            }
            program_number = Some(prog);
            continue;
        }

        if letter == 'N' {
            let _seq = parse_u32_token(line, letter, num_str)?;
            continue;
        }

        let code = if letter == 'G' || letter == 'M' {
            Some(parse_code_num(line, letter, num_str)?)
        } else {
            None
        };

        words.push(Word {
            letter,
            value,
            code,
            raw: format!("{letter}{num_str}"),
        });
    }

    if program_number.is_some() && !words.is_empty() {
        return Err(GcodeError::Syntax {
            line,
            message: "program O-number cannot share a block with motion or modal words".to_string(),
        });
    }

    Ok(ParsedLine {
        line,
        is_percent,
        program_number,
        words,
    })
}

fn skip_paren_comment(line: usize, raw: &str, start: usize) -> Result<usize, GcodeError> {
    let bytes = raw.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            return Err(GcodeError::Syntax {
                line,
                message: "nested '(' inside comment".to_string(),
            });
        }
        if bytes[i] == b')' {
            return Ok(i + 1);
        }
        i += 1;
    }
    Err(GcodeError::Syntax {
        line,
        message: "unclosed '(' comment".to_string(),
    })
}

fn scan_alpha(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        i += 1;
    }
    i
}

fn scan_number(line: usize, letter: char, raw: &str, start: usize) -> Result<(&str, usize), GcodeError> {
    let bytes = raw.as_bytes();
    let len = bytes.len();
    let mut i = start;
    if i < len && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    let mut digits = 0usize;
    while i < len && bytes[i].is_ascii_digit() {
        digits += 1;
        i += 1;
    }
    if i < len && bytes[i] == b'.' {
        i += 1;
        while i < len && bytes[i].is_ascii_digit() {
            digits += 1;
            i += 1;
        }
    }
    if digits == 0 {
        return Err(GcodeError::Syntax {
            line,
            message: format!("missing numeric value after word '{letter}'"),
        });
    }
    if i < len && bytes[i] == b'.' {
        return Err(GcodeError::Syntax {
            line,
            message: format!("malformed number after word '{letter}'"),
        });
    }
    Ok((&raw[start..i], i))
}

fn parse_u32_token(line: usize, letter: char, num_str: &str) -> Result<u32, GcodeError> {
    if num_str.starts_with('-') || num_str.contains('.') {
        return Err(GcodeError::Syntax {
            line,
            message: format!("{letter} word requires a non-negative integer, got '{num_str}'"),
        });
    }
    let trimmed = num_str.strip_prefix('+').unwrap_or(num_str);
    trimmed.parse::<u32>().map_err(|_| GcodeError::Syntax {
        line,
        message: format!("{letter} number '{num_str}' out of range"),
    })
}

fn parse_code_num(line: usize, letter: char, num_str: &str) -> Result<CodeNum, GcodeError> {
    if num_str.starts_with('-') || num_str.starts_with('+') {
        return Err(GcodeError::Syntax {
            line,
            message: format!("signed {letter} code '{letter}{num_str}' is invalid"),
        });
    }
    let (lhs, rhs_opt) = match num_str.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (num_str, None),
    };
    if lhs.is_empty() {
        return Err(GcodeError::Syntax {
            line,
            message: format!("invalid code '{letter}{num_str}'"),
        });
    }
    let major = lhs.parse::<u32>().map_err(|_| GcodeError::Syntax {
        line,
        message: format!("invalid code '{letter}{num_str}'"),
    })?;
    let minor = match rhs_opt {
        None => None,
        Some(rhs) => {
            let trimmed = rhs.trim_end_matches('0');
            if trimmed.is_empty() {
                None
            } else {
                if letter == 'M' {
                    return Err(GcodeError::Syntax {
                        line,
                        message: format!("M-code '{letter}{num_str}' cannot be fractional"),
                    });
                }
                // The minor number keeps its decimal place value (#3657 review 8): only
                // trailing zeros are dropped, so G90.10 is G90.1, but G90.01 is not. Every
                // supported code has one decimal digit; a longer code is refused by name.
                if trimmed.len() > 1 {
                    return Err(GcodeError::UnsupportedCode {
                        line,
                        code: format!("{letter}{num_str}"),
                        reason: "codes with more than one significant decimal place are not supported"
                            .to_string(),
                    });
                }
                let m = trimmed.parse::<u32>().map_err(|_| GcodeError::Syntax {
                    line,
                    message: format!("invalid code '{letter}{num_str}'"),
                })?;
                Some(m)
            }
        }
    };
    let text = match minor {
        Some(m) => format!("{major}.{m}"),
        None => format!("{major}"),
    };
    Ok(CodeNum { major, minor, text })
}

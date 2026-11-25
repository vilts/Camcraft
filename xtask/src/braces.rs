//! Brace counting that ignores braces inside string literals, raw strings, char literals and `//`
//! comments, so the scanner's test spans do not swallow the next test (#764).
//! Provenance: the Rust Reference's lexical structure chapter (string, raw string and char literals).

/// Lexer state carried from one line to the next (strings may span lines).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lex {
    #[default]
    Code,
    /// Inside `"..."`.
    Str,
    /// Inside `r#"..."#` with this many `#`.
    Raw(usize),
    /// Inside a nested block comment.
    Block(usize),
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Net `{` minus `}` of the code on `line` (opens, closes), updating `st` for the next line.
pub fn count(line: &str, st: &mut Lex) -> (i64, i64) {
    let code = code(line, st);
    let open = code.chars().filter(|&c| c == '{').count();
    let close = code.chars().filter(|&c| c == '}').count();
    (open as i64, close as i64)
}

/// The code of `line`: each char inside a string, raw string, char literal or comment becomes a space,
/// and `st` carries the lexer state to the next line (#3706: `mod` tokens are counted in code only).
pub fn code(line: &str, st: &mut Lex) -> String {
    let c: Vec<char> = line.chars().collect();
    let mut keep = vec![false; c.len()];
    let mut i = 0usize;
    while i < c.len() {
        match *st {
            Lex::Str => match c[i] {
                '\\' => i += 1,
                '"' => *st = Lex::Code,
                _ => {}
            },
            Lex::Raw(h) => {
                if c[i] == '"'
                    && c.get(i + 1..i + 1 + h)
                        .is_some_and(|t| t.iter().all(|&x| x == '#'))
                {
                    *st = Lex::Code;
                    i += h;
                }
            }
            Lex::Block(depth) => {
                if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                    *st = Lex::Block(depth + 1);
                    i += 1;
                } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                    *st = if depth == 1 {
                        Lex::Code
                    } else {
                        Lex::Block(depth - 1)
                    };
                    i += 1;
                }
            }
            Lex::Code => match c[i] {
                '"' => *st = Lex::Str,
                '/' if c.get(i + 1) == Some(&'/') => break,
                '/' if c.get(i + 1) == Some(&'*') => {
                    *st = Lex::Block(1);
                    i += 1;
                }
                // `r".."`, `br".."` and `cr".."` (C string, #3711) open a raw string.
                'r' if (i == 0 || !is_ident(c[i - 1]) || c[i - 1] == 'b' || c[i - 1] == 'c')
                    && c.get(i + 1 + c[i + 1..].iter().take_while(|&&x| x == '#').count())
                        == Some(&'"') =>
                {
                    let h = c[i + 1..].iter().take_while(|&&x| x == '#').count();
                    *st = Lex::Raw(h);
                    i += 1 + h;
                }
                '\'' => {
                    if c.get(i + 1) == Some(&'\\') {
                        // Escaped char literal: skip the escaped char, then to the closing quote
                        // (`'\''` and `'\\'` end at their last quote, #3711).
                        i += 3;
                        while i < c.len() && c[i] != '\'' {
                            i += 1;
                        }
                    } else if c.get(i + 2) == Some(&'\'') {
                        i += 2; // 'x' (a lifetime like 'a has no closing quote there)
                    } else {
                        keep[i] = true;
                    }
                }
                _ => keep[i] = true,
            },
        }
        i += 1;
    }
    c.iter().zip(keep).map(|(&ch, k)| if k { ch } else { ' ' }).collect()
}

#[cfg(test)]
#[path = "braces_tests.rs"]
mod tests;

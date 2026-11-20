//! A small JSON reader for what cargo prints (`--message-format=json` lines, `cargo metadata`), so the
//! gate needs no new crate for it (#1951). A bad or too deeply nested text gives `None`, never a panic.
//! Provenance: the JSON grammar of RFC 8259; our own code.

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// The member `key` of an object; `None` for a missing key or another kind of value.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    /// The string member `key`; `None` when it is missing, `null` or not a string.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Json::str)
    }
    /// The items of an array; empty for another kind of value.
    pub fn items(&self) -> &[Json] {
        match self {
            Json::Arr(v) => v,
            _ => &[],
        }
    }
    pub fn is_true(&self) -> bool {
        matches!(self, Json::Bool(true))
    }
}

/// Nesting deeper than this is refused: the texts we read are a few levels deep.
const MAX_DEPTH: usize = 64;

/// The one JSON value of `text` (white space around it is permitted); `None` when it is not valid JSON.
pub fn parse(text: &str) -> Option<Json> {
    let mut p = Reader { b: text.as_bytes(), i: 0 };
    let v = p.value(0)?;
    p.space();
    (p.i == p.b.len()).then_some(v)
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn space(&mut self) {
        while matches!(self.b.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, word: &str) -> Option<()> {
        let end = self.i.checked_add(word.len())?;
        (self.b.get(self.i..end)? == word.as_bytes()).then(|| self.i = end)
    }

    fn value(&mut self, depth: usize) -> Option<Json> {
        if depth > MAX_DEPTH {
            return None;
        }
        self.space();
        match *self.b.get(self.i)? {
            b'n' => self.eat("null").map(|()| Json::Null),
            b't' => self.eat("true").map(|()| Json::Bool(true)),
            b'f' => self.eat("false").map(|()| Json::Bool(false)),
            b'"' => self.string().map(Json::Str),
            b'[' => {
                self.i += 1;
                let mut items = Vec::new();
                self.space();
                if self.eat("]").is_some() {
                    return Some(Json::Arr(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.space();
                    if self.eat(",").is_none() {
                        return self.eat("]").map(|()| Json::Arr(items));
                    }
                }
            }
            b'{' => {
                self.i += 1;
                let mut members = Vec::new();
                self.space();
                if self.eat("}").is_some() {
                    return Some(Json::Obj(members));
                }
                loop {
                    self.space();
                    let key = self.string()?;
                    self.space();
                    self.eat(":")?;
                    members.push((key, self.value(depth + 1)?));
                    self.space();
                    if self.eat(",").is_none() {
                        return self.eat("}").map(|()| Json::Obj(members));
                    }
                }
            }
            _ => self.number(),
        }
    }

    fn number(&mut self) -> Option<Json> {
        let start = self.i;
        while matches!(self.b.get(self.i), Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')) {
            self.i += 1;
        }
        let text = std::str::from_utf8(self.b.get(start..self.i)?).ok()?;
        text.parse::<f64>().ok().filter(|n| n.is_finite()).map(Json::Num)
    }

    fn hex4(&mut self) -> Option<u32> {
        let end = self.i.checked_add(4)?;
        let text = std::str::from_utf8(self.b.get(self.i..end)?).ok()?;
        let n = u32::from_str_radix(text, 16).ok()?;
        self.i = end;
        Some(n)
    }

    fn string(&mut self) -> Option<String> {
        self.eat("\"")?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = *self.b.get(self.i)?;
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let e = *self.b.get(self.i)?;
                    self.i += 1;
                    let ch = match e {
                        b'"' | b'\\' | b'/' => char::from(e),
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            // A high surrogate takes the low surrogate after it; a lone one becomes U+FFFD.
                            let code = if (0xD800..0xDC00).contains(&hi) && self.eat("\\u").is_some() {
                                let lo = self.hex4()?;
                                0x10000 + ((hi - 0xD800) << 10) + lo.checked_sub(0xDC00)?
                            } else {
                                hi
                            };
                            char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER)
                        }
                        _ => return None,
                    };
                    out.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
                }
                0x00..=0x1F => return None,
                _ => out.push(c),
            }
        }
    }
}

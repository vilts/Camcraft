//! The expression half of the test-only macro B reader (#392): a
//! cursor over one block with its spaces removed, and the arithmetic -
//! `#n` / `#[expr]` variables, `[ ]` brackets, `ABS[ ]`, `* /` over
//! `+ -`, unary minus (so both `#100--15` and `#100+15` read), and the
//! comparison operators `EQ NE GT LT GE LE` an `IF[ ]` tests.
//!
//! Provenance: Fanuc custom macro B as its published operator
//! documentation describes it (variables, brackets, functions,
//! conditional expressions). Our own code.

/// One block, upper-cased with its spaces dropped.
pub struct Cursor {
    b: Vec<u8>,
    i: usize,
}

/// What an expression reads a variable through: the controller's
/// variables, skip position and work-offset registers.
pub trait Vars {
    fn read(&self, n: u32) -> Result<f64, String>;
}

impl Cursor {
    pub fn new(text: &str) -> Self {
        let b = text.bytes().filter(|c| !c.is_ascii_whitespace()).map(|c| c.to_ascii_uppercase()).collect();
        Self { b, i: 0 }
    }

    pub fn done(&self) -> bool {
        self.i >= self.b.len()
    }

    pub fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    pub fn rest(&self) -> String {
        String::from_utf8_lossy(self.b.get(self.i..).unwrap_or(&[])).into_owned()
    }

    /// Consume `k` when the block continues with it.
    pub fn kw(&mut self, k: &str) -> bool {
        let hit = self.b.get(self.i..).is_some_and(|r| r.starts_with(k.as_bytes()));
        if hit {
            self.i += k.len();
        }
        hit
    }

    pub fn want(&mut self, k: &str) -> Result<(), String> {
        if self.kw(k) {
            Ok(())
        } else {
            Err(format!("expected {k} at {:?}", self.rest()))
        }
    }

    /// An unsigned decimal: digits with at most one point.
    pub fn num(&mut self) -> Result<f64, String> {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == b'.') {
            self.i += 1;
        }
        let s = String::from_utf8_lossy(self.b.get(start..self.i).unwrap_or(&[])).into_owned();
        s.parse::<f64>().map_err(|_| format!("expected a number at {:?}", self.rest()))
    }

    /// A letter address (`G`, `X`, ...), consumed.
    pub fn letter(&mut self) -> Option<char> {
        let c = self.peek().filter(u8::is_ascii_uppercase)?;
        self.i += 1;
        Some(c as char)
    }

    /// A variable number after `#`: a literal or a bracketed expression.
    pub fn var_no(&mut self, v: &dyn Vars) -> Result<u32, String> {
        let n = if self.peek() == Some(b'[') { self.primary(v)? } else { self.num()? };
        if n < 0.0 || n.fract() != 0.0 || n > 99_999.0 {
            return Err(format!("#{n} is not a variable number"));
        }
        Ok(n as u32)
    }

    pub fn expr(&mut self, v: &dyn Vars) -> Result<f64, String> {
        let mut a = self.term(v)?;
        loop {
            if self.kw("+") {
                a += self.term(v)?;
            } else if self.kw("-") {
                a -= self.term(v)?;
            } else {
                return Ok(a);
            }
        }
    }

    fn term(&mut self, v: &dyn Vars) -> Result<f64, String> {
        let mut a = self.unary(v)?;
        loop {
            if self.kw("*") {
                a *= self.unary(v)?;
            } else if self.kw("/") {
                let d = self.unary(v)?;
                if d == 0.0 {
                    return Err("division by zero".into());
                }
                a /= d;
            } else {
                return Ok(a);
            }
        }
    }

    /// A signed primary - also a word's value (`X-1`, `X[#5221+#190]`).
    pub fn unary(&mut self, v: &dyn Vars) -> Result<f64, String> {
        if self.kw("-") {
            return Ok(-self.unary(v)?);
        }
        if self.kw("+") {
            return self.unary(v);
        }
        self.primary(v)
    }

    fn primary(&mut self, v: &dyn Vars) -> Result<f64, String> {
        if self.kw("[") {
            let a = self.expr(v)?;
            self.want("]")?;
            return Ok(a);
        }
        if self.kw("ABS[") {
            let a = self.expr(v)?;
            self.want("]")?;
            return Ok(a.abs());
        }
        if self.kw("#") {
            let n = self.var_no(v)?;
            return v.read(n);
        }
        self.num()
    }

    /// `a OP b` inside an `IF[ ]`.
    pub fn cond(&mut self, v: &dyn Vars) -> Result<bool, String> {
        let a = self.expr(v)?;
        for (op, f) in [
            ("EQ", (|a, b| a == b) as fn(f64, f64) -> bool),
            ("NE", |a, b| a != b),
            ("GT", |a, b| a > b),
            ("LT", |a, b| a < b),
            ("GE", |a, b| a >= b),
            ("LE", |a, b| a <= b),
        ] {
            if self.kw(op) {
                let b = self.expr(v)?;
                return Ok(f(a, b));
            }
        }
        Err(format!("expected a comparison at {:?}", self.rest()))
    }
}

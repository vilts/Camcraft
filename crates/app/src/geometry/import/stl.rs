//! STL reader: binary and ASCII, sniffed per public-format rules.
//!
//! Binary layout (public): 80-byte header, u32 LE facet count, then
//! count x [12 x f32 LE (normal + 3 vertices) + u16 attribute].
//! ASCII layout: `solid`/`facet normal`/`outer loop`/`vertex`/`endloop`/
//! `endfacet`/`endsolid` token lines.
//!
//! Both flavors stream from a reader (M5.3 E): the file never coexists
//! with its triangles in memory, so a 10M-facet model costs its facet
//! buffer and one 8 MiB window - not twice the disk size. The byte-slice
//! `parse` stays for buffers already in hand (fuzz, tests); it and the
//! streaming paths share the per-facet acceptance law.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek};
use std::path::Path;

use super::{ImportError, MAX_FACETS};
use crate::geometry::{Mesh, Point3, Triangle};

const HEADER_BYTES: usize = 80;
const COUNT_BYTES: usize = 4;
const FACET_BYTES: usize = 50;
/// The one window the streaming paths read through: 50-byte records and
/// physical lines are served from it by slice copy, so the disk sees
/// window-sized reads and the file never doubles in memory.
pub(super) const READ_BUF_BYTES: usize = 8 << 20;

pub fn parse(data: &[u8]) -> Result<Mesh, ImportError> {
    if data.is_empty() {
        return Err(ImportError::Empty);
    }
    if looks_binary(data) {
        parse_binary(data)
    } else {
        parse_ascii(data)
    }
}

/// The per-facet acceptance law, shared by every read path: finite
/// vertices (the file's own normal field is ignored - orientation is
/// recomputed) and a non-zero right-hand normal, or the facet is
/// degenerate and the file is refused. `ctx` names the site in the
/// message ("facet 12" / "line 345"); it is a closure because a
/// 10M-facet stream must not allocate a facet tag it never reports.
fn accepted(a: Point3, b: Point3, c: Point3, ctx: impl Fn() -> String) -> Result<Triangle, ImportError> {
    if !(a.is_finite() && b.is_finite() && c.is_finite()) {
        return Err(ImportError::Stl(format!("{}: non-finite vertex", ctx())));
    }
    let t = Triangle::new(a, b, c);
    let n = t.normal();
    if n.x == 0.0 && n.y == 0.0 && n.z == 0.0 {
        return Err(ImportError::Stl(format!("{} is degenerate", ctx())));
    }
    Ok(t)
}

/// A leading `solid` is ambiguous (binary headers can carry it too): binary
/// is chosen only when the declared count matches the byte size exactly.
/// Anything without the `solid` prefix is binary - the parser then reports
/// precise size errors rather than the ASCII parser reporting garbage.
fn looks_binary(data: &[u8]) -> bool {
    if !data.starts_with(b"solid") {
        return true;
    }
    match read_count(data) {
        Some(count) => match (count as usize).checked_mul(FACET_BYTES) {
            Some(body) => data.len() == HEADER_BYTES + COUNT_BYTES + body,
            None => false,
        },
        None => false,
    }
}

fn read_count(data: &[u8]) -> Option<u32> {
    if data.len() < HEADER_BYTES + COUNT_BYTES {
        return None;
    }
    let b = &data[HEADER_BYTES..HEADER_BYTES + COUNT_BYTES];
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn f32_at(data: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

fn parse_binary(data: &[u8]) -> Result<Mesh, ImportError> {
    let count = match read_count(data) {
        Some(c) => c,
        None => return Err(ImportError::Stl("truncated header".into())),
    };
    if count as usize > MAX_FACETS {
        return Err(ImportError::TooLarge);
    }
    let count = count as usize;
    let body = match count.checked_mul(FACET_BYTES) {
        Some(b) => b,
        None => return Err(ImportError::TooLarge),
    };
    let expect = HEADER_BYTES + COUNT_BYTES + body;
    if data.len() != expect {
        return Err(ImportError::Stl(format!(
            "size {} does not match declared {count} facets (expected {expect} bytes)",
            data.len()
        )));
    }
    if count == 0 {
        return Err(ImportError::Empty);
    }

    let mut facets = Vec::with_capacity(count);
    for i in 0..count {
        let base = HEADER_BYTES + COUNT_BYTES + i * FACET_BYTES;
        let mut p = [0.0f32; 12];
        for (slot, chunk) in p.iter_mut().zip(0..12) {
            *slot = f32_at(data, base + chunk * 4);
        }
        // normal may be zero (writers often skip it) but must not be NaN
        if !p[..3].iter().all(|v| v.is_finite()) {
            return Err(ImportError::Stl(format!("facet {i}: non-finite normal")));
        }
        facets.push(accepted(
            Point3::new(p[3] as f64, p[4] as f64, p[5] as f64),
            Point3::new(p[6] as f64, p[7] as f64, p[8] as f64),
            Point3::new(p[9] as f64, p[10] as f64, p[11] as f64),
            || format!("facet {i}"),
        )?);
    }
    Ok(Mesh::from_facets(facets))
}

/// Stream a whole STL file from disk: the file never coexists with its
/// triangles - peak is the facet buffer plus one window, not twice the
/// disk size (M5.3 E). The sniff is the same size law `looks_binary`
/// applies to a buffer, and both flavors share `accepted`'s per-facet
/// law, so `parse_file` and `parse` accept and refuse the same files
/// with the same verdicts (the equivalence the M5.3 E test pins).
pub fn parse_file(path: &Path) -> Result<Mesh, ImportError> {
    let file = File::open(path)?;
    let size = file.metadata().ok().map(|m| m.len());
    parse_stream(BufReader::with_capacity(READ_BUF_BYTES, file), size)
}

pub(super) fn parse_stream<R: Read + Seek>(
    mut src: BufReader<R>,
    size: Option<u64>,
) -> Result<Mesh, ImportError> {
    // The head is all the sniff needs: 80 header bytes + the u32 count.
    let mut head = [0u8; HEADER_BYTES + COUNT_BYTES];
    let mut n = 0usize;
    while n < head.len() {
        match src.read(&mut head[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) => return Err(ImportError::Io(e)),
        }
    }
    if n == 0 {
        return Err(ImportError::Empty);
    }
    let full = n == head.len();
    let count = full.then(|| read_count(&head)).flatten();
    let binary = count
        .and_then(|c| (c as usize).checked_mul(FACET_BYTES))
        .is_some_and(|body| size == Some((HEADER_BYTES + COUNT_BYTES + body) as u64));
    if head[..HEADER_BYTES].starts_with(b"solid") && !binary {
        // `solid` without a binary-exact byte size: ASCII, re-read from
        // the start through the same window (the head is 84 bytes, the
        // rewind is free - the buffer already holds it).
        src.rewind()?;
        return parse_ascii_stream(&mut src);
    }
    if !full {
        return Err(ImportError::Stl("truncated header".into()));
    }
    let count = match count {
        Some(c) => c as usize,
        None => return Err(ImportError::Stl("truncated header".into())),
    };
    if count > MAX_FACETS {
        return Err(ImportError::TooLarge);
    }
    let body = match count.checked_mul(FACET_BYTES) {
        Some(b) => b,
        None => return Err(ImportError::TooLarge),
    };
    let expect = HEADER_BYTES + COUNT_BYTES + body;
    if size.is_some_and(|s| s != expect as u64) {
        return Err(ImportError::Stl(format!(
            "size {} does not match declared {count} facets (expected {expect} bytes)",
            size.unwrap_or(0)
        )));
    }
    if count == 0 {
        return Err(ImportError::Empty);
    }

    // Records arrive from the window; a 50-byte read_exact over a full
    // BufReader is a slice copy, so the disk sees window-sized reads.
    let mut facets: Vec<Triangle> = Vec::with_capacity(count);
    let mut rec = [0u8; FACET_BYTES];
    for i in 0..count {
        if let Err(e) = src.read_exact(&mut rec) {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                return Err(ImportError::Stl(format!("file ended inside facet {i}")));
            }
            return Err(ImportError::Io(e));
        }
        let f = |k: usize| {
            f32::from_le_bytes([
                rec[k * 4],
                rec[k * 4 + 1],
                rec[k * 4 + 2],
                rec[k * 4 + 3],
            ])
        };
        // normal may be zero (writers often skip it) but must not be NaN
        if !(f(0).is_finite() && f(1).is_finite() && f(2).is_finite()) {
            return Err(ImportError::Stl(format!("facet {i}: non-finite normal")));
        }
        facets.push(accepted(
            Point3::new(f(3) as f64, f(4) as f64, f(5) as f64),
            Point3::new(f(6) as f64, f(7) as f64, f(8) as f64),
            Point3::new(f(9) as f64, f(10) as f64, f(11) as f64),
            || format!("facet {i}"),
        )?);
    }
    Ok(Mesh::from_facets(facets))
}

/// ASCII streamed line by line. `endsolid` stops the PARSE but the read
/// continues, because the buffer path validates the file's whole UTF-8
/// before it breaks - skipping the tail here would accept files the
/// buffer refuses, and the two paths must agree verdict-for-verdict.
fn parse_ascii_stream<R: Read>(src: &mut BufReader<R>) -> Result<Mesh, ImportError> {
    let mut fsm = AsciiFsm::default();
    let mut buf = String::new();
    let mut line = 0usize;
    let mut done = false;
    loop {
        buf.clear();
        match src.read_line(&mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                return Err(ImportError::Stl("not valid UTF-8".into()));
            }
            Err(e) => return Err(ImportError::Io(e)),
        }
        line += 1;
        if !done {
            done = fsm.line(&buf, line)?;
        }
    }
    fsm.finish()
}

fn parse_ascii(data: &[u8]) -> Result<Mesh, ImportError> {
    let text = std::str::from_utf8(data)
        .map_err(|_| ImportError::Stl("not valid UTF-8".into()))?;
    let mut fsm = AsciiFsm::default();
    for (ln, raw) in text.lines().enumerate() {
        if fsm.line(raw, ln + 1)? {
            break;
        }
    }
    fsm.finish()
}

/// The ASCII grammar as a one-line-at-a-time machine, shared by the
/// buffer and streaming paths (M5.3 E): both flavors and both read
/// styles run one law, so `parse` and `parse_file` cannot drift.
#[derive(Default)]
struct AsciiFsm {
    state: State,
    verts: [Option<Point3>; 3],
    vi: usize, // vertices collected in the current facet
    facets: Vec<Triangle>,
    declared: usize,
}

impl AsciiFsm {
    /// Feed one physical line; `true` means `endsolid` closed the solid.
    fn line(&mut self, raw: &str, line: usize) -> Result<bool, ImportError> {
        let mut tok = raw.split_whitespace();
        let word = match tok.next() {
            Some(w) => w,
            None => return Ok(false),
        };
        match word {
            "solid" => {
                if !matches!(self.state, State::Start) {
                    return Err(ImportError::Stl(format!("line {line}: nested solid")));
                }
                self.state = State::InSolid;
            }
            "facet" | "outer" | "endloop" | "endfacet" | "vertex"
                if matches!(self.state, State::Start) =>
            {
                return Err(ImportError::Stl(format!(
                    "line {line}: '{word}' before 'solid'"
                )));
            }
            "facet" => {
                expect_next(&mut tok, "normal", line)?;
                read_f32_triple(&mut tok, line, "normal")?;
                self.state = State::InFacet;
                self.verts = [None, None, None];
                self.vi = 0;
            }
            "outer" => {
                expect_next(&mut tok, "loop", line)?;
            }
            "vertex" if matches!(self.state, State::InFacet) => {
                let p = read_f64_triple(&mut tok, line, "vertex")?;
                if self.vi < 3 {
                    self.verts[self.vi] = Some(p);
                    self.vi += 1;
                } else {
                    return Err(ImportError::Stl(format!("line {line}: >3 vertices")));
                }
            }
            "endloop" => {}
            "endfacet" => {
                if !matches!(self.state, State::InFacet) {
                    return Err(ImportError::Stl(format!("line {line}: stray endfacet")));
                }
                let (Some(a), Some(b), Some(c)) = (self.verts[0], self.verts[1], self.verts[2])
                else {
                    return Err(ImportError::Stl(format!(
                        "line {line}: facet has {} vertices",
                        self.vi
                    )));
                };
                self.facets
                    .push(accepted(a, b, c, || format!("line {line}: facet"))?);
                self.declared += 1;
                if self.declared > MAX_FACETS {
                    return Err(ImportError::TooLarge);
                }
                self.state = State::InSolid;
            }
            "endsolid" => return Ok(true),
            other => {
                return Err(ImportError::Stl(format!(
                    "line {line}: unknown token '{other}'"
                )));
            }
        }
        Ok(false)
    }

    /// End of input. A missing `endsolid` on a complete solid is
    /// accepted (common writer quirk); zero facets is not.
    fn finish(self) -> Result<Mesh, ImportError> {
        if self.declared == 0 {
            return Err(ImportError::Empty);
        }
        Ok(Mesh::from_facets(self.facets))
    }
}

#[derive(Default)]
enum State {
    #[default]
    Start,
    InSolid,
    InFacet,
}

fn expect_next<'a>(
    tok: &mut impl Iterator<Item = &'a str>,
    want: &str,
    line: usize,
) -> Result<(), ImportError> {
    match tok.next() {
        Some(w) if w == want => Ok(()),
        _ => Err(ImportError::Stl(format!("line {line}: expected '{want}'"))),
    }
}

fn read_f32_triple<'a>(
    tok: &mut impl Iterator<Item = &'a str>,
    line: usize,
    what: &str,
) -> Result<[f32; 3], ImportError> {
    let vals = read_float_triple::<f32>(tok, line, what)?;
    if !vals.iter().all(|v| v.is_finite()) {
        return Err(ImportError::Stl(format!(
            "line {line}: non-finite {what} value"
        )));
    }
    Ok(vals)
}

fn read_f64_triple<'a>(
    tok: &mut impl Iterator<Item = &'a str>,
    line: usize,
    what: &str,
) -> Result<Point3, ImportError> {
    let t = read_float_triple::<f64>(tok, line, what)?;
    let p = Point3::new(t[0], t[1], t[2]);
    if !p.is_finite() {
        return Err(ImportError::Stl(format!("line {line}: non-finite {what}")));
    }
    Ok(p)
}

/// Reads exactly 3 whitespace-separated floats from the caller's token
/// iterator.
fn read_float_triple<'a, T>(
    tok: &mut impl Iterator<Item = &'a str>,
    line: usize,
    what: &str,
) -> Result<[T; 3], ImportError>
where
    T: std::str::FromStr,
{
    Ok([
        parse_one(tok, line, what)?,
        parse_one(tok, line, what)?,
        parse_one(tok, line, what)?,
    ])
}

fn parse_one<'a, T>(
    tok: &mut impl Iterator<Item = &'a str>,
    line: usize,
    what: &str,
) -> Result<T, ImportError>
where
    T: std::str::FromStr,
{
    let word = tok
        .next()
        .ok_or_else(|| ImportError::Stl(format!("line {line}: expected a {what} value")))?;
    word.parse::<T>()
        .map_err(|_| ImportError::Stl(format!("line {line}: bad {what} value '{word}'")))
}

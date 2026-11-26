//! A read-only TrueType outline reader: the table directory, `head`, `maxp`, `hhea`, `hmtx`,
//! `OS/2`, `cmap` (formats 4 and 12), `loca` and `glyf` (simple and composite glyphs).
//!
//! Provenance: written from the public OpenType specification (Microsoft Typography) and the
//! Apple TrueType Reference Manual - table layouts only. No font library code was consulted.
//!
//! Contract. Each read is bounds-checked: a short or corrupt table is a [`FontError`], never
//! a panic and never an out-of-range index. A composite glyph is resolved to depth
//! [`MAX_DEPTH`] with at most [`MAX_PARTS`] component visits, and the points of one glyph are
//! capped at [`MAX_POINTS`]. Each coordinate stays in the 16-bit range of the format.

use super::FontError;

/// The deepest chain of composite glyphs that is resolved.
pub const MAX_DEPTH: usize = 8;
/// The most points that one resolved glyph can have.
pub const MAX_POINTS: usize = 100_000;
/// The most component visits that one resolved glyph can make. An empty part adds no point, so
/// the point cap alone does not bound the work of a composite tree.
pub const MAX_PARTS: usize = 4096;

/// The work left for one glyph: component visits and points.
struct Budget {
    parts: usize,
    points: usize,
}

/// One outline point in font units, and whether it lies on the curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutlinePoint {
    pub x: f64,
    pub y: f64,
    pub on: bool,
}

/// The contours of one glyph, in font units.
pub type Contours = Vec<Vec<OutlinePoint>>;

pub struct Font<'a> {
    glyf: &'a [u8],
    loca: &'a [u8],
    hmtx: &'a [u8],
    cmap: Cmap<'a>,
    long_loca: bool,
    num_glyphs: u16,
    num_h_metrics: u16,
    pub units_per_em: u16,
    os2_cap_height: Option<i16>,
}

mod cmap;
use cmap::Cmap;

fn u16_at(d: &[u8], at: usize) -> Result<u16, FontError> {
    let b = d.get(at..at.checked_add(2).ok_or(FontError::Truncated)?).ok_or(FontError::Truncated)?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}

fn i16_at(d: &[u8], at: usize) -> Result<i16, FontError> {
    u16_at(d, at).map(|v| v as i16)
}

fn u32_at(d: &[u8], at: usize) -> Result<u32, FontError> {
    let b = d.get(at..at.checked_add(4).ok_or(FontError::Truncated)?).ok_or(FontError::Truncated)?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn u8_at(d: &[u8], at: usize) -> Result<u8, FontError> {
    d.get(at).copied().ok_or(FontError::Truncated)
}

fn f2dot14(d: &[u8], at: usize) -> Result<f64, FontError> {
    Ok(f64::from(i16_at(d, at)?) / 16384.0)
}

fn to_usize(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

/// Each table as a slice bounded by its declared length and the file length.
fn table<'a>(data: &'a [u8], tag: &[u8; 4]) -> Result<Option<&'a [u8]>, FontError> {
    let count = usize::from(u16_at(data, 4)?);
    for i in 0..count {
        let rec = 12 + 16 * i;
        let name = data.get(rec..rec + 4).ok_or(FontError::Truncated)?;
        if name == tag {
            let offset = to_usize(u32_at(data, rec + 8)?);
            let length = to_usize(u32_at(data, rec + 12)?);
            let end = offset.checked_add(length).ok_or(FontError::Truncated)?;
            if end > data.len() {
                return Err(FontError::Truncated);
            }
            return Ok(Some(&data[offset..end]));
        }
    }
    Ok(None)
}

fn required<'a>(data: &'a [u8], tag: &'static str) -> Result<&'a [u8], FontError> {
    let mut t = [0_u8; 4];
    t.copy_from_slice(tag.as_bytes());
    table(data, &t)?.ok_or(FontError::MissingTable(tag))
}

impl<'a> Font<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Font<'a>, FontError> {
        match data.get(0..4) {
            Some([0, 1, 0, 0]) | Some(b"true") => {}
            Some(b"OTTO") => return Err(FontError::CffOutlines),
            Some(b"ttcf") => return Err(FontError::Collection),
            _ => return Err(FontError::NotAFont),
        }
        let head = required(data, "head")?;
        let units_per_em = u16_at(head, 18)?;
        if !(16..=16384).contains(&units_per_em) {
            return Err(FontError::Corrupt(
                "the units per em are outside 16 to 16384",
            ));
        }
        let long_loca = match i16_at(head, 50)? {
            0 => false,
            1 => true,
            _ => return Err(FontError::Corrupt("the loca format is not 0 or 1")),
        };
        let num_glyphs = u16_at(required(data, "maxp")?, 4)?;
        let num_h_metrics = u16_at(required(data, "hhea")?, 34)?;
        if num_h_metrics == 0 {
            return Err(FontError::Corrupt("the font has no horizontal metrics"));
        }
        let glyf = table(data, b"glyf")?.ok_or(if table(data, b"CFF ")?.is_some() {
            FontError::CffOutlines
        } else {
            FontError::MissingTable("glyf")
        })?;
        let os2_cap_height = match table(data, b"OS/2")? {
            Some(os2) if u16_at(os2, 0)? >= 2 => Some(i16_at(os2, 88)?),
            _ => None,
        };
        Ok(Font {
            glyf,
            loca: required(data, "loca")?,
            hmtx: required(data, "hmtx")?,
            cmap: Cmap::parse(required(data, "cmap")?)?,
            long_loca,
            num_glyphs,
            num_h_metrics,
            units_per_em,
            os2_cap_height,
        })
    }

    /// The glyph of `ch`, or `None` when the font maps it to no glyph.
    pub fn glyph_id(&self, ch: char) -> Result<Option<u16>, FontError> {
        let gid = self.cmap.glyph_id(ch)?;
        Ok(u16::try_from(gid)
            .ok()
            .filter(|&g| g != 0 && g < self.num_glyphs))
    }

    /// The advance width of `gid`, in font units.
    pub fn advance(&self, gid: u16) -> Result<u16, FontError> {
        let index = usize::from(gid.min(self.num_h_metrics - 1));
        if 4 * (index + 1) > self.hmtx.len() {
            return Err(FontError::Truncated);
        }
        u16_at(self.hmtx, 4 * index)
    }

    /// The cap height in font units: the `OS/2` value, else the top of `H`, else 70 % of the
    /// em.
    pub fn cap_height(&self) -> Result<f64, FontError> {
        if let Some(h) = self.os2_cap_height.filter(|&h| h > 0) {
            return Ok(f64::from(h));
        }
        if let Some(gid) = self.glyph_id('H')? {
            let top = self.outline(gid)?.iter().flatten().map(|p| p.y).fold(0.0_f64, f64::max);
            if top > 0.0 {
                return Ok(top);
            }
        }
        Ok(0.7 * f64::from(self.units_per_em))
    }

    fn glyph_range(&self, gid: u16) -> Result<(usize, usize), FontError> {
        let g = usize::from(gid);
        let (a, b) = if self.long_loca {
            (u32_at(self.loca, 4 * g)?, u32_at(self.loca, 4 * g + 4)?)
        } else {
            let a = u16_at(self.loca, 2 * g)?;
            let b = u16_at(self.loca, 2 * g + 2)?;
            (2 * u32::from(a), 2 * u32::from(b))
        };
        let (a, b) = (to_usize(a), to_usize(b));
        if a > b || b > self.glyf.len() {
            return Err(FontError::Corrupt("a glyph lies outside the glyf table"));
        }
        Ok((a, b))
    }

    /// The contours of `gid` in font units, composites resolved. A glyph with no outline (the
    /// space) has no contours.
    pub fn outline(&self, gid: u16) -> Result<Contours, FontError> {
        let mut out = Vec::new();
        let mut budget = Budget { parts: MAX_PARTS, points: MAX_POINTS };
        self.append(gid, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], 0, &mut budget, &mut out)?;
        Ok(out)
    }

    fn append(
        &self,
        gid: u16,
        m: [f64; 6],
        depth: usize,
        budget: &mut Budget,
        out: &mut Contours,
    ) -> Result<(), FontError> {
        if depth > MAX_DEPTH {
            return Err(FontError::Corrupt("composite glyphs nest too deep"));
        }
        if gid >= self.num_glyphs {
            return Err(FontError::Corrupt(
                "a composite glyph names a glyph that does not exist",
            ));
        }
        let (start, end) = self.glyph_range(gid)?;
        if start == end {
            return Ok(());
        }
        let d = self.glyf.get(start..end).ok_or(FontError::Truncated)?;
        let contours = i16_at(d, 0)?;
        if contours >= 0 {
            for contour in simple(d, usize::from(contours.unsigned_abs()))? {
                budget.points = budget
                    .points
                    .checked_sub(contour.len())
                    .ok_or(FontError::Corrupt("a glyph has too many points"))?;
                out.push(contour.into_iter().map(|p| transform(m, p)).collect());
            }
        } else {
            self.composite(d, m, depth, budget, out)?;
        }
        Ok(())
    }

    fn composite(
        &self,
        d: &[u8],
        m: [f64; 6],
        depth: usize,
        budget: &mut Budget,
        out: &mut Contours,
    ) -> Result<(), FontError> {
        let mut at = 10;
        loop {
            budget.parts = budget
                .parts
                .checked_sub(1)
                .ok_or(FontError::Corrupt("a composite glyph has too many parts"))?;
            let flags = u16_at(d, at)?;
            let child = u16_at(d, at + 2)?;
            at += 4;
            if flags & 0x0002 == 0 {
                return Err(FontError::Corrupt(
                    "a composite glyph places a part by point matching",
                ));
            }
            let (dx, dy) = if flags & 0x0001 != 0 {
                at += 4;
                (f64::from(i16_at(d, at - 4)?), f64::from(i16_at(d, at - 2)?))
            } else {
                at += 2;
                (
                    f64::from(u8_at(d, at - 2)? as i8),
                    f64::from(u8_at(d, at - 1)? as i8),
                )
            };
            let (a, b, c, e) = if flags & 0x0008 != 0 {
                at += 2;
                let s = f2dot14(d, at - 2)?;
                (s, 0.0, 0.0, s)
            } else if flags & 0x0040 != 0 {
                at += 4;
                (f2dot14(d, at - 4)?, 0.0, 0.0, f2dot14(d, at - 2)?)
            } else if flags & 0x0080 != 0 {
                at += 8;
                (
                    f2dot14(d, at - 8)?,
                    f2dot14(d, at - 6)?,
                    f2dot14(d, at - 4)?,
                    f2dot14(d, at - 2)?,
                )
            } else {
                (1.0, 0.0, 0.0, 1.0)
            };
            if flags & 0x1800 == 0x1800 {
                return Err(FontError::Corrupt(
                    "a composite glyph has conflicting offset flags",
                ));
            }
            let (dx, dy) = if flags & 0x0800 != 0 {
                (a * dx + c * dy, b * dx + e * dy)
            } else {
                (dx, dy)
            };
            // The part's own matrix, then the parent's: x' = a x + c y + dx, y' = b x + e y + dy.
            let part = [a, b, c, e, dx, dy];
            let joined = [
                m[0] * part[0] + m[2] * part[1],
                m[1] * part[0] + m[3] * part[1],
                m[0] * part[2] + m[2] * part[3],
                m[1] * part[2] + m[3] * part[3],
                m[0] * part[4] + m[2] * part[5] + m[4],
                m[1] * part[4] + m[3] * part[5] + m[5],
            ];
            self.append(child, joined, depth + 1, budget, out)?;
            if flags & 0x0020 == 0 {
                return Ok(());
            }
        }
    }
}

fn transform(m: [f64; 6], p: OutlinePoint) -> OutlinePoint {
    OutlinePoint { x: m[0] * p.x + m[2] * p.y + m[4], y: m[1] * p.x + m[3] * p.y + m[5], on: p.on }
}

/// The contours of a simple glyph: end points, flags (with repeats), then the x and y deltas.
fn simple(d: &[u8], contours: usize) -> Result<Contours, FontError> {
    let mut ends = Vec::with_capacity(contours);
    for i in 0..contours {
        ends.push(usize::from(u16_at(d, 10 + 2 * i)?));
    }
    let Some(&last) = ends.last() else { return Ok(Vec::new()) };
    if ends.windows(2).any(|w| w[0] >= w[1]) {
        return Err(FontError::Corrupt("the contour end points of a glyph do not ascend"));
    }
    let count = last + 1;
    if count > MAX_POINTS {
        return Err(FontError::Corrupt("a glyph has too many points"));
    }
    let instructions = 10 + 2 * contours;
    let mut at = instructions + 2 + usize::from(u16_at(d, instructions)?);
    let mut flags = Vec::with_capacity(count);
    while flags.len() < count {
        let f = u8_at(d, at)?;
        at += 1;
        let repeat = if f & 0x08 != 0 {
            at += 1;
            usize::from(u8_at(d, at - 1)?)
        } else {
            0
        };
        for _ in 0..=repeat {
            flags.push(f);
        }
    }
    flags.truncate(count);
    let mut coord = |short: u8, same: u8| -> Result<Vec<f64>, FontError> {
        let mut v = 0_i32;
        let mut out = Vec::with_capacity(count);
        for &f in &flags {
            if f & short != 0 {
                let b = i32::from(u8_at(d, at)?);
                at += 1;
                v += if f & same != 0 { b } else { -b };
            } else if f & same == 0 {
                v += i32::from(i16_at(d, at)?);
                at += 2;
            }
            // Each sum stays in the i16 range, so the next delta cannot overflow the i32.
            if i16::try_from(v).is_err() {
                return Err(FontError::Corrupt("a glyph coordinate is outside the 16-bit range"));
            }
            out.push(f64::from(v));
        }
        Ok(out)
    };
    let xs = coord(0x02, 0x10)?;
    let ys = coord(0x04, 0x20)?;
    let mut out = Vec::with_capacity(contours);
    let mut first = 0;
    for end in ends {
        out.push((first..=end).map(|i| OutlinePoint { x: xs[i], y: ys[i], on: flags[i] & 0x01 != 0 }).collect());
        first = end + 1;
    }
    Ok(out)
}

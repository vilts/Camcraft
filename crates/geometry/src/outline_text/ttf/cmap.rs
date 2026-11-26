//! Bounded Unicode character maps, from the public OpenType cmap specification.

use super::{FontError, to_usize, u16_at, u32_at};

pub(super) enum Cmap<'a> {
    Format4(&'a [u8]),
    Format12(&'a [u8]),
}

impl<'a> Cmap<'a> {
    pub(super) fn parse(data: &'a [u8]) -> Result<Self, FontError> {
        let count = usize::from(u16_at(data, 2)?);
        data.get(..4 + 8 * count).ok_or(FontError::Truncated)?;
        let mut best = None;
        for i in 0..count {
            let rec = 4 + 8 * i;
            let (platform, encoding) = (u16_at(data, rec)?, u16_at(data, rec + 2)?);
            if !(platform == 0 || (platform == 3 && (encoding == 1 || encoding == 10))) {
                continue;
            }
            let offset = to_usize(u32_at(data, rec + 4)?);
            let tail = data.get(offset..).ok_or(FontError::Truncated)?;
            let format = u16_at(tail, 0)?;
            let length = match format {
                4 => usize::from(u16_at(tail, 2)?),
                12 => to_usize(u32_at(tail, 4)?),
                _ => continue,
            };
            let sub = tail.get(..length).ok_or(FontError::Truncated)?;
            match format {
                12 => {
                    let groups = to_usize(u32_at(sub, 12)?);
                    let end = groups
                        .checked_mul(12)
                        .and_then(|n| n.checked_add(16))
                        .ok_or(FontError::Truncated)?;
                    sub.get(..end).ok_or(FontError::Truncated)?;
                    return Ok(Self::Format12(sub));
                }
                4 => {
                    let seg_count = u16_at(sub, 6)?;
                    if seg_count == 0 || seg_count % 2 != 0 {
                        return Err(FontError::Corrupt(
                            "a cmap segment count is not a positive even number",
                        ));
                    }
                    sub.get(..16 + 4 * usize::from(seg_count))
                        .ok_or(FontError::Truncated)?;
                    if best.is_none() {
                        best = Some(Self::Format4(sub));
                    }
                }
                _ => {}
            }
        }
        best.ok_or(FontError::MissingTable(
            "cmap (a Unicode subtable of format 4 or 12)",
        ))
    }

    pub(super) fn glyph_id(&self, ch: char) -> Result<u32, FontError> {
        let c = u32::from(ch);
        match self {
            Self::Format12(d) => {
                let groups = to_usize(u32_at(d, 12)?);
                for i in 0..groups {
                    let g = 16 + 12 * i;
                    let (start, end) = (u32_at(d, g)?, u32_at(d, g + 4)?);
                    if (start..=end).contains(&c) {
                        return Ok(u32_at(d, g + 8)?.saturating_add(c - start));
                    }
                }
            }
            Self::Format4(d) => {
                let Ok(c) = u16::try_from(c) else {
                    return Ok(0);
                };
                let segs = usize::from(u16_at(d, 6)? / 2);
                let ends = 14;
                let starts = ends + 2 * segs + 2;
                let deltas = starts + 2 * segs;
                let ranges = deltas + 2 * segs;
                for s in 0..segs {
                    if u16_at(d, ends + 2 * s)? < c {
                        continue;
                    }
                    let start = u16_at(d, starts + 2 * s)?;
                    if start > c {
                        break;
                    }
                    let delta = u16_at(d, deltas + 2 * s)?;
                    let range = u16_at(d, ranges + 2 * s)?;
                    return if range == 0 {
                        Ok(u32::from(c.wrapping_add(delta)))
                    } else {
                        let addr = ranges + 2 * s + usize::from(range) + 2 * usize::from(c - start);
                        Ok(match u16_at(d, addr)? {
                            0 => 0,
                            g => u32::from(g.wrapping_add(delta)),
                        })
                    };
                }
            }
        }
        Ok(0)
    }
}

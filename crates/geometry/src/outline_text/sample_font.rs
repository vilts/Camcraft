//! A tiny TrueType font made in memory, for the tests of this module and of the document's
//! text command: no font file needs to be stored or licensed.
//!
//! Provenance: our own glyphs (boxes and simple curves), written as the table layouts of the
//! public OpenType specification. No font file was consulted.
//!
//! The em is 1000 units and `H` is 700 units tall, so the cap height is 700. The glyphs:
//!
//! - ` ` (space): no outline, advance 300.
//! - `H`, `I`: one straight contour each.
//! - `O`: a box with a box hole.
//! - `D`: one contour with two quadratic control points.
//! - `o`: one contour of four control points and no on-curve point.
//! - `8`: a composite of two `O`s at half size, one over the other.
//! - `X`: two bars that cross (an overlap that a pocket cannot take).

/// One contour: (x, y, on the curve) for each point.
pub type Contour = Vec<(i16, i16, bool)>;

/// One glyph of a test font.
pub enum Glyph {
    Empty,
    Simple(Vec<Contour>),
    /// (glyph index, x offset, y offset, scale in 2.14 fixed point) for each part.
    Composite(Vec<(u16, i16, i16, i16)>),
    /// The `glyf` bytes as written, for a glyph that the other forms cannot state.
    Raw(Vec<u8>),
}

fn rect(x0: i16, y0: i16, x1: i16, y1: i16, clockwise: bool) -> Contour {
    let mut c = vec![(x0, y0, true), (x0, y1, true), (x1, y1, true), (x1, y0, true)];
    if !clockwise {
        c.reverse();
    }
    c
}

/// The glyph table: (character, advance, glyph). Glyph 0 is `.notdef`.
fn glyphs() -> Vec<(Option<char>, u16, Glyph)> {
    let h = vec![
        (0, 0, true), (0, 700, true), (100, 700, true), (100, 400, true), (400, 400, true), (400, 700, true),
        (500, 700, true), (500, 0, true), (400, 0, true), (400, 300, true), (100, 300, true), (100, 0, true),
    ];
    let d = vec![(0, 0, true), (0, 700, true), (300, 700, true), (600, 700, false), (600, 350, true), (600, 0, false), (300, 0, true)];
    let o = vec![(300, 0, false), (0, 250, false), (300, 500, false), (600, 250, false)];
    vec![
        (None, 500, Glyph::Empty),
        (Some(' '), 300, Glyph::Empty),
        (Some('H'), 600, Glyph::Simple(vec![h])),
        (Some('I'), 200, Glyph::Simple(vec![rect(0, 0, 100, 700, true)])),
        (Some('O'), 700, Glyph::Simple(vec![rect(0, 0, 600, 700, true), rect(150, 150, 450, 550, false)])),
        (Some('D'), 700, Glyph::Simple(vec![d])),
        (Some('o'), 700, Glyph::Simple(vec![o])),
        (Some('8'), 400, Glyph::Composite(vec![(4, 0, 0, 0x2000), (4, 0, 360, 0x2000)])),
        (Some('X'), 700, Glyph::Simple(vec![rect(0, 300, 600, 400, true), rect(250, 0, 350, 700, true)])),
    ]
}

fn be16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn be32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn len16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn glyph_bytes(glyph: &Glyph) -> Vec<u8> {
    let mut g = Vec::new();
    match glyph {
        Glyph::Empty => {}
        Glyph::Raw(bytes) => g.extend_from_slice(bytes),
        Glyph::Simple(contours) => {
            let points: Vec<_> = contours.iter().flatten().copied().collect();
            let xs = points.iter().map(|p| p.0);
            let ys = points.iter().map(|p| p.1);
            be16(&mut g, len16(contours.len()));
            for v in [xs.clone().min(), ys.clone().min(), xs.max(), ys.max()] {
                be16(&mut g, v.unwrap_or(0) as u16);
            }
            let mut end = 0_usize;
            for c in contours {
                end += c.len();
                be16(&mut g, len16(end.saturating_sub(1)));
            }
            be16(&mut g, 0); // no instructions
            g.extend(points.iter().map(|p| u8::from(p.2)));
            for axis in 0..2 {
                let mut last = 0_i16;
                for p in &points {
                    let v = if axis == 0 { p.0 } else { p.1 };
                    be16(&mut g, v.wrapping_sub(last) as u16);
                    last = v;
                }
            }
        }
        Glyph::Composite(parts) => {
            be16(&mut g, u16::MAX); // -1 contours
            for _ in 0..4 {
                be16(&mut g, 0);
            }
            for (i, &(gid, dx, dy, scale)) in parts.iter().enumerate() {
                let more = if i + 1 < parts.len() { 0x0020 } else { 0 };
                be16(&mut g, 0x0001 | 0x0002 | 0x0008 | more);
                be16(&mut g, gid);
                be16(&mut g, dx as u16);
                be16(&mut g, dy as u16);
                be16(&mut g, scale as u16);
            }
        }
    }
    if g.len() % 2 == 1 {
        g.push(0);
    }
    g
}

/// The font file's bytes.
#[must_use]
pub fn sample_font() -> Vec<u8> {
    build_font(&glyphs(), None)
}

/// A font of `table`: (character, advance, glyph) for each glyph, glyph 0 first. With
/// `cap_height`, the font has an `OS/2` table of version 2 that states it.
#[must_use]
pub fn build_font(table: &[(Option<char>, u16, Glyph)], cap_height: Option<i16>) -> Vec<u8> {
    let n = len16(table.len());
    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    let mut hmtx = Vec::new();
    for (_, advance, glyph) in table {
        be32(&mut loca, len32(glyf.len()));
        glyf.extend(glyph_bytes(glyph));
        be16(&mut hmtx, *advance);
        be16(&mut hmtx, 0);
    }
    be32(&mut loca, len32(glyf.len()));

    // cmap: one format 4 subtable (platform 3, encoding 1), one segment for each character.
    let mut chars: Vec<(u16, u16)> =
        table.iter().enumerate().filter_map(|(gid, (ch, _, _))| ch.map(|c| (u32::from(c) as u16, len16(gid)))).collect();
    chars.sort_unstable();
    chars.push((0xFFFF, 0));
    let segs = len16(chars.len());
    let mut sub = Vec::new();
    be16(&mut sub, 4);
    be16(&mut sub, 16 + 8 * segs);
    be16(&mut sub, 0);
    be16(&mut sub, 2 * segs);
    for _ in 0..3 {
        be16(&mut sub, 0); // search hints, unused by the reader
    }
    chars.iter().for_each(|&(c, _)| be16(&mut sub, c));
    be16(&mut sub, 0);
    chars.iter().for_each(|&(c, _)| be16(&mut sub, c));
    chars.iter().for_each(|&(c, gid)| be16(&mut sub, if c == 0xFFFF { 1 } else { gid.wrapping_sub(c) }));
    chars.iter().for_each(|_| be16(&mut sub, 0));
    let mut cmap = Vec::new();
    be16(&mut cmap, 0);
    be16(&mut cmap, 1);
    be16(&mut cmap, 3);
    be16(&mut cmap, 1);
    be32(&mut cmap, 12);
    cmap.extend(sub);

    let mut head = vec![0_u8; 54];
    head[18..20].copy_from_slice(&1000_u16.to_be_bytes());
    head[50..52].copy_from_slice(&1_u16.to_be_bytes()); // long loca
    let mut maxp = vec![0_u8; 6];
    maxp[0..4].copy_from_slice(&0x0000_5000_u32.to_be_bytes());
    maxp[4..6].copy_from_slice(&n.to_be_bytes());
    let mut hhea = vec![0_u8; 36];
    hhea[34..36].copy_from_slice(&n.to_be_bytes());

    let mut tables: Vec<(&[u8; 4], Vec<u8>)> =
        vec![(b"cmap", cmap), (b"glyf", glyf), (b"head", head), (b"hhea", hhea), (b"hmtx", hmtx), (b"loca", loca), (b"maxp", maxp)];
    if let Some(cap) = cap_height {
        let mut os2 = vec![0_u8; 96];
        os2[0..2].copy_from_slice(&2_u16.to_be_bytes());
        os2[88..90].copy_from_slice(&cap.to_be_bytes());
        tables.push((b"OS/2", os2));
    }
    let mut out = Vec::new();
    be32(&mut out, 0x0001_0000);
    be16(&mut out, len16(tables.len()));
    for _ in 0..3 {
        be16(&mut out, 0);
    }
    let mut offset = 12 + 16 * tables.len();
    for (tag, data) in &tables {
        out.extend_from_slice(*tag);
        be32(&mut out, 0); // checksum, not checked by the reader
        be32(&mut out, len32(offset));
        be32(&mut out, len32(data.len()));
        offset += data.len().next_multiple_of(4);
    }
    for (_, data) in &tables {
        out.extend_from_slice(data);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    out
}

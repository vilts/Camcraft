//! Minimal clean-room uncompressed RGB PNG encoder (RFC 1950 zlib + RFC 1951 stored blocks, ISO/IEC 15948).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

pub fn default_diff_dir() -> PathBuf {
    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
    let base = Path::new(env!("CARGO_TARGET_TMPDIR"));
    // Cargo supplies this at compile time, not as a runtime environment variable.
    loop {
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let dir = base.join(format!("shape-mask-diffs-{}-{id}", std::process::id()));
        match fs::create_dir_all(base).and_then(|()| fs::create_dir(&dir)) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            _ => return dir, // Leave other errors to the PNG writer's named diagnostic.
        }
    }
}

pub fn write_diff_mask(
    dir: &Path,
    name: &str,
    size: (usize, usize),
    expected: &[bool],
    expected_d1: &[bool],
    swept: &[bool],
    highlight: Option<(usize, usize)>,
) -> Result<PathBuf, String> {
    let (w, h) = size;
    let mut rgb = vec![24u8; w * h * 3];
    for i in 0..w * h {
        let color = match (expected[i], swept[i], expected_d1[i]) {
            (_, true, false) => [220, 48, 48],     // Red: overcut
            (true, false, _) => [52, 132, 224],   // Blue: uncut leftover
            (true, true, _) => [68, 172, 108],    // Green: covered as expected
            (false, true, true) => [216, 176, 56], // Amber: 1-cell border halo
            (false, false, _) => [24, 26, 32],    // Dark charcoal: outside
        };
        let base = i * 3;
        rgb[base..base + 3].copy_from_slice(&color);
    }
    if let Some((cx, cy)) = highlight {
        for ny in cy.saturating_sub(2)..=(cy + 2).min(h - 1) {
            for nx in cx.saturating_sub(2)..=(cx + 2).min(w - 1) {
                let base = (ny * w + nx) * 3;
                rgb[base..base + 3].copy_from_slice(&[255, 255, 64]);
            }
        }
    }
    let clean: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let path = dir.join(format!("{clean}.png"));
    write_rgb(&path, w, h, &rgb)?;
    Ok(path)
}

pub fn write_rgb(path: &Path, width: usize, height: usize, rgb: &[u8]) -> Result<(), String> {
    let bytes = encode_rgb(width, height, rgb)?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| format!("create png dir {}: {e}", parent.display()))?;
    }
    fs::write(path, &bytes).map_err(|e| format!("write png {}: {e}", path.display()))
}

pub fn encode_rgb(width: usize, height: usize, rgb: &[u8]) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 {
        return Err("png dimensions must be positive".into());
    }
    let expected_len = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(3))
        .ok_or_else(|| "png byte count overflow".to_string())?;
    if rgb.len() != expected_len {
        return Err(format!("expected {expected_len} RGB bytes, got {}", rgb.len()));
    }
    let w32 = u32::try_from(width).map_err(|_| "png width exceeds u32".to_string())?;
    let h32 = u32::try_from(height).map_err(|_| "png height exceeds u32".to_string())?;

    let row_bytes = width * 3;
    let mut raw = Vec::with_capacity((row_bytes + 1) * height);
    for y in 0..height {
        raw.push(0); // filter type 0 (None)
        let row = &rgb[y * row_bytes..(y + 1) * row_bytes];
        raw.extend_from_slice(row);
    }

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w32.to_be_bytes());
    ihdr.extend_from_slice(&h32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit RGB, deflate, filter 0, no interlace

    let idat = zlib_stored(&raw);
    let mut out = Vec::with_capacity(PNG_SIG.len() + 25 + idat.len() + 24);
    out.extend_from_slice(&PNG_SIG);
    push_chunk(&mut out, b"IHDR", &ihdr)?;
    push_chunk(&mut out, b"IDAT", &idat)?;
    push_chunk(&mut out, b"IEND", &[])?;
    Ok(out)
}

fn push_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) -> Result<(), String> {
    let len = u32::try_from(data.len()).map_err(|_| "png chunk exceeds u32".to_string())?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let crc = crc32(tag, data);
    out.extend_from_slice(&crc.to_be_bytes());
    Ok(())
}

fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + 16 + (raw.len() / 65535) * 5);
    out.extend_from_slice(&[0x78, 0x01]);
    let mut offset = 0usize;
    while offset < raw.len() {
        let end = (offset + 65535).min(raw.len());
        let is_final = end == raw.len();
        let len = (end - offset) as u16;
        out.push(u8::from(is_final));
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(&raw[offset..end]);
        offset = end;
    }
    let adler = adler32(raw);
    out.extend_from_slice(&adler.to_be_bytes());
    out
}

fn crc32(tag: &[u8; 4], data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in tag.iter().chain(data.iter()) {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let mut s1 = 1u32;
    let mut s2 = 0u32;
    for &b in data {
        s1 = (s1 + u32::from(b)) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    (s2 << 16) | s1
}

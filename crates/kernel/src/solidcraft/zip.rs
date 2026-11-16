//! A bounded, in-memory ZIP reader for Solidcraft design containers
//! (#1456): the central directory is read, named entries are looked up and
//! inflated under a size cap, their lengths and CRC-32 checked. Nothing is
//! extracted to disk and entry names are only compared, never used as paths.
//! Provenance: written from PKWARE's published APPNOTE.TXT (the .ZIP file
//! format specification: sections 4.3.7 local header, 4.3.12 central
//! directory header, 4.3.16 end of central directory, 4.4.4 flags, 4.4.5
//! methods 0 and 8); the budgets follow Solidcraft's published container
//! rules (docs/design-container.md, "Bounds and recovery", tangency-labs/
//! solidcraft, Apache-2.0). Deflate itself is flate2's (RFC 1951).

use std::io::Read;

/// The most entries a container may list (Solidcraft's own bound).
pub const MAX_ENTRIES: usize = 10_000;

const EOCD_SIG: u32 = 0x0605_4b50;
const CDH_SIG: u32 = 0x0201_4b50;
const LFH_SIG: u32 = 0x0403_4b50;
const EOCD_LEN: usize = 22;
const CDH_LEN: usize = 46;
const LFH_LEN: usize = 30;

/// One central directory entry.
struct Entry {
    name: Vec<u8>,
    flags: u16,
    method: u16,
    crc: u32,
    compressed: usize,
    size: usize,
    local: usize,
}

/// A container's directory over its bytes.
pub struct Archive<'a> {
    bytes: &'a [u8],
    entries: Vec<Entry>,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*s.first()?, *s.get(1)?]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    let a: [u8; 4] = s.try_into().ok()?;
    Some(u32::from_le_bytes(a))
}

/// Whether `bytes` start with a ZIP local-header signature.
pub fn looks_like_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x50, 0x4b, 0x03, 0x04])
}

impl<'a> Archive<'a> {
    /// The directory of `bytes`. `Err` names what is wrong: not a ZIP, a
    /// multi-disk or ZIP64 archive, too many entries, a directory outside
    /// the file, or a name listed twice.
    pub fn open(bytes: &'a [u8]) -> Result<Self, String> {
        if !looks_like_zip(bytes) || bytes.len() < EOCD_LEN {
            return Err("not a ZIP container".into());
        }
        // The end record sits in the last 22 + 65535 (comment) bytes.
        let lowest = bytes.len().saturating_sub(EOCD_LEN + usize::from(u16::MAX));
        let mut at = bytes.len() - EOCD_LEN;
        let eocd = loop {
            if u32_at(bytes, at) == Some(EOCD_SIG) {
                break at;
            }
            if at == lowest {
                return Err("no ZIP end-of-directory record".into());
            }
            at -= 1;
        };
        let field = |off: usize| u16_at(bytes, eocd + off).ok_or("truncated ZIP end record");
        let (disk, cd_disk, here, total) = (field(4)?, field(6)?, field(8)?, field(10)?);
        if disk != 0 || cd_disk != 0 || here != total {
            return Err("multi-disk ZIP archives are refused".into());
        }
        let cd_size = u32_at(bytes, eocd + 12).ok_or("truncated ZIP end record")?;
        let cd_off = u32_at(bytes, eocd + 16).ok_or("truncated ZIP end record")?;
        if total == u16::MAX || cd_size == u32::MAX || cd_off == u32::MAX {
            return Err("ZIP64 archives are refused".into());
        }
        let count = usize::from(total);
        if count > MAX_ENTRIES {
            return Err(format!(
                "{count} entries is past the {MAX_ENTRIES}-entry limit"
            ));
        }
        let start = usize::try_from(cd_off).map_err(|_| "ZIP directory offset")?;
        let len = usize::try_from(cd_size).map_err(|_| "ZIP directory size")?;
        let dir = start
            .checked_add(len)
            .filter(|end| *end <= eocd)
            .and_then(|end| bytes.get(start..end))
            .ok_or("the ZIP directory lies outside the file")?;
        let mut entries: Vec<Entry> = Vec::new();
        let mut p = 0usize;
        for _ in 0..count {
            let bad = || "a truncated ZIP directory entry".to_string();
            if u32_at(dir, p) != Some(CDH_SIG) {
                return Err(bad());
            }
            let nlen = usize::from(u16_at(dir, p + 28).ok_or_else(bad)?);
            let xlen = usize::from(u16_at(dir, p + 30).ok_or_else(bad)?);
            let clen = usize::from(u16_at(dir, p + 32).ok_or_else(bad)?);
            let disk_start = u16_at(dir, p + 34).ok_or_else(bad)?;
            let name_at = p + CDH_LEN;
            let name = dir.get(name_at..name_at + nlen).ok_or_else(bad)?.to_vec();
            let compressed = u32_at(dir, p + 20).ok_or_else(bad)?;
            let size = u32_at(dir, p + 24).ok_or_else(bad)?;
            let local = u32_at(dir, p + 42).ok_or_else(bad)?;
            if disk_start != 0 {
                return Err("multi-disk ZIP archives are refused".into());
            }
            if compressed == u32::MAX || size == u32::MAX || local == u32::MAX {
                return Err("ZIP64 archives are refused".into());
            }
            if entries.iter().any(|e| e.name == name) {
                return Err(format!(
                    "entry '{}' is listed twice",
                    String::from_utf8_lossy(&name)
                ));
            }
            entries.push(Entry {
                name,
                flags: u16_at(dir, p + 8).ok_or_else(bad)?,
                method: u16_at(dir, p + 10).ok_or_else(bad)?,
                crc: u32_at(dir, p + 16).ok_or_else(bad)?,
                compressed: usize::try_from(compressed).map_err(|_| bad())?,
                size: usize::try_from(size).map_err(|_| bad())?,
                local: usize::try_from(local).map_err(|_| bad())?,
            });
            p = name_at + nlen + xlen + clen;
        }
        Ok(Archive { bytes, entries })
    }

    /// Whether the archive lists an entry of this name.
    pub fn has(&self, name: &str) -> bool {
        self.entries.iter().any(|e| e.name == name.as_bytes())
    }

    /// The uncompressed bytes of entry `name`, at most `cap` of them. `Err`
    /// names a missing or encrypted entry, an unknown method, a declared or
    /// real size past `cap`, a length or CRC-32 that disagrees with the
    /// directory, or a corrupt deflate stream.
    pub fn read(&self, name: &str, cap: usize) -> Result<Vec<u8>, String> {
        let e = self
            .entries
            .iter()
            .find(|e| e.name == name.as_bytes())
            .ok_or_else(|| format!("missing entry '{name}'"))?;
        if e.flags & 1 != 0 {
            return Err(format!("entry '{name}' is encrypted"));
        }
        if e.size > cap {
            return Err(format!(
                "entry '{name}' declares {} bytes, past its {cap}-byte limit",
                e.size
            ));
        }
        let b = self.bytes;
        let bad = || format!("entry '{name}': truncated local header");
        if u32_at(b, e.local) != Some(LFH_SIG) {
            return Err(bad());
        }
        let nlen = usize::from(u16_at(b, e.local.saturating_add(26)).ok_or_else(bad)?);
        let xlen = usize::from(u16_at(b, e.local.saturating_add(28)).ok_or_else(bad)?);
        let data = e
            .local
            .checked_add(LFH_LEN + nlen + xlen)
            .and_then(|s| Some((s, s.checked_add(e.compressed)?)))
            .and_then(|(s, end)| b.get(s..end))
            .ok_or_else(|| format!("entry '{name}': data lies outside the file"))?;
        let out = match e.method {
            0 => data.to_vec(),
            8 => {
                let mut out = Vec::new();
                let limit = u64::try_from(e.size).unwrap_or(u64::MAX).saturating_add(1);
                flate2::read::DeflateDecoder::new(data)
                    .take(limit)
                    .read_to_end(&mut out)
                    .map_err(|err| format!("entry '{name}': corrupt deflate data ({err})"))?;
                out
            }
            m => {
                return Err(format!(
                    "entry '{name}': compression method {m} is not supported"
                ));
            }
        };
        if out.len() != e.size {
            return Err(format!(
                "entry '{name}': {} bytes, the directory says {}",
                out.len(),
                e.size
            ));
        }
        let mut crc = flate2::Crc::new();
        crc.update(&out);
        if crc.sum() != e.crc {
            return Err(format!("entry '{name}': CRC-32 mismatch"));
        }
        Ok(out)
    }
}

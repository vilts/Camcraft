//! Helpers of the Solidcraft import tests (#1456): the sample design's
//! entries read out of its ZIP container, and a stored (method 0) ZIP
//! written back from edited entries, so each refusal is pinned on a
//! corrupted copy built in the test.
//! Provenance: our own test design; the ZIP layout is PKWARE's published
//! APPNOTE.TXT (4.3.7 local header, 4.3.12 central directory header, 4.3.16
//! end record). The sample is the maintainer's own design, testdata/models.

#![allow(dead_code)] // each test binary uses its own part

use std::io::Read;
use std::path::PathBuf;

use camcraft_kernel::solidcraft::hex_digest;
use serde_json::Value;

pub const SAMPLE: &str = "solidcraft_for_beginners.sc";

pub fn sample_bytes() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models")
        .join(SAMPLE);
    match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => panic!("{}: {e}", path.display()),
    }
}

fn u16_at(b: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes([b[at], b[at + 1]]))
}

fn u32_at(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize
}

/// The entries of a (trusted, test-only) ZIP, in directory order.
pub fn entries(zip: &[u8]) -> Vec<(String, Vec<u8>)> {
    let eocd = (0..=zip.len() - 22)
        .rev()
        .find(|&i| zip[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
        .expect("end record");
    let count = u16_at(zip, eocd + 10);
    let mut p = u32_at(zip, eocd + 16);
    let mut out = Vec::new();
    for _ in 0..count {
        let method = u16_at(zip, p + 10);
        let csize = u32_at(zip, p + 20);
        let (nlen, xlen, clen) = (
            u16_at(zip, p + 28),
            u16_at(zip, p + 30),
            u16_at(zip, p + 32),
        );
        let local = u32_at(zip, p + 42);
        let name = String::from_utf8_lossy(&zip[p + 46..p + 46 + nlen]).into_owned();
        let start = local + 30 + u16_at(zip, local + 26) + u16_at(zip, local + 28);
        let raw = &zip[start..start + csize];
        let data = match method {
            0 => raw.to_vec(),
            _ => {
                let mut v = Vec::new();
                flate2::read::DeflateDecoder::new(raw)
                    .read_to_end(&mut v)
                    .expect("inflate");
                v
            }
        };
        out.push((name, data));
        p += 46 + nlen + xlen + clen;
    }
    out
}

/// A ZIP of `entries`, all stored (method 0), with true CRC-32s.
pub fn zip(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut dir = Vec::new();
    for (name, data) in entries {
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let (crc, size) = (crc.sum(), data.len() as u32);
        let local = out.len() as u32;
        let n = name.as_bytes();
        let common = |v: &mut Vec<u8>| {
            v.extend_from_slice(&20u16.to_le_bytes()); // version needed
            v.extend_from_slice(&0u16.to_le_bytes()); // flags
            v.extend_from_slice(&0u16.to_le_bytes()); // method: stored
            v.extend_from_slice(&[0; 4]); // time, date
            v.extend_from_slice(&crc.to_le_bytes());
            v.extend_from_slice(&size.to_le_bytes());
            v.extend_from_slice(&size.to_le_bytes());
            v.extend_from_slice(&(n.len() as u16).to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes()); // extra
        };
        out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
        common(&mut out);
        out.extend_from_slice(n);
        out.extend_from_slice(data);
        dir.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
        dir.extend_from_slice(&20u16.to_le_bytes()); // version made by
        common(&mut dir);
        dir.extend_from_slice(&[0; 6]); // comment length, disk, internal attributes
        dir.extend_from_slice(&[0; 4]); // external attributes
        dir.extend_from_slice(&local.to_le_bytes());
        dir.extend_from_slice(n);
    }
    let (cd_off, cd_len) = (out.len() as u32, dir.len() as u32);
    out.extend_from_slice(&dir);
    out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
    let count = entries.len() as u16;
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&cd_len.to_le_bytes());
    out.extend_from_slice(&cd_off.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// The sample's parts, ready to edit and write back with [`Parts::build`].
#[derive(Clone)]
pub struct Parts {
    pub entries: Vec<(String, Vec<u8>)>,
}

impl Parts {
    pub fn sample() -> Self {
        Parts {
            entries: entries(&sample_bytes()),
        }
    }

    pub fn get(&self, name: &str) -> Vec<u8> {
        self.entries
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, d)| d.clone())
            .unwrap_or_default()
    }

    pub fn set(&mut self, name: &str, data: Vec<u8>) {
        if let Some(e) = self.entries.iter_mut().find(|(n, _)| n == name) {
            e.1 = data;
        }
    }

    pub fn json(&self, name: &str) -> Value {
        serde_json::from_slice(&self.get(name)).expect("sample JSON")
    }

    pub fn set_json(&mut self, name: &str, v: &Value) {
        self.set(name, serde_json::to_vec(v).expect("JSON"));
    }

    /// Edits the manifest, leaving its digests as they were.
    pub fn manifest(&mut self, f: impl FnOnce(&mut Value)) {
        let mut m = self.json("cache/model.json");
        f(&mut m);
        self.set_json("cache/model.json", &m);
    }

    /// Re-keys the manifest to the current document and payload, so an
    /// edit behind them is seen by the check after the digests.
    pub fn rekey(&mut self) {
        let (doc, payload) = (self.get("document.json"), self.get("cache/bodies.bin"));
        self.manifest(|m| {
            m["document"] = Value::from(hex_digest(&doc));
            m["payload"] = Value::from(hex_digest(&payload));
        });
    }

    pub fn build(&self) -> Vec<u8> {
        zip(&self.entries)
    }
}

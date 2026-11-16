//! Hostile Solidcraft bodies (#1467): the sample's final body decoded,
//! edited and re-encoded (or its bytes damaged), then wrapped back with true
//! digests and CRC-32s, so every case reaches the body decoder and the
//! reader's own vertex check rather than stopping at the container. A
//! non-finite or far-off coordinate is refused by name; byte damage to the
//! body never panics or hangs; a deflate entry that inflates past its
//! declared size is cut off at that size.
//! Provenance: our own tests over the maintainer's sample design
//! (testdata/models/solidcraft_for_beginners.sc); the ZIP layout is PKWARE's
//! published APPNOTE.TXT.
//!
//! Gate map (#1467): covers camcraft_kernel::solidcraft.

mod sc_util;
mod step_util;

use std::io::Write;
use std::time::Duration;

use camcraft_kernel::KernelError;
use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcraft::{MAX_COORD_MM, SolidcraftImport, import_solidcraft};
use sc_util::Parts;
use serde_json::json;
use solidcore::kernel::CancelToken;
use solidcore_record::native;

fn import(bytes: &[u8]) -> Result<SolidcraftImport, KernelError> {
    import_solidcraft(&mut SolidcoreKernel::default(), bytes, &CancelToken::new())
}

#[track_caller]
fn refused(case: &str, bytes: &[u8], want: &[&str]) {
    match import(bytes) {
        Ok(i) => panic!("{case}: imported '{}' instead of refusing", i.solid.name),
        Err(e) => {
            let msg = e.to_string();
            assert!(
                want.iter().all(|w| msg.contains(w)),
                "{case}: '{msg}' does not say all of {want:?}"
            );
        }
    }
}

/// The sample's final body slice: (offset, length). It is the last slice.
fn body_slice(p: &Parts) -> (usize, usize) {
    let m = p.json("cache/model.json");
    let s = &m["slices"][13];
    let n = |i: usize| s[i].as_u64().unwrap_or(0) as usize;
    (n(0), n(1))
}

/// The sample with its final body's bytes replaced by `body`, the slice
/// table and both digests made to agree, written back as a ZIP.
fn with_body(p: &Parts, body: &[u8]) -> Vec<u8> {
    let (at, len) = body_slice(p);
    let mut payload = p.get("cache/bodies.bin");
    assert_eq!(at + len, payload.len(), "the final body is the last slice");
    payload.truncate(at);
    payload.extend_from_slice(body);
    let mut c = p.clone();
    c.set("cache/bodies.bin", payload);
    c.manifest(|m| m["slices"][13][1] = json!(body.len()));
    c.rekey();
    c.build()
}

fn sample_body(p: &Parts) -> (Vec<u8>, native::Saved) {
    let (at, len) = body_slice(p);
    let bytes = p.get("cache/bodies.bin")[at..at + len].to_vec();
    let saved = match native::decode(&bytes) {
        Ok(s) => s,
        Err(e) => panic!("sample body: {e}"),
    };
    (bytes, saved)
}

fn encode(s: &native::Saved) -> Vec<u8> {
    match native::encode(s) {
        Ok(b) => b,
        Err(e) => panic!("encode: {e}"),
    }
}

#[test]
fn hostile_coordinates_are_refused_by_name() {
    let p = Parts::sample();
    let (raw, saved) = sample_body(&p);
    let fresh = || match native::decode(&raw) {
        Ok(s) => s,
        Err(e) => panic!("sample body: {e}"),
    };
    assert!(!saved.solid.vertices.is_empty());
    // The splice itself is sound: the body re-encoded unchanged imports.
    if let Err(e) = import(&with_body(&p, &encode(&saved))) {
        panic!("re-encoded sample: {e}");
    }
    // Past the reader's limit but inside the decoder's own (1e12).
    for (case, x) in [("far", 1.0e9), ("just past", MAX_COORD_MM * 1.01), ("far neg", -5.0e7)] {
        let mut s = fresh();
        s.solid.vertices[0].point[1] = x;
        refused(
            case,
            &with_body(&p, &encode(&s)),
            &["final body", "vertex 0", "coordinate limit"],
        );
    }
    // NaN, infinities and a coordinate past 1e12 cannot be encoded: a
    // sentinel coordinate's bytes (tag 5, f64 LE) are patched in place.
    let sentinel = 4_242.424_242_f64;
    let mut s = fresh();
    s.solid.vertices[0].point[0] = sentinel;
    let body = encode(&s);
    let mut needle = vec![5u8];
    needle.extend_from_slice(&sentinel.to_le_bytes());
    let hits: Vec<usize> = body
        .windows(needle.len())
        .enumerate()
        .filter(|(_, w)| *w == needle.as_slice())
        .map(|(i, _)| i)
        .collect();
    assert_eq!(hits.len(), 1, "the sentinel is written once, as tag 5 + f64");
    for (case, x, want) in [
        ("NaN", f64::NAN, "non-finite"),
        ("+inf", f64::INFINITY, "non-finite"),
        ("-inf", f64::NEG_INFINITY, "non-finite"),
        ("1e13", 1.0e13, "non-finite"),
    ] {
        let mut b = body.clone();
        b[hits[0] + 1..hits[0] + 9].copy_from_slice(&x.to_le_bytes());
        refused(case, &with_body(&p, &b), &["final body", want]);
    }
}

/// xorshift64*: a fixed, reproducible stream.
fn next(s: &mut u64) -> u64 {
    *s ^= *s >> 12;
    *s ^= *s << 25;
    *s ^= *s >> 27;
    s.wrapping_mul(0x2545_f491_4f6c_dd1d)
}

#[test]
fn damaged_bodies_never_panic_or_hang() {
    let p = Parts::sample();
    let (body, _) = sample_body(&p);
    let mut seed = 0x1467_bd1e_1467_bd1e_u64;
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    let pick = |s: &mut u64, len: usize| 8 + (next(s) % (len as u64 - 8)) as usize;
    for i in 0..24 {
        let mut b = body.clone();
        match i % 4 {
            // Random bytes past the magic.
            0 | 1 => {
                for _ in 0..1 + next(&mut seed) % 8 {
                    let at = pick(&mut seed, b.len());
                    b[at] = next(&mut seed) as u8;
                }
            }
            // A length or count word blown up to 0xFFFFFFFF.
            2 => {
                let at = pick(&mut seed, b.len() - 4);
                b[at..at + 4].copy_from_slice(&[0xff; 4]);
            }
            // Cut short.
            _ => b.truncate(pick(&mut seed, b.len())),
        }
        cases.push((format!("body {i}"), with_body(&p, &b)));
    }
    let limit = Duration::from_secs(120);
    let outcome = step_util::bounded_cancel("body mutations", limit, move |stop| {
        let (mut ok, mut panics) = (0usize, 0usize);
        for (case, bytes) in &cases {
            let mut k = SolidcoreKernel::default();
            match import_solidcraft(&mut k, bytes, stop) {
                Ok(_) => ok += 1,
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("panicked") {
                        panics += 1;
                    } else if !msg.contains("final body") {
                        return Err(format!("{case}: '{msg}' stopped before the body"));
                    }
                }
            }
            if stop.is_cancelled() {
                return Err(format!("time bound reached at {case}"));
            }
        }
        Ok((ok, panics))
    });
    match outcome {
        Ok((ok, panics)) => println!(
            "{ok} damaged bodies still imported; {panics} caught panics reported by name"
        ),
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn a_deflate_entry_past_its_declared_size_is_cut_off() {
    // container.json as deflate: 64 MiB of spaces declared as 64 bytes.
    let data = vec![b' '; 64 << 20];
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    if let Err(e) = enc.write_all(&data) {
        panic!("deflate: {e}");
    }
    let packed = match enc.finish() {
        Ok(b) => b,
        Err(e) => panic!("deflate: {e}"),
    };
    let mut crc = flate2::Crc::new();
    crc.update(&data[..64]);
    let name = b"container.json";
    let header = |v: &mut Vec<u8>| {
        v.extend_from_slice(&20u16.to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes());
        v.extend_from_slice(&8u16.to_le_bytes()); // deflate
        v.extend_from_slice(&[0; 4]);
        v.extend_from_slice(&crc.sum().to_le_bytes());
        v.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        v.extend_from_slice(&64u32.to_le_bytes()); // the lie
        v.extend_from_slice(&(name.len() as u16).to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes());
    };
    let mut z = vec![0x50, 0x4b, 0x03, 0x04];
    header(&mut z);
    z.extend_from_slice(name);
    z.extend_from_slice(&packed);
    let cd = z.len() as u32;
    let mut dir = vec![0x50, 0x4b, 0x01, 0x02];
    dir.extend_from_slice(&20u16.to_le_bytes());
    header(&mut dir);
    dir.extend_from_slice(&[0; 10]);
    dir.extend_from_slice(&0u32.to_le_bytes());
    dir.extend_from_slice(name);
    z.extend_from_slice(&dir);
    z.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 1, 0, 1, 0]);
    z.extend_from_slice(&(dir.len() as u32).to_le_bytes());
    z.extend_from_slice(&cd.to_le_bytes());
    z.extend_from_slice(&0u16.to_le_bytes());
    refused("bomb", &z, &["container.json", "65 bytes, the directory says 64"]);
}

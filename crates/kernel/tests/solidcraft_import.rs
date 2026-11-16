//! The Solidcraft import's laws (#1456): the sample design's cached final
//! body imports as one checked solid in mm whose extent is the design's own
//! parameters (d1 = 125 mm diameter, d2 = 85 mm height), and every way a
//! `.sc` file can be wrong, stale or out of reach is a named refusal, built
//! here as a corrupted copy of the sample. A mutation run pins that no byte
//! damage panics or hangs.
//! Provenance: our own test design; the sample is the maintainer's design
//! (testdata/models/solidcraft_for_beginners.sc).
//!
//! Gate map (#1456): covers camcraft_kernel::solidcraft.

mod sc_util;
mod step_util;

use std::time::{Duration, Instant};

use camcraft_kernel::KernelError;
use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcraft::{SolidcraftImport, import_solidcraft};
use sc_util::{Parts, sample_bytes, zip};
use serde_json::{Value, json};
use solidcore::kernel::CancelToken;

fn import(bytes: &[u8]) -> Result<SolidcraftImport, KernelError> {
    import_solidcraft(&mut SolidcoreKernel::default(), bytes, &CancelToken::new())
}

/// The import of `bytes` must be refused with `want` in its message.
#[track_caller]
fn refused(case: &str, bytes: &[u8], want: &str) {
    match import(bytes) {
        Ok(i) => panic!("{case}: imported '{}' instead of refusing", i.solid.name),
        Err(e) => {
            let msg = e.to_string();
            assert!(msg.contains(want), "{case}: '{msg}' does not say '{want}'");
        }
    }
}

#[test]
fn sample_imports_its_final_body_at_the_design_size() {
    let started = Instant::now();
    let i = match import(&sample_bytes()) {
        Ok(i) => i,
        Err(e) => panic!("sample: {e}"),
    };
    println!("import took {:?}", started.elapsed());
    assert_eq!(i.solid.name, "Body1");
    assert_eq!(i.design, "solidcraft_for_beginners");
    assert_eq!(i.units, "mm");
    assert!(i.solid.measure.volume > 0.0);
    // The port's exact box (it can be too large on trimmed cylinders, #939,
    // so equality here is the stronger pin). The machining mesh is not
    // used: it refuses this body at every chord today (#1463).
    let (lo, hi) = (i.solid.measure.min, i.solid.measure.max);
    println!("extent {lo:?} .. {hi:?}");
    let want = ([-62.5, -62.5, 0.0], [62.5, 62.5, 85.0]);
    for a in 0..3 {
        assert!(
            (lo[a] - want.0[a]).abs() < 1e-6,
            "min {lo:?}, want {:?}",
            want.0
        );
        assert!(
            (hi[a] - want.1[a]).abs() < 1e-6,
            "max {hi:?}, want {:?}",
            want.1
        );
    }
    // The same parts written back stored (the refusals' base) import too.
    let rebuilt = match import(&Parts::sample().build()) {
        Ok(r) => r,
        Err(e) => panic!("rebuilt sample: {e}"),
    };
    assert_eq!(rebuilt.solid.name, "Body1");
}

#[test]
fn not_a_container_is_refused() {
    refused("empty", b"", "empty file");
    refused("text", b"solid cube\nendsolid\n", "not a ZIP");
    refused("STEP", b"ISO-10303-21;\nHEADER;\n", "not a ZIP");
    let mut no_end = sample_bytes();
    no_end.truncate(no_end.len() - 22);
    refused("no end record", &no_end, "end-of-directory");
}

#[test]
fn container_damage_is_refused() {
    let good = Parts::sample().build();
    let end = good.len() - 22;
    // A stored byte of document.json flipped: its CRC-32 disagrees.
    let mut crc = good.clone();
    crc[30 + "document.json".len() + 5] ^= 0x20;
    refused("crc", &crc, "CRC-32");
    // Entry count 0xFFFF in the end record: a ZIP64 archive.
    let mut z64 = good.clone();
    z64[end + 8..end + 12].copy_from_slice(&[0xff, 0xff, 0xff, 0xff]);
    refused("zip64", &z64, "ZIP64");
    let mut disk = good.clone();
    disk[end + 4] = 1;
    refused("multi-disk", &disk, "multi-disk");
    let mut dir = good.clone();
    dir[end + 16..end + 20].copy_from_slice(&u32::MAX.wrapping_sub(1).to_le_bytes());
    refused("directory outside", &dir, "outside the file");
    // The first entry marked encrypted, in its directory header.
    let p = Parts::sample();
    let cd = u32::from_le_bytes([
        good[end + 16],
        good[end + 17],
        good[end + 18],
        good[end + 19],
    ]);
    let mut enc = good.clone();
    enc[cd as usize + 8] |= 1;
    refused("encrypted", &enc, "encrypted");
    // A name listed twice.
    let mut dup = p.entries.clone();
    dup.push(dup[0].clone());
    refused("duplicate", &zip(&dup), "listed twice");
    // Past the entry budget.
    let many: Vec<(String, Vec<u8>)> = (0..10_001).map(|i| (format!("f{i}"), Vec::new())).collect();
    refused("entries", &zip(&many), "entry limit");
    // container.json past its size cap.
    let mut big = p.clone();
    big.set("container.json", vec![b' '; 5000]);
    refused("container cap", &big.build(), "limit");
}

#[test]
fn documents_we_cannot_read_are_refused() {
    let p = Parts::sample();
    let mut fmt = p.clone();
    fmt.set(
        "container.json",
        br#"{"format":"solidcraft-container/2"}"#.to_vec(),
    );
    refused("container format", &fmt.build(), "container format");
    let mut doc = p.json("document.json");
    doc["units"] = json!("in");
    let mut inch = p.clone();
    inch.set_json("document.json", &doc);
    inch.rekey();
    refused("inch", &inch.build(), "units 'in'");
    doc["units"] = json!("mm");
    doc["format"] = json!("freecad/1");
    let mut other = p.clone();
    other.set_json("document.json", &doc);
    other.rekey();
    refused(
        "document format",
        &other.build(),
        "not a Solidcraft document",
    );
    let mut bare = p.clone();
    bare.entries.retain(|(n, _)| !n.starts_with("cache/"));
    refused("no cache", &bare.build(), "no model cache");
    let mut half = p.clone();
    half.entries.retain(|(n, _)| n != "cache/bodies.bin");
    refused("no payload", &half.build(), "no model cache");
}

#[test]
fn a_cache_that_is_not_this_documents_is_refused() {
    let p = Parts::sample();
    for (field, value, want) in [
        ("format", "solidcraft-model-cache/9", "model cache format"),
        ("format", "solidcraft-model-cache/11", "model cache format"),
        ("producer", "solidcraft-occt-adapter/1", "producer"),
        ("kernel", "truck", "kernel 'truck'"),
    ] {
        let mut c = p.clone();
        c.manifest(|m| m[field] = json!(value));
        refused(value, &c.build(), want);
    }
    // The document edited after the cache was written: stale.
    let mut stale = p.clone();
    let mut doc = p.get("document.json");
    doc.push(b' ');
    stale.set("document.json", doc);
    refused("stale", &stale.build(), "stale");
    // One payload byte changed under the manifest's digest.
    let mut bin = p.clone();
    let mut payload = p.get("cache/bodies.bin");
    payload[100] ^= 1;
    bin.set("cache/bodies.bin", payload);
    refused("payload", &bin.build(), "payload digest");
}

/// The last slice of the sample (the final body's).
fn last_slice(p: &Parts) -> (usize, usize) {
    let m = p.json("cache/model.json");
    let s = &m["slices"][13];
    (
        s[0].as_u64().unwrap_or(0) as usize,
        s[1].as_u64().unwrap_or(0) as usize,
    )
}

/// A named manifest edit and the refusal it must draw.
type Edit = (&'static str, Box<dyn Fn(&mut Value)>, &'static str);

#[test]
fn a_bad_body_table_or_body_is_refused() {
    let p = Parts::sample();
    let edits: [Edit; 7] = [
        (
            "slice past",
            Box::new(|m| m["slices"][13][1] = json!(821_601)),
            "runs past",
        ),
        (
            "slice gap",
            Box::new(|m| m["slices"][1][0] = json!(31_806)),
            "overlap or leave gaps",
        ),
        (
            "slice short",
            Box::new(|m| m["slices"][13][1] = json!(821_599)),
            "trailing data",
        ),
        (
            "several",
            Box::new(|m| m["model"]["states"][18]["bodies"] = json!([[12, 2]])),
            "several bodies",
        ),
        (
            "none",
            Box::new(|m| m["model"]["states"][18]["bodies"] = json!([])),
            "holds no body",
        ),
        (
            "placed copy",
            Box::new(|m| {
                m["bodies"][13] = json!([13, 0, [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]])
            }),
            "placed copy",
        ),
        (
            "all skipped",
            Box::new(|m| {
                if let Some(fs) = m["model"]["features"].as_array_mut() {
                    for f in fs {
                        f["skipped"] = json!(true);
                    }
                }
            }),
            "no evaluated feature",
        ),
    ];
    for (case, edit, want) in edits {
        let mut c = p.clone();
        c.manifest(|m| edit(m));
        refused(case, &c.build(), want);
    }
    let mut run = p.clone();
    run.manifest(|m| m["model"]["states"][18]["bodies"] = json!([[13, 9]]));
    refused("run", &run.build(), "body run out of range");
    // The body's bytes replaced, re-keyed so the digest check passes.
    let (at, _) = last_slice(&p);
    for (case, head, want) in [
        ("bad magic", &b"SCBREP99"[..], "bad magic"),
        ("mesh body", &b"SCMESH01"[..], "mesh body"),
        (
            "garbled",
            &b"SCBREP05\xff\xff\xff\xff"[..],
            "does not decode",
        ),
    ] {
        let mut c = p.clone();
        let mut payload = p.get("cache/bodies.bin");
        payload[at..at + head.len()].copy_from_slice(head);
        c.set("cache/bodies.bin", payload);
        c.rekey();
        refused(case, &c.build(), want);
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
fn damaged_files_never_panic_or_hang() {
    let original = sample_bytes();
    let parts = Parts::sample();
    let manifest = parts.get("cache/model.json");
    let mut seed = 0x1456_5c5c_1456_5c5c_u64;
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..48 {
        let mut b = original.clone();
        for _ in 0..1 + next(&mut seed) % 8 {
            let at = (next(&mut seed) % b.len() as u64) as usize;
            b[at] = next(&mut seed) as u8;
        }
        if i % 4 == 0 {
            b.truncate((next(&mut seed) % b.len() as u64) as usize);
        }
        cases.push((format!("bytes {i}"), b));
    }
    // Manifest text damage behind a valid container: the JSON and table paths.
    for i in 0..16 {
        let mut m = manifest.clone();
        for _ in 0..1 + next(&mut seed) % 4 {
            let at = (next(&mut seed) % m.len() as u64) as usize;
            m[at] = b"0123456789[],:\"-"[(next(&mut seed) % 16) as usize];
        }
        let mut c = parts.clone();
        c.set("cache/model.json", m);
        cases.push((format!("manifest {i}"), c.build()));
    }
    let limit = Duration::from_secs(120);
    let outcome = step_util::bounded_cancel("mutations", limit, move |stop| {
        let mut ok = 0usize;
        for (case, bytes) in &cases {
            let mut k = SolidcoreKernel::default();
            if import_solidcraft(&mut k, bytes, stop).is_ok() {
                ok += 1;
            }
            if stop.is_cancelled() {
                return Err(format!("time bound reached at {case}"));
            }
        }
        Ok(ok)
    });
    match outcome {
        Ok(ok) => println!("{ok} damaged copies still imported (damage in unread bytes)"),
        Err(e) => panic!("{e}"),
    }
}

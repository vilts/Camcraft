//! Corrupt-file corpus of the port's native B-rep reader (M12.1 stage 3):
//! blank, foreign, truncated and field-corrupted `.smb` files through the
//! raw reader and the seam's importer. Never a panic, never a hang (every
//! case runs under a time bound); the importer hands out a model only when
//! the edit left it whole, and then exactly the closed-form 10-box.
//! Provenance: our own corpus, shaped after the Prime Directive's
//! degenerate-input list; edits are made on the port's own writer output.
//!
//! Gate map (M12.1): covers camcraft_kernel::solidcore_import (read_smb,
//! import_smb, the heal guards).

mod smb_util;

use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_import::{MAX_SMB_BYTES, import_smb, read_smb};
use camcraft_kernel::{Kernel, KernelError, Measure};
use smb_util::{bounded, box_text, edit_record, gapped_box, port_verdict};
use solidcore::topology::Brep;

/// Edits that leave the model itself whole: the importer must hand out the
/// exact 10-box (volume AND bounding box). A dropped final newline loses
/// nothing; a zero or negative zone tolerance is replaced by the reader's
/// default, and the importer says so in a pinned note. A future format
/// version is NOT intact: the seam refuses it (decision 5).
const INTACT: &[&str] = &[
    "last byte dropped",
    "zero zone tolerance",
    "negative zone tolerance",
];

/// Corrupt native files: each name, the raw reader's verdict and the
/// importer's verdict, pinned so a port bump that changes any of them fails
/// here and gets looked at. Every importer verdict outside [`INTACT`] must
/// be an `Err` - the importer never hands out a corrupt model.
fn corpus() -> Vec<(&'static str, Vec<u8>)> {
    let t = box_text();
    let b = t.as_bytes();
    let cut = |num: usize, den: usize| b.get(..b.len() * num / den).unwrap_or_default().to_vec();
    let noise: Vec<u8> = (0u32..4096)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    vec![
        ("empty file", Vec::new()),
        ("one space", b" ".to_vec()),
        (
            "text, not a B-rep",
            b"solid cube\nfacet normal 0 0 1\n".to_vec(),
        ),
        ("binary noise 4 KiB", noise),
        ("header only", b"Version 42\nHealer Version 10\n".to_vec()),
        (
            "future version",
            t.replacen("Version 42", "Version 9999", 1).into_bytes(),
        ),
        ("cut at 10%", cut(1, 10)),
        ("cut at 50%", cut(1, 2)),
        ("cut at 90%", cut(9, 10)),
        (
            "last byte dropped",
            b.get(..b.len().saturating_sub(1))
                .unwrap_or_default()
                .to_vec(),
        ),
        (
            "NaN vertex",
            edit_record(&t, "8 Vertices In Brep", 5, |_| "nan".into()).into_bytes(),
        ),
        (
            "inf vertex",
            edit_record(&t, "8 Vertices In Brep", 5, |_| "inf".into()).into_bytes(),
        ),
        (
            "zero zone tolerance",
            t.replacen(
                "0.0000100000000000 - Brep ZoneTol3d",
                "0.0 - Brep ZoneTol3d",
                1,
            )
            .into_bytes(),
        ),
        (
            "negative zone tolerance",
            t.replacen(
                "0.0000100000000000 - Brep ZoneTol3d",
                "-1.0 - Brep ZoneTol3d",
                1,
            )
            .into_bytes(),
        ),
        (
            "NaN zone tolerance",
            t.replacen(
                "0.0000100000000000 - Brep ZoneTol3d",
                "nan - Brep ZoneTol3d",
                1,
            )
            .into_bytes(),
        ),
        (
            "face count 4294967295",
            t.replacen("6 Faces In Brep", "4294967295 Faces In Brep", 1)
                .into_bytes(),
        ),
        (
            "face count -1",
            t.replacen("6 Faces In Brep", "-1 Faces In Brep", 1)
                .into_bytes(),
        ),
        (
            "face surface index out of range",
            edit_record(&t, "6 Faces In Brep", 5, |_| "77".into()).into_bytes(),
        ),
        (
            "faceuse face index out of range",
            edit_record(&t, "12 Faceuses In Brep", 4, |_| "99".into()).into_bytes(),
        ),
        ("vertex gap 1.0 (unhealable)", gapped_box(1.0).into_bytes()),
        (
            "one face dropped (open shell)",
            t.replacen("6 Faces In Brep", "5 Faces In Brep", 1)
                .into_bytes(),
        ),
        (
            "control point count 1e15",
            t.replacen(
                "1 Number of Control Points",
                "1000000000000000 Number of Control Points",
                1,
            )
            .into_bytes(),
        ),
    ]
}

/// What the raw reader and the importer do with each corpus file, pinned.
/// FINDING against the port's reader: it returns `Ok` with a closed box
/// whose vertex sits 1.0 off its edges; the importer refuses it (the gap is
/// past `HEAL_GAP_LIMIT` -> `Err invalid`). An absurd declared count (faces
/// 2^32-1, control points 1e15) is refused by the port before it allocates
/// (it checks counts against the bytes left).
///
/// Re-triaged at the pin of #916: solidcore 1.0.1's reader returned `Ok`
/// with an EMPTY B-rep (no faces) for a blank file, plain text, binary
/// noise and a header with no body, and only the seam's heal guard refused
/// them (`Err invalid`). The pinned commit's reader refuses all four itself
/// (`Err failed`), so both columns changed for those rows. A NaN zone
/// tolerance is still refused by the seam before the reader runs.
const KNOWN_SMB_VERDICTS: &[(&str, &str, &str)] = &[
    ("empty file", "Err refused", "Err refused"),
    ("one space", "Err failed", "Err failed"),
    ("text, not a B-rep", "Err failed", "Err failed"),
    ("binary noise 4 KiB", "Err failed", "Err failed"),
    ("header only", "Err failed", "Err failed"),
    ("future version", "Err refused", "Err refused"),
    ("cut at 10%", "Err failed", "Err failed"),
    ("cut at 50%", "Err failed", "Err failed"),
    ("cut at 90%", "Err failed", "Err failed"),
    ("last byte dropped", "Ok V=1000.000", "Ok V=1000.000"),
    ("NaN vertex", "Err failed", "Err failed"),
    ("inf vertex", "Err failed", "Err failed"),
    (
        "zero zone tolerance",
        "Ok V=1000.000",
        "Ok V=1000.000 [zone tolerance 0e0 replaced by 1e-4]",
    ),
    (
        "negative zone tolerance",
        "Ok V=1000.000",
        "Ok V=1000.000 [zone tolerance -1e0 replaced by 1e-4]",
    ),
    ("NaN zone tolerance", "Err refused", "Err refused"),
    ("face count 4294967295", "Err failed", "Err failed"),
    ("face count -1", "Err failed", "Err failed"),
    (
        "face surface index out of range",
        "Err failed",
        "Err failed",
    ),
    (
        "faceuse face index out of range",
        "Err failed",
        "Err failed",
    ),
    (
        "vertex gap 1.0 (unhealable)",
        "Ok V=1000.000",
        "Err invalid",
    ),
    ("one face dropped (open shell)", "Err failed", "Err failed"),
    ("control point count 1e15", "Err failed", "Err failed"),
];

#[test]
fn corrupt_native_files_never_panic_or_hang() {
    let mut observed = Vec::new();
    let mut bad_intact = Vec::new();
    let cube = Measure {
        volume: 1000.0,
        min: [0.0; 3],
        max: [10.0; 3],
    };
    for (name, bytes) in corpus() {
        let (raw, imp, m) = bounded(name, move || {
            let mut k = SolidcoreKernel::default();
            let raw = read_smb(&mut k, &bytes);
            let raw = port_verdict(raw, &mut k);
            let mut imp = import_smb(&mut k, &bytes);
            let notes: Vec<String> = imp
                .as_ref()
                .map(|i| i.notes.iter().map(ToString::to_string).collect())
                .unwrap_or_default();
            // Measured in place: the port's B-rep is no longer `Clone` at
            // the pinned commit (#916), and a measure leaves it as it was.
            let bbox = imp
                .as_mut()
                .ok()
                .and_then(|i| k.measure(&mut i.brep).ok());
            let mut imp = port_verdict(imp.map(|i| i.brep), &mut k);
            if !notes.is_empty() {
                imp = format!("{imp} [{}]", notes.join("; "));
            }
            (raw, imp, bbox)
        });
        println!("{name:<34} read -> {raw:<22} import -> {imp}");
        if INTACT.contains(&name) {
            match m {
                Some(m) if m.bbox_deviation(&cube) <= 1e-9 && (m.volume - 1000.0).abs() <= 1e-9 => {
                }
                other => bad_intact.push(format!("{name}: {other:?}")),
            }
        }
        observed.push((name.to_string(), raw, imp));
    }
    assert!(
        bad_intact.is_empty(),
        "intact files not the exact 10-box: {bad_intact:?}"
    );
    let wrong: Vec<_> = observed
        .iter()
        .filter(|(n, _, i)| {
            if INTACT.contains(&n.as_str()) {
                !i.starts_with("Ok V=1000.000")
            } else {
                i.starts_with("Ok")
            }
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "importer verdict wrong (corrupt model handed out, or intact one lost): {wrong:?}"
    );
    let pinned: Vec<(String, String, String)> = KNOWN_SMB_VERDICTS
        .iter()
        .map(|(n, r, i)| ((*n).to_string(), (*r).to_string(), (*i).to_string()))
        .collect();
    assert_eq!(
        observed, pinned,
        "reader verdicts changed - re-triage the findings"
    );
}

#[test]
fn seam_refuses_degenerate_heal_and_oversized_files() {
    let mut k = SolidcoreKernel::default();
    let r = k.heal(Brep::new());
    assert!(
        matches!(r, Err(KernelError::InvalidInput { op: "heal", .. })),
        "heal(empty): {:?}",
        r.map(|_| "Ok")
    );
    // an open sheet is not a solid, before or after healing
    let sheet = solidcore::primitive::api_primitives::create_plane(
        &mut k.k,
        solidcore::math::Point3::new(0.0, 0.0, 0.0),
        10.0,
        10.0,
    );
    assert!(sheet.is_ok(), "fixture sheet not built: {sheet:?}");
    let sheet = sheet.unwrap_or_default();
    assert!(!sheet.faces().is_empty(), "fixture sheet has no face");
    let (k, r) = bounded("heal sheet", move || {
        let r = k.heal(sheet).map(|_| "Ok");
        (k, r)
    });
    let mut k = k;
    println!("heal(sheet) -> {r:?}");
    assert!(r.is_err(), "heal(sheet) came back Ok");
    let big = vec![b' '; MAX_SMB_BYTES + 1];
    let r = read_smb(&mut k, &big);
    assert!(
        matches!(r, Err(KernelError::InvalidInput { op: "read_smb", .. })),
        "oversized: {:?}",
        r.map(|_| "Ok")
    );
}

/// The heal guard on the B-rep tolerance, reached directly: a 10-box whose
/// tolerance is zero, negative, NaN or infinite is refused by name before
/// the port's heal runs (its stitch doubles the tolerance from a tenth of
/// it and would never end from zero - upstream HANG-24). Every call runs
/// under the time bound, since an unguarded port call could spin.
#[test]
fn heal_refuses_nonsense_brep_tolerance() {
    for tol in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let r = bounded("heal at nonsense tolerance", move || {
            let mut k = SolidcoreKernel::default();
            let mut b = k.make_box([0.0; 3], [10.0; 3])?;
            b.set_tolerance(tol);
            k.heal(b).map(|_| "Ok")
        });
        println!("heal at tolerance {tol} -> {r:?}");
        assert!(
            matches!(r, Err(KernelError::InvalidInput { op: "heal", .. })),
            "heal at tolerance {tol}: {r:?}"
        );
    }
}

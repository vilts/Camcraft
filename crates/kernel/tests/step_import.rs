//! The solidcore-side STEP importer on its own (M12.1 stage 4, #916): the
//! closed forms and units it must hand out, and a corrupt-file corpus -
//! blank, foreign, truncated and field-corrupted STEP text. Never a panic,
//! never a hang (every case runs under a time bound that cancels the
//! import); a file the importer accepts is whole, anything else is refused
//! by name. Verdicts are pinned so a pin bump that changes one fails here.
//! Provenance: our own corpus, shaped after the Prime Directive's
//! degenerate-input list; edits are made on the repo's own box.step.
//!
//! Gate map (M12.1): covers camcraft_kernel::solidcore_step (read_step,
//! import_step, the size and time bounds).

mod step_util;

use std::path::PathBuf;
use std::time::Duration;

use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::{MAX_STEP_BYTES, import_step, read_step};
use camcraft_kernel::{KernelError, Measure};
use solidcore::kernel::CancelToken;
use step_util::{CASE_TIMEOUT, bounded_cancel, truck_read};

fn models_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models")
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(f64::MIN_POSITIVE)
}

/// The closed forms the importer must hand out exactly: the unit box of
/// box.step, the same box from the file with an empty parameter list
/// (#887), and the stated units of the inch and metre fixtures (#306).
#[test]
fn importer_hands_out_closed_forms_and_units() {
    let import = |file: &'static str| {
        let bytes = std::fs::read(models_dir().join(file)).unwrap_or_default();
        bounded_cancel(file, CASE_TIMEOUT, move |stop| {
            let mut k = SolidcoreKernel::default();
            import_step(&mut k, &bytes, stop)
                .map(|i| (i.solids.iter().map(|s| (s.measure, s.area)).collect::<Vec<_>>(), i.length_units_mm))
        })
    };
    let unit_box = Measure {
        volume: 1.0,
        min: [0.0; 3],
        max: [1.0; 3],
    };
    for file in ["box.step", "empty_list.step"] {
        match import(file) {
            Ok((solids, units)) => {
                assert_eq!(units, [1.0], "{file}: millimetres");
                assert_eq!(solids.len(), 1, "{file}: one solid");
                for (m, area) in solids {
                    assert!((m.volume - 1.0).abs() <= 1e-9, "{file}: volume {}", m.volume);
                    assert!(m.bbox_deviation(&unit_box) <= 1e-9, "{file}: box {m:?}");
                    assert!((area - 6.0).abs() <= 1e-9, "{file}: area {area}");
                }
            }
            Err(e) => panic!("{file}: {e}"),
        }
    }
    for (file, unit, solids) in [
        ("Milling-test-inch.step", 25.4, 1),
        ("sample_ap214.step", 1000.0, 3),
    ] {
        match import(file) {
            Ok((s, units)) => {
                assert_eq!(units, [unit], "{file}: mm per unit");
                assert_eq!(s.len(), solids, "{file}: solids");
            }
            Err(e) => panic!("{file}: {e}"),
        }
    }
    // The inch part is the millimetre part: same solid once in millimetres.
    match (import("Milling-test.step"), import("Milling-test-inch.step")) {
        (Ok((mm, _)), Ok((inch, _))) => {
            let (a, b) = (mm.first().map(|s| s.0.volume), inch.first().map(|s| s.0.volume));
            match (a, b) {
                (Some(a), Some(b)) => assert!(rel(a, b) <= 1e-9, "mm {a} vs inch {b}"),
                other => panic!("no solid: {other:?}"),
            }
        }
        other => panic!("Milling-test pair: {:?}", other.0.err().or(other.1.err())),
    }
    for file in ["tool_csys_none.step", "tool_csys_degenerate.step"] {
        match import(file) {
            Err(KernelError::InvalidResult { detail, .. }) => {
                assert_eq!(detail, "the file holds no solid", "{file}");
            }
            other => panic!("{file}: expected a named refusal, got {:?}", other.map(|_| "Ok")),
        }
    }
}

/// Compact verdict of an import: the error's kind, or the solids' volume.
fn verdict(r: Result<f64, KernelError>) -> String {
    match r {
        Ok(v) => format!("Ok V={v:.3}"),
        Err(KernelError::InvalidInput { .. }) => "Err refused".into(),
        Err(KernelError::Failed { .. }) => "Err failed".into(),
        Err(KernelError::InvalidResult { .. }) => "Err invalid".into(),
        Err(KernelError::Unsupported { .. }) => "Err unsupported".into(),
        Err(KernelError::TimedOut { .. }) => "Err timed out".into(),
    }
}

fn box_text() -> String {
    let bytes = std::fs::read(models_dir().join("box.step")).unwrap_or_default();
    assert!(!bytes.is_empty(), "box.step missing");
    String::from_utf8_lossy(&bytes).replace("\r\n", "\n")
}

/// `text` with the first `from` replaced by `to`; the edit must apply.
fn edit(text: &str, from: &str, to: &str) -> Vec<u8> {
    assert!(text.contains(from), "corpus edit no longer applies: {from}");
    text.replacen(from, to, 1).into_bytes()
}

const ORIGIN: &str = "#158 = CARTESIAN_POINT('', (0.0, 0.0, 0.0));";
const SOLID: &str = "#16 = MANIFOLD_SOLID_BREP('', #17);";

fn corpus() -> Vec<(&'static str, Vec<u8>)> {
    let t = box_text();
    let b = t.as_bytes();
    let cut = |num: usize, den: usize| b.get(..b.len() * num / den).unwrap_or_default().to_vec();
    let noise: Vec<u8> = (0u32..4096)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    let nested = format!(
        "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1 = A({}1{});\nENDSEC;\nEND-ISO-10303-21;\n",
        "(".repeat(100_000),
        ")".repeat(100_000)
    );
    vec![
        ("empty file", Vec::new()),
        ("one space", b" ".to_vec()),
        ("text, not STEP", b"solid cube\nfacet normal 0 0 1\n".to_vec()),
        ("binary noise 4 KiB", noise),
        ("header only", b"ISO-10303-21;\nHEADER;\nENDSEC;\n".to_vec()),
        (
            "no entities",
            b"ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n".to_vec(),
        ),
        ("cut at 10%", cut(1, 10)),
        ("cut at 50%", cut(1, 2)),
        ("cut at 90%", cut(9, 10)),
        ("parameter list 100000 deep", nested.into_bytes()),
        (
            "shell names a missing face",
            edit(&t, "CLOSED_SHELL('', (#18, ", "CLOSED_SHELL('', (#99999, "),
        ),
        (
            "solid names itself as its shell",
            edit(&t, SOLID, "#16 = MANIFOLD_SOLID_BREP('', #16);"),
        ),
        (
            "plane placed by itself",
            edit(&t, "#80 = PLANE('', #81);", "#80 = PLANE('', #80);"),
        ),
        (
            "one face dropped (open shell)",
            edit(&t, "(#18, #25, #32, #39, #46, #53)", "(#25, #32, #39, #46, #53)"),
        ),
        (
            "vertex moved 0.5 off its edges",
            edit(&t, ORIGIN, "#158 = CARTESIAN_POINT('', (-0.5, 0.0, 0.0));"),
        ),
        (
            "vertex at 1e300",
            edit(&t, ORIGIN, "#158 = CARTESIAN_POINT('', (1.0E300, 0.0, 0.0));"),
        ),
        (
            "point with one coordinate",
            edit(&t, ORIGIN, "#158 = CARTESIAN_POINT('', (0.0));"),
        ),
        (
            "length unit of zero inches",
            edit(
                &t,
                "#12 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );",
                "#12 = ( CONVERSION_BASED_UNIT('INCH', #900) LENGTH_UNIT() NAMED_UNIT(*) );\n\
                 #900 = LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.0), #901);\n\
                 #901 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );",
            ),
        ),
        (
            "second copy of the solid",
            edit(&t, SOLID, "#16 = MANIFOLD_SOLID_BREP('', #17);\n#990 = MANIFOLD_SOLID_BREP('', #17);"),
        ),
    ]
}

/// What `import_step` does with each corpus file, pinned. Nothing here may
/// be `Ok` unless the edit left every solid of the file whole. The two
/// `Ok` rows, each looked at:
/// - "point with one coordinate": ISO 10303-42 lets a point list one to
///   three coordinates; the reader takes the missing ones as 0, and the
///   edited point is the origin, so the model is the same 1 mm box.
/// - "second copy of the solid": the file states two solids on one shell
///   and gets two (V = 2). The importer hands out what the file states; it
///   does not look for coincident solids (recorded in the decision file).
///
/// "length unit of zero inches" is refused by the SEAM: the reader alone
/// reads a unit it cannot convert as millimetres, silently.
///
/// Third column: truck-stepio on the same bytes, as the bench reads it
/// (ruststep + `Table` + mesh; none of crates/app's checks after the
/// mesh), with its mesh's open triangle sides. FINDINGS against that
/// path, recorded and not chased here: it answers `Ok` with an OPEN mesh
/// (open > 0) for a shell with a face dropped, dangling or unreadable and
/// for a vertex moved off its edges or sent to 1e300, and it reads neither
/// the solid item nor the length unit. What crates/app does with such a
/// mesh afterwards is outside this crate's bench.
const KNOWN_STEP_VERDICTS: &[(&str, &str, &str)] = &[
    ("empty file", "Err refused", "Err"),
    ("one space", "Err refused", "Err"),
    ("text, not STEP", "Err refused", "Err"),
    ("binary noise 4 KiB", "Err refused", "Err"),
    ("header only", "Err refused", "Err"),
    ("no entities", "Err invalid", "Err"),
    ("cut at 10%", "Err refused", "Err"),
    ("cut at 50%", "Err refused", "Err"),
    ("cut at 90%", "Err refused", "Err"),
    ("parameter list 100000 deep", "Err refused", "Err"),
    ("shell names a missing face", "Err invalid", "Ok V=1.000 open=4"),
    ("solid names itself as its shell", "Err invalid", "Ok V=1.000 open=0"),
    ("plane placed by itself", "Err invalid", "Ok V=1.000 open=4"),
    ("one face dropped (open shell)", "Err invalid", "Ok V=1.000 open=4"),
    ("vertex moved 0.5 off its edges", "Err invalid", "Ok V=1.000 open=4"),
    ("vertex at 1e300", "Err invalid", "Ok V=1.000 open=3"),
    ("point with one coordinate", "Ok V=1.000", "Ok V=1.000 open=0"),
    ("length unit of zero inches", "Err refused", "Ok V=1.000 open=0"),
    ("second copy of the solid", "Ok V=2.000", "Ok V=1.000 open=0"),
];

#[test]
fn corrupt_step_files_never_panic_or_hang() {
    let mut observed = Vec::new();
    for (name, bytes) in corpus() {
        let bytes_for_truck = bytes.clone();
        let v = bounded_cancel(name, CASE_TIMEOUT, move |stop| {
            let mut k = SolidcoreKernel::default();
            let r = import_step(&mut k, &bytes, stop);
            if let Err(e) = &r {
                println!("{name:<36} {e}");
            }
            verdict(r.map(|i| i.solids.iter().map(|s| s.measure.volume).sum()))
        });
        let b = bytes_for_truck;
        let t = bounded_cancel(name, CASE_TIMEOUT, move |_| match truck_read(&b, 1.0) {
            Ok(t) => format!("Ok V={:.3} open={}", t.mesh.volume, t.mesh.open_edges),
            Err(e) => {
                println!("{name:<36} truck: {e}");
                "Err".to_string()
            }
        });
        println!("{name:<36} -> solidcore {v} | truck {t}");
        observed.push((name.to_string(), v, t));
    }
    let pinned: Vec<(String, String, String)> = KNOWN_STEP_VERDICTS
        .iter()
        .map(|(n, v, t)| (n.to_string(), v.to_string(), t.to_string()))
        .collect();
    assert_eq!(observed, pinned, "importer verdicts changed - re-triage");
}

/// The size bound and the time bound, each refused by name.
#[test]
fn importer_keeps_its_size_and_time_bounds() {
    let over = bounded_cancel("oversized", CASE_TIMEOUT, |stop| {
        let mut k = SolidcoreKernel::default();
        let big = vec![b' '; MAX_STEP_BYTES + 1];
        read_step(&mut k, &big, stop).map(|_| ())
    });
    match over {
        Err(KernelError::InvalidInput { detail, .. }) => {
            assert!(detail.contains("byte limit"), "{detail}");
        }
        other => panic!("oversized file: {other:?}"),
    }
    // A bound already reached: nothing is parsed.
    let text = box_text().into_bytes();
    let stop = CancelToken::new();
    stop.cancel();
    let mut k = SolidcoreKernel::default();
    match import_step(&mut k, &text, &stop).map(|_| ()) {
        Err(KernelError::TimedOut { op, stage }) => {
            assert_eq!((op, stage), ("read_step", "before parsing"));
        }
        other => panic!("cancelled before the start: {other:?}"),
    }
    // The kernel is usable again and keeps no token of the refused call.
    assert!(k.k.cancel.is_none(), "the caller's kernel keeps the import's token");
    stop.reset();
    let again = import_step(&mut k, &text, &stop).map(|i| i.solids.len());
    assert_eq!(again.ok(), Some(1), "the same kernel imports after a time-out");
    // A bound reached while the parser runs ends it by name: a large valid
    // exchange structure, cancelled a moment after the import starts.
    let mut big = String::from("ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n");
    for i in 1..=600_000u32 {
        big.push_str(&format!("#{i} = CARTESIAN_POINT('', (0.0, 0.0, 0.0));\n"));
    }
    big.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    let r = bounded_cancel("cancelled mid-parse", Duration::from_millis(1), move |stop| {
        let mut k = SolidcoreKernel::default();
        import_step(&mut k, big.as_bytes(), stop).map(|_| ())
    });
    match r {
        Err(KernelError::TimedOut { .. }) => {}
        // A machine fast enough to parse it all inside the bound still
        // refuses the file by name: it holds no solid.
        Err(KernelError::InvalidResult { detail, .. }) => {
            assert_eq!(detail, "the file holds no solid");
        }
        other => panic!("cancelled mid-parse: {other:?}"),
    }
}

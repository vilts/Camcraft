//! Shared helpers of the native-format benches (heal_import.rs,
//! import_corpus.rs): a time bound per port call, the 10-box as the port
//! writes it, record edits on that text, and a compact verdict.
//! Provenance: our own bench design (M12.1 retirement gate).

use std::sync::mpsc;
use std::time::Duration;

use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_import::write_smb;
use camcraft_kernel::{Kernel, KernelError};
use solidcore::topology::Brep;

/// No single port call in this bench may take longer than this (release).
pub const CASE_TIMEOUT: Duration = Duration::from_secs(60);

/// Runs `f` on its own thread and fails the test if it does not answer in
/// [`CASE_TIMEOUT`] (a hang is a finding, never a silent wait).
pub fn bounded<T: Send + 'static>(name: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(CASE_TIMEOUT) {
        Ok(v) => v,
        Err(e) => panic!("{name}: no answer within {CASE_TIMEOUT:?} ({e}) - a hang or a panic"),
    }
}

/// The text of the 10-box as the port writes it.
pub fn box_text() -> String {
    let mut k = SolidcoreKernel::default();
    let s = k.make_box([0.0; 3], [10.0; 3]);
    assert!(s.is_ok(), "fixture box: {:?}", s.as_ref().map(|_| "Ok"));
    let bytes = s.and_then(|s| write_smb(&k, &s));
    assert!(bytes.is_ok(), "write: {bytes:?}");
    String::from_utf8_lossy(&bytes.unwrap_or_default()).into_owned()
}

/// `text` with token `col` of the first record after the line equal to
/// `section` (skipping `//` comment and blank lines) replaced by `f(token)`.
pub fn edit_record(text: &str, section: &str, col: usize, f: impl Fn(&str) -> String) -> String {
    let mut out = Vec::new();
    let mut armed = false;
    let mut done = false;
    for line in text.lines() {
        if !done && armed && !line.starts_with("//") && !line.trim().is_empty() {
            let mut toks: Vec<String> = line.split_whitespace().map(str::to_string).collect();
            if let Some(t) = toks.get_mut(col) {
                *t = f(t);
            }
            out.push(toks.join(" "));
            done = true;
            continue;
        }
        if line.trim() == section {
            armed = true;
        }
        out.push(line.to_string());
    }
    out.join("\n") + "\n"
}

/// The first vertex's X moved by `gap` (vertex record: index, flags, two
/// user ids, tolerance, X, Y, Z). The edge curves still end at the old
/// corner, so the file carries a vertex-edge gap of `gap`.
pub fn gapped_box(gap: f64) -> String {
    edit_record(&box_text(), "8 Vertices In Brep", 5, |t| {
        format!("{:.16}", t.parse::<f64>().unwrap_or(0.0) + gap)
    })
}

/// Compact verdict of a reader / importer result.
pub fn port_verdict(r: Result<Brep, KernelError>, k: &mut SolidcoreKernel) -> String {
    match r {
        Err(KernelError::InvalidInput { .. }) => "Err refused".into(),
        Err(KernelError::Failed { .. }) => "Err failed".into(),
        Err(KernelError::InvalidResult { .. }) => "Err invalid".into(),
        Err(KernelError::Unsupported { .. }) => "Err unsupported".into(),
        Err(KernelError::TimedOut { .. }) => "Err timed out".into(),
        Ok(mut b) => match k.measure(&mut b) {
            Ok(m) => format!("Ok V={:.3}", m.volume),
            Err(_) => format!("Ok faces={} no measure", b.faces().len()),
        },
    }
}

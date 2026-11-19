//! `camcraft-cl` - headless CLFile inspector (M13.3): reads a CL text
//! dump and prints its move statistics. Provenance: our own tool over
//! our own dump grammar (`cl::text`); the move classes are RS-274's.

use camcraft_cl::cl::{text, MoveKind};
use std::process::ExitCode;

const USAGE: &str = "\
camcraft-cl - CamCraft CLFile inspector (headless)

USAGE:
    camcraft-cl <file.cl>

Reads a CL text dump (the tests/golden/*.cl format) and prints move
counts, path lengths, bounds and the chaining check. Exits non-zero on
an unreadable or malformed file, or a path whose moves do not chain.

OPTIONS:
    -h, --help        print this text
    --version         print the version
";

fn main() -> ExitCode {
    log_init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut paths = Vec::new();
    for a in &args {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--version" => {
                println!("camcraft-cl {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            o if o.starts_with("--") => return die(format!("unknown option {o:?} (try --help)")),
            o => paths.push(o.to_string()),
        }
    }
    let [path] = paths.as_slice() else {
        return die("expected exactly one CLFile path (try --help)".into());
    };
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return die(format!("cannot read {path}: {e}")),
    };
    let t = match text::from_text(&src) {
        Ok(t) => t,
        Err(e) => return die(format!("cannot parse {path}: {e}")),
    };
    let (mut rapid, mut feed, mut plunge, mut arcs) = (0usize, 0usize, 0usize, 0usize);
    let (mut rapid_len, mut cut_len) = (0.0f64, 0.0f64);
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for m in &t.moves {
        match m.kind {
            MoveKind::Rapid => rapid += 1,
            MoveKind::Feed => feed += 1,
            MoveKind::Plunge => plunge += 1,
        }
        arcs += usize::from(m.arc.is_some());
        let len: f64 = m
            .segments3(0.01)
            .iter()
            .map(|(a, b)| ((b.x - a.x).powi(2) + (b.y - a.y).powi(2) + (b.z - a.z).powi(2)).sqrt())
            .sum();
        if m.kind == MoveKind::Rapid {
            rapid_len += len;
        } else {
            cut_len += len;
        }
        for (i, v) in [m.to.x, m.to.y, m.to.z].into_iter().enumerate() {
            lo[i] = lo[i].min(v);
            hi[i] = hi[i].max(v);
        }
    }
    println!("file: {path}");
    println!("moves: {} (rapid {rapid}, feed {feed}, plunge {plunge}, arcs {arcs})", t.len());
    println!("cycles: {}", t.cycles.len());
    println!("rapid length: {rapid_len:.3} mm");
    println!("cut length: {cut_len:.3} mm");
    if t.is_empty() {
        println!("bounds: none (no moves)");
    } else {
        println!(
            "bounds: X {:.3}..{:.3}  Y {:.3}..{:.3}  Z {:.3}..{:.3}",
            lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]
        );
    }
    println!("prismatic: {}", t.is_prismatic());
    let chained = t.is_chained();
    println!("chained: {chained}");
    if !(chained && t.all_finite()) {
        return die(format!("{path}: the path does not chain or is not finite"));
    }
    ExitCode::SUCCESS
}

fn die(msg: String) -> ExitCode {
    eprintln!("camcraft-cl: {msg}");
    ExitCode::FAILURE
}

/// #163: the library's diagnostics are `tracing` events; print them on
/// stderr, filtered by `CAMCRAFT_LOG`, else `RUST_LOG`, else `info` - the
/// app binaries' rule. A second init is a no-op, never a panic.
fn log_init() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("CAMCRAFT_LOG")
        .or_else(|_| tracing_subscriber::EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).try_init();
}

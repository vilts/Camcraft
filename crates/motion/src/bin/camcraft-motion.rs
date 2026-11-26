//! `camcraft-motion` - headless toolpath generator (M13.3): runs the
//! serpentine facing generator over a rectangle and writes a CLFile.
//! Provenance: facing passes per the `machining2d::face` module
//! (public machining literature); the CLI shape and the CL text dump
//! (`cl::text`) are ours.

use camcraft_motion::cl::text;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{face, FaceHoles, FaceParams};
use std::process::ExitCode;

const USAGE: &str = "\
camcraft-motion - CamCraft toolpath generator (headless)

USAGE:
    camcraft-motion face [OPTIONS] <x0> <y0> <x1> <y1> <tool-diameter> <stepover> <cut-z> <clearance-z>

Faces the rectangle x0..x1, y0..y1 (mm) at cut-z with serpentine
passes and writes the toolpath as a CL text dump (stdout by default).
Exits non-zero on nonsense parameters or an unwritable output.

OPTIONS:
    -o <file>         write the CLFile here instead of stdout
    --feed <mm/min>   cutting feed (default 1000)
    -h, --help        print this text
    --version         print the version
";

fn main() -> ExitCode {
    log_init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out: Option<String> = None;
    let mut feed = 1000.0f64;
    let mut pos: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--version" => {
                println!("camcraft-motion {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "-o" => match it.next() {
                Some(v) => out = Some(v.clone()),
                None => return die("-o needs a value".into()),
            },
            "--feed" => match it.next().map(|v| v.parse::<f64>()) {
                Some(Ok(v)) if v.is_finite() && v > 0.0 => feed = v,
                _ => return die("--feed needs a positive number".into()),
            },
            o if o.starts_with("--") => return die(format!("unknown option {o:?} (try --help)")),
            o => pos.push(o.to_string()),
        }
    }
    let [cmd, rest @ ..] = pos.as_slice() else {
        return die("expected a command (try --help)".into());
    };
    if cmd != "face" {
        return die(format!("unknown command {cmd:?} (try --help)"));
    }
    if rest.len() != 8 {
        return die("face takes eight numbers (try --help)".into());
    }
    let mut n = [0.0f64; 8];
    for (slot, s) in n.iter_mut().zip(rest) {
        match s.parse::<f64>() {
            Ok(v) if v.is_finite() => *slot = v,
            _ => return die(format!("not a finite number: {s:?}")),
        }
    }
    let [x0, y0, x1, y1, d, stepover, cut_z, clearance_z] = n;
    if !(x1 > x0 && y1 > y0) {
        return die("rectangle needs x0 < x1 and y0 < y1".into());
    }
    if clearance_z <= cut_z {
        return die("clearance-z must be above cut-z".into());
    }
    let corners = vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ];
    let region = match Loop::new(corners).and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => return die(format!("bad rectangle: {e}")),
    };
    let p = FaceParams {
        tool_diameter: d,
        stepover,
        clearance_z,
        cut_z,
        top_z: None,
        stepdown: None,
        feed,
        plunge_feed: feed / 3.0,
        climb: true,
        blend_turns: false, blend_radius: None,
        holes: FaceHoles::default(),
        stock_top: None,
    };
    let path = match face(&region, &p) {
        Ok(t) => t,
        Err(e) => return die(e.to_string()),
    };
    let dump = match text::to_text(&path) {
        Ok(s) => s,
        Err(e) => return die(e),
    };
    for note in &path.notes {
        eprintln!("camcraft-motion: note: {note}");
    }
    match out {
        Some(f) => {
            if let Err(e) = std::fs::write(&f, &dump) {
                return die(format!("cannot write {f}: {e}"));
            }
            eprintln!("camcraft-motion: {} moves written to {f}", path.len());
        }
        None => print!("{dump}"),
    }
    ExitCode::SUCCESS
}

fn die(msg: String) -> ExitCode {
    eprintln!("camcraft-motion: {msg}");
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

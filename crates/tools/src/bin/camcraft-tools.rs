//! `camcraft-tools` - headless tool-catalog CLI (M13.3): lists and
//! sanity-checks the built-in catalog, and computes speeds and feeds
//! from the cutting table. Provenance: n = Vc*1000/(pi*D) and
//! vf = fz*z*n are the public machining-handbook formulas the
//! `cutting` module already states; the CLI shape is ours.

use camcraft_tools::cutting::{feed_for, rpm_for};
use camcraft_tools::tools::{catalog, ToolMaterial, WorkMaterial};
use std::process::ExitCode;

const USAGE: &str = "\
camcraft-tools - CamCraft tool catalog (headless)

USAGE:
    camcraft-tools list
    camcraft-tools speeds <work-material> <tool-material> <diameter-mm> <flutes>

COMMANDS:
    list      print the built-in catalog; exits non-zero if any tool
              fails its sanity check
    speeds    spindle speed and feed window from the cutting table

    work materials: aluminium_6061 mild_steel stainless_304
                    grey_cast_iron brass plastics_pom
                    softwood hardwood mdf plywood acrylic hdpe
    tool materials: hss carbide

OPTIONS:
    -h, --help        print this text
    --version         print the version
";

fn main() -> ExitCode {
    log_init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => die("expected a command (try --help)".into()),
        Some("-h" | "--help") => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("--version") => {
            println!("camcraft-tools {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("list") if args.len() == 1 => list(),
        Some("speeds") if args.len() == 5 => speeds(&args[1], &args[2], &args[3], &args[4]),
        Some("list" | "speeds") => die("wrong number of arguments (try --help)".into()),
        Some(other) => die(format!("unknown command {other:?} (try --help)")),
    }
}

fn die(msg: String) -> ExitCode {
    eprintln!("camcraft-tools: {msg}");
    ExitCode::FAILURE
}

fn list() -> ExitCode {
    let mut bad = 0usize;
    for (name, t) in catalog() {
        let ok = t.is_sane();
        bad += usize::from(!ok);
        println!(
            "{name:<28} {:<10} D{:<8.3} flute {:<8.3} {}",
            format!("{:?}", t.kind),
            t.diameter,
            t.flute_len,
            if ok { "ok" } else { "INSANE" }
        );
    }
    if bad > 0 {
        return die(format!("{bad} catalog tool(s) fail the sanity check"));
    }
    ExitCode::SUCCESS
}

fn speeds(work: &str, tool: &str, d: &str, z: &str) -> ExitCode {
    let Some(wm) = WorkMaterial::from_word(work) else {
        return die(format!("unknown work material {work:?} (try --help)"));
    };
    let Some(tm) = ToolMaterial::from_word(tool) else {
        return die(format!("unknown tool material {tool:?} (try --help)"));
    };
    let d = match d.parse::<f64>() {
        Ok(v) if v.is_finite() && v > 0.0 && v <= 1000.0 => v,
        _ => return die(format!("diameter must be a number in (0, 1000] mm, got {d:?}")),
    };
    let z = match z.parse::<u32>() {
        Ok(v) if (1..=64).contains(&v) => v,
        _ => return die(format!("flutes must be an integer in 1..=64, got {z:?}")),
    };
    let w = wm.window(tm);
    let s = wm.chip_scale(d);
    let (fz0, fz1) = (w.fz10.0 * s, w.fz10.1 * s);
    let (n0, n1) = (rpm_for(w.vc.0, d), rpm_for(w.vc.1, d));
    println!("material: {} / {}", wm.word(), tm.label());
    println!("vc: {:.0}..{:.0} m/min", w.vc.0, w.vc.1);
    println!("fz: {fz0:.4}..{fz1:.4} mm/tooth");
    println!("rpm: {n0:.0}..{n1:.0}");
    println!("feed: {:.0}..{:.0} mm/min", feed_for(fz0, z, n0), feed_for(fz1, z, n1));
    ExitCode::SUCCESS
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

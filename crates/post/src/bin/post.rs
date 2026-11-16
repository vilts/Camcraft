//! `post` - our command-line post processor (our own post.exe).
//!
//! Provenance: the CLI shape (`post [OPTIONS] <config> <intermediate>
//! <output>`, plus machine and log paths, with counted warnings
//! reported rather than fatal) follows the published Fusion 360 post
//! engine's documented behavior; this Rust, our flag spellings, and
//! our output text are ours. Development tool and CI check runner:
//! the in-app validation gate (M1.7b) is what refuses a machine.

use camcraft_post::post::{self, engine};
use serde_json::Value;
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "\
post - CamCraft post processor (clean-room)

USAGE:
    post [OPTIONS] <config.cps> <intermediate.cnc|.cl> <output.nc>

ARGS:
    <config>          post script (.cps) to run
    <intermediate>    input stream (.cnc, or any golden .cl dump)
    <output>          program file to write

OPTIONS:
    --machine <mch>   machine definition (.mch JSON)
    --log <file>      write the warning list here instead of stderr
    --trace <file>    write the block-by-block trace here (stage D)
    --units <mm|inch> the program's unit (#902; default mm)
    -h, --help        print this text
    --version         print the version
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut machine: Option<String> = None;
    let mut log: Option<String> = None;
    let mut trace: Option<String> = None;
    let mut units: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--version" => {
                println!("post {} (boa_engine)", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--machine" | "--log" | "--trace" | "--units" => {
                let slot = match a.as_str() {
                    "--machine" => &mut machine,
                    "--log" => &mut log,
                    "--units" => &mut units,
                    _ => &mut trace,
                };
                match it.next() {
                    Some(v) => *slot = Some(v.clone()),
                    None => {
                        eprintln!("post: {a} needs a value");
                        return ExitCode::FAILURE;
                    }
                }
            }
            other if other.starts_with("--") => {
                eprintln!("post: unknown option {other:?} (try --help)");
                return ExitCode::FAILURE;
            }
            other => positional.push(other.to_string()),
        }
    }
    if positional.len() != 3 {
        eprintln!("post: expected exactly three paths (try --help)");
        return ExitCode::FAILURE;
    }
    run(
        &positional[0],
        &positional[1],
        &positional[2],
        machine.as_deref(),
        log.as_deref(),
        trace.as_deref(),
        units.as_deref(),
    )
}

/// The machine value the engine posts with (#996): a vendor kinematics
/// tree (`kinematics.default.parts`, every shipped `.mch`) is read the
/// way the app reads it - parsed, then mirrored into the static grammar
/// the script's `machine` global and the invert pass walk
/// (`post::machine::mirror`, which carries the file's inverted axes as
/// `invertedAxes`). A file already in the static grammar has no tree to
/// mirror and posts as written, as it always did.
fn script_machine(raw: Value, parsed: &camcraft_post::machine::Machine) -> Value {
    if raw.pointer("/kinematics/default/parts").is_some() {
        post::machine::mirror(parsed)
    } else {
        raw
    }
}

fn die(msg: String) -> ExitCode {
    eprintln!("post: {msg}");
    ExitCode::FAILURE
}

fn run(
    config: &str,
    input: &str,
    output: &str,
    machine: Option<&str>,
    log: Option<&str>,
    trace: Option<&str>,
    units: Option<&str>,
) -> ExitCode {
    let cps = match std::fs::read_to_string(config) {
        Ok(s) => s,
        Err(e) => return die(format!("cannot read {config}: {e}")),
    };
    let stream = match std::fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => return die(format!("cannot read {input}: {e}")),
    };
    let mch: Value = match machine {
        Some(path) => {
            let text = match std::fs::read_to_string(path) {
                Ok(t) => t,
                Err(e) => return die(format!("cannot read {path}: {e}")),
            };
            // #373/#374: a file-form machine stating an inverted axis the
            // post cannot translate (Z, any letter but X/Y), or a garbled
            // direction, refuses by name - never standard-sign code for it.
            // An inverted X/Y posts: the engine mirrors the stream.
            let parsed = match camcraft_post::machine::parse_mch(&text) {
                Ok(m) => {
                    if let Some(why) = m.inverted_post_refusal() {
                        return die(why);
                    }
                    m
                }
                Err(e) => return die(format!("{path}: {e}")),
            };
            let raw = match post::machine::from_json(&text) {
                Ok(v) => v,
                Err(e) => return die(format!("cannot parse {path}: {e}")),
            };
            script_machine(raw, &parsed)
        }
        None => Value::Null,
    };
    // #902: the document's unit, stated the way the app states it; the
    // engine refuses a word that is not mm or inch by name.
    let options = units.map_or(Value::Null, |u| serde_json::json!({ "Units": u }));
    match engine::run_with(&cps, &mch, &stream, &options) {
        Ok(out) => {
            if let Err(e) = std::fs::write(Path::new(output), &out.program) {
                return die(format!("cannot write {output}: {e}"));
            }
            let lines = out.program.matches('\n').count();
            if !out.warnings.is_empty() {
                let text: String = out
                    .warnings
                    .iter()
                    .map(|w| format!("warning: {w}\n"))
                    .collect();
                match log {
                    Some(path) => {
                        if let Err(e) = std::fs::write(Path::new(path), &text) {
                            return die(format!("cannot write {path}: {e}"));
                        }
                        eprintln!("post: {} warning(s) logged to {path}", out.warnings.len());
                    }
                    None => {
                        eprint!("{text}post: {} warning(s)", out.warnings.len());
                        eprintln!();
                    }
                }
            }
            if let Some(path) = trace {
                // The debugging trace (M3.3 stage D): every block of
                // the program annotated with the toolpath record that
                // wrote it - the whole stream-to-line story in one
                // file. Swallowed records appear owning no lines,
                // visible by name; silent loss is the failure mode.
                let lines: Vec<&str> = out.program.lines().collect();
                let mut t = String::new();
                for b in &out.blocks {
                    match b.record {
                        Some(i) => t.push_str(&format!("; [record {i}] {}\n", b.label)),
                        None => t.push_str(&format!("; {}\n", b.label)),
                    }
                    let lo = b.first.min(lines.len());
                    let hi = b.last.min(lines.len());
                    for (n, ln) in lines.iter().enumerate().skip(lo).take(hi - lo) {
                        t.push_str(&format!("{:>5}  {ln}\n", n + 1));
                    }
                }
                if let Err(e) = std::fs::write(Path::new(path), &t) {
                    return die(format!("cannot write {path}: {e}"));
                }
            }
            println!(
                "post: {input} -> {output}: {} line(s), {} mapped record(s), {} blocks, {} swallowed",
                lines,
                out.line_map.iter().filter(|l| l.record.is_some()).count(),
                out.blocks.len(),
                out.swallowed,
            );
            ExitCode::SUCCESS
        }
        Err(e) => die(e.to_string()),
    }
}

//! The headless door (M5.3 G): `camcraft-cli` is the batch lane's
//! argv - two commands over one host. `script` runs a job file
//! through `script::run`; `process` runs a project through a script
//! the CLI generates (load, generate, post, verify), so the command
//! line speaks no dialect the verb table does not already have. The
//! exit code is the verdict: 0 the job stood, 1 a verb (or the
//! verify report) refused, 2 the command line itself was wrong or a
//! file could not be read. Diagnostics ride the streams the same way
//! every batch lane reads them: logs and the run's answer on stdout,
//! refusal lines on stderr.
//!
//! Provenance: our own CLI shape, specced from this repo's ROADMAP
//! line "CLI: headless project processing and regression runs".
//! Clean-room, public sources only.

use crate::script;

use serde_json::json;
use std::path::{Path, PathBuf};

const USAGE: &str = "\
camcraft-cli - CamCraft headless batch lane (clean-room)

USAGE:
    camcraft-cli script   <job.js>
    camcraft-cli process  <job.camproj> [-o <out.nc>] [--cps <name|file.cps>]
    camcraft-cli mcp      [--connect <host:port>]
    camcraft-cli help
    camcraft-cli version
    camcraft-cli <command> --help    the flags of one command

COMMANDS:
    script    run a job script through the cam verbs (assets/scripts/
              cam.d.ts types them; every world write rides the commands)
    process   headless project processing: load, generate, and - with
              -o - post; verify's verdict is the exit code
    mcp       a Model Context Protocol server on stdio (JSON-RPC 2.0):
              an AI agent drives one headless document through tools;
              with --connect, a stdio bridge to a running app started
              with --mcp-port (the agent drives the open window live)

A clean run prints its logs, then the last verb's JSON answer on
stdout; refusals print on stderr. Exit: 0 stood, 1 refused, 2 usage.
";

/// Per-command usage (`camcraft-cli <command> --help` or `-h`): printed
/// to stdout, exit 0, and the command never runs - `mcp --help` does
/// not start the server.
const SCRIPT_USAGE: &str = "\
camcraft-cli script <job.js>

    run a job script through the cam verbs (assets/scripts/cam.d.ts
    types them; every world write rides the commands)

ARGUMENTS:
    <job.js>          the job file; must end in .js

OPTIONS:
    -h, --help        print this usage and exit 0

Exit: 0 stood, 1 refused, 2 usage.
";

const PROCESS_USAGE: &str = "\
camcraft-cli process <job.camproj> [-o <out.nc>] [--cps <name|file.cps>]

    headless project processing: load, generate, and - with -o - post;
    verify's verdict is the exit code

ARGUMENTS:
    <job.camproj>     the project to bring current

OPTIONS:
    -o, --out <out.nc>      post the program to this file
    --cps <name|file.cps>   the post to use (takes effect with -o)
    -h, --help              print this usage and exit 0

Exit: 0 stood, 1 refused, 2 usage.
";

const MCP_USAGE: &str = "\
camcraft-cli mcp [--connect <host:port>]

    a Model Context Protocol server on stdio (JSON-RPC 2.0): an AI agent
    drives one headless document through tools. Stdout carries the
    protocol, so --help prints here and does not start the server.

OPTIONS:
    --connect <host:port>   bridge stdio to a running app started with
                            --mcp-port (the agent drives the open window)
    -h, --help              print this usage and exit 0

Exit: 0 stood, 1 session ended with an error, 2 usage.
";

/// Every lane that takes `--help`: its name paired with its usage
/// text. The one list `command_usage` reads and the help test walks,
/// so a lane added here is checked and a lane added elsewhere is
/// caught by the test's cross-check against the top-level COMMANDS.
pub const HELP_LANES: [(&str, &str); 3] = [
    ("script", SCRIPT_USAGE),
    ("process", PROCESS_USAGE),
    ("mcp", MCP_USAGE),
];

/// The top-level usage text (`camcraft-cli help`).
pub const TOP_USAGE: &str = USAGE;

/// The usage text for a lane that takes `--help`, or `None` for any
/// other word. Keeps the flag list next to the parser it describes.
fn command_usage(cmd: &str) -> Option<&'static str> {
    HELP_LANES
        .iter()
        .find(|(name, _)| *name == cmd)
        .map(|(_, text)| *text)
}

/// The process lane's generated job: the verbs a headless project
/// run always means, spelled exactly as a hand-written script would.
/// No second code path: the CLI's project processing IS a script run.
fn process_script(proj: &Path, out: Option<&Path>, cps: Option<&str>) -> String {
    let mut opts = json!({ "to": out.map(|p| p.display().to_string()) });
    if let Some(c) = cps {
        opts["cps"] = json!(c);
    }
    let mut s = format!(
        "cam.load({});\ncam.generate();\n",
        json!(proj.display().to_string())
    );
    if out.is_some() {
        s.push_str(&format!("cam.post({opts});\n"));
    }
    s.push_str("cam.verify()");
    s
}

/// One finished run on the streams: logs, then the parked answer
/// (stdout), refusals (stderr). The returned code is the verdict.
fn report(out: &script::ScriptOutcome, post_requested: bool) -> i32 {
    for line in &out.logs {
        println!("{line}");
    }
    for line in &out.errors {
        eprintln!("camcraft-cli: {line}");
    }
    if !out.result.is_null() {
        println!("{}", out.result);
    }
    if !out.ok {
        return 1;
    }
    // No post asked for: the verify report is the verdict, and a
    // stale or gated job does not exit clean (verify is a question,
    // the CLI's contract is the answer).
    if !post_requested && out.result.get("ok") == Some(&json!(false)) {
        eprintln!("camcraft-cli: the job is not clean - see the verify report");
        return 1;
    }
    0
}

/// The argv entry: `main` hands this everything after the program
/// name and exits by its answer. Testable in-process, one parse.
pub fn run(args: &[String]) -> i32 {
    let Some(cmd) = args.first().map(String::as_str) else {
        eprint!("{USAGE}");
        return 2;
    };
    match cmd {
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            0
        }
        "version" | "--version" => {
            println!("camcraft-cli {} (boa_engine)", env!("CARGO_PKG_VERSION"));
            0
        }
        // Any position: `process x.camproj --help` answers too. Checked
        // before the lane parses, so no lane starts (or reads stdin).
        lane => match command_usage(lane) {
            Some(text) if args[1..].iter().any(|a| a == "--help" || a == "-h") => {
                print!("{text}");
                0
            }
            _ => match lane {
                "script" => lane_script(&args[1..]),
                "process" => lane_process(&args[1..]),
                "mcp" => lane_mcp(&args[1..]),
                other => {
                    eprintln!("camcraft-cli: unknown command {other:?} (try help)");
                    2
                }
            },
        },
    }
}

fn read_source(path: &Path) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("camcraft-cli: cannot read {}: {e}", path.display());
            None
        }
    }
}

fn lane_script(args: &[String]) -> i32 {
    let [job] = args else {
        eprintln!("camcraft-cli: script takes exactly one .js file");
        return 2;
    };
    let path = PathBuf::from(job);
    if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("js")) {
        eprintln!("camcraft-cli: {} is not a .js job", path.display());
        return 2;
    }
    let Some(source) = read_source(&path) else { return 2 };
    report(&script::run(&source), false)
}

fn lane_process(args: &[String]) -> i32 {
    let mut proj: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut cps: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" | "--out" => {
                let Some(p) = it.next() else {
                    eprintln!("camcraft-cli: {a} needs a path");
                    return 2;
                };
                out = Some(PathBuf::from(p));
            }
            "--cps" => {
                let Some(c) = it.next() else {
                    eprintln!("camcraft-cli: --cps needs a value");
                    return 2;
                };
                cps = Some(c.clone());
            }
            other if other.starts_with('-') => {
                eprintln!("camcraft-cli: unknown option {other:?} (try help)");
                return 2;
            }
            other => {
                if proj.is_some() {
                    eprintln!("camcraft-cli: process takes one project (extra: {other:?})");
                    return 2;
                }
                proj = Some(PathBuf::from(other));
            }
        }
    }
    let Some(path) = proj else {
        eprintln!("camcraft-cli: process needs a .camproj project");
        return 2;
    };
    if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("camproj")) {
        eprintln!("camcraft-cli: {} is not a .camproj project", path.display());
        return 2;
    }
    let source = process_script(&path, out.as_deref(), cps.as_deref());
    report(&script::run(&source), out.is_some())
}

/// The MCP lane (#128): stdin/stdout are the protocol, so nothing else
/// may print on stdout; a broken pipe or unreadable input ends the
/// session with exit 1 and the reason on stderr.
fn lane_mcp(args: &[String]) -> i32 {
    match args {
        [] => {}
        // #183: bridge stdio to a running app's live session.
        [flag, addr] if flag == "--connect" => {
            return match crate::mcp::bridge(addr, std::io::stdin(), std::io::stdout().lock()) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("camcraft-cli: mcp --connect {addr}: {e}");
                    1
                }
            };
        }
        _ => {
            eprintln!("camcraft-cli: mcp takes no arguments, or --connect <host:port>");
            return 2;
        }
    }
    let stdin = std::io::stdin();
    match crate::mcp::serve(stdin.lock(), std::io::stdout().lock()) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("camcraft-cli: mcp session ended: {e}");
            1
        }
    }
}

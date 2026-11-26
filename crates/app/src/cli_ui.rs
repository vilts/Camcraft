//! `camcraft-cli --ui ...` (#3751): send MCP tool calls to a running
//! CamCraft window instead of a headless document. The window runs each
//! call on its own History, so the change shows at once and Ctrl+Z
//! undoes it. With no window, the lane says so and exits 3; it never
//! falls back to a headless run.
//!
//! Provenance: our own CLI shape over our own UI link
//! (docs/decisions/3751-cli-ui-link.md). Clean-room, public sources only.

use std::path::PathBuf;

use serde_json::Value;

use crate::mcp::{link, LinkError, Select};

pub const UI_USAGE: &str = "\
camcraft-cli --ui [--window <pid|project>] <verb>

    send commands to a running CamCraft window: each call runs on the
    window's own undo history, so the change shows in the window at once
    and Ctrl+Z undoes it. Never falls back to a headless document.

VERBS:
    call <tool> [<json-args>]   one MCP tool call (the tools of
                                `camcraft-cli mcp reference`); prints the
                                tool's JSON answer
    mcp                         a stdio MCP bridge to the window
    windows                     list the link files of the running windows
                                (pid, version, project); a file that a
                                crashed window left shows too

OPTIONS:
    --window <pid|project>      the window to use, by process id or by the
                                project path it has open (needed when more
                                than one window runs)
    -h, --help                  print this usage and exit 0

The windows are found through the link files in the user's folder
(CAMCRAFT_UI_LINK_DIR overrides it). A window started with
CAMCRAFT_UI_LINK=off opens no link.

Exit: 0 stood, 1 refused, 2 usage, 3 no running window, 4 window busy.
";

/// The lane: `args` is everything after `--ui`. The windows are looked
/// for in the user's link folder.
pub fn run(args: &[String]) -> i32 {
    run_in(link::dir(), args)
}

/// [`run`] with the link folder given (`None`: the user has none). The
/// UI tests drive this, with the folder of the window they started.
pub fn run_in(dir: Option<PathBuf>, args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{UI_USAGE}");
        return 0;
    }
    let mut select = Select::Only;
    let mut rest = args;
    if let [flag, value, tail @ ..] = args
        && flag == "--window"
    {
        select = Select::parse(value);
        rest = tail;
    } else if args.first().is_some_and(|a| a == "--window") {
        eprintln!("camcraft-cli: --window needs a process id or a project path");
        return 2;
    }
    let Some(dir) = dir else {
        eprintln!("camcraft-cli: --ui: no user folder to look for windows in (set {})", link::DIR_ENV);
        return 3;
    };
    match rest {
        [verb, tool, json @ ..] if verb == "call" && json.len() <= 1 => {
            let args = match json.first().map(|t| serde_json::from_str::<Value>(t)) {
                None => Value::Object(serde_json::Map::new()),
                Some(Ok(v @ Value::Object(_))) => v,
                Some(Ok(_)) => {
                    eprintln!("camcraft-cli: --ui call {tool}: the arguments must be a JSON object");
                    return 2;
                }
                Some(Err(e)) => {
                    eprintln!("camcraft-cli: --ui call {tool}: the arguments are not JSON: {e}");
                    return 2;
                }
            };
            call(&dir, &select, tool, &args)
        }
        [verb] if verb == "mcp" => {
            match crate::mcp::link_bridge(&dir, &select, std::io::stdin(), std::io::stdout().lock()) {
                Ok(()) => 0,
                Err(e) => fail(&e),
            }
        }
        [verb] if verb == "windows" => windows(&dir),
        _ => {
            eprintln!("camcraft-cli: --ui takes call <tool> [<json-args>], mcp or windows (try --ui --help)");
            2
        }
    }
}

fn fail(e: &LinkError) -> i32 {
    eprintln!("camcraft-cli: --ui: {e}");
    e.exit_code()
}

fn call(dir: &std::path::Path, select: &Select, tool: &str, args: &Value) -> i32 {
    let result = match crate::mcp::link_call(dir, select, tool, args) {
        Ok(r) => r,
        Err(e) => return fail(&e),
    };
    let text = |r: &Value| {
        r["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n")
    };
    if result["isError"] == Value::Bool(true) {
        eprintln!("camcraft-cli: {}", text(&result));
        return 1;
    }
    match result.get("structuredContent") {
        Some(v) => println!("{}", serde_json::to_string_pretty(v).unwrap_or_default()),
        None => println!("{}", text(&result)),
    }
    0
}

fn windows(dir: &std::path::Path) -> i32 {
    let entries = link::entries(dir);
    if entries.is_empty() {
        eprintln!("camcraft-cli: --ui: no running CamCraft window: no link file in {}", dir.display());
        return 3;
    }
    for e in entries {
        let project = e.project.map_or_else(|| "(unsaved)".to_owned(), |p: PathBuf| p.display().to_string());
        println!("{}\t{}\t{}", e.pid, e.app_version, project);
    }
    0
}

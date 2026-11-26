//! `camcraft-cli --skill <folder|claude|copilot> install` (#3751 part 2):
//! write an AI agent skill that teaches this CLI. One source: the
//! hand-written body `cli_skill.md` plus the CLI's own usage texts, so
//! the command reference in the skill is the help text the binary
//! prints and cannot drift from it. Each target only wraps that text in
//! the header its agent reads. An existing file is never replaced
//! without `--force`.
//!
//! Provenance: our own CLI shape and our own skill text
//! (docs/decisions/3751-cli-skill-install.md). Target paths from the
//! public Agent Skills `SKILL.md` layout of Claude Code and the GitHub
//! Copilot repository custom instructions docs. Clean-room.

use std::path::{Path, PathBuf};

pub const SKILL_USAGE: &str = "\
camcraft-cli --skill <folder|claude|copilot> install [--dir <path>] [--user] [--force]

    write an AI agent skill that teaches this CLI: the lanes, the exit
    codes, examples and the full help text. Writes nothing outside the
    path it prints.

TARGETS:
    folder    <dir>/SKILL.md, in the folder that --dir names (needed)
    claude    .claude/skills/camcraft-cli/SKILL.md in the current folder
              (or --dir); with --user, in the home folder instead
    copilot   .github/instructions/camcraft-cli.instructions.md in the
              current folder (or --dir), a Copilot instructions file

OPTIONS:
    --dir <path>      the folder to write into (default: the current folder)
    --user            claude only: install for the user, in ~/.claude/skills
    --force           replace a skill file that is already there
    -h, --help        print this usage and exit 0

Exit: 0 written, 1 refused (the file is there, or a write failed), 2 usage.
";

/// The skill's name: the folder name under `skills/` and the `name:`
/// of the SKILL.md header.
pub const SKILL_NAME: &str = "camcraft-cli";

const SKILL_BODY: &str = include_str!("cli_skill.md");

const DESCRIPTION: &str = "Drive the CamCraft CAM program from the command line with camcraft-cli: \
process a project to G-code, run a job script, serve MCP tools, or send tool calls to an open \
CamCraft window (--ui). Use when the user asks to generate, post or verify a CamCraft project, \
or to automate CamCraft.";

/// Where a skill goes. `Folder` writes into the given folder itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Folder,
    Claude,
    Copilot,
}

impl Target {
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "folder" => Some(Self::Folder),
            "claude" => Some(Self::Claude),
            "copilot" => Some(Self::Copilot),
            _ => None,
        }
    }

    /// The file, relative to the base folder.
    pub fn file(self) -> PathBuf {
        match self {
            Self::Folder => PathBuf::from("SKILL.md"),
            Self::Claude => [".claude", "skills", SKILL_NAME, "SKILL.md"].iter().collect(),
            Self::Copilot => {
                [".github", "instructions", &format!("{SKILL_NAME}.instructions.md")].iter().collect()
            }
        }
    }

    /// The whole file: the target's header, then [`skill_text`].
    pub fn content(self) -> String {
        let head = match self {
            Self::Folder | Self::Claude => {
                // Quoted: the description has ": " in it, which a plain
                // YAML scalar cannot hold. DESCRIPTION has no '"' or '\'.
                format!("---\nname: {SKILL_NAME}\ndescription: \"{DESCRIPTION}\"\n---\n\n")
            }
            Self::Copilot => "---\napplyTo: \"**\"\n---\n\n".to_owned(),
        };
        head + &skill_text()
    }
}

/// The skill text every target shares: the body, then each usage text
/// the CLI prints (`help` and each `<command> --help`).
pub fn skill_text() -> String {
    let mut s = SKILL_BODY.trim_end().to_owned();
    s.push_str("\n\n## Command reference\n\n### camcraft-cli help\n\n```text\n");
    s.push_str(crate::cli::TOP_USAGE);
    s.push_str("```\n");
    for (name, usage) in crate::cli::HELP_LANES {
        s.push_str(&format!("\n### camcraft-cli {name} --help\n\n```text\n{usage}```\n"));
    }
    s
}

/// The lane: `args` is everything after `--skill`.
pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{SKILL_USAGE}");
        return 0;
    }
    let usage = |msg: &str| {
        eprintln!("camcraft-cli: --skill: {msg} (try --skill --help)");
        2
    };
    let (target, rest) = match args {
        [t, verb, rest @ ..] if verb == "install" => match Target::parse(t) {
            Some(target) => (target, rest),
            None => return usage(&format!("unknown target {t:?}; use folder, claude or copilot")),
        },
        _ => return usage("use --skill <folder|claude|copilot> install"),
    };
    let (mut dir, mut user, mut force) = (None, false, false);
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dir" => match it.next() {
                Some(d) => dir = Some(PathBuf::from(d)),
                None => return usage("--dir needs a folder"),
            },
            "--user" => user = true,
            "--force" => force = true,
            other => return usage(&format!("unknown option {other:?}")),
        }
    }
    if user && target != Target::Claude {
        return usage("--user is for the claude target only");
    }
    if user && dir.is_some() {
        return usage("--user and --dir exclude each other");
    }
    let base = match (target, dir, user) {
        (_, Some(d), _) => d,
        (Target::Folder, None, _) => return usage("the folder target needs --dir <path>"),
        (_, None, true) => match home() {
            Some(h) => h,
            None => {
                eprintln!("camcraft-cli: --skill: no home folder (HOME or USERPROFILE is not set)");
                return 1;
            }
        },
        (_, None, false) => PathBuf::from("."),
    };
    match install(target, &base, force) {
        Ok(path) => {
            println!("camcraft-cli: wrote the {SKILL_NAME} skill to {}", path.display());
            0
        }
        Err(e) => {
            eprintln!("camcraft-cli: --skill: {e}");
            1
        }
    }
}

/// Write the target's file under `base`. Refuses, and writes nothing,
/// when the file is there and `force` is off.
pub fn install(target: Target, base: &Path, force: bool) -> Result<PathBuf, String> {
    let path = base.join(target.file());
    if !force && (path.exists() || path.is_symlink()) {
        return Err(format!(
            "{} is already there; nothing was written (use --force to replace it)",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("cannot make {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, target.content()).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(path)
}

fn home() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .iter()
        .filter_map(std::env::var_os)
        .find(|v| !v.is_empty())
        .map(PathBuf::from)
}

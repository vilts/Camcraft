//! The cadmpeg runner (#1873): convert a native CAD file to STEP with the
//! optional external `cadmpeg` program, then hand the STEP text to the
//! STEP importer of this crate. Desktop only: a browser has no processes.
//!
//! The order is fixed: find the program, read its version, run its own
//! `check`, run `convert`, read the STEP file. Each step runs in one
//! private folder (`run::Sandbox`) under a time limit and size limits. A
//! step that fails refuses the import by name; no later step runs and
//! nothing is imported. The file path is one argument of the child, never
//! a part of a shell line.
//!
//! Provenance: our own design, specced from issue #1873. The command line
//! (`--version`, `check --json`, `convert -o --to step --reject-lossy`)
//! and the exit statuses (0 good, 1 negative verdict, 2 operational) are
//! from cadmpeg 0.6.12's public help text and README. No cadmpeg code is
//! copied or linked, and no HSMWorks material was consulted.

mod run;

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{Format, Graded, ImportError, native};
use crate::commands::CommandError;
use run::{Limits, Output, RunError, Sandbox};

/// The oldest cadmpeg that an import accepts.
pub const MIN_VERSION: (u64, u64, u64) = (0, 6, 12);
/// The time limit of `check` and of `convert`, each.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
/// The time limit of the `--version` question.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// What each refusal ends with: the document did not change.
const NOTHING: &str = "nothing was imported";

/// Where the program is and what a conversion may use.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The program's stated path (`ViewPrefs::cadmpeg_path`). It is used
    /// alone when set: a wrong path refuses and does not fall back.
    pub path: Option<PathBuf>,
    /// The folder list to search when no path is stated (`PATH`).
    pub search_path: Option<OsString>,
    /// The time limit of `check` and of `convert`, each.
    pub timeout: Duration,
    /// The most bytes that the converted STEP file may hold.
    pub max_step_bytes: u64,
    /// Where the private folder of a conversion is made.
    pub temp_root: PathBuf,
}

impl Settings {
    /// The settings of this user: the stated path from the preferences
    /// file, the process's `PATH`, the default limits, the temp folder.
    #[must_use]
    pub fn from_prefs() -> Self {
        let stated = crate::prefs::store_path().and_then(|file| crate::prefs::load(&file).0.cadmpeg_path);
        Self {
            path: stated.map(PathBuf::from),
            search_path: std::env::var_os("PATH"),
            timeout: DEFAULT_TIMEOUT,
            max_step_bytes: super::MAX_FILE_BYTES,
            temp_root: std::env::temp_dir(),
        }
    }
}

/// Convert `input` to STEP and import it within `budget` facets. The STEP
/// text passes each guard of the STEP importer, as a STEP file from disk
/// does; a closed shell that meshes open is kept and named
/// (`Graded::open`). `units` is the caller's length unit for converted
/// STEP text that states none (#1874): it is refused, never guessed.
pub fn load_open_within(input: &Path, budget: usize, settings: &Settings, units: Option<crate::units::Unit>) -> Result<Graded, CommandError> {
    let step = to_step(input, settings)?;
    Ok(super::parse_model_open_in(&step, Format::Step, budget, units)?)
}

/// The STEP text that cadmpeg makes from `input`, or a named refusal.
pub fn to_step(input: &Path, settings: &Settings) -> Result<Vec<u8>, CommandError> {
    let exe = resolve(settings, &native::extension(input))?;
    let meta = std::fs::metadata(input).map_err(ImportError::Io)?;
    if !meta.is_file() {
        return Err(CommandError::Needs(format!("{} is not a file; {NOTHING}", input.display())));
    }
    if meta.len() > super::MAX_FILE_BYTES {
        return Err(ImportError::TooLarge.into());
    }
    // An absolute path cannot be read as an option of the child.
    let input = std::path::absolute(input).map_err(ImportError::Io)?;
    let name = input.file_name().map_or_else(|| input.display().to_string(), |n| n.to_string_lossy().into_owned());
    let sandbox = Sandbox::new(&settings.temp_root).map_err(|e| {
        CommandError::Needs(format!("no private folder under {} for the conversion: {e}; {NOTHING}", settings.temp_root.display()))
    })?;
    version(&exe, &sandbox, settings)?;
    check(&exe, &input, &name, &sandbox, settings)?;
    convert(&exe, &input, &name, &sandbox, settings)
}

/// The program to run: the stated path, or the first match on the search
/// path. A missing program names cadmpeg and the file's extension.
fn resolve(settings: &Settings, ext: &str) -> Result<PathBuf, CommandError> {
    if let Some(stated) = &settings.path {
        if !runnable(stated) {
            return Err(CommandError::Needs(format!(
                "the cadmpeg path in the preferences ({}) is not a program file; correct it, or clear it to search PATH",
                stated.display()
            )));
        }
        return std::path::absolute(stated).map_err(|e| ImportError::Io(e).into());
    }
    let program = format!("cadmpeg{}", std::env::consts::EXE_SUFFIX);
    settings
        .search_path
        .as_deref()
        .into_iter()
        .flat_map(std::env::split_paths)
        // An empty entry means the current folder: never searched.
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(&program))
        .find(|candidate| runnable(candidate))
        .ok_or_else(|| {
            CommandError::Needs(format!(
                "install cadmpeg to import .{ext} files: the program was not found on PATH; or set its path in the preferences"
            ))
        })
}

/// A file that the system can start.
fn runnable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// Refuse a program older than [`MIN_VERSION`], or one whose `--version`
/// text states no version.
fn version(exe: &Path, sandbox: &Sandbox, settings: &Settings) -> Result<(), CommandError> {
    let limits = Limits { timeout: settings.timeout.min(PROBE_TIMEOUT), watch: None };
    let out = stage("--version", &limits, run::run(exe, &[OsStr::new("--version")], sandbox, &limits))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next().unwrap_or_default().trim();
    let word = line.rsplit(char::is_whitespace).next().unwrap_or_default();
    let Some((found, pre_release)) = parse_version(word).filter(|_| out.code == Some(0)) else {
        return Err(CommandError::Invalid(format!(
            "{} is not a usable cadmpeg: `--version` printed '{}' ({})",
            exe.display(),
            clip(line),
            exit_text(out.code)
        )));
    };
    if found < MIN_VERSION || (found == MIN_VERSION && pre_release) {
        let (major, minor, patch) = MIN_VERSION;
        return Err(CommandError::Needs(format!(
            "cadmpeg {word} at {} is too old: an import needs {major}.{minor}.{patch} or newer; update cadmpeg",
            exe.display()
        )));
    }
    Ok(())
}

/// `MAJOR.MINOR.PATCH` with an optional `-pre` and `+build` part: the
/// three numbers, and if it is a pre-release.
fn parse_version(word: &str) -> Option<((u64, u64, u64), bool)> {
    let word = word.strip_prefix('v').unwrap_or(word);
    let word = word.split_once('+').map_or(word, |(core, _)| core);
    let (core, pre_release) = word.split_once('-').map_or((word, false), |(core, _)| (core, true));
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some((version, pre_release))
}

/// Run cadmpeg's own check. Its exit status is the verdict (1 on an
/// `error` or `blocking` finding). The JSON report is read as well: a
/// blocking finding refuses with exit status 0 too, and so does a report
/// that cannot be read.
fn check(exe: &Path, input: &Path, name: &str, sandbox: &Sandbox, settings: &Settings) -> Result<(), CommandError> {
    let limits = Limits { timeout: settings.timeout, watch: None };
    let args = [OsStr::new("check"), OsStr::new("--json"), input.as_os_str()];
    let out = stage("check", &limits, run::run(exe, &args, sandbox, &limits))?;
    let blocking = serde_json::from_slice::<serde_json::Value>(&out.stdout).ok().map(|report| blocking_findings(&report));
    let findings = |n: usize| format!("{n} blocking finding{}", if n == 1 { "" } else { "s" });
    if out.code != Some(0) {
        let detail = match blocking {
            Some(n) if n > 0 => findings(n),
            _ => last_line(&out.stderr),
        };
        return Err(CommandError::Needs(format!("cadmpeg check refused {name} ({}): {detail}; {NOTHING}", exit_text(out.code))));
    }
    match blocking {
        Some(0) => Ok(()),
        Some(n) => Err(CommandError::Needs(format!("cadmpeg check has {} for {name}; {NOTHING}", findings(n)))),
        None => Err(CommandError::Needs(format!("cadmpeg check gave no report that can be read for {name}; {NOTHING}"))),
    }
}

/// The findings with severity `error` or `blocking`, in each `findings`
/// list of the report at any depth.
fn blocking_findings(report: &serde_json::Value) -> usize {
    let blocks = |finding: &serde_json::Value| {
        finding
            .get("severity")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|s| s.eq_ignore_ascii_case("error") || s.eq_ignore_ascii_case("blocking"))
    };
    match report {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(key, value)| match value.as_array() {
                Some(list) if key == "findings" => list.iter().filter(|f| blocks(f)).count(),
                _ => blocking_findings(value),
            })
            .sum(),
        serde_json::Value::Array(list) => list.iter().map(blocking_findings).sum(),
        _ => 0,
    }
}

/// Convert to STEP and read the file. An export loss refuses
/// (`--reject-lossy=export`); `--allow-errors` is never given.
fn convert(exe: &Path, input: &Path, name: &str, sandbox: &Sandbox, settings: &Settings) -> Result<Vec<u8>, CommandError> {
    let step = sandbox.path().join("converted.step");
    let max = settings.max_step_bytes;
    let limits = Limits { timeout: settings.timeout, watch: Some((&step, max)) };
    let args = [
        OsStr::new("convert"),
        input.as_os_str(),
        OsStr::new("-o"),
        step.as_os_str(),
        OsStr::new("--to"),
        OsStr::new("step"),
        OsStr::new("--reject-lossy=export"),
    ];
    let out = stage("convert", &limits, run::run(exe, &args, sandbox, &limits))?;
    if out.code != Some(0) {
        return Err(CommandError::Needs(format!(
            "cadmpeg convert refused {name} ({}): {}; {NOTHING}",
            exit_text(out.code),
            last_line(&out.stderr)
        )));
    }
    match run::len(&step) {
        0 => Err(CommandError::Needs(format!("cadmpeg wrote no STEP output for {name}; {NOTHING}"))),
        size if size > max => Err(oversize(max)),
        _ => run::read_bounded(&step, max).map_err(|e| ImportError::Io(e).into()),
    }
}

/// The named refusal of a run that gave no output.
fn stage(what: &str, limits: &Limits<'_>, result: Result<Output, RunError>) -> Result<Output, CommandError> {
    result.map_err(|error| match error {
        RunError::Io(e) => CommandError::Needs(format!("cadmpeg did not run `{what}`: {e}; {NOTHING}")),
        RunError::Timeout => {
            CommandError::Needs(format!("cadmpeg did not finish `{what}` within {:?} and was stopped; {NOTHING}", limits.timeout))
        }
        RunError::Flood => CommandError::Needs(format!(
            "cadmpeg printed more than {} MiB during `{what}` and was stopped; {NOTHING}",
            run::CAPTURE_MAX >> 20
        )),
        RunError::Oversize => oversize(limits.watch.map_or(0, |(_, max)| max)),
    })
}

fn oversize(max: u64) -> CommandError {
    CommandError::Needs(format!("the STEP file that cadmpeg wrote is larger than the import limit of {max} bytes; {NOTHING}"))
}

fn exit_text(code: Option<i32>) -> String {
    code.map_or_else(|| "ended by a signal".to_owned(), |c| format!("exit status {c}"))
}

/// The last line of the child's error text that is not empty, clipped.
fn last_line(stderr: &str) -> String {
    stderr.lines().rev().map(str::trim).find(|l| !l.is_empty()).map_or_else(|| "it gave no message".to_owned(), clip)
}

/// At most 300 characters of `text`: a child's words never flood a message.
fn clip(text: &str) -> String {
    text.chars().take(300).collect()
}

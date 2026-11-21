//! The lane record of `gate batch --lane N/M` and `cargo xtask gate-verdict`: one verdict from the records
//! of all lanes, with each lane's runner, time and result and the red steps by name (#2908).
//! Provenance: issue #2908 (lanes hand back artifacts; one verdict for the rolling issue); reads only the
//! files our own gate writes. Standard library only.

use std::path::{Path, PathBuf};

use crate::exec::Run;
use crate::lane_split::{self, Lane};

/// The file a lane run writes into its `gate-logs/batch-<stamp>/`, beside the step logs.
pub const RECORD: &str = "lane-result.tsv";

/// One step of a lane record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub ok: bool,
    pub secs: u64,
    pub name: String,
    pub note: String,
}

/// What one lane reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub lane: Lane,
    pub runner: String,
    /// `green`, `red` or `void` (the checkout moved, #1201).
    pub result: String,
    pub wall: u64,
    /// The steps of the lane's plan; more than `rows.len()` when a failed build or a void stopped the run.
    pub planned: usize,
    pub rows: Vec<Row>,
    /// Where the record was read from (empty for a record that was not read from a file).
    pub from: String,
}

fn flat(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

/// The record text: `key<TAB>value` lines, then one `step<TAB>ok|FAIL<TAB>secs<TAB>name<TAB>note` line
/// for each step that ran.
pub fn render(r: &Record) -> String {
    let mut out = format!(
        "lane\t{}\nrunner\t{}\nresult\t{}\nwall\t{}\nplanned\t{}\n",
        r.lane.label(),
        flat(&r.runner),
        r.result,
        r.wall,
        r.planned
    );
    for row in &r.rows {
        let state = if row.ok { "ok" } else { "FAIL" };
        out.push_str(&format!("step\t{state}\t{}\t{}\t{}\n", row.secs, flat(&row.name), flat(&row.note)));
    }
    out
}

/// Reads a record text back. A record without its lane, result or planned count is an error: the
/// collector then counts the lane as not reported, never as green.
pub fn parse(text: &str, from: &str) -> Result<Record, String> {
    let (mut lane, mut runner, mut result, mut wall, mut planned) = (None, String::new(), None, 0u64, None);
    let mut rows = Vec::new();
    for l in text.lines() {
        let f: Vec<&str> = l.split('\t').collect();
        match f.as_slice() {
            ["lane", v] => lane = Some(lane_split::parse(v).map_err(|e| format!("{from}: {e}"))?),
            ["runner", v] => runner = (*v).to_string(),
            ["result", v @ ("green" | "red" | "void")] => result = Some((*v).to_string()),
            ["wall", v] => wall = v.parse().unwrap_or(0),
            ["planned", v] => planned = v.parse::<usize>().ok(),
            ["step", state @ ("ok" | "FAIL"), secs, name, note] => rows.push(Row {
                ok: *state == "ok",
                secs: secs.parse().unwrap_or(0),
                name: (*name).to_string(),
                note: (*note).to_string(),
            }),
            _ => return Err(format!("{from}: line not of a lane record: '{l}'")),
        }
    }
    match (lane, result, planned) {
        (Some(lane), Some(result), Some(planned)) => {
            Ok(Record { lane, runner, result, wall, planned, rows, from: from.to_string() })
        }
        _ => Err(format!("{from}: the lane record has no lane, result or planned line")),
    }
}

/// The runner's name: `RUNNER_NAME` (GitHub Actions sets it), else the host name variables, else `unknown`.
pub fn runner_name(var: impl Fn(&str) -> Option<String>) -> String {
    ["RUNNER_NAME", "HOSTNAME", "COMPUTERNAME"]
        .into_iter()
        .filter_map(var)
        .map(|v| v.trim().to_string())
        .find(|v| !v.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

/// Writes the record of a finished lane run into its log dir. A dry run writes nothing. A record that
/// cannot be written is said so: the collector then reports the lane as missing, which is red.
pub fn record(run: &Run, lane: Lane, planned: usize) {
    if run.dry {
        return;
    }
    let rows: Vec<Row> = run
        .done
        .iter()
        .map(|d| Row { ok: d.ok, secs: d.secs, name: d.name.clone(), note: d.note.clone() })
        .collect();
    let result = if run.void.is_some() {
        "void"
    } else if planned > 0 && rows.iter().all(|r| r.ok) && rows.len() == planned {
        "green"
    } else {
        "red"
    };
    let rec = Record {
        lane,
        runner: runner_name(|k| std::env::var(k).ok()),
        result: result.into(),
        wall: run.clock.elapsed().as_secs(),
        planned,
        rows,
        from: String::new(),
    };
    let file = run.log_dir.join(RECORD);
    match std::fs::write(&file, render(&rec)) {
        Ok(()) => println!("lane {} record: {}", lane.label(), file.display()),
        Err(e) => println!("note: could not write {}: {e} (the verdict will count lane {} as missing)", file.display(), lane.label()),
    }
}

/// Each `RECORD` file under `dir`, at most `depth` directories down (a downloaded artifact holds
/// `gate-logs/batch-<stamp>/lane-result.tsv`), sorted.
pub fn find(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut paths: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            if depth > 0 {
                find(&p, depth - 1, out);
            }
        } else if p.file_name().is_some_and(|n| n == RECORD) {
            out.push(p);
        }
    }
}

fn mins(secs: u64) -> String {
    format!("{}.{} min", secs / 60, secs % 60 / 6)
}

/// The verdict text (Markdown) and whether it is green. Green needs one record for each lane `1..=M`,
/// all with the same M, each green and complete. `problems` are the records that could not be read.
pub fn verdict(records: &[Record], problems: &[String]) -> (String, bool) {
    let of = records.iter().map(|r| r.lane.of).max().unwrap_or(0);
    let mut faults: Vec<String> = problems.to_vec();
    let mut table = String::from("| Lane | Runner | Time | Result | Steps | Record |\n|---|---|---|---|---|---|\n");
    let mut red_steps = Vec::new();
    let mut reported = 0;
    for r in records.iter().filter(|r| r.lane.of != of) {
        faults.push(format!("lane {} ({}) is of a different split than the {of} lanes of this verdict", r.lane.label(), r.from));
    }
    for n in 1..=of {
        let label = format!("{n}/{of}");
        let mine: Vec<&Record> = records.iter().filter(|r| r.lane.of == of && r.lane.index == n).collect();
        let [r] = mine.as_slice() else {
            let what = if mine.is_empty() { "no record".to_string() } else { format!("{} records", mine.len()) };
            table.push_str(&format!("| {label} | - | - | {what} | - | - |\n"));
            faults.push(format!("lane {label} has {what}: it must have exactly one"));
            continue;
        };
        reported += 1;
        let ran = r.rows.len();
        table.push_str(&format!(
            "| {label} | {} | {} | {} | {ran} of {} | `{}` |\n",
            r.runner,
            mins(r.wall),
            r.result,
            r.planned,
            r.from
        ));
        for row in r.rows.iter().filter(|row| !row.ok) {
            red_steps.push(format!("lane {label} ({}): {} - {}", r.runner, row.name, row.note));
        }
        if r.result == "void" {
            faults.push(format!("lane {label} is void: the checkout moved during the run"));
        } else if r.planned == 0 {
            faults.push(format!("lane {label} planned no steps: an empty lane is incomplete"));
        } else if ran < r.planned {
            faults.push(format!("lane {label} ran {ran} of {} step(s): the other steps did not run", r.planned));
        } else if r.result != "green" && r.rows.iter().all(|row| row.ok) {
            faults.push(format!("lane {label} reports {} with no red step", r.result));
        }
    }
    if of == 0 {
        faults.push("no lane record was found".into());
    }
    let green = faults.is_empty() && red_steps.is_empty();
    let mut out = format!(
        "## Batch gate verdict: {} ({reported} of {of} lane(s) reported)\n\n{table}",
        if green { "GREEN" } else { "RED" }
    );
    for (title, lines) in [("Red steps", &red_steps), ("Lane faults", &faults)] {
        if !lines.is_empty() {
            out.push_str(&format!("\n{title}:\n"));
            lines.iter().for_each(|l| out.push_str(&format!("- {l}\n")));
        }
    }
    (out, green)
}

/// `cargo xtask gate-verdict <dir>...`: prints the verdict of the lane records under the dirs. Exit 0 when
/// green, 1 when red (a red step, a missing, double, void or unreadable lane).
pub fn run(args: &[String]) -> Result<i32, String> {
    if args.is_empty() || args.iter().any(|a| a.starts_with('-')) {
        return Err("gate-verdict needs one or more directories that hold lane records (gate-logs/, or the downloaded lane artifacts)".into());
    }
    let mut files = Vec::new();
    for a in args {
        let dir = Path::new(a);
        if !dir.is_dir() {
            return Err(format!("gate-verdict: '{a}' is not a directory"));
        }
        find(dir, 6, &mut files);
    }
    let (mut records, mut problems) = (Vec::new(), Vec::new());
    for f in &files {
        let from = f.display().to_string().replace('\\', "/");
        match std::fs::read_to_string(f).map_err(|e| format!("{from}: {e}")).and_then(|t| parse(&t, &from)) {
            Ok(r) => records.push(r),
            Err(e) => problems.push(e),
        }
    }
    let (text, green) = verdict(&records, &problems);
    println!("{text}");
    Ok(i32::from(!green))
}

//! The headless door (M5.3 G): `camcraft-cli`'s contract is the
//! process contract - exit codes, and who prints where. What must
//! stay true: the CLI is thin over the script host (every run here
//! is a `script::run` someone could have typed), usage refuses with
//! 2, a job that refused exits 1, and a job that stood exits 0 and
//! leaves its answer on stdout. The spawns are real processes: CI
//! runs this door, not a Rust call.
//!
//! Gate map (M4.4): covers app::cli, app::cli_skill, app::script.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, History, SetStock};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::project;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

use std::path::PathBuf;
use std::process::{Command, Output};

fn tmp(tag: &str, ext: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-cli-{tag}-{}.{}", std::process::id(), ext));
    let _ = std::fs::remove_file(&p);
    p
}

/// The fixture both CLI lanes share: the scripting suite's document -
/// box stock and one clean facing op - saved so `process` has a
/// project to bring current. If this refuses, the door is fine and
/// the fixture is wrong.
fn fixture_proj() -> World {
    let world = new_world();
    let mut history = History::default();
    let cmd = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("catalog tool"));
    let params = OpParams::Face { holes: Default::default(), blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, cmd.expect("create"));
    world
}

fn cli_str(args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_camcraft-cli"));
    cmd.args(args);
    cmd.output().expect("the CLI binary runs")
}

fn code(o: &Output) -> i32 {
    o.status.code().unwrap_or(-1)
}

#[test]
fn help_and_version_stand_up() {
    let o = cli_str(&["help"]);
    assert_eq!(code(&o), 0);
    assert!(String::from_utf8_lossy(&o.stdout).contains("USAGE"), "help shows the shape");
    let o = cli_str(&["version"]);
    assert_eq!(code(&o), 0);
    assert!(String::from_utf8_lossy(&o.stdout).contains("camcraft-cli"), "version names itself");
}

#[test]
fn no_command_and_strangers_refuse_with_a_two() {
    // 2 is "the command line was wrong", never "the job ran bad".
    let o = cli_str(&[]);
    assert_eq!(code(&o), 2);
    let o = cli_str(&["frobnicate"]);
    assert_eq!(code(&o), 2);
    assert!(String::from_utf8_lossy(&o.stderr).contains("frobnicate"), "refusals name what they met");
    let o = cli_str(&["script", "job.camproj"]);
    assert_eq!(code(&o), 2);
    let o = cli_str(&["process", "project.js"]);
    assert_eq!(code(&o), 2);
    let o = cli_str(&["script", "/no/such/job.js"]);
    assert_eq!(code(&o), 2, "an unreadable file is a usage-side failure");
}

#[test]
fn a_script_lane_prints_logs_then_the_answer() {
    let job = tmp("job", "js");
    std::fs::write(&job, "cam.log(\"one\"); 1 + 2").expect("write job");
    let o = cli_str(&["script", job.to_str().unwrap()]);
    assert_eq!(code(&o), 0, "{:?}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert_eq!(out.trim().lines().collect::<Vec<_>>(), ["one", "3"], "logs, then the parked answer");
}

#[test]
fn a_refused_script_exits_one_with_the_line_on_stderr() {
    let job = tmp("bad", "js");
    std::fs::write(&job, "cam.operations(\"extra\")").expect("write job");
    let o = cli_str(&["script", job.to_str().unwrap()]);
    assert_eq!(code(&o), 1);
    assert!(String::from_utf8_lossy(&o.stderr).contains("operations:"), "the refusal reaches stderr by verb");
}

#[test]
fn process_lane_loads_generates_and_posts_the_project() {
    let proj = tmp("proc", "camproj");
    let world = fixture_proj();
    project::save(&world, &proj).expect("save fixture");
    let nc = tmp("proc", "nc");
    let o = cli_str(&["process", proj.to_str().unwrap(), "-o", nc.to_str().unwrap()]);
    assert_eq!(code(&o), 0, "{:?}", String::from_utf8_lossy(&o.stderr));
    assert!(nc.exists(), "the -o lane posts");
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("\"ok\":true"), "verify's answer is on stdout: {text}");
    assert!(text.contains("\"status\":\"ok\""), "the report speaks a fresh op: {text}");
}

#[test]
fn process_lane_without_o_judges_rather_than_writes() {
    let proj = tmp("judge", "camproj");
    let world = fixture_proj();
    project::save(&world, &proj).expect("save fixture");
    let o = cli_str(&["process", proj.to_str().unwrap()]);
    assert_eq!(code(&o), 0, "a fresh-after-generate job stands");
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("\"gate\""), "the answer is the verify report: {text}");
    // A named post rides the same door: --cps is a post option, not a CLI flag.
    let nc = tmp("judge-haas", "nc");
    let o = cli_str(&["process", proj.to_str().unwrap(), "-o", nc.to_str().unwrap(), "--cps", "Haas"]);
    assert_eq!(code(&o), 0, "{:?}", String::from_utf8_lossy(&o.stderr));
    assert!(nc.exists());
}

#[test]
fn a_post_that_refuses_exits_one_and_says_why() {
    let proj = tmp("refuse", "camproj");
    let world = fixture_proj();
    project::save(&world, &proj).expect("save fixture");
    // A directory that is not there: the post verb refuses, the
    // script stops on the Error, and no verify answer prints.
    let nc = tmp("refuse", "no-such-dir").join("out.nc");
    let o = cli_str(&["process", proj.to_str().unwrap(), "-o", nc.to_str().unwrap()]);
    assert_eq!(code(&o), 1);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("post:"), "the verb names itself: {err}");
    assert!(!o.stdout.contains(&b'{'), "a stopped script parks no answer: {}", String::from_utf8_lossy(&o.stdout));
}

/// Spawns the CLI with `stdin` piped in: a lane that starts serving
/// (mcp) would answer the JSON-RPC line written here on stdout.
fn cli_with_stdin(args: &[&str], stdin: &str) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_camcraft-cli"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the CLI binary runs");
    if let Some(mut pipe) = child.stdin.take() {
        // A lane that exits before reading closes the pipe: not a failure here.
        let _ = pipe.write_all(stdin.as_bytes());
    }
    child.wait_with_output().expect("the CLI finishes")
}

/// The flag and argument names a usage text lists under ARGUMENTS /
/// OPTIONS: the first column of each entry, split on ", " and spaces.
fn listed_names(usage: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut inside = false;
    for line in usage.lines() {
        if line == "ARGUMENTS:" || line == "OPTIONS:" {
            inside = true;
            continue;
        }
        if !line.starts_with(' ') {
            inside = false;
        }
        let entry = line.trim_start();
        if inside && (entry.starts_with('-') || entry.starts_with('<')) {
            // The first column ends at the first run of two spaces.
            let col = entry.split("  ").next().unwrap_or(entry);
            for part in col.split(", ") {
                if let Some(w) = part.split_whitespace().next() {
                    names.push(w.to_string());
                }
            }
        }
    }
    names
}

#[test]
fn every_subcommand_answers_help_and_names_its_flags() {
    // Driven from the lane list the CLI itself dispatches on: a lane
    // added there is checked here with no edit to this test.
    for (cmd, usage) in camcraft_app::cli::HELP_LANES {
        let names = listed_names(usage);
        assert!(names.contains(&"--help".to_string()), "{cmd} usage lists --help");
        for flag in ["--help", "-h"] {
            let o = cli_str(&[cmd, flag]);
            let text = String::from_utf8_lossy(&o.stdout);
            assert_eq!(code(&o), 0, "{cmd} {flag} exits 0: {text}");
            assert!(text.contains(&format!("camcraft-cli {cmd}")), "{cmd} {flag} names its command: {text}");
            assert_eq!(text, usage, "{cmd} {flag} prints the lane's usage");
            for name in &names {
                assert!(text.contains(name.as_str()), "{cmd} {flag} names {name}: {text}");
            }
            assert!(o.stderr.is_empty(), "{cmd} {flag} is quiet on stderr");
        }
    }
}

#[test]
fn every_listed_command_is_a_help_lane() {
    // A command named in the top-level COMMANDS block without a
    // HELP_LANES entry fails here; `help`/`version` take no flags.
    let mut cmds = Vec::new();
    let mut inside = false;
    for line in camcraft_app::cli::TOP_USAGE.lines() {
        if line == "COMMANDS:" {
            inside = true;
            continue;
        }
        if inside
            && line.starts_with("    ")
            && !line.starts_with("     ")
            && let Some(w) = line.split_whitespace().next()
        {
            cmds.push(w.to_string());
        }
    }
    assert!(!cmds.is_empty(), "the COMMANDS block was parsed");
    for c in cmds {
        assert!(
            camcraft_app::cli::HELP_LANES.iter().any(|(n, _)| *n == c),
            "command {c} is listed but has no HELP_LANES entry"
        );
    }
}

#[test]
fn help_flag_answers_anywhere_in_the_argv() {
    // The project does not exist: the help answer must not read it.
    let o = cli_str(&["process", "/no/such/project.camproj", "--help"]);
    assert_eq!(code(&o), 0);
    assert!(String::from_utf8_lossy(&o.stdout).contains("--cps"), "process help names --cps");
}

#[test]
fn mcp_help_does_not_start_the_server() {
    // A live server would answer this initialize line on stdout.
    let init = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n";
    let o = cli_with_stdin(&["mcp", "--help"], init);
    assert_eq!(code(&o), 0);
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(!text.contains("jsonrpc"), "help must not speak the protocol: {text}");
    assert!(text.contains("--connect"), "mcp help names --connect");
    // Control: the same line, no help, is answered - so the check above can fail.
    let served = cli_with_stdin(&["mcp"], init);
    assert!(String::from_utf8_lossy(&served.stdout).contains("jsonrpc"), "a plain mcp answers the line");
}

#[test]
fn unknown_flags_still_refuse_with_a_two_and_help_lists_per_command() {
    assert_eq!(code(&cli_str(&["process", "p.camproj", "--bogus"])), 2);
    assert_eq!(code(&cli_str(&["script", "--bogus"])), 2);
    assert_eq!(code(&cli_str(&["mcp", "--bogus"])), 2);
    let o = cli_str(&["help"]);
    assert!(String::from_utf8_lossy(&o.stdout).contains("<command> --help"), "top-level help points at per-command help");
}

#[test]
fn script_lane_accepts_camscript_extension() {
    let job = tmp("job", "camscript");
    std::fs::write(&job, "cam.log('camscript'); 6 * 7").expect("write camscript");
    let o = cli_str(&["script", job.to_str().unwrap()]);
    let _ = std::fs::remove_file(&job);
    assert_eq!(code(&o), 0, "{:?}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert_eq!(out.trim().lines().collect::<Vec<_>>(), ["camscript", "42"]);
}

/// A fresh, empty folder for one `--skill` test.
fn skill_dir(tag: &str) -> PathBuf {
    let d = tmp(&format!("skill-{tag}"), "d");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("temp folder");
    d
}

/// `camcraft-cli --skill ...` in `cwd`, with HOME/USERPROFILE at `home`.
fn skill_cli(cwd: &std::path::Path, home: &std::path::Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_camcraft-cli"));
    cmd.arg("--skill").args(args).current_dir(cwd).env("HOME", home).env("USERPROFILE", home);
    cmd.output().expect("the CLI binary runs")
}

fn read(p: PathBuf) -> String {
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{} was written: {e}", p.display()))
}

#[test]
fn skill_folder_target_writes_skill_md_into_the_given_folder() {
    let d = skill_dir("folder");
    let out = d.join("my-skills");
    let o = skill_cli(&d, &d, &["folder", "install", "--dir", out.to_str().unwrap()]);
    assert_eq!(code(&o), 0, "{}", String::from_utf8_lossy(&o.stderr));
    let text = read(out.join("SKILL.md"));
    assert!(text.starts_with("---\nname: camcraft-cli\ndescription: \""), "a SKILL.md header: {text}");
    assert!(text.contains("## Command reference"), "the help text rides along");
    assert_eq!(code(&skill_cli(&d, &d, &["folder", "install"])), 2, "folder needs --dir");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn skill_claude_target_writes_the_project_skill_or_with_user_the_home_skill() {
    let (proj, home) = (skill_dir("claude-proj"), skill_dir("claude-home"));
    let o = skill_cli(&proj, &home, &["claude", "install"]);
    assert_eq!(code(&o), 0, "{}", String::from_utf8_lossy(&o.stderr));
    let text = read(proj.join(".claude/skills/camcraft-cli/SKILL.md"));
    assert!(text.starts_with("---\nname: camcraft-cli\n"), "{text}");
    assert!(!home.join(".claude").exists(), "the project scope writes nothing in the home folder");
    let o = skill_cli(&proj, &home, &["claude", "install", "--user"]);
    assert_eq!(code(&o), 0, "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(read(home.join(".claude/skills/camcraft-cli/SKILL.md")), text, "one text for both scopes");
    for d in [proj, home] {
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn skill_copilot_target_writes_an_instructions_file_for_all_paths() {
    let d = skill_dir("copilot");
    let o = skill_cli(&d, &d, &["copilot", "install"]);
    assert_eq!(code(&o), 0, "{}", String::from_utf8_lossy(&o.stderr));
    let text = read(d.join(".github/instructions/camcraft-cli.instructions.md"));
    assert!(text.starts_with("---\napplyTo: \"**\"\n---\n"), "{text}");
    assert!(!d.join(".github/copilot-instructions.md").exists(), "the user's own file is not touched");
    assert_eq!(code(&skill_cli(&d, &d, &["copilot", "install", "--user"])), 2, "--user is claude only");
    assert_eq!(code(&skill_cli(&d, &d, &["cursor", "install"])), 2, "an unknown target refuses");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn skill_install_never_replaces_a_file_without_force() {
    let d = skill_dir("force");
    let file = d.join(".claude/skills/camcraft-cli/SKILL.md");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "the user's own text").unwrap();
    let o = skill_cli(&d, &d, &["claude", "install"]);
    assert_eq!(code(&o), 1, "an existing file refuses");
    assert!(String::from_utf8_lossy(&o.stderr).contains("--force"), "the refusal names the way out");
    assert_eq!(read(file.clone()), "the user's own text", "nothing was written");
    assert_eq!(code(&skill_cli(&d, &d, &["claude", "install", "--force"])), 0);
    assert!(read(file).contains("## Command reference"), "--force replaces it");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn skill_text_names_every_cli_lane_and_no_other() {
    // Drift test: the installed skill names each lane the CLI dispatches,
    // and each `camcraft-cli <word>` it names is a lane that exists.
    let text = camcraft_app::cli_skill::Target::Claude.content();
    for (lane, _) in camcraft_app::cli::HELP_LANES {
        assert!(text.contains(&format!("camcraft-cli {lane}")), "the skill names {lane}");
    }
    let known: Vec<&str> = camcraft_app::cli::HELP_LANES.iter().map(|(n, _)| *n).chain(["help", "version"]).collect();
    for rest in text.split("camcraft-cli ").skip(1) {
        let word = rest.split(|c: char| c.is_whitespace() || c == '`').next().unwrap_or("");
        if word.starts_with(|c: char| c.is_ascii_lowercase()) || word.starts_with("--") {
            assert!(known.contains(&word), "the skill names a lane that does not exist: {word:?}");
        }
    }
}

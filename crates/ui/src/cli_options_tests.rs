//! Pins for #2082: the help text, the table and the parser agree.

use crate::cli_options::{files, help_text, parse, read_only, Parsed, ENV_VARS, OPTIONS};

fn argv(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn help_names_each_option_and_variable_of_the_table() {
    let text = help_text();
    for o in OPTIONS {
        assert!(text.contains(o.flag), "help misses {}", o.flag);
        if let Some(s) = o.short {
            assert!(text.contains(s), "help misses {s}");
        }
        if let Some(e) = o.env {
            assert!(text.contains(e), "help misses {e}");
        }
        assert!(text.contains(o.help), "help misses the line of {}", o.flag);
    }
    for e in ENV_VARS {
        assert!(text.contains(e.name), "help misses {}", e.name);
    }
}

#[test]
fn each_flag_the_readers_use_is_in_the_table() {
    // The flags that the owners of each value read, spelled as they spell them.
    for flag in ["--gpu-backend", "--gpu", "--mcp-port", "--headless", "--ui", "--read-only"] {
        assert!(OPTIONS.iter().any(|o| o.flag == flag), "{flag} is not in the table");
    }
    for env in ["CAMCRAFT_GPU_BACKEND", "CAMCRAFT_GPU", "CAMCRAFT_MCP_PORT"] {
        assert!(OPTIONS.iter().any(|o| o.env == Some(env)), "{env} is not in the table");
    }
}

#[test]
fn each_table_option_is_accepted_by_the_parser() {
    for o in OPTIONS {
        let args = match o.value {
            Some(_) => argv(&[o.flag, "x"]),
            None => argv(&[o.flag]),
        };
        assert!(parse(&args).is_ok(), "{} refused", o.flag);
    }
}

#[test]
fn help_and_version_are_found_in_any_place() {
    assert_eq!(parse(&argv(&["-h"])), Ok(Parsed::Help));
    assert_eq!(parse(&argv(&["--headless", "--help"])), Ok(Parsed::Help));
    assert_eq!(parse(&argv(&["--version"])), Ok(Parsed::Version));
    assert_eq!(parse(&argv(&["--gpu", "quadro", "--mcp-port=7000"])), Ok(Parsed::Run));
    assert_eq!(parse(&argv(&[])), Ok(Parsed::Run));
}

#[test]
fn unknown_option_gives_an_error_that_names_it() {
    let err = parse(&argv(&["--gpu-bakend", "dx12"])).unwrap_err();
    assert!(err.contains("--gpu-bakend") && err.contains("--help"), "{err}");
    let err = parse(&argv(&["--bogus=1"])).unwrap_err();
    assert!(err.contains("--bogus"), "{err}");
    assert!(parse(&argv(&["-x"])).is_err());
    assert!(parse(&argv(&["--headless=yes"])).unwrap_err().contains("takes no value"));
}

#[test]
fn read_only_is_a_switch_that_keeps_the_file_after_it() {
    // #3609: the switch takes no value, so the next argument stays a file.
    let args = argv(&["--read-only", "job.camproj"]);
    assert_eq!(parse(&args), Ok(Parsed::Run));
    assert!(read_only(&args));
    assert_eq!(files(&args), ["job.camproj"]);
    assert!(read_only(&argv(&["job.camproj", "--read-only"])), "in any place");
    assert!(!read_only(&argv(&["job.camproj"])), "off by default");
    assert!(parse(&argv(&["--read-only=yes"])).unwrap_err().contains("takes no value"));
    assert!(!read_only(&argv(&["--read-only", "--bogus"])), "refused arguments open nothing");
}

#[test]
fn run_refuses_unknown_option_with_code_2_and_opens_no_window() {
    assert_eq!(crate::run(&argv(&["--nonsense"])), 2);
    assert_eq!(crate::run(&argv(&["--help"])), 0);
    assert_eq!(crate::run(&argv(&["--version"])), 0);
}

#[test]
fn start_mode_flags_are_read_and_minimized_implies_no_activate() {
    use crate::cli_options::{start_mode, StartMode};
    assert_eq!(start_mode(&argv(&[])), StartMode::default());
    assert_eq!(start_mode(&argv(&["a.camproj"])), StartMode::default());
    assert_eq!(start_mode(&argv(&["--no-activate", "a.camproj"])), StartMode { minimized: false, no_activate: true });
    assert_eq!(start_mode(&argv(&["--minimized"])), StartMode { minimized: true, no_activate: true });
    // A flag is not a file argument.
    assert_eq!(crate::cli_options::files(&argv(&["--minimized", "a.camproj"])), vec!["a.camproj".to_owned()]);
    // A refused command line gives the normal start.
    assert_eq!(start_mode(&argv(&["--minimized", "--bogus"])), StartMode::default());
}

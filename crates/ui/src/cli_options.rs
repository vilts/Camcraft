//! The start options of `camcraft-ui` (#2082): one table lists each option
//! and each environment variable. The checker (`parse`) and the `--help`
//! text (`help_text`) read that table, so they cannot disagree.
//! Provenance: our own design, specced from issue #2082; the option shapes
//! follow the common `--flag value` / `--flag=value` command-line habit.
//! The value checks stay with the owners of each option (`egui_app::gpu`,
//! `camcraft_app::mcp::port_from`); this module only knows the names.

/// One start option.
pub struct Opt {
    /// The long name, with the leading dashes.
    pub flag: &'static str,
    /// An optional short name (`-h`).
    pub short: Option<&'static str>,
    /// The value form for the help line, or `None` for a switch.
    pub value: Option<&'static str>,
    /// The environment variable that gives the same choice, if any.
    pub env: Option<&'static str>,
    /// What the program does when the option is absent.
    pub default: &'static str,
    /// One plain sentence.
    pub help: &'static str,
}

/// Every option that the program reads. A new option goes here first.
pub const OPTIONS: &[Opt] = &[
    Opt {
        flag: "--help",
        short: Some("-h"),
        value: None,
        env: None,
        default: "off",
        help: "Print this text and exit. No window opens.",
    },
    Opt {
        flag: "--version",
        short: None,
        value: None,
        env: None,
        default: "off",
        help: "Print the program version and exit. No window opens.",
    },
    Opt {
        flag: "--headless",
        short: None,
        value: None,
        env: None,
        default: "off",
        help: "Start with no window. Run a short self-check of the 3D view and exit.",
    },
    Opt {
        flag: "--ui",
        short: None,
        value: Some("<name>"),
        env: None,
        default: "egui",
        help: "Choose the user interface. Only \"egui\" exists.",
    },
    Opt {
        flag: "--gpu-backend",
        short: None,
        value: Some("<dx12|metal|vulkan|gl>"),
        env: Some("CAMCRAFT_GPU_BACKEND"),
        default: "automatic",
        help: "Try this graphics system first. The other systems stay as a fallback. Also accepted: d3d12 (dx12), gles and opengl (gl).",
    },
    Opt {
        flag: "--gpu",
        short: None,
        value: Some("<adapter name>"),
        env: Some("CAMCRAFT_GPU"),
        default: "automatic",
        help: "Use the graphics card whose name holds this text. Capitals do not matter.",
    },
    Opt {
        flag: "--mcp-port",
        short: None,
        value: Some("<port>"),
        env: Some("CAMCRAFT_MCP_PORT"),
        default: "off",
        help: "Open the MCP server for AI agents on this local port (0 to 65535).",
    },
];

/// An environment variable that has no option of its own.
pub struct EnvVar {
    pub name: &'static str,
    pub value: &'static str,
    pub default: &'static str,
    pub help: &'static str,
}

/// The environment variables that have no option. The variables of an
/// option are in [`OPTIONS`].
pub const ENV_VARS: &[EnvVar] = &[
    EnvVar {
        name: "CAMCRAFT_LOG",
        value: "<filter>",
        default: "info, with quiet graphics libraries",
        help: "How much the log says, for example debug. RUST_LOG is the fallback.",
    },
    EnvVar {
        name: "CAMCRAFT_CONFIG_DIR",
        value: "<folder>",
        default: "the user settings folder of the system",
        help: "Folder for the list of recent files.",
    },
    EnvVar {
        name: "CAMCRAFT_MACHINES",
        value: "<folder>",
        default: "camcraft/machines in the user data folder",
        help: "Folder for the user machine files.",
    },
    EnvVar {
        name: "CAMCRAFT_FIXTURES",
        value: "<folder>",
        default: "the user data folder",
        help: "Folder for the user fixture library.",
    },
    EnvVar {
        name: "CAMCRAFT_THEMES",
        value: "<folder>",
        default: "assets/themes next to the program",
        help: "Folder that holds the theme files.",
    },
];

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Parsed {
    /// Start as usual (the owners of each option check its value).
    Run,
    /// Print the help text and exit with code 0.
    Help,
    /// Print the version and exit with code 0.
    Version,
}

fn find(name: &str) -> Option<&'static Opt> {
    OPTIONS.iter().find(|o| o.flag == name || o.short == Some(name))
}

/// Check each argument against [`OPTIONS`]. An unknown option is an error
/// that names it and points to `--help`; the program never ignores it.
/// An argument without a leading dash is not an option and passes.
pub fn parse(args: &[String]) -> Result<Parsed, String> {
    let (mut help, mut version) = (false, false);
    let mut it = args.iter().peekable();
    while let Some(arg) = it.next() {
        if !arg.starts_with('-') || arg == "-" {
            continue;
        }
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n, Some(v)),
            _ => (arg.as_str(), None),
        };
        let Some(opt) = find(name) else {
            return Err(format!("unknown option '{name}'. Run camcraft-ui --help to see the options."));
        };
        match (opt.value, inline) {
            (None, Some(_)) => {
                return Err(format!("option '{}' takes no value. Run camcraft-ui --help to see the options.", opt.flag));
            }
            // The value follows. Its owner checks it; a value that looks
            // like an option is left to be read as one.
            (Some(_), None) if it.peek().is_some_and(|n| !n.starts_with('-')) => {
                it.next();
            }
            _ => {}
        }
        match opt.flag {
            "--help" => help = true,
            "--version" => version = true,
            _ => {}
        }
    }
    Ok(if help {
        Parsed::Help
    } else if version {
        Parsed::Version
    } else {
        Parsed::Run
    })
}

/// The text of `--help`, built from the tables.
pub fn help_text() -> String {
    let mut out = String::from("camcraft-ui - a CAM program that turns 3D models into NC programs\n\n");
    out.push_str("Usage: camcraft-ui [options]\n\nOptions:\n");
    for o in OPTIONS {
        let mut head = String::new();
        if let Some(s) = o.short {
            head.push_str(s);
            head.push_str(", ");
        }
        head.push_str(o.flag);
        if let Some(v) = o.value {
            head.push(' ');
            head.push_str(v);
        }
        out.push_str(&format!("  {head}\n      {}\n", o.help));
        if let Some(e) = o.env {
            out.push_str(&format!("      Environment variable: {e}. The option wins over the variable.\n"));
        }
        out.push_str(&format!("      Default: {}\n", o.default));
    }
    out.push_str("\nEnvironment variables:\n");
    for o in OPTIONS {
        if let (Some(e), Some(v)) = (o.env, o.value) {
            out.push_str(&format!("  {e} {v}\n      Same as {}.\n", o.flag));
        }
    }
    for e in ENV_VARS {
        out.push_str(&format!("  {} {}\n      {}\n      Default: {}\n", e.name, e.value, e.help, e.default));
    }
    out.push_str("\nA value can follow as --option value or as --option=value.\n");
    out
}

/// The text of `--version`.
pub fn version_text() -> String {
    format!("camcraft-ui {}", crate::build_info::VERSION)
}

# Starting CamCraft: options and environment variables

Run `camcraft-ui --help` to print every start option and every environment
variable. No window opens. `camcraft-ui --version` prints the version.

An option can follow as `--option value` or as `--option=value`. An option
wins over its environment variable. An option that the program does not know
gives an error that names it. The program does not start in that case.

## Options

| Option | Value | Environment variable | Default | What it does |
|---|---|---|---|---|
| `--help`, `-h` | none | none | off | Print the help text and exit. |
| `--version` | none | none | off | Print the version and exit. |
| `--headless` | none | none | off | Run with no window: a short self-check of the 3D view. With a file, the run opens that file and draws it in place of the sample; a file that does not open fails the run. |
| `--ui` | `<name>` | none | `egui` | Choose the interface. Only `egui` exists. |
| `--gpu-backend` | `dx12`, `metal`, `vulkan` or `gl` | `CAMCRAFT_GPU_BACKEND` | automatic | Try this graphics system first. |
| `--gpu` | part of an adapter name | `CAMCRAFT_GPU` | automatic | Use the graphics card whose name holds this text. |
| `--mcp-port` | port number | `CAMCRAFT_MCP_PORT` | off | Open the MCP server for AI agents on this local port. |
| `--no-activate` | none | none | off | Show the window but do not make it the foreground window. The window that has the focus keeps it. |
| `--minimized` | none | none | off | Start with the window minimized. It does not take the focus and it covers no other window. This also sets `--no-activate`. |
| `--read-only` | none | none | off | Open the project file read-only. Save does not write over the file: it opens Save As, which writes your own copy. The title bar says read-only. Changes are allowed. A model file is imported as usual. |

## Environment variables without an option

| Variable | Default | What it does |
|---|---|---|
| `CAMCRAFT_LOG` | `info` | How much the log says. `RUST_LOG` is the fallback. |
| `CAMCRAFT_CONFIG_DIR` | user settings folder | Folder for the list of recent files. |
| `CAMCRAFT_MACHINES` | user data folder | Folder for the user machine files. |
| `CAMCRAFT_FIXTURES` | user data folder | Folder for the user fixture library. |
| `CAMCRAFT_THEMES` | `assets/themes` next to the program | Folder that holds the theme files. |

The table of options is in `crates/ui/src/cli_options.rs`. The help text and
the option check both read that table.

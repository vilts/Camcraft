//! The egui shell (M1.4.2 migration), selected with `--ui egui`.
//!
//! Graduated from the M1.4.1 spike binary: `shell` runs the loop, `world`
//! collects the scene through the shared `crate::interact` law and holds the
//! demo seed, `theme` carries the color tokens, `painted` is the draw-list
//! port (ribbon cells, tree rows), `panes` holds the keyboard layer and the
//! painted ribbon and browser (M-b), and `pages` the property pages (M-c).
//! Since M-d the window opens on a plain empty document - `world::seed` is
//! what the harnesses and the headless proof call directly. Since M-e the
//! viewport takes pointer gestures: `shell`'s viewport panel drives the same
//! `crate::interact` pick, hover and camera functions the imgui shell does,
//! from egui's own input state. Since M-f the surfaces here fire commands
//! through the shared `crate::actions` layer - the entry every surface,
//! here and (until it retired) on the imgui side, calls - so the effects
//! live once. At M-g imgui retired and this is the shell
//! (docs/spike-egui-go-no-go.md, migration law); the generators, commands
//! and persistence underneath were shared and untouched throughout. Since
//! M1.5 the viewport wears its corner furniture - `viewcube` owns the
//! cube's drawing, its face/edge/corner pick law and the nav bar, and
//! keeps the gesture-eating rule (a press on the furniture arms nothing)
//! in one place.
//! Provenance: our own code, from our own spike.

// pub(crate) for its own unit tests and the shell's init path: the M4.2
// GPU backend pick - parse, honest fallback chain, adapter log line.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod frame_watch;
pub(crate) mod gpu;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod gpu_init;
pub(crate) mod gpu_select;
// pub: the M6.1 D readability golden reads it beside the two built-in
// columns, and `theme::visuals` + `theme_files::parse` + `painted`'s
// glyph tint are the consumers (ratio, on-fill, the follow law, the gate).
pub mod contrast;
mod boundary;
mod gcode_highlight;
mod gcode_view;
mod debug_info;
mod status_alert;

mod stock_info;
// pub: M13.4's ColorVision layer - the viewport, the traced stock and
// the overlay lines read the palette; `cvd` is its maths.
pub mod colorvision;
pub mod ground_ink;
pub mod window_chrome;
mod post_options;
pub mod cvd;
mod icons;
mod op_icons;
mod tool_icons;
mod layout;
mod load_field;
// pub(crate) past egui_app: `State::machine_editor` names its draft
// type (M3.3 stage C), so state.rs has to see through the door.
pub(crate) mod machine_editor;
mod machine_grid;
// pub(crate) for the same reason: `State::post_refusal` names its type.
pub(crate) mod post_refusal;
// #867: the one copyable error widget.
pub(crate) mod error_text;
// pub(crate) for the same reason: `State::machines_pane` (#287).
pub(crate) mod machines_pane;
// pub(crate): `MachinesPane::editor` names its type (#383).
pub(crate) mod machine_file_editor;
mod feeds_page;
mod spindle_row;
mod approach_page;
mod lead_page;
mod op_tabs;
mod painted;
mod tree_badges;
mod tree_glyphs;
mod pages;
mod step_budget_row;
// #1094: the stock page's raw-stock-only checkbox.
mod raw_stock_row;
// #371: the tool editor's Body section and its picking preview.
mod tool_body_page;
// #630: the tool editor's lathe orientation row.
mod tool_orientation_row;
mod tool_turning_row;
mod tool_profile_page;
// #793: the built-in taper holder picker in the Profile section.
mod tool_holder_picker;
// #93 slice 2: the Profile tab's drawn cutter, neck and shank.
mod tool_cutter_page;
mod tool_cutter_params;
mod setup_wcs_pick;
// #343: a setup's own node pages (settings, machine, post, fixtures, tools).
mod setup_pages;
mod setup_post_page;
// #509: one fixture's row on the Fixtures page, sized to the pane.
mod fixture_rows;
// #561: the locate-in-fixture mate tools (Fixtures page and Model page).
mod locate_form;
// #326: the Setups page's part-alignment control.
mod setup_align_pick;
pub(crate) use setup_align_pick::align_text;
// #330: the Setups page's lay-on-a-plane row.
mod setup_plane_pick;
mod setup_axis_pick;
// #555: the Setups page's lay-on-three-points row.
mod setup_points_pick;
// #701: the Setups page's set-WCS-from-features role row.
mod wcs_pick_row;
// #316: the setup page's "relative to the previous setup" control.
mod setup_relative_pick;
mod setup_workholding_row;
// The operation dialog, split out of `pages` with the property grid
// (user review 2026-11-06): the frame, the Geometry pick block, the
// params dispatcher and its kind-family pages, the shared widgets.
mod fields;
mod geo_page;
mod heights_page;
mod holes_page;
mod pockets_page;
#[cfg(test)]
pub(crate) use pockets_page::row_text as pockets_row_text;
mod probe_page;
// #670: the Plasma Profile's page.
mod plasma_page;
mod trochoid_page;
mod kind_editors;
mod mill2d_more_page;
mod mill2d_page;
mod mill3d_page;
mod multiaxis_page;
mod op_page;
// #144: macOS Cmd+Q / app-menu Quit routed through the guarded Quit.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod os_quit;
mod params_page;
mod rotary_page;
mod turning_more_page;
mod turning_page;
// #107: Save / Don't Save / Cancel over unsaved changes.
mod unsaved_prompt;
mod panes;
// #441: the ribbon's Theme door and its dropdown.
mod theme_drop;
mod recent_drop;
// #343: the browser tree, split from panes (rows / pane / operation menu).
mod browser_tree;
// #155: Pause / Home entry rows and their menu.
mod tree_entry;
mod browser_pane;
mod tree_menu;
mod fixture_menu;
// #339 slice 6: the fixture library's Save / Insert dialogs.
pub(crate) mod fixture_library_dialog;
pub(crate) mod fixture_vise_dialog;
pub(crate) mod fixture_plate_dialog;
mod fixture_plate_rows;
mod fixture_modvise_rows;
pub(crate) mod fixture_accessory_dialog;
mod fixture_accessory_rows;
pub(crate) mod shell;
// #31: the winit window, loop and per-frame draw - native only; the
// browser build keeps `shell` (the shared frame) and refuses `run` below.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod shell_window;
// #31: the browser shell - eframe's web runner over a canvas; wasm32 only.
#[cfg(target_arch = "wasm32")]
pub(crate) mod shell_web;
// #31: the 3D pass and repaint law both shells share.
pub(crate) mod view_pass;
// pub past egui_app from M6.1 C: the switcher's resolution seam
// (state.rs) installs the live table, the format tests read the
// columns beside their parser, and every draw site already read
// `current()` through this module.
pub mod theme;
// pub: the M6.1 B format tests read it straight, and stage C's
// switcher is the next reader (discover -> cache -> apply).
pub mod theme_files;
// pub(crate) past egui_app: the crate-level law tests read the scene
// digest straight (the imgui panels' fold checks moved there at M-g), and
// the view cube's pick law straight.
pub(crate) mod viewcube;
pub(crate) mod world;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod heights_tests;
#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
mod fixture_ux_tests;
#[cfg(test)]
mod locate_tests;
#[cfg(test)]
mod fixture_library_tests;
#[cfg(test)]
mod fixture_vise_tests;
#[cfg(test)]
mod fixture_vise_stock_tests;
#[cfg(test)]
mod fixture_self_centering_tests;
#[cfg(test)]
mod fixture_plate_tests;
#[cfg(test)]
mod fixture_modvise_tests;
#[cfg(test)]
mod fixture_accessory_tests;
#[cfg(test)]
mod footer_tests;
#[cfg(test)]
mod dialog_tests;
#[cfg(test)]
mod coolant_tests;
#[cfg(test)]
mod thread_hand_tests;
#[cfg(test)]
mod after_op_tests;
#[cfg(test)]
mod colorvision_tests;
#[cfg(test)]
mod viewport_ground_tests;
#[cfg(test)]
mod tree_ink_tests;
#[cfg(test)]
mod pin_tests;
#[cfg(test)]
mod ribbon_icon_tests;
#[cfg(test)]
mod plasma_ribbon_tests;
#[cfg(test)]
mod machine_mode_tests;
#[cfg(test)]
mod debug_info_tests;
#[cfg(test)]
mod fresh_op_tests;
#[cfg(test)]
mod tool_number_tests;
#[cfg(test)]
mod setup_wcs_tests;
#[cfg(test)]
mod setup_align_tests;
#[cfg(test)]
mod setup_relative_tests;
mod setup_stock_pick;
#[cfg(test)]
mod setup_stock_tests;
#[cfg(test)]
mod incoming_view_tests;
#[cfg(test)]
mod setup_plane_tests;
#[cfg(test)]
mod setup_axis_tests;
#[cfg(test)]
mod setup_points_tests;
#[cfg(test)]
mod wcs_pick_tests;
#[cfg(test)]
mod ball_corner_tests;
#[cfg(test)]
mod gate_route_tests;

/// Entry for `--ui egui`; returns the process exit code. `--headless`
/// selects the no-window proof (seeded document, fallback adapter, View3D
/// readback); anything else opens the windowed shell.
pub fn run(args: &[String]) -> i32 {
    // M4.2: the backend pick is validated before anything opens - a bad
    // name refuses by name (exit 2, the `--ui` shape), never silently auto.
    let env = std::env::var("CAMCRAFT_GPU_BACKEND").ok();
    let pick = match gpu::pick_from(args, env.as_deref()) {
        Ok(p) => p,
        Err(err) => {
            diag_warn!("gpu: {err}");
            return 2;
        }
    };
    // #74: the adapter-name choice (`--gpu <substring>` over CAMCRAFT_GPU).
    let want_env = std::env::var("CAMCRAFT_GPU").ok();
    let want = match gpu::adapter_from(args, want_env.as_deref()) {
        Ok(w) => w,
        Err(err) => {
            diag_warn!("gpu: {err}");
            return 2;
        }
    };
    // #183: the live MCP server is opt-in - off unless asked for.
    let mcp_env = std::env::var("CAMCRAFT_MCP_PORT").ok();
    let mcp_port = match camcraft_app::mcp::port_from(args, mcp_env.as_deref()) {
        Ok(p) => p,
        Err(err) => {
            diag_warn!("mcp: {err}");
            return 2;
        }
    };
    if args.iter().any(|a| a == "--headless") {
        if pick != gpu::GpuPick::Auto || want.is_some() {
            diag_info!("gpu: --headless pins the fallback adapter for determinism; backend and adapter picks ignored");
        }
        run_headless()
    } else {
        run_windowed(pick, want, mcp_port)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run_headless() -> i32 {
    match shell::headless::run() {
        Ok(()) => {
            diag_info!("egui shell headless: seeded document, command world, View3D green");
            0
        }
        Err(err) => {
            diag_warn!("egui shell headless FAILED: {err}");
            1
        }
    }
}

/// #31: the headless proof blocks on the adapter and the readback; the
/// browser build refuses it by name like the windowed entry.
#[cfg(target_arch = "wasm32")]
fn run_headless() -> i32 {
    diag_warn!("{WASM_NO_WINDOW}");
    2
}

#[cfg(not(target_arch = "wasm32"))]
fn run_windowed(pick: gpu::GpuPick, want: Option<String>, mcp_port: Option<u16>) -> i32 {
    shell_window::run_windowed(pick, want, mcp_port)
}

/// #31: the browser build has no winit window to open - its shell is the
/// web runner still owed on that issue - so the windowed entry refuses by
/// name instead of pretending.
#[cfg(target_arch = "wasm32")]
fn run_windowed(_pick: gpu::GpuPick, _want: Option<String>, _mcp_port: Option<u16>) -> i32 {
    diag_warn!("{WASM_NO_WINDOW}");
    2
}

/// What the wasm32 build says when asked for a native window (#31).
#[cfg(target_arch = "wasm32")]
pub(crate) const WASM_NO_WINDOW: &str =
    "camcraft-ui: the windowed shell and the headless proof are native-only; in a browser the page starts the shell (camcraft_start, #31)";

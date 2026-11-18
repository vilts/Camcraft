//! CamCraft UI shell (clean-room): winit + egui (docked panels) + wgpu viewport.
//!
//! Layering (AGENTS.md): ui -> app -> flecs. This crate renders and routes
//! input; every persistent world change goes through `camcraft_app::commands`.
//! The imgui stack that lived here until M1.4.2 M-g is retired: `egui_app`
//! is the shell, `interact` and `state` the shared law underneath it.

// #31: on wasm32 the browser shell (`egui_app::shell_web`) draws the shared
// frame, but 17 helpers only the native window, the adapter-by-adapter
// bring-up and the headless proof reach (gpu_select, gpu's backend mask,
// world::seed, the recent-files read) are left without a caller there.
// Native builds keep the dead-code lint; gating those helpers one by one
// is owed on #31, after which this line goes.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

// First, so every module below sees its macros (`diag_warn!`, `diag_info!`).
#[macro_use]
pub mod diag;

pub mod cli_options;
#[cfg(test)]
mod cli_options_tests;

pub mod actions;
pub mod play_precalc;
pub mod align_pick_viz;
pub(crate) mod machine_modes;
mod axis_note;
mod build_info;
pub mod boundary_draft;
pub mod box_pick;
pub mod browser_files;
pub mod boundary_viz;
pub mod slot_manual_viz;
pub mod plate_viz;
pub mod boundary_summary;
pub mod edge_pick;
pub mod pick_ink;
pub mod feature_click;
pub mod egui_app;
pub mod feedback;
pub mod gen_worker;
pub mod file_dialog;
pub mod file_pick;
pub mod frame_cost;
pub mod frame_memo;
pub mod incoming_view;
pub mod headless_gpu;
pub mod interact;
pub mod locate_pick;
pub mod machined;
mod setup_display;
mod floor;
mod work_envelope;
pub mod view_frame;
pub mod mcp_host;
pub mod operations;
pub mod path_lines;
pub mod pick_line_memo;
pub(crate) mod refusal_ghost;
pub mod post_file;
pub mod plane_pick;
pub mod pick_words;
pub mod axis_pick;
pub mod cylinder_pick;
pub mod origin_pick;
pub mod points_pick;
pub mod start_exit_pick;
pub mod wire_sync_pick;
pub mod wcs_pick;
pub mod shortcut_keys;
pub mod state;
pub mod theme_dir;
pub mod trace;
pub mod unsaved;
pub mod vertex_pick;
pub mod viewport;
pub mod wcs_circle_pick;

#[cfg(test)]
mod file_pick_tests;
#[cfg(test)]
mod law_tests;
#[cfg(test)]
mod refusal_focus_tests;
#[cfg(test)]
mod detect_holes_note_tests;
#[cfg(test)]
mod refusal_ghost_tests;
#[cfg(test)]
mod helix_view_tests;
#[cfg(test)]
mod field_view_tests;
#[cfg(test)]
mod play_precalc_tests;
#[cfg(test)]
mod machined_tests;
#[cfg(test)]
mod play_bench_tests;
#[cfg(test)]
mod vertex_pick_tests;
#[cfg(test)]
mod centre_pick_tests;
#[cfg(test)]
mod setup_chip_tests;
#[cfg(test)]
mod pockets_tests;
#[cfg(test)]
mod wcs_gizmo_tests;
#[cfg(test)]
mod axis_note_tests;

/// Entry point used by `main.rs`; returns the process exit code. The egui
/// shell is the shell: `--headless` runs its no-window proof, anything else
/// opens the window. `--ui egui` still selects it (the M1.4.2 migration
/// flag); `--ui <anything else>` refuses by name - the imgui stack retired
/// at M-g and there is nothing left to select.
/// Provenance: our own shell, specced from public CAM workflow conventions.
pub fn run(args: &[String]) -> i32 {
    // #2082: help, version and unknown options are settled before any
    // window, GPU device or server is made.
    match cli_options::parse(args) {
        Ok(cli_options::Parsed::Help) => {
            diag::info(&cli_options::help_text());
            return 0;
        }
        Ok(cli_options::Parsed::Version) => {
            diag::info(&cli_options::version_text());
            return 0;
        }
        Ok(cli_options::Parsed::Run) => {}
        Err(err) => {
            diag_warn!("{err}");
            return 2;
        }
    }
    let mut it = args.iter();
    while let Some(a) = it.next() {
        // Both `--ui value` and `--ui=value` give the same result.
        let value = if a == "--ui" {
            Some(it.next().map(String::as_str))
        } else {
            a.strip_prefix("--ui=").map(Some)
        };
        if let Some(value) = value {
            match value {
                None | Some("") => {
                    diag_warn!("--ui needs a value: egui is the only shell (imgui retired at M1.4.2 M-g)");
                    return 2;
                }
                Some("egui") => {}
                Some(other) => {
                    diag_warn!("unknown --ui {other}: the imgui stack retired at M1.4.2 M-g; egui is the only shell");
                    return 2;
                }
            }
        }
    }
    egui_app::run(args)
}

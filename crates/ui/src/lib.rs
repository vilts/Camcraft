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

pub mod actions;
pub(crate) mod machine_modes;
mod axis_note;
mod build_info;
pub mod boundary_draft;
pub mod boundary_viz;
pub mod plate_viz;
pub mod boundary_summary;
pub mod edge_pick;
pub mod feature_click;
pub mod egui_app;
pub mod feedback;
pub mod file_dialog;
pub mod incoming_view;
pub mod headless_gpu;
pub mod interact;
pub mod locate_pick;
pub mod machined;
pub mod mcp_host;
pub mod operations;
pub mod path_lines;
pub(crate) mod refusal_ghost;
pub mod post_file;
pub mod plane_pick;
pub mod axis_pick;
pub mod points_pick;
pub mod wcs_pick;
pub mod state;
pub mod theme_dir;
pub mod trace;
pub mod unsaved;
pub mod vertex_pick;
pub mod viewport;

#[cfg(test)]
mod law_tests;
#[cfg(test)]
mod refusal_focus_tests;
#[cfg(test)]
mod refusal_ghost_tests;
#[cfg(test)]
mod helix_view_tests;
#[cfg(test)]
mod field_view_tests;
#[cfg(test)]
mod machined_tests;
#[cfg(test)]
mod play_bench_tests;
#[cfg(test)]
mod vertex_pick_tests;
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
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--ui" {
            match it.next().map(String::as_str) {
                None => {
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

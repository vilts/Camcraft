//! Test-only: one process-wide lock so the lib tests that make a wgpu
//! instance (directly, or through egui_kittest's screenshot renderer) use
//! the GPU one at a time (#3589: parallel instance creation and teardown
//! ended the test process with STATUS_ACCESS_VIOLATION on Windows 11).
//! Provenance: our own test harness; std::sync primitives only.
//!
//! Take the guard before the first call that can make an instance (for a
//! kittest harness: before the harness itself, so the harness and its lazy
//! renderer drop first and the guard drops last).
//!
//! The lock itself lives in `crate::headless_gpu` (#3605), so that the
//! lib tests, the shared test device and the integration binaries take
//! the same one lock.

pub(crate) use crate::headless_gpu::gpu_guard;
use crate::headless_gpu::{gpu_holders, test_gpu};

/// The lib tests that make a wgpu instance. Each must take [`gpu_guard`]
/// (or the shared device, [`test_gpu`], which takes it);
/// `every_gpu_test_takes_the_lock` reads their source to pin it.
const GPU_TESTS: [(&str, &str); 15] = [
    ("tests.rs", "fn headless_run_succeeds()"),
    // #3379: the pixel snapshots; `snapshot` makes the kittest renderer.
    ("tests.rs", "fn egui_shell_snapshot()"),
    ("tests.rs", "fn egui_shell_snapshot_light()"),
    ("tests.rs", "fn tool_editor_snapshot()"),
    ("gpu.rs", "fn adapter_probe_names_the_gpu()"),
    ("report_tests.rs", "fn the_door_opens_the_window_and_a_kind_button_files_the_url()"),
    ("report_tests.rs", "fn the_title_field_travels_into_the_url()"),
    ("report_tests.rs", "fn a_kind_button_asks_the_shell_for_the_whole_window()"),
    ("report_tests.rs", "fn the_answer_copies_the_picture_and_says_the_word()"),
    ("report_tests.rs", "fn the_picture_is_still_copied_after_the_window_is_closed()"),
    ("report_tests.rs", "fn no_answer_copies_nothing()"),
    // #3605: the view pass test and the three ignored evidence renderers.
    ("view_pass.rs", "fn fit_target_never_asks_the_device_for_a_texture_past_its_limit()"),
    ("tests/guide_images.rs", "fn render_all_guide_images()"),
    ("tests/review_1710_evidence.rs", "fn review_1710_wcs_page_and_viewport_evidence()"),
    ("viewport_bar_tooltip_tests.rs", "fn generate_evidence_screenshots()"),
];

/// #3605: the integration binaries whose tests share one device. Each
/// takes it through `headless_gpu::test_gpu` and never makes its own.
const SHARED_DEVICE_BINARIES: [(&str, &str); 2] = [
    ("tests/trace_composite.rs", include_str!("../../tests/trace_composite.rs")),
    ("tests/trace_parity.rs", include_str!("../../tests/trace_parity.rs")),
];

/// #3379: the tests of `tests/egui_app.rs` whose run refuses or fails
/// before it asks for a device. Each other test there must take [`gpu_guard`].
const EGUI_APP_NO_GPU: [&str; 5] = [
    "selecting_the_retired_imgui_shell_refuses_with_rc_2",
    "ui_flag_without_a_value_refuses_with_rc_2",
    "ui_equals_form_refuses_like_the_space_form",
    "a_headless_start_refuses_a_file_it_cannot_open_by_name",
    "a_headless_start_with_a_broken_model_fails_by_name",
];

/// The first line of code in the body of the first function in `src`.
fn first_statement(src: &str) -> &str {
    let body = src.find('{').map_or("", |open| &src[open + 1..]);
    body.trim_start().lines().find(|l| !l.trim_start().starts_with("//")).unwrap_or("").trim_start()
}

fn takes_the_guard(statement: &str) -> bool {
    statement.starts_with("let _gpu = ") && statement.contains("gpu_guard()")
}

/// #3379: parallel headless runs made and dropped devices at the same time.
#[test]
fn every_headless_run_of_the_egui_app_binary_takes_the_lock() {
    let src = include_str!("../../tests/egui_app.rs");
    let mut no_gpu: Vec<&str> = EGUI_APP_NO_GPU.to_vec();
    let mut unlocked = Vec::new();
    for test in src.split("#[test]").skip(1) {
        let Some((_, rest)) = test.split_once("fn ") else { continue };
        let Some((name, rest)) = rest.split_once('(') else { continue };
        if let Some(at) = no_gpu.iter().position(|n| *n == name) {
            no_gpu.remove(at);
        } else if !takes_the_guard(first_statement(rest)) {
            unlocked.push(name);
        }
    }
    assert!(
        unlocked.is_empty(),
        "tests/egui_app.rs: start with `let _gpu = ...gpu_guard();`, or list a test that makes no device: {unlocked:?}"
    );
    assert!(no_gpu.is_empty(), "no such test in tests/egui_app.rs: {no_gpu:?}");
}

fn source_of(file: &str) -> &'static str {
    match file {
        "tests.rs" => include_str!("tests.rs"),
        "gpu.rs" => include_str!("gpu.rs"),
        "report_tests.rs" => include_str!("report_tests.rs"),
        "view_pass.rs" => include_str!("view_pass.rs"),
        "tests/guide_images.rs" => include_str!("tests/guide_images.rs"),
        "tests/review_1710_evidence.rs" => include_str!("tests/review_1710_evidence.rs"),
        "viewport_bar_tooltip_tests.rs" => include_str!("viewport_bar_tooltip_tests.rs"),
        other => panic!("no source table entry for {other}"),
    }
}

#[test]
fn every_gpu_test_takes_the_lock() {
    for (file, sig) in GPU_TESTS {
        let src = source_of(file);
        let start = src.find(sig).unwrap_or_else(|| panic!("{file}: {sig} not found"));
        // The guard is the first statement, before any harness or instance.
        let first = first_statement(&src[start..]);
        let shared = first.contains("headless_gpu::test_gpu(");
        assert!(
            takes_the_guard(first) || shared,
            "{file}: {sig} must start with `let _gpu = gpu_guard();` or take `test_gpu`, found {first:?}"
        );
    }
}

#[test]
fn the_trace_binaries_share_one_locked_device() {
    for (file, src) in SHARED_DEVICE_BINARIES {
        assert!(src.contains("headless_gpu::test_gpu("), "{file}: takes the shared device through `test_gpu`");
        for own in ["device_for_test(", "headless_gpu::device(", "request_device("] {
            assert!(!src.contains(own), "{file}: makes its own device with `{own}`; use `test_gpu` (#3605)");
        }
    }
}

/// #3605 pin: tests on several threads ask for the device at the same
/// time. Each gets the one shared device, alone, and the process lives.
#[test]
fn devices_asked_from_many_threads_are_one_device_used_one_at_a_time() {
    let threads: Vec<_> = (0..8)
        .map(|_| {
            std::thread::spawn(|| {
                let mut seen = Vec::new();
                for _ in 0..4 {
                    let Some(gpu) = test_gpu("gpu_test_lock").expect("adapter") else { return seen };
                    assert_eq!(gpu_holders().0, 1, "a second test holds the GPU");
                    // Use the device under the lock: one buffer, one submit.
                    let buf = gpu.device.create_buffer(&egui_wgpu::wgpu::BufferDescriptor {
                        label: Some("#3605 pin"),
                        size: 256,
                        usage: egui_wgpu::wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    gpu.queue.write_buffer(&buf, 0, &[7; 256]);
                    gpu.queue.submit([]);
                    seen.push(std::sync::Arc::as_ptr(&gpu.device) as usize);
                }
                seen
            })
        })
        .collect();
    let mut devices = std::collections::HashSet::new();
    for t in threads {
        let seen = t.join().unwrap_or_else(|_| panic!("a thread saw a second holder"));
        devices.extend(seen);
    }
    assert!(devices.len() <= 1, "one device for the process, found {}", devices.len());
    assert_eq!(gpu_holders().1, 1, "the GPU was held twice at once");
}

#[test]
fn the_lock_lets_one_holder_in_at_a_time() {
    let threads: Vec<_> = (0..8)
        .map(|_| {
            std::thread::spawn(|| {
                for _ in 0..20 {
                    let _gpu = gpu_guard();
                    assert_eq!(gpu_holders().0, 1);
                    std::thread::yield_now();
                }
            })
        })
        .collect();
    for t in threads {
        assert!(t.join().is_ok(), "a holder saw a second holder");
    }
    assert_eq!(gpu_holders().1, 1, "the GPU was held twice at once");
}

#[test]
fn a_panic_with_the_lock_held_does_not_block_the_next_test() {
    let failed = std::thread::spawn(|| {
        let _gpu = gpu_guard();
        panic!("a GPU test fails while it holds the lock");
    })
    .join();
    assert!(failed.is_err());
    // The lock is poisoned now; the next holder still gets in, alone.
    let _gpu = gpu_guard();
    assert_eq!(gpu_holders().0, 1);
}

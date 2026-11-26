//! The one compute device of the process (#2620): made on the first
//! request, cached for the life of the process, never made for each
//! operation or each frame. It is never dropped: a device made and dropped
//! in one process can race its sibling processes in the driver
//! (`crates/ui/tests/gpu_render.rs`, the shared device note).
//! Provenance: our own policy over wgpu's public device API.

use std::sync::{Arc, OnceLock};

use crate::adapter::{device, HeadlessGpu};
use crate::switch::{env_switch, log_fallback_once};

static SHARED: OnceLock<Result<HeadlessGpu, String>> = OnceLock::new();
static FAILED: OnceLock<String> = OnceLock::new();

fn disable(reason: String) {
    // Keep the first fault, including when device loss follows an uncaptured error.
    if FAILED.set(reason).is_ok()
        && let Some(reason) = FAILED.get()
    {
        log_fallback_once(reason);
    }
}

/// The process's compute device, or why there is none: the switch is off,
/// no adapter, the probe found no compute, or the device failed. A failed
/// device is not made again: the CPU path runs for the rest of the process.
pub fn shared_device() -> Result<&'static HeadlessGpu, String> {
    env_switch()?;
    let made = SHARED.get_or_init(|| {
        let gpu = device()?;
        gpu.device.on_uncaptured_error(Arc::new(|error| {
            disable(format!("uncaptured GPU error: {error}"));
        }));
        gpu.device.set_device_lost_callback(|reason, msg| {
            disable(format!("GPU compute device lost ({reason:?}): {msg}"));
        });
        Ok(gpu)
    });
    let gpu = made.as_ref().map_err(Clone::clone)?;
    if let Some(reason) = FAILED.get() {
        return Err(reason.clone());
    }
    gpu.compute.as_ref().map_err(|e| format!("{e} ({})", gpu.name))?;
    Ok(gpu)
}

#[cfg(test)]
mod tests {
    use super::shared_device;
    use crate::{cut_cpu, cut_or_cpu, gpu_required, CutGrid, CutInput, CutItem, CutPath, COMPUTE_ENV};

    #[test]
    fn shared_device_faults_use_cpu() {
        const FAULT_ENV: &str = "CAMCRAFT_TEST_SHARED_GPU_FAULT";
        let Ok(fault) = std::env::var(FAULT_ENV) else {
            // Each fault needs its own process: shared_device stays off after a fault.
            for fault in ["lost", "uncaptured"] {
                let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
                    .args(["--exact", "shared::tests::shared_device_faults_use_cpu", "--nocapture"])
                    .env(FAULT_ENV, fault)
                    .env(COMPUTE_ENV, "on")
                    .output()
                    .expect("start isolated GPU fault test");
                eprint!("{}", String::from_utf8_lossy(&output.stderr));
                assert!(
                    output.status.success(),
                    "{fault} fault child failed:\n{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            return;
        };

        let gpu = match shared_device() {
            Ok(gpu) => gpu,
            Err(reason) => {
                assert!(!gpu_required(), "CAMCRAFT_REQUIRE_GPU=1 but {reason}");
                eprintln!("*** shared_device_faults_use_cpu: SKIPPED - NOT a pass: {reason} ***");
                return;
            }
        };
        match fault.as_str() {
            "lost" => {
                // Destroy simulates a lost device through wgpu's real callback.
                gpu.device.destroy();
                let _ = gpu.device.poll(wgpu::PollType::Poll);
            }
            "uncaptured" => {
                // No error scope: this must reach the installed handler, without a panic.
                let _ = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("issue 3533: unscoped validation error"),
                    size: 4,
                    usage: wgpu::BufferUsages::empty(),
                    mapped_at_creation: false,
                });
            }
            other => panic!("unknown isolated GPU fault: {other}"),
        }
        let reason = shared_device().err().expect("fault must disable the shared GPU");
        assert!(reason.contains(&fault), "fault reason must name {fault}: {reason}");
        if fault == "uncaptured" {
            let _ = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("issue 3533: second unscoped validation error"),
                size: 4,
                usage: wgpu::BufferUsages::empty(),
                mapped_at_creation: false,
            });
            gpu.device.destroy();
            let _ = gpu.device.poll(wgpu::PollType::Poll);
            assert_eq!(shared_device().err(), Some(reason.clone()), "retain the first fault reason");
        }

        let input = CutInput {
            grid: CutGrid { dims: [2; 3], cell: 1.0, cap: 1.0 },
            items: vec![CutItem {
                a: [0.5; 3],
                b: [1.5, 0.5, 0.5],
                r: 0.75,
                flank: 0.0,
                margin: 0.01,
                ball: true,
                base: [0; 3],
                lo: [0; 3],
                hi: [1; 3],
            }],
        };
        let expected = cut_cpu(&input).expect("valid CPU cut");
        for _ in 0..2 {
            let output = cut_or_cpu(&input).expect("GPU fault must retain the CPU result");
            assert_eq!(output.path, CutPath::Cpu { reason: reason.clone() });
            assert_eq!(output.values, expected);
        }
        eprintln!("shared_device_faults_use_cpu: exercised {fault} on {}", gpu.name);
    }
}

//! The incremental stock cut (#2629, GPU stock sim S3a): a batch cut onto
//! an earlier layer (a playback keyframe), on the GPU or by the CPU law.
//! Pins: GPU and CPU agree within the claim tolerance (or the test skips
//! by name when no adapter exists); the off switch takes the CPU law; a
//! lost device takes the CPU law and never crashes.
//! #3589: this binary touches the one shared device of its process only
//! (`shared_device`), never a second `wgpu` instance. The switch and the
//! device-loss cases run in a child process of this binary, because each
//! leaves the shared device off for the rest of its process.
//! Gate map: covers camcraft_gpu::cut, camcraft_gpu::cut_gpu,
//! camcraft_gpu::cut_buf, camcraft_gpu::cut_run. Our own tests.

use camcraft_gpu::{
    cut_cpu, cut_cpu_onto, cut_onto_or_cpu, cut_onto_or_cpu_on, gpu_required, shared_device, spot_check_onto, spot_probes, CutGrid,
    CutInput, CutItem, CutKernel, CutPath, COMPUTE_ENV, SPOT_MARGIN_FRACTION,
};

/// The child-process mode of the switch and device-loss pins.
const CHILD_ENV: &str = "CAMCRAFT_TEST_CUT_ONTO_CHILD";

/// The claim margin of every fixture item; the GPU and the CPU law may
/// differ by [`SPOT_MARGIN_FRACTION`] of it (the cell tolerance of S2).
const MARGIN: f32 = 1e-4;

const GRID: CutGrid = CutGrid { dims: [32, 32, 16], cell: 0.5, cap: 1.0 };

/// One chord (absolute mm) as one item for each brick of the lattice.
fn chord(a: [f32; 3], b: [f32; 3], r: f32, flank: f32) -> Vec<CutItem> {
    let mut items = Vec::new();
    for bz in 0..GRID.dims[2] / 8 {
        for by in 0..GRID.dims[1] / 8 {
            for bx in 0..GRID.dims[0] / 8 {
                let base = [bx, by, bz].map(|v| (v * 8) as i32);
                let o = base.map(|v| v as f32 * GRID.cell);
                let rel = |p: [f32; 3]| [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
                items.push(CutItem {
                    a: rel(a),
                    b: rel(b),
                    r,
                    flank,
                    margin: MARGIN,
                    ball: flank == 0.0,
                    base,
                    lo: [0; 3],
                    hi: [7; 3],
                });
            }
        }
    }
    items
}

/// Keyframe moves (`earlier`) and the moves after it (`later`): a ball
/// chord first, then a tilted flat chord (the golden section) and a
/// level flat chord (the closed form).
fn fixture() -> (CutInput, CutInput) {
    let earlier = CutInput { grid: GRID, items: chord([2.0, 3.0, 6.0], [13.0, 11.0, 4.0], 2.0, 0.0) };
    let mut later = chord([3.0, 12.0, 5.0], [14.0, 4.0, 6.5], 1.5, 6.0);
    later.extend(chord([1.0, 8.0, 3.5], [15.0, 8.0, 3.5], 1.0, 6.0));
    (earlier, CutInput { grid: GRID, items: later })
}

fn same_bits(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
}

/// The largest |a - b| of two layers, both clamped at `-cap` (a value
/// under it never shows).
fn max_gap(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x.max(-GRID.cap) - y.max(-GRID.cap)).abs()).fold(0.0, f32::max)
}

#[test]
fn the_incremental_cpu_law_is_the_one_batch_cut_bit_for_bit() {
    let (earlier, later) = fixture();
    let key = cut_cpu(&earlier).expect("the keyframe cut");
    let onto = cut_cpu_onto(&later, &key).expect("the cut onto the keyframe");
    let mut all = earlier.clone();
    all.items.extend(later.items.iter().copied());
    let one = cut_cpu(&all).expect("the one batch");
    let claimed = |l: &[f32]| l.iter().filter(|v| **v > -GRID.cap).count();
    assert!(claimed(&key) > 100 && claimed(&one) > claimed(&key), "the fixture cuts too little");
    assert!(same_bits(&onto, &one), "the cut onto a keyframe must equal the one batch cut");
    let floor = vec![-4.0 * GRID.cap; key.len()];
    assert!(same_bits(&cut_cpu_onto(&earlier, &floor).expect("onto the floor"), &key), "the floor start is cut_cpu");
}

#[test]
fn a_bad_base_layer_is_refused_by_name() {
    let (_, later) = fixture();
    let n = later.grid.texels().expect("the fixture lattice");
    let err = cut_cpu_onto(&later, &vec![0.0; n - 1]).expect_err("a short base");
    assert!(err.contains("base layer has"), "{err}");
    let mut nan = vec![-4.0; n];
    nan[7] = f32::NAN;
    for (name, base) in [("NaN", nan), ("infinity", vec![f32::NEG_INFINITY; n])] {
        let err = cut_cpu_onto(&later, &base).expect_err(name);
        assert!(err.contains("is not finite"), "{name}: {err}");
        let err = cut_onto_or_cpu_on(Err("no device".into()), &later, &base).expect_err(name);
        assert!(err.contains("is not finite"), "{name}: the fallback rule refuses too: {err}");
    }
}

#[test]
fn a_tie_with_the_keyframe_does_not_refuse_a_good_gpu_layer() {
    // A later batch that repeats a chord of the keyframe (a spring pass)
    // ties with the base layer. A GPU value a little deeper than the CPU
    // base (as a hardware adapter can give) is inside the item's bound;
    // the review of PR #3878 found that the check refused it at 1 ULP.
    let (earlier, _) = fixture();
    let key = cut_cpu(&earlier).expect("the keyframe cut");
    let again = earlier.clone();
    let cpu = cut_cpu_onto(&again, &key).expect("the spring pass");
    assert!(same_bits(&cpu, &key), "a repeated chord cuts nothing new");
    let at = *spot_probes(&again).expect("the probes").first().expect("an item probe");
    let mut gpu = cpu.clone();
    gpu[at] = cpu[at] + 0.25 * MARGIN;
    spot_check_onto(&again, &key, &gpu).unwrap_or_else(|e| panic!("a tie inside the bound was refused: {e}"));
    gpu[at] = cpu[at] + 2.0 * MARGIN;
    let err = spot_check_onto(&again, &key, &gpu).expect_err("a value past the bound");
    assert!(err.contains(&format!("texel {at}")), "{err}");
}

#[test]
fn the_gpu_cut_onto_a_keyframe_matches_the_cpu_law_or_skips_by_name() {
    let (earlier, later) = fixture();
    let key = cut_cpu(&earlier).expect("the keyframe cut");
    let want = cut_cpu_onto(&later, &key).expect("the CPU law");
    let gpu = match shared_device() {
        Ok(gpu) => gpu,
        Err(why) => {
            assert!(!gpu_required() || why.contains("off"), "CAMCRAFT_REQUIRE_GPU=1 but {why}");
            eprintln!("*** cut_onto: GPU parity SKIPPED - NOT a pass: {why} ***");
            let out = cut_onto_or_cpu(&later, &key).expect("the CPU law answers");
            assert!(matches!(out.path, CutPath::Cpu { .. }), "no device: the CPU law must answer");
            assert!(same_bits(&out.values, &want));
            return;
        }
    };
    let compute = gpu.compute.as_ref().expect("shared_device has compute");
    let kernel = CutKernel::new(&gpu.device).unwrap_or_else(|e| panic!("the kernel builds on {}: {e}", gpu.name));
    let got = kernel.run_onto(&gpu.device, &gpu.queue, compute, &later, &key).unwrap_or_else(|e| panic!("{}: {e}", gpu.name));
    let gap = max_gap(&got, &want);
    assert!(gap <= SPOT_MARGIN_FRACTION * MARGIN, "max |GPU - CPU law| {gap:e} on {}", gpu.name);
    spot_check_onto(&later, &key, &got).unwrap_or_else(|e| panic!("{e}"));

    // The display path: the same cut into a caller buffer, no read-back
    // by the kernel. The test reads the buffer back to compare.
    use wgpu::BufferUsages as U;
    let bytes: Vec<u8> = key.iter().flat_map(|v| v.to_le_bytes()).collect();
    let size = bytes.len() as u64 + 64; // a larger buffer than the lattice
    let target = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cut_onto test target"),
        size,
        usage: U::STORAGE | U::COPY_SRC | U::COPY_DST,
        mapped_at_creation: false,
    });
    gpu.queue.write_buffer(&target, 0, &bytes);
    kernel.cut_into(&gpu.device, &gpu.queue, compute, &later, &target).unwrap_or_else(|e| panic!("{e}"));
    let read = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cut_onto test read"),
        size: bytes.len() as u64,
        usage: U::MAP_READ | U::COPY_DST,
        mapped_at_creation: false,
    });
    let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    enc.copy_buffer_to_buffer(&target, 0, &read, 0, bytes.len() as u64);
    gpu.queue.submit([enc.finish()]);
    read.slice(..).map_async(wgpu::MapMode::Read, |r| r.expect("the test map"));
    gpu.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).expect("the test poll");
    let into: Vec<f32> =
        read.slice(..).get_mapped_range().expect("the test range").as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect();
    assert!(same_bits(&into, &got), "cut_into and run_onto differ on {}", gpu.name);

    let small = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cut_onto test small"),
        size: 16,
        usage: U::STORAGE,
        mapped_at_creation: false,
    });
    let err = kernel.cut_into(&gpu.device, &gpu.queue, compute, &later, &small).expect_err("a short target");
    assert!(err.message.contains("target buffer has 16 bytes"), "{err}");

    let via = cut_onto_or_cpu(&later, &key).expect("the shared path answers");
    assert_eq!(via.path, CutPath::Gpu { adapter: gpu.name.clone() }, "the shared path fell back");
    assert!(same_bits(&via.values, &got), "the shared and the direct runs differ");
    eprintln!("cut_onto: GPU parity on {}: max |GPU - CPU law| {gap:e}", gpu.name);
}

/// Run this test again in a child process with `CHILD_ENV=<mode>`.
fn in_child(test: &str, mode: &str, compute: &str) {
    let exe = std::env::current_exe().expect("the test executable");
    let out = std::process::Command::new(exe)
        .args(["--exact", test, "--nocapture"])
        .env(CHILD_ENV, mode)
        .env(COMPUTE_ENV, compute)
        .output()
        .expect("start the child test");
    eprint!("{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "the {mode} child failed:\n{text}");
    assert!(text.contains("1 passed"), "the {mode} child ran no test:\n{text}");
}

#[test]
fn the_off_switch_takes_the_cpu_law_by_name() {
    let Ok(mode) = std::env::var(CHILD_ENV) else {
        return in_child("the_off_switch_takes_the_cpu_law_by_name", "off", "off");
    };
    assert_eq!(mode, "off");
    let (earlier, later) = fixture();
    let key = cut_cpu(&earlier).expect("the keyframe cut");
    let out = cut_onto_or_cpu(&later, &key).expect("the CPU law answers");
    match &out.path {
        CutPath::Cpu { reason } => assert!(reason.contains(COMPUTE_ENV), "the reason must name the switch: {reason}"),
        CutPath::Gpu { adapter } => panic!("{COMPUTE_ENV}=off but the GPU on {adapter} answered"),
    }
    assert!(same_bits(&out.values, &cut_cpu_onto(&later, &key).expect("the CPU law")));
}

#[test]
fn a_lost_device_takes_the_cpu_law_and_never_crashes() {
    let Ok(mode) = std::env::var(CHILD_ENV) else {
        return in_child("a_lost_device_takes_the_cpu_law_and_never_crashes", "lost", "on");
    };
    assert_eq!(mode, "lost");
    let (earlier, later) = fixture();
    let key = cut_cpu(&earlier).expect("the keyframe cut");
    let want = cut_cpu_onto(&later, &key).expect("the CPU law");
    let gpu = match shared_device() {
        Ok(gpu) => gpu,
        Err(why) => {
            assert!(!gpu_required(), "CAMCRAFT_REQUIRE_GPU=1 but {why}");
            eprintln!("*** cut_onto: device loss SKIPPED - NOT a pass: {why} ***");
            return;
        }
    };
    let compute = gpu.compute.as_ref().expect("shared_device has compute");
    let kernel = CutKernel::new(&gpu.device).unwrap_or_else(|e| panic!("the kernel builds on {}: {e}", gpu.name));
    // Destroy fires wgpu's real device-lost callback.
    gpu.device.destroy();
    let _ = gpu.device.poll(wgpu::PollType::Poll);

    // The kernel itself on the lost device: an error, never a panic.
    let target = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cut_onto lost target"),
        size: (want.len() * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let direct = kernel.run_onto(&gpu.device, &gpu.queue, compute, &later, &key);
    assert!(direct.is_err(), "a lost device must not give a layer");
    let into = kernel.cut_into(&gpu.device, &gpu.queue, compute, &later, &target);
    assert!(into.is_err(), "cut_into on a lost device must be an Err, never a silent submission");

    // The fallback rule on that device and kernel: the CPU law.
    let out = cut_onto_or_cpu_on(Ok((gpu, &kernel)), &later, &key).expect("the CPU law answers");
    assert!(matches!(out.path, CutPath::Cpu { .. }), "a lost device must take the CPU law");
    assert!(same_bits(&out.values, &want));
    // The shared path names the loss.
    let out = cut_onto_or_cpu(&later, &key).expect("the CPU law answers");
    match &out.path {
        CutPath::Cpu { reason } => assert!(reason.contains("lost"), "the reason must name the loss: {reason}"),
        CutPath::Gpu { adapter } => panic!("the lost device on {adapter} answered"),
    }
    assert!(same_bits(&out.values, &want));
    eprintln!("cut_onto: device loss exercised on {}", gpu.name);
}

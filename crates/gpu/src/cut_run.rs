//! The fallback rule of the stock removal kernel (#2627, GPU stock sim
//! S2): the GPU answers only when its device, its kernel and a spot
//! check against the CPU law all agree; otherwise the CPU law answers
//! and the reason is logged one time. A timed-out batch or a failed
//! spot check turns the GPU path off for the rest of the process. The
//! incremental cut onto an earlier layer (#2629, S3a) takes the same rule.
//! Provenance: our own rule (decision 2627).

use std::sync::OnceLock;

use crate::adapter::HeadlessGpu;
use crate::cut::{cell_cut, check_base, cut_cpu, cut_cpu_onto, slot, CutGrid, CutInput, CutItem};
use crate::cut_gpu::CutKernel;
use crate::shared::shared_device;
use crate::switch::log_fallback_once;

/// Item probes in the spot check of a batch, at most.
pub const SPOT_TEXELS: usize = 64;

/// Fixed lattice places that the spot check also compares, so a wrong
/// value away from every sampled item is found.
pub const SPOT_LATTICE_TEXELS: usize = 8;

/// The spot check's bound, as a fraction of the winning item's claim
/// margin: the GPU value and the CPU law may differ by at most this
/// much of the margin, so the GPU cut stays under the f64 cut.
pub const SPOT_MARGIN_FRACTION: f32 = 0.5;

/// Which law answered a batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CutPath {
    /// The kernel ran on this adapter (`"<name> via <backend>"`).
    Gpu { adapter: String },
    /// The CPU law ran; the reason the GPU did not.
    Cpu { reason: String },
}

/// The cut layer of one batch and the law that made it.
#[derive(Clone, Debug, PartialEq)]
pub struct CutOutput {
    pub values: Vec<f32>,
    pub path: CutPath,
}

static KERNEL: OnceLock<Result<CutKernel, String>> = OnceLock::new();
static OFF: OnceLock<String> = OnceLock::new();

/// Cut `input` on the process's shared device, or by the CPU law with
/// the reason. `Err` only for an input that neither law can cut (it
/// names the bad value).
pub fn cut_or_cpu(input: &CutInput) -> Result<CutOutput, String> {
    input.check()?;
    let gpu = shared_device().and_then(|gpu| {
        let kernel = KERNEL.get_or_init(|| CutKernel::new(&gpu.device));
        kernel.as_ref().map(|k| (gpu, k)).map_err(Clone::clone)
    });
    cut_or_cpu_on(gpu, input)
}

/// [`cut_or_cpu`] on a given device and kernel (or on the reason that
/// there is none): the GPU result when it passes the spot check, else
/// the CPU law.
pub fn cut_or_cpu_on(gpu: Result<(&HeadlessGpu, &CutKernel), String>, input: &CutInput) -> Result<CutOutput, String> {
    answer(gpu, input, None)
}

/// The incremental cut (#2629, GPU stock sim S3a): cut `input` onto the
/// earlier layer `base` on the process's shared device, or by the CPU law
/// (`cut_cpu_onto`) with the reason. The run-time choice is the one of
/// [`cut_or_cpu`]: the off switch, no adapter, no compute, a lost device
/// and a failed spot check all give the CPU law. `Err` only for an input
/// or a base layer that neither law can cut.
pub fn cut_onto_or_cpu(input: &CutInput, base: &[f32]) -> Result<CutOutput, String> {
    input.check()?;
    let gpu = shared_device().and_then(|gpu| {
        let kernel = KERNEL.get_or_init(|| CutKernel::new(&gpu.device));
        kernel.as_ref().map(|k| (gpu, k)).map_err(Clone::clone)
    });
    cut_onto_or_cpu_on(gpu, input, base)
}

/// [`cut_onto_or_cpu`] on a given device and kernel, or on the reason
/// that there is none.
pub fn cut_onto_or_cpu_on(
    gpu: Result<(&HeadlessGpu, &CutKernel), String>,
    input: &CutInput,
    base: &[f32],
) -> Result<CutOutput, String> {
    answer(gpu, input, Some(base))
}

/// The one fallback rule of both entries: `base` is `None` for a cut
/// from the floor.
fn answer(gpu: Result<(&HeadlessGpu, &CutKernel), String>, input: &CutInput, base: Option<&[f32]>) -> Result<CutOutput, String> {
    let n = input.check()?;
    if let Some(base) = base {
        check_base(base, n)?;
    }
    let run = || -> Result<CutOutput, String> {
        if let Some(why) = OFF.get() {
            return Err(why.clone());
        }
        let (gpu, kernel) = gpu?;
        let compute = gpu.compute.as_ref().map_err(Clone::clone)?;
        let values = match base {
            None => kernel.run(&gpu.device, &gpu.queue, compute, input),
            Some(base) => kernel.run_onto(&gpu.device, &gpu.queue, compute, input, base),
        }
        .inspect_err(|e| {
            if let Some(why) = e.off_reason() {
                let _ = OFF.set(why);
            }
        })
        .map_err(|e| e.message)?;
        check_against(input, base, &values).inspect_err(|e| {
            let _ = OFF.set(format!("{e}; the GPU stock cut is off for this process"));
        })?;
        Ok(CutOutput { values, path: CutPath::Gpu { adapter: gpu.name.clone() } })
    };
    match run() {
        Ok(out) => Ok(out),
        Err(reason) => {
            log_fallback_once(&reason);
            let values = match base {
                None => cut_cpu(input)?,
                Some(base) => cut_cpu_onto(input, base)?,
            };
            Ok(CutOutput { values, path: CutPath::Cpu { reason } })
        }
    }
}

/// Compare the GPU layer with the CPU law on the spot probes: the
/// deepest claimed cell of evenly spaced items and [`SPOT_LATTICE_TEXELS`]
/// fixed places across the lattice, each evaluated over every item that
/// covers it. Values below `-cap` never show and compare clamped there.
pub fn spot_check(input: &CutInput, gpu: &[f32]) -> Result<(), String> {
    check_against(input, None, gpu)
}

/// [`spot_check`] for an incremental cut (#2629): the CPU law at each
/// probe starts from the `base` layer's value there.
pub fn spot_check_onto(input: &CutInput, base: &[f32], gpu: &[f32]) -> Result<(), String> {
    check_against(input, Some(base), gpu)
}

fn check_against(input: &CutInput, base: Option<&[f32]>, gpu: &[f32]) -> Result<(), String> {
    let n = input.check()?;
    if gpu.len() != n {
        return Err(format!("stock cut kernel: the GPU gave {} texels, not {n}", gpu.len()));
    }
    if let Some(base) = base {
        check_base(base, n)?;
    }
    let g = &input.grid;
    for at in spot_probes(input)? {
        let start = base.and_then(|b| b.get(at).copied()).unwrap_or(-4.0 * g.cap);
        let (want, margin) = want_at(input, start, cell_of_slot(at, g.dims));
        let got = gpu.get(at).copied().unwrap_or(f32::NAN);
        let (w, h) = (want.max(-g.cap), got.max(-g.cap));
        let bound = (margin * SPOT_MARGIN_FRACTION).max(f32::EPSILON * g.cap);
        if !got.is_finite() || (h - w).abs() > bound {
            return Err(format!(
                "stock cut kernel: the GPU texel {at} reads {got}, the CPU law {want} (bound {bound}); the GPU result is not used"
            ));
        }
    }
    Ok(())
}

/// The lattice slots that [`spot_check`] compares: the deepest claimed
/// cell of each sampled item, then [`SPOT_LATTICE_TEXELS`] fixed places
/// across the lattice. The item probes come first, so a test can take
/// the first one as a claimed cell.
pub fn spot_probes(input: &CutInput) -> Result<Vec<usize>, String> {
    input.check()?;
    let g = &input.grid;
    let mut probes = Vec::with_capacity(SPOT_TEXELS + SPOT_LATTICE_TEXELS);
    let step = input.items.len().div_ceil(SPOT_TEXELS).max(1);
    for item in input.items.iter().step_by(step) {
        let Some(local) = deepest_local(item, g) else {
            continue;
        };
        let Some(at) = slot(item, local, g.dims) else {
            return Err("stock cut kernel: a checked item left the lattice".into());
        };
        probes.push(at);
    }
    // 8 fixed places: the quarter and three-quarter cell of each axis.
    let [dx, dy, dz] = g.dims.map(|d| d as usize);
    for z in [dz / 4, 3 * dz / 4] {
        for y in [dy / 4, 3 * dy / 4] {
            for x in [dx / 4, 3 * dx / 4] {
                probes.push((z * dy + y) * dx + x);
            }
        }
    }
    Ok(probes)
}

/// The local cell of `item` with the deepest cut (the largest value; the
/// layer folds with the max law), or `None` when the item claims no cell.
fn deepest_local(item: &CutItem, grid: &CutGrid) -> Option<[u32; 3]> {
    let (mut deepest, mut at) = (None, None);
    for z in item.lo[2]..=item.hi[2] {
        for y in item.lo[1]..=item.hi[1] {
            for x in item.lo[0]..=item.hi[0] {
                let local = [x, y, z];
                let Some(v) = cell_cut(item, local, grid) else {
                    continue;
                };
                if deepest.is_none_or(|d| v > d) {
                    (deepest, at) = (Some(v), Some(local));
                }
            }
        }
    }
    at
}

/// The CPU cut at absolute cell `at` from the layer value `start`, and
/// the claim margin that bounds the GPU value there: the largest margin
/// of an item whose CPU value is within its own margin of the result.
/// An item that ties with the winner (or with a base value that wins, for
/// example a spring pass over a keyframe) can come out up to its bound
/// deeper on the GPU, so its margin counts too (#2629, S3b); a tie must
/// not refuse a good GPU layer and latch the GPU path off. `0` when no
/// item comes near.
fn want_at(input: &CutInput, start: f32, at: [u32; 3]) -> (f32, f32) {
    let g = &input.grid;
    let claims = || {
        input.items.iter().filter_map(move |item| {
            let l: [i64; 3] = std::array::from_fn(|a| i64::from(at[a]) - i64::from(item.base[a]));
            let inside = (0..3).all(|a| i64::from(item.lo[a]) <= l[a] && l[a] <= i64::from(item.hi[a]));
            if !inside {
                return None;
            }
            let l = l.map(|v| u32::try_from(v).unwrap_or(0));
            cell_cut(item, l, g).map(|v| (v, item.margin))
        })
    };
    let want = claims().fold(start, |w, (v, _)| w.max(v));
    let margin = claims().filter(|(v, m)| want - v <= *m).fold(0.0f32, |acc, (_, m)| acc.max(m));
    (want, margin)
}

/// The absolute cell `[x, y, z]` of lattice slot `s`.
fn cell_of_slot(s: usize, dims: [u32; 3]) -> [u32; 3] {
    let [dx, dy, _] = dims.map(|d| d as usize);
    [(s % dx) as u32, ((s / dx) % dy) as u32, (s / (dx * dy)) as u32]
}

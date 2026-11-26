//! camcraft-gpu: the headless GPU compute seam (#2620). One windowless
//! device for each process, a capability probe, the off switch and the
//! CPU fallback rule, and the kernels: conservative facet binning, and
//! the stock removal cut (#2627, display only) and its incremental start
//! onto an earlier layer (#2629).
//! Provenance: our own seam over wgpu's public API; the binning law is
//! uniform-grid partitioning from C. Ericson, "Real-Time Collision
//! Detection" (2005), ch. 7.
//!
//! The fallback rule: every entry answers `Result`. An `Err` names the
//! reason (no adapter, no compute, the switch is off, a lost device, a
//! buffer past the limit, a failed kernel or read-back), and the caller
//! runs its own CPU code. A GPU answer is only a filter: the caller's f64
//! law stays the judge, so no posted output depends on this crate.

mod adapter;
pub mod binning;
#[cfg(not(target_arch = "wasm32"))]
mod binning_gpu;
// #2627: the stock removal kernel (ball and flat, linear moves).
pub mod cut;
#[cfg(not(target_arch = "wasm32"))]
mod cut_buf;
#[cfg(not(target_arch = "wasm32"))]
mod cut_gpu;
#[cfg(not(target_arch = "wasm32"))]
mod cut_run;
pub mod probe;
#[cfg(not(target_arch = "wasm32"))]
mod shared;
pub mod switch;

pub use adapter::{device_async, gpu_required, AdapterKind, HeadlessGpu, REQUIRE_GPU_ENV};
#[cfg(not(target_arch = "wasm32"))]
pub use adapter::{device, device_for_test};
pub use binning::{bin_twin, pad_boxes, plan, CellLists, GridSpec, PaddedBoxes, Plan};
#[cfg(not(target_arch = "wasm32"))]
pub use binning_gpu::{bin_facets, bin_on};
pub use cut::{cut_cpu, cut_cpu_onto, CutGrid, CutInput, CutItem, CUT_BRICK, MAX_CUT_TEXELS};
#[cfg(not(target_arch = "wasm32"))]
pub use cut_gpu::{CutError, CutErrorKind, CutKernel, CUT_WAIT, ITEMS_PER_DISPATCH};
#[cfg(not(target_arch = "wasm32"))]
pub use cut_run::{
    cut_onto_or_cpu, cut_onto_or_cpu_on, cut_or_cpu, cut_or_cpu_on, spot_check, spot_check_onto, spot_probes, CutOutput,
    CutPath, SPOT_LATTICE_TEXELS, SPOT_MARGIN_FRACTION, SPOT_TEXELS,
};
pub use probe::{probe, Compute};
#[cfg(not(target_arch = "wasm32"))]
pub use shared::shared_device;
pub use switch::{compute_switch, COMPUTE_ENV};

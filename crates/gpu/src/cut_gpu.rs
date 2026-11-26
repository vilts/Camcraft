//! The GPU half of the stock removal kernel (#2627, GPU stock sim S2):
//! the pipelines of `cut.wgsl`, the upload of a checked batch, the
//! fill (or encode) / cut / decode passes, and a read-back with a time
//! bound. #2629 (S3a) adds the incremental start: a batch cut onto an
//! earlier layer, read back (`run_onto`) or left in a buffer of the
//! caller (`cut_into`). Every failure (a limit, a validation, internal or
//! out-of-memory error, a failed map, a poll past its time bound) is an
//! `Err` with the reason; the caller then takes the CPU law
//! (`cut_run::cut_or_cpu`).
//! Provenance: our own dispatch over wgpu's public compute API.

use std::time::Duration;

use crate::cut::{check_base, CutInput};
use crate::cut_buf::{item_bytes, layer_bytes, read_back, Layout, PARAMS_BYTES};
use crate::probe::Compute;

const SHADER: &str = include_str!("cut.wgsl");

/// Work items in one dispatch of `cs_cut`: small enough that one
/// dispatch of the golden-section path (about 34 M pose evaluations)
/// stays far under a 2 s driver watchdog on a slow device.
pub const ITEMS_PER_DISPATCH: u32 = 256;

/// The longest wait for one batch to come back. Past it the batch is a
/// failure and the CPU law answers.
pub const CUT_WAIT: Duration = Duration::from_secs(10);

/// The kind of a kernel failure, so the caller's latch reads a value and
/// not the text of the reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutErrorKind {
    /// A wait ended without an answer: the poll passed [`CUT_WAIT`], or
    /// the read-back map did not complete. A device that does not answer
    /// once is not asked again.
    NoAnswer,
    /// Any other failure: a limit, a validation, a memory error, or a
    /// batch that the kernel refuses.
    Other,
}

/// A failure of the GPU half: the reason and its kind.
#[derive(Clone, Debug, PartialEq)]
pub struct CutError {
    pub kind: CutErrorKind,
    pub message: String,
}

impl CutError {
    /// A failure that is not a timeout.
    pub fn other(message: impl Into<String>) -> CutError {
        CutError { kind: CutErrorKind::Other, message: message.into() }
    }

    /// A wait that ended without an answer.
    pub fn no_answer(message: impl Into<String>) -> CutError {
        CutError { kind: CutErrorKind::NoAnswer, message: message.into() }
    }

    /// The reason to turn the GPU path off for the rest of the process,
    /// or `None` when this failure leaves the path on.
    pub fn off_reason(&self) -> Option<String> {
        (self.kind == CutErrorKind::NoAnswer)
            .then(|| format!("{}; the GPU stock cut is off for this process", self.message))
    }
}

impl From<String> for CutError {
    fn from(message: String) -> CutError {
        CutError::other(message)
    }
}

impl std::fmt::Display for CutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// The compiled kernel for one device: build it one time, run it often.
pub struct CutKernel {
    bgl: wgpu::BindGroupLayout,
    fill: wgpu::ComputePipeline,
    encode: wgpu::ComputePipeline,
    cut: wgpu::ComputePipeline,
    decode: wgpu::ComputePipeline,
}

/// How a batch starts: from the empty `-4 cap` floor, or from the layer
/// that the target buffer already holds (#2629, a playback keyframe).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Start {
    Floor,
    Layer,
}

/// The error class of a wgpu error, for the fallback reason.
pub(crate) fn gpu_error(err: wgpu::Error) -> String {
    let kind = match &err {
        wgpu::Error::OutOfMemory { .. } => "out of memory",
        wgpu::Error::Internal { .. } => "internal",
        wgpu::Error::Validation { .. } => "validation",
    };
    format!("stock cut kernel: GPU {kind} error: {err}")
}

/// Run `f` inside the three error scopes; any captured error wins.
fn scoped<T, E: From<String>>(device: &wgpu::Device, f: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    let oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let result = f();
    let mut failure = None;
    for scope in [validation, internal, oom] {
        if let Some(err) = pollster::block_on(scope.pop()) {
            failure.get_or_insert(gpu_error(err));
        }
    }
    match failure {
        Some(err) => Err(E::from(err)),
        None => result,
    }
}

fn entry(binding: u32, ty: wgpu::BufferBindingType, dynamic: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: dynamic, min_binding_size: None },
        count: None,
    }
}

fn buffer(device: &wgpu::Device, label: &str, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage, mapped_at_creation: false })
}

impl CutKernel {
    /// Compile the four pipelines on `device`.
    pub fn new(device: &wgpu::Device) -> Result<CutKernel, String> {
        scoped(device, || {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("camcraft-gpu stock cut"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
            use wgpu::BufferBindingType as B;
            let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camcraft-gpu stock cut"),
                entries: &[
                    entry(0, B::Uniform, true),
                    entry(1, B::Storage { read_only: true }, false),
                    entry(2, B::Storage { read_only: false }, false),
                ],
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("camcraft-gpu stock cut"),
                bind_group_layouts: &[Some(&bgl)],
                immediate_size: 0,
            });
            let pipe = |name: &str| {
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(name),
                    layout: Some(&layout),
                    module: &module,
                    entry_point: Some(name),
                    compilation_options: Default::default(),
                    cache: None,
                })
            };
            Ok(CutKernel {
                fill: pipe("cs_fill"),
                encode: pipe("cs_encode"),
                cut: pipe("cs_cut"),
                decode: pipe("cs_decode"),
                bgl,
            })
        })
    }

    /// Cut `input` on `device`: the dense f32 layer of `cut_cpu`'s
    /// layout. Blocks for at most about [`CUT_WAIT`]. `input` must have
    /// passed [`CutInput::check`]; it is checked again here.
    pub fn run(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        compute: &Compute,
        input: &CutInput,
    ) -> Result<Vec<f32>, CutError> {
        let n_tex = input.check()?;
        let plan = Layout::of(device, compute, input, n_tex)?;
        scoped(device, || {
            use wgpu::BufferUsages as U;
            let keys = buffer(device, "stock cut keys", plan.out_bytes, U::STORAGE | U::COPY_SRC);
            let enc = self.record(device, queue, input, &plan, &keys, Start::Floor)?;
            read_after(device, queue, enc, &keys, plan.out_bytes)
        })
    }

    /// The incremental cut (#2629, S3a), read back: cut `input` onto the
    /// earlier layer `base` (the layout of `cut_cpu`). It answers what
    /// `cut_cpu_onto` answers on the same values. Blocks for at most
    /// about [`CUT_WAIT`].
    pub fn run_onto(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        compute: &Compute,
        input: &CutInput,
        base: &[f32],
    ) -> Result<Vec<f32>, CutError> {
        let n_tex = input.check()?;
        check_base(base, n_tex)?;
        let plan = Layout::of(device, compute, input, n_tex)?;
        let bytes = layer_bytes(base)?;
        scoped(device, || {
            use wgpu::BufferUsages as U;
            let keys = buffer(device, "stock cut layer", plan.out_bytes, U::STORAGE | U::COPY_SRC | U::COPY_DST);
            queue.write_buffer(&keys, 0, &bytes);
            let enc = self.record(device, queue, input, &plan, &keys, Start::Layer)?;
            read_after(device, queue, enc, &keys, plan.out_bytes)
        })
    }

    /// The incremental cut (#2629, S3a) into a buffer of the caller, with
    /// no read-back. `target` holds a layer in the `cut_cpu` layout (a
    /// display buffer, such as the viewport's cut buffer); afterwards it
    /// holds that layer with `input` cut into it. It needs `STORAGE` usage
    /// and four bytes for each texel. The caller polls the returned
    /// submission. A NaN in `target` stays a NaN (display only). Errors
    /// are caught by error scopes on the calling thread.
    pub fn cut_into(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        compute: &Compute,
        input: &CutInput,
        target: &wgpu::Buffer,
    ) -> Result<wgpu::SubmissionIndex, CutError> {
        let n_tex = input.check()?;
        let plan = Layout::of(device, compute, input, n_tex)?;
        if !target.usage().contains(wgpu::BufferUsages::STORAGE) {
            return Err(CutError::other("stock cut kernel: the target buffer has no STORAGE usage"));
        }
        if target.size() < plan.out_bytes {
            return Err(CutError::other(format!(
                "stock cut kernel: the target buffer has {} bytes, the lattice needs {}",
                target.size(),
                plan.out_bytes
            )));
        }
        scoped(device, || {
            let enc = self.record(device, queue, input, &plan, target, Start::Layer)?;
            Ok(queue.submit([enc.finish()]))
        })
    }

    /// Upload the batch and record the passes on `target`: the start
    /// (fill or encode), the chunked cut, the decode.
    fn record(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input: &CutInput,
        p: &Layout,
        target: &wgpu::Buffer,
        start: Start,
    ) -> Result<wgpu::CommandEncoder, String> {
        use wgpu::BufferUsages as U;
        let params = buffer(device, "stock cut params", p.params_bytes, U::UNIFORM | U::COPY_DST);
        let items = buffer(device, "stock cut items", p.item_bytes, U::STORAGE | U::COPY_DST);
        queue.write_buffer(&params, 0, &p.params);
        queue.write_buffer(&items, 0, &item_bytes(&input.items, p.item_bytes)?);
        let sized = |buffer, size| wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer, offset: 0, size });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camcraft-gpu stock cut"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: sized(&params, wgpu::BufferSize::new(PARAMS_BYTES)) },
                wgpu::BindGroupEntry { binding: 1, resource: items.as_entire_binding() },
                // the lattice bytes only: a larger caller buffer can pass the binding limit
                wgpu::BindGroupEntry { binding: 2, resource: sized(target, wgpu::BufferSize::new(p.out_bytes)) },
            ],
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("stock cut") });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("stock cut"), timestamp_writes: None });
            pass.set_bind_group(0, &group, &[0]);
            pass.set_pipeline(match start {
                Start::Floor => &self.fill,
                Start::Layer => &self.encode,
            });
            pass.dispatch_workgroups(p.tex_groups[0], p.tex_groups[1], 1);
            pass.set_pipeline(&self.cut);
            for (k, count) in p.chunks.iter().enumerate() {
                pass.set_bind_group(0, &group, &[p.offset(k)?]);
                pass.dispatch_workgroups(*count, 1, 1);
            }
            pass.set_bind_group(0, &group, &[0]);
            pass.set_pipeline(&self.decode);
            pass.dispatch_workgroups(p.tex_groups[0], p.tex_groups[1], 1);
        }
        Ok(enc)
    }
}

/// Copy `keys` out after the recorded passes, submit, and read it back.
fn read_after(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut enc: wgpu::CommandEncoder,
    keys: &wgpu::Buffer,
    out_bytes: u64,
) -> Result<Vec<f32>, CutError> {
    use wgpu::BufferUsages as U;
    let read = buffer(device, "stock cut read", out_bytes, U::MAP_READ | U::COPY_DST);
    enc.copy_buffer_to_buffer(keys, 0, &read, 0, out_bytes);
    let at = queue.submit([enc.finish()]);
    read_back(device, &read, at, out_bytes)
}

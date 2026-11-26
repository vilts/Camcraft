//! The GPU half of conservative facet binning (#2620): upload the padded
//! boxes, dispatch `binning.wgsl`, read the cell bit sets back. Every
//! failure (a plan past the device limits, a validation error, a failed
//! map, a lost device) is an `Err` with the reason, and the caller keeps
//! its CPU f64 grid. The result is only a filter: the caller's exact f64
//! test still decides every facet. Provenance: our own dispatch over
//! wgpu's public compute API.

use std::sync::mpsc;

use crate::adapter::HeadlessGpu;
use crate::binning::{host_vec, pad_boxes, plan, CellLists, GridSpec, PaddedBoxes, Plan};
use crate::shared::shared_device;
use crate::switch::{env_switch, log_fallback_once};

const SHADER: &str = include_str!("binning.wgsl");

/// Bin `boxes` (f64, tool radius already added) on the process's shared
/// device. `Err` names why the GPU did not run; that reason is logged one
/// time for each process, and the caller runs its CPU grid.
pub fn bin_facets(boxes: &[[f64; 4]], spec: &GridSpec) -> Result<CellLists, String> {
    let run = || -> Result<CellLists, String> {
        env_switch()?;
        let padded = pad_boxes(boxes, spec)?;
        let gpu = shared_device()?;
        bin_on(gpu, &padded)
    };
    run().inspect_err(|e| log_fallback_once(e))
}

/// Bin the padded boxes on `gpu`. Blocks until the read-back is done.
pub fn bin_on(gpu: &HeadlessGpu, boxes: &PaddedBoxes) -> Result<CellLists, String> {
    let compute = gpu.compute.as_ref().map_err(Clone::clone)?;
    let p = plan(boxes, compute)?;
    if p.n == 0 {
        return CellLists::from_bits(&p, boxes.nx, boxes.ny, &[]);
    }
    let oom = gpu.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let internal = gpu.device.push_error_scope(wgpu::ErrorFilter::Internal);
    let validation = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let result = dispatch(gpu, boxes, &p).and_then(|read| read_back(gpu, &read, &p));
    // Pop every scope even if dispatch or mapping failed. Keep them active
    // through submission, device polling and read-back for asynchronous errors.
    let mut failure = None;
    for scope in [validation, internal, oom] {
        if let Some(err) = pollster::block_on(scope.pop()) {
            failure = Some(gpu_error(err));
        }
    }
    if let Some(err) = failure {
        return Err(err);
    }
    CellLists::from_bits(&p, boxes.nx, boxes.ny, &result?)
}

fn gpu_error(err: wgpu::Error) -> String {
    let kind = match &err {
        wgpu::Error::OutOfMemory { .. } => "out of memory",
        wgpu::Error::Internal { .. } => "internal",
        wgpu::Error::Validation { .. } => "validation",
    };
    format!("binning: GPU {kind} error: {err}")
}

fn params_bytes(boxes: &PaddedBoxes, p: &Plan) -> [u8; 32] {
    let mut v = [0; 32];
    for (chunk, w) in v[..16].as_chunks_mut::<4>().0.iter_mut().zip([boxes.nx, boxes.ny, p.n, p.words]) {
        chunk.copy_from_slice(&w.to_le_bytes());
    }
    v[16..20].copy_from_slice(&boxes.s.to_le_bytes());
    v
}

fn box_bytes(boxes: &PaddedBoxes) -> Result<Vec<u8>, String> {
    let mut v = host_vec(boxes.boxes.len().checked_mul(16).ok_or("binning: host upload size overflow")?)?;
    for b in &boxes.boxes {
        for f in b {
            v.extend_from_slice(&f.to_le_bytes());
        }
    }
    Ok(v)
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Record and submit the dispatch; the staging buffer to read.
fn dispatch(gpu: &HeadlessGpu, boxes: &PaddedBoxes, p: &Plan) -> Result<wgpu::Buffer, String> {
    let dev = &gpu.device;
    let shader = dev.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("camcraft-gpu binning"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let uniform = wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    };
    let bgl = dev.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("camcraft-gpu binning"),
        entries: &[uniform, storage_entry(1, true), storage_entry(2, false)],
    });
    let layout = dev.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("camcraft-gpu binning"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pipeline = dev.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("camcraft-gpu binning"),
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("cs_bin"),
        compilation_options: Default::default(),
        cache: None,
    });
    let buf =
        |label, size, usage| dev.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage, mapped_at_creation: false });
    use wgpu::BufferUsages as U;
    let params = buf("binning params", 32, U::UNIFORM | U::COPY_DST);
    let input = buf("binning boxes", p.in_bytes, U::STORAGE | U::COPY_DST);
    let bits = buf("binning bits", p.out_bytes, U::STORAGE | U::COPY_SRC);
    let read = buf("binning read", p.out_bytes, U::MAP_READ | U::COPY_DST);
    gpu.queue.write_buffer(&params, 0, &params_bytes(boxes, p));
    gpu.queue.write_buffer(&input, 0, &box_bytes(boxes)?);
    let group = dev.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("camcraft-gpu binning"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: input.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: bits.as_entire_binding() },
        ],
    });
    let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("camcraft-gpu binning") });
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("binning"), timestamp_writes: None });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(p.groups, 1, 1);
    }
    enc.copy_buffer_to_buffer(&bits, 0, &read, 0, p.out_bytes);
    gpu.queue.submit([enc.finish()]);
    Ok(read)
}

fn read_back(gpu: &HeadlessGpu, read: &wgpu::Buffer, p: &Plan) -> Result<Vec<u32>, String> {
    let (tx, rx) = mpsc::channel();
    read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| format!("binning: the GPU poll failed: {e}"))?;
    match rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(format!("binning: the read-back map failed: {e}")),
        Err(_) => return Err("binning: the read-back map never completed".into()),
    }
    let words = {
        let view = read.get_mapped_range(..).map_err(|e| format!("binning: the read-back range failed: {e:?}"))?;
        if view.len() as u64 != p.out_bytes {
            return Err(format!("binning: the read-back has {} bytes, not {}", view.len(), p.out_bytes));
        }
        let mut words = host_vec(view.len() / 4)?;
        words.extend(view.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)));
        words
    };
    read.unmap();
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::gpu_error;

    #[test]
    fn gpu_failures_name_the_error_class_for_cpu_fallback() {
        let source = || Box::new(std::io::Error::other("injected")) as wgpu::ErrorSource;
        for (err, kind) in [
            (wgpu::Error::OutOfMemory { source: source() }, "out of memory"),
            (wgpu::Error::Internal { source: source(), description: "injected".into() }, "internal"),
            (wgpu::Error::Validation { source: source(), description: "injected".into() }, "validation"),
        ] {
            let message = gpu_error(err);
            assert!(message.starts_with(&format!("binning: GPU {kind} error:")), "{message}");
        }
    }
}

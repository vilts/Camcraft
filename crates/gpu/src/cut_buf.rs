//! The buffers of the stock removal kernel (#2627, GPU stock sim S2; the
//! incremental start of #2629): the batch plan checked against the device
//! limits, the byte form of the items and of a layer, and the read-back
//! with a time bound. Split from `cut_gpu.rs` (the 400-line law).
//! Provenance: our own buffer plan over wgpu's public compute API.

use std::sync::mpsc;
use std::time::Duration;

use crate::cut::{CutInput, CutItem};
use crate::cut_gpu::{CutError, CUT_WAIT, ITEMS_PER_DISPATCH};
use crate::probe::Compute;

/// Bytes of one `Item` in `cut.wgsl` (five 16-byte rows).
const ITEM_BYTES: u64 = 80;

/// Bytes of `Params` in `cut.wgsl`.
pub(crate) const PARAMS_BYTES: u64 = 48;

/// The buffer plan of one batch, checked against the device limits.
pub(crate) struct Layout {
    pub(crate) params: Vec<u8>,
    pub(crate) params_bytes: u64,
    stride: u64,
    pub(crate) item_bytes: u64,
    pub(crate) out_bytes: u64,
    pub(crate) tex_groups: [u32; 2],
    pub(crate) chunks: Vec<u32>,
}

impl Layout {
    pub(crate) fn of(device: &wgpu::Device, compute: &Compute, input: &CutInput, n_tex: usize) -> Result<Layout, String> {
        let limit = compute.max_storage_buffer_binding_size.min(compute.max_buffer_size);
        let over = |what: &str, bytes: u64| format!("stock cut kernel: the {what} need {bytes} bytes, over the device limit {limit}");
        let out_bytes = (n_tex as u64).saturating_mul(4);
        let n_items = input.items.len() as u64;
        // an empty batch still binds a one-item buffer
        let item_bytes = n_items.max(1).saturating_mul(ITEM_BYTES);
        if out_bytes > limit {
            return Err(over("texels", out_bytes));
        }
        if item_bytes > limit {
            return Err(over("work items", item_bytes));
        }
        let max_dim = compute.max_compute_workgroups_per_dimension.max(1);
        let groups = u32::try_from(n_tex.div_ceil(64)).map_err(|_| "stock cut kernel: too many texel groups".to_string())?;
        let row = groups.min(max_dim);
        let rows = groups.div_ceil(row.max(1));
        if rows > max_dim {
            return Err(format!("stock cut kernel: {groups} texel groups pass the dispatch limit {max_dim}"));
        }
        let mut chunks = Vec::new();
        let n = u32::try_from(n_items).map_err(|_| "stock cut kernel: too many work items".to_string())?;
        let mut first = 0u32;
        while first < n {
            chunks.push((n - first).min(ITEMS_PER_DISPATCH));
            first = first.saturating_add(ITEMS_PER_DISPATCH);
        }
        let stride = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(PARAMS_BYTES).next_multiple_of(16);
        let blocks = chunks.len().max(1);
        let params_bytes = stride.saturating_mul(blocks as u64);
        let mut params = Vec::new();
        params.try_reserve_exact(params_bytes as usize).map_err(|e| format!("stock cut kernel: no host memory: {e}"))?;
        params.resize(params_bytes as usize, 0);
        let g = &input.grid;
        let n_tex32 = u32::try_from(n_tex).map_err(|_| "stock cut kernel: too many texels".to_string())?;
        for (k, block) in params.chunks_exact_mut(stride as usize).enumerate() {
            let count = chunks.get(k).copied().unwrap_or(0);
            let first = u32::try_from(k).unwrap_or(u32::MAX).saturating_mul(ITEMS_PER_DISPATCH);
            let words = [g.dims[0], g.dims[1], g.dims[2], n_tex32, first, count, row, g.floor_key(), g.cell.to_bits(), g.cap.to_bits()];
            for (dst, w) in block.as_chunks_mut::<4>().0.iter_mut().zip(words) {
                dst.copy_from_slice(&w.to_le_bytes());
            }
        }
        Ok(Layout { params, params_bytes, stride, item_bytes, out_bytes, tex_groups: [row, rows], chunks })
    }

    pub(crate) fn offset(&self, k: usize) -> Result<u32, String> {
        u32::try_from(self.stride.saturating_mul(k as u64)).map_err(|_| "stock cut kernel: the uniform offset overflows".into())
    }
}

pub(crate) fn item_bytes(items: &[CutItem], bytes: u64) -> Result<Vec<u8>, String> {
    let mut v = Vec::new();
    v.try_reserve_exact(bytes as usize).map_err(|e| format!("stock cut kernel: no host memory: {e}"))?;
    for it in items {
        let row = |xyz: [u32; 3], w: u32| [xyz[0], xyz[1], xyz[2], w];
        let f = |p: [f32; 3]| p.map(f32::to_bits);
        let words = [
            row(f(it.a), it.r.to_bits()),
            row(f(it.b), it.flank.to_bits()),
            row(it.base.map(|v| v as u32), u32::from(it.ball)),
            row(it.lo, it.margin.to_bits()),
            row(it.hi, 0),
        ];
        for w in words.iter().flatten() {
            v.extend_from_slice(&w.to_le_bytes());
        }
    }
    v.resize(bytes as usize, 0);
    Ok(v)
}

/// The little-endian bytes of a layer (the `cs_decode` form), for the
/// upload of a base layer (#2629).
pub(crate) fn layer_bytes(layer: &[f32]) -> Result<Vec<u8>, String> {
    let mut v = Vec::new();
    v.try_reserve_exact(layer.len().saturating_mul(4)).map_err(|e| format!("stock cut kernel: no host memory: {e}"))?;
    for x in layer {
        v.extend_from_slice(&x.to_le_bytes());
    }
    Ok(v)
}

/// Map the read buffer and wait at most [`CUT_WAIT`] for it.
pub(crate) fn read_back(
    device: &wgpu::Device,
    read: &wgpu::Buffer,
    at: wgpu::SubmissionIndex,
    out_bytes: u64,
) -> Result<Vec<f32>, CutError> {
    let (tx, rx) = mpsc::channel();
    read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::Wait { submission_index: Some(at), timeout: Some(CUT_WAIT) })
        .map_err(|e| CutError::no_answer(format!("stock cut kernel: the GPU did not answer within {CUT_WAIT:?}: {e}")))?;
    map_wait_end(rx.recv_timeout(Duration::from_millis(100)))?;
    let out = {
        let view = read
            .get_mapped_range(..)
            .map_err(|e| CutError::other(format!("stock cut kernel: the read-back range failed: {e:?}")))?;
        if view.len() as u64 != out_bytes {
            return Err(CutError::other(format!("stock cut kernel: the read-back has {} bytes, not {out_bytes}", view.len())));
        }
        let mut out = Vec::new();
        out.try_reserve_exact(view.len() / 4).map_err(|e| CutError::other(format!("stock cut kernel: no host memory: {e}")))?;
        out.extend(view.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)));
        out
    };
    read.unmap();
    Ok(out)
}

/// Classify the read-back map wait: a wait that ends without the map is
/// a device that did not answer (the kind that the latch reads), while a
/// map that fails with an error is not a stalled device.
pub(crate) fn map_wait_end(wait: Result<Result<(), wgpu::BufferAsyncError>, mpsc::RecvTimeoutError>) -> Result<(), CutError> {
    match wait {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(CutError::other(format!("stock cut kernel: the read-back map failed: {e}"))),
        Err(_) => Err(CutError::no_answer("stock cut kernel: the read-back map did not complete")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cut_gpu::CutErrorKind;

    #[test]
    fn the_read_back_wait_that_ends_is_a_no_answer() {
        // The map wait that ends is the same kind as the poll wait: the
        // latch must fire for its text, not only for "did not answer".
        let ended = map_wait_end(Err(mpsc::RecvTimeoutError::Timeout));
        assert_eq!(ended.expect_err("the map wait ended").kind, CutErrorKind::NoAnswer);
        assert!(map_wait_end(Ok(Ok(()))).is_ok(), "a map that completed is not a failure");
    }

    #[test]
    fn a_layer_uploads_as_its_little_endian_bits() {
        let bytes = layer_bytes(&[1.0, -0.5]).expect("two texels");
        assert_eq!(bytes.len(), 8);
        assert_eq!(&bytes[..4], &1.0f32.to_le_bytes());
        assert_eq!(&bytes[4..], &(-0.5f32).to_le_bytes());
    }
}

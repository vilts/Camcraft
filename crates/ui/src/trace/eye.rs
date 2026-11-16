//! The viewport's stock eye (M7.2 B3b): owns the trace pass and the
//! composite, keeps the document's field view resident on the GPU, and
//! re-marches only when the camera, the extent or the field changed.
//! Provenance: Hart's sphere tracing (SIGGRAPH 1996) via `TracePass`
//! and the sim's CPU `preview`; frame caching is our own design.
//!
//! ## The laws here
//!
//! - **The GPU is a speedup, never a dependency.** A device that
//!   refuses the kernel, or a lattice past its storage limit, marches
//!   on the CPU instead - at a thumbnail resolution, upsampled by the
//!   same composite. Only a device that cannot composite at all keeps
//!   the sheet, and the shell says so.
//! - **A refusal is said once.** `prepare` answers `Err` the first time
//!   a frame fails, then quietly keeps the sheet for that same state -
//!   the status line is not spammed every frame, and the log names it.
//! - **Nothing stale.** The field the GPU holds is the `Arc` the state
//!   holds; a different (or absent) field view clears the resident
//!   lattice and the cached frame before anything else happens - except
//!   a playback frame of the same packed session, which rewrites only
//!   its cut layer in place (and still drops the cached frame) - only
//!   the patched brick runs when the patch's chain covers the resident
//!   frame (M13.1), the whole layer on any doubt.
//! - **The CPU march never blocks the frame (M13.1 leftover).** With no
//!   GPU scene the march runs on `cpu_eye`'s worker; `prepare` asks for
//!   the picture and returns at once. While the first picture is
//!   pending the slot draws what it drew before (`Ok(false)`: the sheet
//!   or the machined mesh); after that the last finished picture is
//!   blitted exactly as it was marched (its own camera and `mvp`) until
//!   the next one lands - a playback frame of the same session keeps
//!   the picture, never a sheet flicker between two marches.

use std::sync::Arc;
use std::time::Duration;

use egui_wgpu::wgpu;
use wgpu::util::DeviceExt;

use camcraft_app::stock_sim::preview::{pack_cut, pack_field, pack_target};
use camcraft_app::stock_sim::{Preview, PreviewCamera};
use camcraft_app::stock_view::{CutLayer, FieldView};
use camcraft_app::geometry::Point3;

use super::composite::{Composite, FrameGeom};
use super::cpu_eye::{CpuEye, CpuJob};
use super::eye_geom::{cam_bits, ortho_word, TAN_HALF_FOV};
pub use super::eye_geom::{encode, fit, preview_camera};
use super::TracePass;
use crate::viewport::Camera;

/// GPU trace budget per frame: past it the trace runs at an integer
/// fraction of the viewport (a 4K panel traces at half resolution).
pub(crate) const GPU_PIXELS: u64 = 1 << 21;

/// CPU fallback budget: a thumbnail the march answers interactively.
const CPU_PIXELS: u64 = 1 << 15;

/// What a cached frame was marched for (opaque outside the eye: the
/// CPU worker only carries it back).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Key {
    cam: [u64; 5],
    view: (u32, u32),
}

impl Key {
    /// A key no frame is ever shown under (a lost worker's answer).
    pub(crate) const NONE: Key = Key { cam: [u64::MAX; 5], view: (0, 0) };
}

struct Frame {
    key: Key,
    /// The field it was marched from (a CPU picture may outlive its
    /// source by a playback frame - the module law).
    source: Arc<FieldView>,
    bind: wgpu::BindGroup,
    // The bind group keeps the buffer alive; held for clarity of ownership.
    _buf: wgpu::Buffer,
    k: u32,
    trace: (u32, u32),
    /// The eye and projection it was marched for: what it blits with.
    pcam: PreviewCamera,
    mvp: [[f32; 4]; 4],
}

/// The trace + composite pair and whatever they hold resident.
pub struct StockEye {
    trace: Option<TracePass>,
    composite: Composite,
    source: Option<Arc<FieldView>>,
    /// The resident field is on the GPU (false: the CPU marches it).
    gpu_scene: bool,
    frame: Option<Frame>,
    /// The state a refusal was last said for (not re-said, not retried).
    refused: Option<Key>,
    /// The verdict palette the composite paints with (M13.4).
    verdict: [[f32; 3]; 4],
    /// The CPU march's worker, started on the first CPU frame.
    cpu: Option<CpuEye>,
}

impl StockEye {
    /// Build the eye for the viewport's color `format`. The trace pass
    /// may refuse (the CPU then marches); the composite may not - `Err`
    /// means this device keeps the sheet.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Result<Self, String> {
        pollster::block_on(Self::new_async(device, format))
    }

    /// `new`, awaiting the device instead of blocking on it (#31): the
    /// browser shell builds the eye this way, after its first frame.
    pub async fn new_async(device: &wgpu::Device, format: wgpu::TextureFormat) -> Result<Self, String> {
        let composite = Composite::new_async(device, format).await?;
        let trace = match TracePass::new_async(device).await {
            Ok(t) => Some(t),
            Err(e) => {
                diag_warn!("stock eye: {e} - the CPU march stands in");
                None
            }
        };
        let verdict = Self::verdict_of(&crate::egui_app::colorvision::Semantic::OFF);
        Ok(StockEye { trace, composite, source: None, gpu_scene: false, frame: None, refused: None, verdict, cpu: None })
    }

    /// Wait up to `timeout` for the CPU worker to finish a picture;
    /// tests only - the UI never waits. False with no worker or none.
    pub fn wait_cpu_picture(&self, timeout: Duration) -> bool {
        self.cpu.as_ref().is_some_and(|c| c.wait_picture(timeout))
    }

    fn verdict_of(p: &crate::egui_app::colorvision::Semantic) -> [[f32; 3]; 4] {
        [p.clean, p.rest, p.gouge, p.unjudged]
    }

    /// The verdict palette the next `prepare` stages (M13.4's colour-
    /// vision remap): clean, rest, gouge, unjudged.
    pub fn set_palette(&mut self, p: &crate::egui_app::colorvision::Semantic) {
        self.verdict = Self::verdict_of(p);
    }

    /// The same eye with its GPU march dropped: the CPU fallback a
    /// refusing device lands on, reachable on purpose for the tests.
    #[must_use]
    pub fn cpu_only(mut self) -> Self {
        self.trace = None;
        self.clear();
        self
    }

    /// Forget the resident field and frame (deviate off, document switch).
    pub fn clear(&mut self) {
        self.source = None;
        self.gpu_scene = false;
        self.frame = None;
        self.refused = None;
    }

    /// Ready a traced frame of `fv` for this camera and extent,
    /// recording any GPU march into `encoder`. `Ok(true)`: `draw` will
    /// paint it this frame. `Ok(false)`: keep the sheet (no field, or a
    /// refusal already said). `Err`: a NEW refusal, said once.
    #[allow(clippy::too_many_arguments)] // device, queue, encoder + the frame's own four
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        fv: Option<&Arc<FieldView>>,
        cam: &Camera,
        mvp: &[[f32; 4]; 4],
        view: (u32, u32),
    ) -> Result<bool, String> {
        let Some(fv) = fv else {
            if self.source.is_some() {
                self.clear();
            }
            return Ok(false);
        };
        if !self.source.as_ref().is_some_and(|s| Arc::ptr_eq(s, fv)) {
            if self.rewrite_cut(queue, fv) {
                // Same playback session: the cut layer moved, nothing else.
                self.source = Some(Arc::clone(fv));
                self.frame = None;
                self.refused = None;
            } else {
                // A CPU picture of the same packed session stands until
                // the next one lands (the module law); anything else
                // is forgotten with the lattice.
                let keep = (!self.gpu_scene).then(|| self.frame.take()).flatten().filter(|f| same_session(&f.source, fv));
                self.clear();
                self.frame = keep;
                self.source = Some(Arc::clone(fv));
                self.gpu_scene = self.upload(device, fv);
            }
        }
        let pcam = preview_camera(cam).ok_or("the camera basis is degenerate")?;
        let key = Key { cam: cam_bits(&pcam), view };
        if self.refused == Some(key) {
            return Ok(false);
        }
        if self.frame.as_ref().is_none_or(|f| f.key != key || !Arc::ptr_eq(&f.source, fv)) {
            if self.gpu_scene {
                self.frame = None;
            }
            match self.march(device, queue, encoder, fv, &pcam, key, mvp) {
                Ok(Some(f)) => self.frame = Some(f),
                Ok(None) => {} // a CPU picture is pending: what stands, stands
                Err(e) => {
                    self.refused = Some(key);
                    diag_warn!("stock eye: {e}");
                    return Err(e);
                }
            }
        }
        let Some(f) = &self.frame else { return Ok(false) };
        let v = |p: Point3| [p.x as f32, p.y as f32, p.z as f32];
        let geom = FrameGeom {
            eye: v(f.pcam.eye),
            fwd: v(f.pcam.fwd),
            right: v(f.pcam.right),
            up: v(f.pcam.up),
            tan_half_fov: TAN_HALF_FOV as f32,
            ortho_half_height: ortho_word(&f.pcam),
            view: f.key.view,
            k: f.k,
            trace: f.trace,
            verdict: self.verdict,
        };
        self.composite.write(queue, &f.mvp, &geom);
        Ok(true)
    }

    /// Paint the prepared frame into the open 3D pass (a no-op when
    /// `prepare` did not answer `Ok(true)` this frame).
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if let Some(f) = &self.frame {
            self.composite.draw(pass, &f.bind);
        }
    }

    /// A playback frame of the resident session (the same packed base)
    /// only re-sends its cut layer - just the patched runs when the
    /// patch's chain covers the resident frame, else the whole layer
    /// (repacked here when the frame carries only a patch): true when
    /// that is done.
    fn rewrite_cut(&self, queue: &wgpu::Queue, fv: &FieldView) -> bool {
        let held = self.source.as_ref().and_then(|s| s.packed.as_ref());
        let (Some(held), Some(next), Some(trace)) = (held, fv.packed.as_ref(), self.trace.as_ref()) else {
            return false;
        };
        if !(self.gpu_scene && Arc::ptr_eq(&held.base, &next.base)) {
            return false;
        }
        match &next.cut {
            CutLayer::Patch { patch, .. } if next.patches(held.seq) => trace.write_cut_patch(queue, patch).is_ok(),
            _ => full_cut(fv).is_some_and(|c| trace.write_cut(queue, &c).is_ok()),
        }
    }

    /// Put `fv`'s lattices on the GPU; false (and a log line) when the
    /// pass is absent or refuses them - the CPU marches instead. A view
    /// packed off-thread (playback) is sent as packed; any other is
    /// packed here.
    fn upload(&mut self, device: &wgpu::Device, fv: &FieldView) -> bool {
        let Some(trace) = self.trace.as_mut() else { return false };
        if let Some(p) = &fv.packed {
            let Some(cut) = full_cut(fv) else {
                diag_warn!("stock eye: the playback cut layer would not pack - the CPU march stands in");
                return false;
            };
            return match trace.upload_layers(device, &p.base.meta, &p.base.mesh, &cut, p.base.target.as_ref()) {
                Ok(()) => true,
                Err(e) => {
                    diag_warn!("stock eye: {e} - the CPU march stands in");
                    false
                }
            };
        }
        let Some(stock) = fv.stock.field().and_then(pack_field) else {
            diag_warn!("stock eye: the stock field would not pack - the CPU march stands in");
            return false;
        };
        let target = fv.target.as_ref().and_then(|t| t.field()).and_then(pack_target);
        match trace.upload(device, &stock, target.as_ref()) {
            Ok(()) => true,
            Err(e) => {
                diag_warn!("stock eye: {e} - the CPU march stands in");
                false
            }
        }
    }

    /// March `fv` for `key`: `Ok(None)` when the CPU worker has the
    /// frame in hand and nothing is finished yet.
    #[allow(clippy::too_many_arguments)] // device, queue, encoder + the frame's own four
    fn march(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        fv: &Arc<FieldView>,
        pcam: &PreviewCamera,
        key: Key,
        mvp: &[[f32; 4]; 4],
    ) -> Result<Option<Frame>, String> {
        let gpu = self.trace.as_ref().filter(|_| self.gpu_scene);
        let budget = if gpu.is_some() { GPU_PIXELS } else { CPU_PIXELS };
        let (k, tw, th) = fit(key.view, budget);
        let (buf, source) = match gpu {
            Some(t) => (t.encode(device, queue, encoder, pcam, tw, th, fv.tol)?, Arc::clone(fv)),
            None => {
                let Some((pv, src)) = self.cpu_picture(fv, pcam, key, (tw, th))? else { return Ok(None) };
                let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("stock-eye-cpu-frame"),
                    contents: &encode(&pv),
                    usage: wgpu::BufferUsages::STORAGE,
                });
                (buf, src)
            }
        };
        let bind = self.composite.bind(device, &buf);
        Ok(Some(Frame { key, source, bind, _buf: buf, k, trace: (tw, th), pcam: *pcam, mvp: *mvp }))
    }

    /// The worker's picture for `key`, with the view it marched: `fv`
    /// itself, or an older view of the same playback session (the
    /// newest is asked for, and the finished one stands meanwhile -
    /// a march slower than the transport still lands every picture it
    /// finishes). Asked for when not already in flight; `Ok(None)`
    /// while pending. A picture for a moved camera is thrown away.
    fn cpu_picture(
        &mut self,
        fv: &Arc<FieldView>,
        pcam: &PreviewCamera,
        key: Key,
        (w, h): (u32, u32),
    ) -> Result<Option<(Preview, Arc<FieldView>)>, String> {
        if self.cpu.is_none() {
            self.cpu = Some(CpuEye::spawn()?);
        }
        let cpu = self.cpu.as_ref().ok_or("the CPU eye has no worker")?;
        let shows = |f: &Arc<FieldView>| Arc::ptr_eq(f, fv) || same_session(f, fv);
        if let Some(p) = cpu.take() {
            if p.key == Key::NONE {
                return Err(p.frame.err().unwrap_or_else(|| "cpu eye: the worker is lost".into()));
            }
            if p.key == key && let Some(src) = p.fv.filter(shows) {
                let pv = p.frame?;
                if !cpu.in_flight(key, fv) && !Arc::ptr_eq(&src, fv) {
                    cpu.request(CpuJob { fv: Arc::clone(fv), cam: *pcam, w, h, key });
                }
                return Ok(Some((pv, src)));
            }
        }
        if !cpu.in_flight(key, fv) {
            cpu.request(CpuJob { fv: Arc::clone(fv), cam: *pcam, w, h, key });
        }
        Ok(None)
    }
}

/// Two views of one playback session: the same packed base.
fn same_session(a: &FieldView, b: &FieldView) -> bool {
    match (&a.packed, &b.packed) {
        (Some(x), Some(y)) => Arc::ptr_eq(&x.base, &y.base),
        _ => false,
    }
}

/// A packed frame's whole cut layer: carried, or (a patch frame the
/// resident state cannot take) repacked from the frame's own stock.
fn full_cut(fv: &FieldView) -> Option<Arc<Vec<f32>>> {
    let p = fv.packed.as_ref()?;
    match &p.cut {
        CutLayer::Full(c) => Some(Arc::clone(c)),
        CutLayer::Patch { .. } => fv.stock.field().map(|f| Arc::new(pack_cut(f, &p.base.meta))),
    }
}

